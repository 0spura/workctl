use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

const DEADLINE: Duration = Duration::from_secs(30);
const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const READ_CHUNK_BYTES: usize = 8192;

#[derive(Debug)]
pub enum ProcessError {
    NotFound,
    Timeout,
    OutputLimit,
    Io,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    cwd: Option<&Path>,
) -> Result<ProcessOutput, ProcessError> {
    run_with_deadline(program, args, input, cwd, DEADLINE)
}

pub(crate) fn run_with_deadline(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    cwd: Option<&Path>,
    deadline: Duration,
) -> Result<ProcessOutput, ProcessError> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let mut child = command.spawn().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ProcessError::NotFound
        } else {
            ProcessError::Io
        }
    })?;
    let stdout = child.stdout.take().ok_or(ProcessError::Io)?;
    let stderr = child.stderr.take().ok_or(ProcessError::Io)?;
    let (stdout_sender, stdout_receiver) = mpsc::sync_channel(1);
    drop(thread::spawn(move || {
        let _ = stdout_sender.send(drain(stdout, true));
    }));
    let (stderr_sender, stderr_receiver) = mpsc::sync_channel(1);
    drop(thread::spawn(move || {
        let _ = stderr_sender.send(drain(stderr, true));
    }));
    let input_writer = input.map(|bytes| {
        let stdin = child.stdin.take();
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(thread::spawn(move || {
            let result = match stdin {
                Some(mut stdin) => match stdin.write_all(&bytes) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                    Err(_) => Err(ProcessError::Io),
                },
                None => Err(ProcessError::Io),
            };
            let _ = sender.send(result);
        }));
        receiver
    });

    let wait_result = remaining(started, deadline)
        .and_then(|remaining| child.wait_timeout(remaining).map_err(|_| ProcessError::Io));
    let status = match wait_result {
        Ok(Some(status)) => status,
        Ok(None) | Err(ProcessError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProcessError::Timeout);
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

    let (stdout, stdout_overflow) = receive(stdout_receiver, started, deadline)??;
    let (stderr, stderr_overflow) = receive(stderr_receiver, started, deadline)??;
    if let Some(writer) = input_writer {
        receive(writer, started, deadline)??;
    }
    if stdout_overflow || stderr_overflow {
        return Err(ProcessError::OutputLimit);
    }

    Ok(ProcessOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

fn remaining(started: Instant, deadline: Duration) -> Result<Duration, ProcessError> {
    deadline
        .checked_sub(started.elapsed())
        .ok_or(ProcessError::Timeout)
}

fn receive<T>(
    receiver: Receiver<T>,
    started: Instant,
    deadline: Duration,
) -> Result<T, ProcessError> {
    receiver
        .recv_timeout(remaining(started, deadline)?)
        .map_err(|error| match error {
            RecvTimeoutError::Timeout => ProcessError::Timeout,
            RecvTimeoutError::Disconnected => ProcessError::Io,
        })
}

fn drain<R: Read>(mut reader: R, capture: bool) -> Result<(Vec<u8>, bool), ProcessError> {
    let mut captured = Vec::new();
    let mut overflow = false;
    let mut total = 0usize;
    let mut chunk = [0u8; READ_CHUNK_BYTES];

    loop {
        let count = reader.read(&mut chunk).map_err(|_| ProcessError::Io)?;
        if count == 0 {
            break;
        }
        if capture {
            let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
            let keep = count.min(remaining);
            captured.extend_from_slice(&chunk[..keep]);
            overflow |= keep != count;
        } else {
            total = total.saturating_add(count);
            overflow |= total > MAX_CAPTURE_BYTES;
        }
    }
    Ok((captured, overflow))
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{run_with_deadline, ProcessError, MAX_CAPTURE_BYTES};

    #[test]
    fn terminates_a_child_at_the_deadline() {
        let args = ["-c", "exec sleep 2"].map(str::to_owned);
        let start = Instant::now();
        let error =
            run_with_deadline("sh", &args, None, None, Duration::from_millis(20)).unwrap_err();
        assert!(matches!(error, ProcessError::Timeout));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn deadline_covers_readers_of_inherited_pipes() {
        let args = ["-c", "(sleep 0.25) & exit 0"].map(str::to_owned);
        let start = Instant::now();
        let error =
            run_with_deadline("sh", &args, None, None, Duration::from_millis(20)).unwrap_err();
        assert!(matches!(error, ProcessError::Timeout));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn rejects_captured_output_above_the_fixed_limit() {
        let byte_count = (MAX_CAPTURE_BYTES + 1).to_string();
        let args = ["-c".to_owned(), byte_count, "/dev/zero".to_owned()];
        let error =
            run_with_deadline("head", &args, None, None, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(error, ProcessError::OutputLimit));
    }

    #[test]
    fn rejects_oversized_discarded_stderr_too() {
        let byte_count = (MAX_CAPTURE_BYTES + 1).to_string();
        let script = format!("head -c {byte_count} /dev/zero >&2");
        let args = ["-c".to_owned(), script];
        let error = run_with_deadline("sh", &args, None, None, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(error, ProcessError::OutputLimit));
    }
}
