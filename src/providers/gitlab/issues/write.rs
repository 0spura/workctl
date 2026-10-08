use crate::domain::{AppError, Issue};
use crate::providers::gitlab::issues::{GitLabIssueCreate, GitLabIssueUpdate, GitLabIssues};

pub(super) fn create(
    provider: &GitLabIssues,
    issue: &GitLabIssueCreate,
) -> Result<Issue, AppError> {
    if issue.description == "-" {
        return Err(AppError::invalid_input(
            "a description consisting only of '-' cannot be passed to glab",
        ));
    }
    let mut args = vec![
        "issue".to_owned(),
        "create".to_owned(),
        "--repo".to_owned(),
        provider.repo_argument(),
        format!("--title={}", issue.title),
        "--description-file=-".to_owned(),
    ];
    push_values(&mut args, "--label", &issue.labels);
    push_values(&mut args, "--assignee", &issue.assignees);
    if let Some(milestone) = &issue.milestone {
        args.push(format!("--milestone={milestone}"));
    }
    if issue.confidential {
        args.push("--confidential".to_owned());
    }
    if let Some(weight) = issue.weight {
        args.push(format!("--weight={weight}"));
    }
    if let Some(due_date) = &issue.due_date {
        args.push(format!("--due-date={due_date}"));
    }

    let output = provider.run_glab_mutation(&args, Some(issue.description.as_bytes().to_vec()))?;
    let number = created_issue_number(provider, &output)?;
    if issue.weight == Some(0) {
        let args = [
            "issue".to_owned(),
            "update".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            provider.repo_argument(),
            "--weight=0".to_owned(),
        ];
        provider.run_glab_mutation(&args, None)?;
    }
    read_back(provider, number)
}

pub(super) fn update(
    provider: &GitLabIssues,
    number: u64,
    patch: &GitLabIssueUpdate,
) -> Result<Issue, AppError> {
    if patch.description.as_deref() == Some("") {
        return Err(AppError::invalid_input(
            "glab issue update cannot clear an issue description with an empty value",
        ));
    }
    if patch.description.as_deref() == Some("-") {
        return Err(AppError::invalid_input(
            "a description consisting only of '-' cannot be passed to glab",
        ));
    }
    if patch.is_empty() {
        return Err(AppError::invalid_input("update requires a field to change"));
    }

    let mut args = vec![
        "issue".to_owned(),
        "update".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo_argument(),
    ];
    if let Some(title) = &patch.title {
        args.push(format!("--title={title}"));
    }
    if patch.description.is_some() {
        args.push("--description-file=-".to_owned());
    }
    push_values(&mut args, "--label", &patch.labels_add);
    push_values(&mut args, "--unlabel", &patch.labels_remove);
    for assignee in &patch.assignees {
        args.push(format!("--assignee={assignee}"));
    }
    if patch.unassign {
        args.push("--unassign".to_owned());
    }
    if let Some(milestone) = &patch.milestone {
        args.push(format!("--milestone={milestone}"));
    }
    if let Some(confidential) = patch.confidential {
        args.push(if confidential {
            "--confidential".to_owned()
        } else {
            "--public".to_owned()
        });
    }
    if let Some(weight) = patch.weight {
        args.push(format!("--weight={weight}"));
    }
    if let Some(due_date) = &patch.due_date {
        args.push(format!("--due-date={due_date}"));
    }

    provider.run_glab_mutation(
        &args,
        patch
            .description
            .as_ref()
            .map(|description| description.as_bytes().to_vec()),
    )?;
    read_back(provider, number)
}

fn read_back(provider: &GitLabIssues, number: u64) -> Result<Issue, AppError> {
    provider
        .show(number)
        .map_err(|_| AppError::gitlab_write_uncertain())
        .and_then(|issue| {
            if issue.number == number {
                Ok(issue)
            } else {
                Err(AppError::gitlab_write_uncertain())
            }
        })
}
fn push_values(args: &mut Vec<String>, flag: &str, values: &[String]) {
    for value in values {
        args.push(format!("{flag}={value}"));
    }
}

fn created_issue_number(provider: &GitLabIssues, output: &[u8]) -> Result<u64, AppError> {
    let output = std::str::from_utf8(output).map_err(|_| AppError::gitlab_write_uncertain())?;
    let url = output.trim();
    let prefix = format!("{}/-/issues/", provider.repo_argument());
    let number = url
        .strip_prefix(&prefix)
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|suffix| suffix.parse::<u64>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(AppError::gitlab_write_uncertain)?;
    Ok(number)
}

impl GitLabIssueUpdate {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.labels_add.is_empty()
            && self.labels_remove.is_empty()
            && self.assignees.is_empty()
            && !self.unassign
            && self.milestone.is_none()
            && self.confidential.is_none()
            && self.weight.is_none()
            && self.due_date.is_none()
    }
}
