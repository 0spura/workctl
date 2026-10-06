pub mod issues;

use crate::domain::AppError;
use crate::process::runner::{self, ProcessError};

/// The only GitLab host this build resolves, so every invocation pins it explicitly.
pub(super) const HOST: &str = "gitlab.com";

/// The repository argument `glab` receives: a full URL, so the host never depends on the
/// directory `workctl` happens to run in.
pub(super) fn repo_argument(repo: &str) -> String {
    format!("https://{HOST}/{repo}")
}

/// Runs `glab`, optionally sending text through stdin, and maps process failures onto the shared
/// error contract.
pub(super) fn run_glab(args: &[String]) -> Result<Vec<u8>, AppError> {
    run_glab_with_input(args, None)
}

pub(super) fn run_glab_with_input(
    args: &[String],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, AppError> {
    let output = runner::run("glab", args, input, None).map_err(map_process_error)?;
    if !output.success {
        return Err(AppError::gitlab_cli());
    }
    Ok(output.stdout)
}

/// A write failure after process launch can leave the remote mutation committed.
pub(super) fn run_glab_mutation(
    args: &[String],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, AppError> {
    let output = runner::run("glab", args, input, None).map_err(|error| match error {
        ProcessError::NotFound => AppError::dependency(),
        ProcessError::Timeout | ProcessError::OutputLimit | ProcessError::Io => {
            AppError::gitlab_write_uncertain()
        }
    })?;
    if !output.success {
        return Err(AppError::gitlab_write_uncertain());
    }
    Ok(output.stdout)
}

pub(super) fn authenticate() -> Result<(), AppError> {
    let args = ["auth", "status", "--hostname", HOST].map(str::to_owned);
    let output = runner::run("glab", &args, None, None).map_err(map_process_error)?;
    if output.success {
        Ok(())
    } else {
        Err(AppError::gitlab_authentication())
    }
}

pub(super) fn map_process_error(error: ProcessError) -> AppError {
    match error {
        ProcessError::NotFound => AppError::dependency(),
        ProcessError::Timeout => AppError::timeout(),
        ProcessError::OutputLimit => AppError::output_limit(),
        ProcessError::Io => AppError::gitlab_cli(),
    }
}
