use crate::domain::{AppError, Issue};
use crate::providers::github::issues::{GitHubIssues, read};
use crate::providers::{IssuePatch, NewIssue};

pub(super) fn create(provider: &GitHubIssues, issue: &NewIssue) -> Result<Issue, AppError> {
    if !issue.attachments.is_empty() {
        super::super::require_attachment_support()?;
    }
    let milestone = super::super::resolve_milestone(&provider.repo, issue.milestone.as_deref())?;
    let mut args = vec![
        "issue".to_owned(),
        "create".to_owned(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--title".to_owned(),
        issue.title.clone(),
        "--body-file".to_owned(),
        "-".to_owned(),
    ];
    push_repeated(&mut args, "--assignee", &issue.assignees);
    push_repeated(&mut args, "--label", &issue.labels);
    if let Some(milestone) = milestone {
        args.extend(["--milestone".to_owned(), milestone]);
    }
    push_repeated(&mut args, "--project", &issue.projects);
    push_repeated(&mut args, "--attach", &issue.attachments);
    let output =
        super::super::run_gh_raw(&args, Some(issue.body.as_bytes().to_vec())).map_err(|_| {
            if issue.attachments.is_empty() {
                AppError::github_cli()
            } else {
                AppError::attachment_create_uncertain()
            }
        })?;
    if !output.success {
        return Err(if issue.attachments.is_empty() {
            AppError::github_cli()
        } else {
            AppError::attachment_create_uncertain()
        });
    }
    read::show(provider, parse_issue_number(&output.stdout)?)
}

pub(super) fn edit(
    provider: &GitHubIssues,
    number: u64,
    patch: &IssuePatch,
    body: Option<&str>,
) -> Result<Issue, AppError> {
    if !patch.attachments.is_empty() {
        super::super::require_attachment_support()?;
    }
    let milestone = super::super::resolve_milestone(&provider.repo, patch.milestone.as_deref())?;
    let mut args = vec![
        "issue".to_owned(),
        "edit".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if let Some(title) = &patch.title {
        args.extend(["--title".to_owned(), title.clone()]);
    }
    if body.is_some() {
        args.extend(["--body-file".to_owned(), "-".to_owned()]);
    }
    push_repeated(&mut args, "--add-assignee", &patch.assignees_add);
    push_repeated(&mut args, "--remove-assignee", &patch.assignees_remove);
    push_repeated(&mut args, "--add-label", &patch.labels_add);
    push_repeated(&mut args, "--remove-label", &patch.labels_remove);
    if let Some(milestone) = milestone {
        args.extend(["--milestone".to_owned(), milestone]);
    } else if patch.clear_milestone {
        args.push("--remove-milestone".to_owned());
    }
    push_repeated(&mut args, "--add-project", &patch.projects_add);
    push_repeated(&mut args, "--remove-project", &patch.projects_remove);
    push_repeated(&mut args, "--attach", &patch.attachments);
    provider.run_gh(&args, body.map(|body| body.as_bytes().to_vec()))?;
    read::show(provider, number)
}

fn push_repeated(args: &mut Vec<String>, flag: &str, values: &[String]) {
    for value in values {
        args.push(flag.to_owned());
        args.push(value.clone());
    }
}

fn parse_issue_number(output: &[u8]) -> Result<u64, AppError> {
    let output = std::str::from_utf8(output).map_err(|_| AppError::provider_response())?;
    let url = output
        .lines()
        .find(|line| line.contains("/issues/"))
        .ok_or_else(AppError::provider_response)?;
    let number = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(AppError::provider_response)?;
    number.parse().map_err(|_| AppError::provider_response())
}
