use crate::domain::{AppError, CheckRun, PullRequest, PullRequestSummary};
use crate::providers::PrQuery;
use crate::providers::github::prs::{GitHubPulls, mapping};

const SHOW_FIELDS: &str = "number,title,body,state,isDraft,url,baseRefName,headRefName,author,createdAt,updatedAt,mergedAt,mergeable,reviewDecision,labels,assignees";
const LIST_FIELDS: &str = "number,title,state,isDraft,url,baseRefName,headRefName,updatedAt";
const CHECK_FIELDS: &str = "name,state,bucket,description,link,workflow";

pub(super) fn list(
    provider: &GitHubPulls,
    query: &PrQuery,
) -> Result<Vec<PullRequestSummary>, AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--state".to_owned(),
        query.state.clone(),
        "--limit".to_owned(),
        query.limit.to_string(),
    ];
    for label in &query.labels {
        args.push("--label".to_owned());
        args.push(label.clone());
    }
    for (flag, value) in [
        ("--assignee", &query.assignee),
        ("--author", &query.author),
        ("--base", &query.base),
        ("--head", &query.head),
        ("--search", &query.search),
    ] {
        if let Some(value) = value {
            args.push(flag.to_owned());
            args.push(value.clone());
        }
    }
    if query.draft {
        args.push("--draft".to_owned());
    }
    args.push("--json".to_owned());
    args.push(LIST_FIELDS.to_owned());
    let output = provider.run_gh(&args, None)?;
    let mut pull_requests = mapping::pull_request_summaries(&output)?;
    pull_requests.truncate(query.limit);
    Ok(pull_requests)
}

pub(super) fn show(provider: &GitHubPulls, number: u64) -> Result<PullRequest, AppError> {
    // `gh pr view` exits non-zero when the number is not a pull request, so the process status is
    // inspected directly: `run_gh` would collapse that signal into a generic CLI failure.
    let args = [
        "pr".to_owned(),
        "view".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--json".to_owned(),
        SHOW_FIELDS.to_owned(),
    ];
    let output = provider.run_gh_raw(&args, None)?;
    if !output.success {
        return Err(AppError::not_pull_request());
    }
    mapping::pull_request(&output.stdout)
}

pub(super) fn diff(
    provider: &GitHubPulls,
    number: u64,
    name_only: bool,
) -> Result<String, AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "diff".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if name_only {
        args.push("--name-only".to_owned());
    }
    let output = provider.run_gh(&args, None)?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

pub(super) fn checks(
    provider: &GitHubPulls,
    number: u64,
    required: bool,
) -> Result<Vec<CheckRun>, AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "checks".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if required {
        args.push("--required".to_owned());
    }
    args.push("--json".to_owned());
    args.push(CHECK_FIELDS.to_owned());

    // `gh pr checks` exits non-zero for failing (1) and pending (8) checks while still printing a
    // valid report on stdout, and writes "no checks reported" to stderr when the pull request has
    // none, so the exit status cannot be trusted and the raw result is read instead.
    let output = provider.run_gh_raw(&args, None)?;
    match mapping::check_runs(&output.stdout) {
        Ok(checks) => Ok(checks),
        Err(_) if String::from_utf8_lossy(&output.stderr).contains("no checks reported") => {
            Ok(Vec::new())
        }
        Err(_) => Err(AppError::github_cli()),
    }
}
