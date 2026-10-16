use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::domain::AppError;

const API_URL: &str = "https://api.github.com/repos/0spura/workctl/git/ref/heads/main";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_RESPONSE_BYTES: usize = 1024;
const CURRENT_REVISION: &str = env!("WORKCTL_GIT_SHA");
const INSTALL_COMMAND: &str =
    "cargo install --git https://github.com/0spura/workctl --locked --force";

#[derive(Debug, Deserialize)]
struct ReferenceResponse {
    object: CommitObject,
}

#[derive(Debug, Deserialize)]
struct CommitObject {
    sha: String,
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_command: Option<&'static str>,
    /// True when this run replaced the installed binary with `latest`.
    pub installed: bool,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct CheckCache {
    checked_at: u64,
    latest: String,
}

pub fn check() -> Result<UpdateStatus, AppError> {
    if !valid_revision(CURRENT_REVISION) {
        return Err(AppError::context(
            "cannot determine the installed workctl revision; install from the Git repository",
        ));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|_| AppError::context("could not check workctl updates from GitHub"))?;
    let result = runtime.block_on(fetch_latest_revision());
    runtime.shutdown_background();
    let latest = match result {
        Ok(latest) => latest,
        // The unauthenticated API is rate limited; the Git transport is not.
        Err(error) => ls_remote_revision().ok_or(error)?,
    };
    Ok(update_status(CURRENT_REVISION.to_ascii_lowercase(), latest))
}

/// Checks for a newer revision and, unless `check_only`, installs it with Cargo.
///
/// Cargo's progress goes to stderr; stdout stays reserved for the command result.
pub fn run(check_only: bool) -> Result<UpdateStatus, AppError> {
    let mut status = check()?;
    if check_only || !status.update_available {
        return Ok(status);
    }
    let mut words = INSTALL_COMMAND.split(' ');
    let program = words.next().unwrap_or("cargo");
    let result = std::process::Command::new(program)
        .args(words)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
    match result {
        Ok(code) if code.success() => {
            status.installed = true;
            status.install_command = None;
            Ok(status)
        }
        Ok(_) => Err(AppError::context(
            "cargo install failed; see the cargo output above",
        )),
        Err(_) => Err(AppError::context(
            "could not run cargo; install Rust's cargo or run the install command manually",
        )),
    }
}

/// Reads `main`'s revision through `git ls-remote`, which has no API rate limit.
fn ls_remote_revision() -> Option<String> {
    let output = std::process::Command::new("git")
        .args([
            "ls-remote",
            "https://github.com/0spura/workctl",
            "refs/heads/main",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8(output.stdout).ok()?;
    let sha = text.split_whitespace().next()?;
    valid_revision(sha).then(|| sha.to_ascii_lowercase())
}

async fn fetch_latest_revision() -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::context("could not check workctl updates from GitHub"))?;
    let mut response = client
        .get(API_URL)
        .header(
            reqwest::header::USER_AGENT,
            concat!("workctl/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| AppError::context("could not check workctl updates from GitHub"))?;

    let mut body = Vec::with_capacity(512);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::context("could not check workctl updates from GitHub"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(AppError::context(
                "GitHub returned an oversized update response",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    parse_latest_revision(&body)
}

fn parse_latest_revision(body: &[u8]) -> Result<String, AppError> {
    let response: ReferenceResponse = serde_json::from_slice(body)
        .map_err(|_| AppError::context("GitHub returned an invalid workctl update response"))?;
    if !valid_revision(&response.object.sha) {
        return Err(AppError::context(
            "GitHub returned an invalid workctl revision",
        ));
    }
    Ok(response.object.sha.to_ascii_lowercase())
}

fn update_available(current: &str, latest: &str) -> bool {
    !latest.starts_with(current) && !current.starts_with(latest)
}

fn update_status(current: String, latest: String) -> UpdateStatus {
    let update_available = update_available(&current, &latest);
    UpdateStatus {
        current,
        latest,
        update_available,
        install_command: update_available.then_some(INSTALL_COMMAND),
        installed: false,
    }
}

fn valid_revision(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn cache_is_fresh(now: u64, checked_at: u64) -> bool {
    checked_at <= now && now - checked_at < CHECK_INTERVAL.as_secs()
}

pub fn check_if_due() -> Option<UpdateStatus> {
    if !valid_revision(CURRENT_REVISION) {
        return None;
    }
    let path = cache_path()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if let Some(contents) = read_cache(&path) {
        if let Ok(cache) = serde_json::from_slice::<CheckCache>(&contents) {
            if valid_revision(&cache.latest) && cache_is_fresh(now, cache.checked_at) {
                return Some(update_status(
                    CURRENT_REVISION.to_ascii_lowercase(),
                    cache.latest.to_ascii_lowercase(),
                ));
            }
        }
    }

    let status = check().ok()?;
    if let Ok(contents) = serde_json::to_vec(&CheckCache {
        checked_at: now,
        latest: status.latest.clone(),
    }) {
        write_cache(&path, &contents, now);
    }
    Some(status)
}

fn read_cache(path: &Path) -> Option<Vec<u8>> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .ok()?
    };

    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        let mut options = OpenOptions::new();
        let file = options
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return None;
        }
        file
    };

    #[cfg(unix)]
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut contents = Vec::with_capacity(128);
    file.take(MAX_RESPONSE_BYTES as u64)
        .read_to_end(&mut contents)
        .ok()?;
    (contents.len() < MAX_RESPONSE_BYTES).then_some(contents)
}

fn write_cache(path: &Path, contents: &[u8], now: u64) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(metadata) = fs::symlink_metadata(parent) else {
        return;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let temp_path = parent.join(format!(".{name}.{}-{now}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let Ok(mut file) = options.open(&temp_path) else {
        return;
    };
    if file.write_all(contents).is_err() {
        let _ = fs::remove_file(&temp_path);
        return;
    }
    drop(file);
    if fs::rename(&temp_path, path).is_err() {
        let _ = fs::remove_file(&temp_path);
    }
}

fn cache_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())?;
    #[cfg(not(target_os = "windows"))]
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".cache"))
                .filter(|path| path.is_absolute())
        })?;
    Some(root.join("workctl").join("update-check.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_github_git_reference_sha() {
        let sha = "a".repeat(40);
        let response = format!(r#"{{"object":{{"sha":"{sha}"}}}}"#);
        assert_eq!(parse_latest_revision(response.as_bytes()).unwrap(), sha);
    }

    #[test]
    fn rejects_invalid_git_reference_response() {
        assert!(parse_latest_revision(br#"{"object":{"sha":"invalid"}}"#).is_err());
    }

    #[test]
    fn revision_validation_requires_full_hex_sha() {
        assert!(valid_revision(&"a".repeat(40)));
        assert!(!valid_revision("abcdef"));
        assert!(!valid_revision(&"g".repeat(40)));
    }

    #[test]
    fn equal_or_prefix_revisions_are_current() {
        assert!(!update_available("abcdef", "abcdef"));
        assert!(!update_available("abcdef", "abcdef1234"));
        assert!(!update_available("abcdef1234", "abcdef"));
    }

    #[test]
    fn newer_revision_includes_install_command_in_json() {
        let status = update_status("a".repeat(40), "b".repeat(40));
        assert!(status.update_available);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["update_available"], true);
        assert_eq!(json["install_command"], INSTALL_COMMAND);
    }

    #[test]
    fn different_revisions_need_update() {
        assert!(update_available("abcdef1234", "fedcba9876"));
    }

    #[test]
    fn future_cache_timestamps_are_stale() {
        assert!(!cache_is_fresh(100, 101));
    }

    #[test]
    fn cache_entries_expire_at_one_day() {
        assert!(cache_is_fresh(86_399, 0));
        assert!(!cache_is_fresh(86_400, 0));
    }

    #[cfg(unix)]
    #[test]
    fn cache_write_replaces_symlink_without_touching_target() {
        use std::os::unix::fs::symlink;

        let directory =
            std::env::temp_dir().join(format!("workctl-update-cache-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("target");
        let cache = directory.join("update-check.json");
        fs::write(&target, b"keep").unwrap();
        symlink(&target, &cache).unwrap();

        write_cache(&cache, b"cache", 123);

        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert_eq!(read_cache(&cache).as_deref(), Some(&b"cache"[..]));
        fs::remove_dir_all(directory).unwrap();
    }
}
