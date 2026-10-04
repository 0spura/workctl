pub mod issues;
pub mod prs;

use crate::domain::AppError;
use crate::process::runner::{self, ProcessError};

/// Runs `gh`, mapping a process failure onto the shared error contract.
pub(super) fn run_gh(args: &[String], input: Option<Vec<u8>>) -> Result<Vec<u8>, AppError> {
    let output = run_gh_raw(args, input)?;
    if !output.success {
        return Err(AppError::github_cli());
    }
    Ok(output.stdout)
}

/// Runs `gh` and returns the raw result, including a non-zero exit status.
///
/// `gh` reports some outcomes through the exit status while still printing a usable payload, so a
/// caller that can interpret the status itself uses this instead of `run_gh`.
pub(super) fn run_gh_raw(
    args: &[String],
    input: Option<Vec<u8>>,
) -> Result<runner::ProcessOutput, AppError> {
    runner::run("gh", args, input, None).map_err(map_process_error)
}

pub(super) fn authenticate() -> Result<(), AppError> {
    let args = ["auth", "status", "--hostname", "github.com"].map(str::to_owned);
    let output = runner::run("gh", &args, None, None).map_err(map_process_error)?;
    if output.success {
        Ok(())
    } else {
        Err(AppError::authentication())
    }
}

pub(super) fn map_process_error(error: ProcessError) -> AppError {
    match error {
        ProcessError::NotFound => AppError::dependency(),
        ProcessError::Timeout => AppError::timeout(),
        ProcessError::OutputLimit => AppError::output_limit(),
        ProcessError::Io => AppError::github_cli(),
    }
}

pub(super) fn require_attachment_support() -> Result<(), AppError> {
    let args = ["--version"].map(str::to_owned);
    let output = runner::run("gh", &args, None, None).map_err(map_process_error)?;
    if !output.success {
        return Err(AppError::github_cli());
    }
    let version =
        std::str::from_utf8(&output.stdout).map_err(|_| AppError::attachment_cli_version())?;
    let version = version
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(parse_version)
        .ok_or_else(AppError::attachment_cli_version)?;
    if version >= (2, 99, 0) {
        Ok(())
    } else {
        Err(AppError::attachment_cli_version())
    }
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
    .filter(|_| parts.next().is_none())
}
/// Milestone selector meaning "the nearest open milestone due today or later".
pub(super) const CURRENT_MILESTONE: &str = "@current";

/// Resolves an explicit milestone selector into the value `gh` receives.
///
/// `None` means the caller omitted `--milestone`: nothing is looked up and the field stays unset.
/// `@current` selects the nearest open milestone due today or later and fails when none qualifies.
/// Any other value is a milestone name passed through verbatim.
pub(super) fn resolve_milestone(
    repo: &str,
    milestone: Option<&str>,
) -> Result<Option<String>, AppError> {
    match milestone {
        None => Ok(None),
        Some(CURRENT_MILESTONE) => current_milestone(repo)?.map(Some).ok_or_else(|| {
            AppError::invalid_input(
                "no open milestone is due today or later for @current; pass --milestone explicitly",
            )
        }),
        Some(name) => Ok(Some(name.to_owned())),
    }
}

/// Selects the nearest open milestone due today or later.
///
/// GitHub orders milestones by due date, but this validates every candidate so overdue or
/// undated milestones cannot be selected.
fn current_milestone(repo: &str) -> Result<Option<String>, AppError> {
    let endpoint = format!("repos/{repo}/milestones");
    let args = [
        "api",
        "--paginate",
        "--slurp",
        "-X",
        "GET",
        endpoint.as_str(),
        "-f",
        "state=open",
        "-f",
        "sort=due_on",
        "-f",
        "direction=asc",
        "-F",
        "per_page=100",
    ]
    .map(str::to_owned);
    let output = run_gh(&args, None)?;
    let value: serde_json::Value =
        serde_json::from_slice(&output).map_err(|_| AppError::provider_response())?;
    let pages = value.as_array().ok_or_else(AppError::provider_response)?;
    let today = utc_epoch_day();
    let mut nearest: Option<(i64, String)> = None;
    let mut tied = false;
    for page in pages {
        let milestones = page.as_array().ok_or_else(AppError::provider_response)?;
        for milestone in milestones {
            let due_on = match milestone.get("due_on") {
                Some(serde_json::Value::Null) => continue,
                Some(serde_json::Value::String(due_on)) => due_on,
                _ => return Err(AppError::provider_response()),
            };
            let due_day = parse_due_day(due_on).ok_or_else(AppError::provider_response)?;
            if due_day < today {
                continue;
            }
            let title = milestone
                .get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|title| !title.trim().is_empty())
                .ok_or_else(AppError::provider_response)?;
            match nearest.as_ref() {
                None => {
                    nearest = Some((due_day, title.to_owned()));
                    tied = false;
                }
                Some((nearest_day, _)) if due_day < *nearest_day => {
                    nearest = Some((due_day, title.to_owned()));
                    tied = false;
                }
                Some((nearest_day, _)) if due_day == *nearest_day => tied = true,
                _ => {}
            }
        }
    }
    if tied {
        return Err(AppError::invalid_input(
            "multiple open milestones share the nearest due date; pass --milestone explicitly",
        ));
    }
    Ok(nearest.map(|(_, title)| title))
}

fn parse_due_day(value: &str) -> Option<i64> {
    let date = value.get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1..=month_days[(month - 1) as usize]).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn utc_epoch_day() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .div_euclid(86_400) as i64
}

#[cfg(test)]
mod milestone_tests {
    use super::parse_due_day;

    #[test]
    fn parses_valid_calendar_dates_from_github_timestamps() {
        assert_eq!(parse_due_day("2024-02-29T00:00:00Z"), Some(19_782));
        assert_eq!(parse_due_day("2023-02-29T00:00:00Z"), None);
        assert_eq!(parse_due_day("2024-13-01T00:00:00Z"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_only_three_component_release_versions() {
        assert_eq!(parse_version("2.99.0"), Some((2, 99, 0)));
        assert_eq!(parse_version("2.100.1"), Some((2, 100, 1)));
        assert_eq!(parse_version("2.98"), None);
        assert_eq!(parse_version("2.99.0-beta"), None);
    }
}
