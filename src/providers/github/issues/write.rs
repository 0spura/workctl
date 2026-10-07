use crate::domain::{AppError, Issue};
use crate::providers::github::issues::{GitHubIssues, NativeIssueEdit, read};
use crate::providers::{IssuePatch, NewIssue};

pub(super) fn create(
    provider: &GitHubIssues,
    issue: &NewIssue,
    project_plan: Option<&super::super::projects::ProjectPlan>,
) -> Result<Issue, AppError> {
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
    if project_plan.is_none() {
        push_repeated(&mut args, "--project", &issue.projects);
    }
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
    let issue_url = match parse_issue_url(&output.stdout, &provider.repo) {
        Ok(url) => url,
        Err(error) => {
            let Some(plan) = project_plan else {
                return Err(error);
            };
            let mut pending = vec!["project membership".to_owned()];
            pending.extend(super::super::projects::pending(plan, 0));
            return Err(AppError::partial_success(
                serde_json::json!({
                    "type": "issue",
                    "url": null,
                    "target": plan.url,
                }),
                &["issue creation".to_owned()],
                &pending,
            ));
        }
    };
    let number = parse_issue_number_from_url(&issue_url)?;
    if let Some(plan) = project_plan {
        let mut completed = vec!["issue creation".to_owned()];
        if super::super::projects::add_item(plan, &issue_url).is_err() {
            let mut pending = vec!["project membership".to_owned()];
            pending.extend(super::super::projects::pending(plan, 0));
            return Err(AppError::partial_success(
                serde_json::json!({
                    "type": "issue",
                    "number": number,
                    "url": issue_url,
                    "target": plan.url,
                }),
                &completed,
                &pending,
            ));
        }
        completed.push("project membership".to_owned());
        let mut applied = Vec::with_capacity(plan.assignments.len());
        if super::super::projects::set_fields(plan, &issue_url, &mut applied).is_err() {
            let pending = super::super::projects::pending(plan, applied.len());
            completed.extend(applied);
            return Err(AppError::partial_success(
                serde_json::json!({
                    "type": "issue",
                    "number": number,
                    "url": issue_url,
                    "target": plan.url,
                }),
                &completed,
                &pending,
            ));
        }
        completed.extend(applied);
        return read::show(provider, number).map_err(|_| {
            AppError::partial_success(
                serde_json::json!({
                    "type": "issue",
                    "number": number,
                    "url": issue_url,
                    "target": plan.url,
                }),
                &completed,
                &["issue readback".to_owned()],
            )
        });
    }
    read::show(provider, number)
}

/// Applies an issue edit — the generic patch plus the native fields — together with any
/// validated Project field changes.
///
/// The issue-level change is written first, then every Project field serially, then the issue is
/// read back. A field-only edit never calls `gh issue edit`, so it cannot send a no-op issue
/// update. The remote writes are not transactional and are never retried: a failure before any
/// completed operation returns its own error, while a failure after one returns `partial_success`
/// naming the completed and pending operations. The `updated_at` guard covers the issue edit
/// only; Project field changes are not part of the issue revision.
pub(super) fn edit_native(
    provider: &GitHubIssues,
    number: u64,
    patch: &IssuePatch,
    native: &NativeIssueEdit,
    body: Option<&str>,
    plan: Option<&super::super::projects::ProjectPlan>,
) -> Result<Issue, AppError> {
    let mut completed = Vec::new();
    if has_issue_writes(patch, native) {
        edit_issue(provider, number, patch, native, body)?;
        completed.push("issue edit".to_owned());
    }
    let Some(plan) = plan else {
        return read::show(provider, number).map_err(|error| {
            if completed.is_empty() {
                error
            } else {
                AppError::partial_success(
                    serde_json::json!({
                        "type": "issue", "number": number,
                        "url": canonical_issue_url(&provider.repo, number),
                    }),
                    &completed,
                    &["issue readback".to_owned()],
                )
            }
        });
    };
    let issue_url = canonical_issue_url(&provider.repo, number);
    let mut applied = Vec::with_capacity(plan.assignments.len());
    if let Err(error) = super::super::projects::set_fields(plan, &issue_url, &mut applied) {
        if completed.is_empty() && applied.is_empty() {
            return Err(error);
        }
        let pending = super::super::projects::pending(plan, applied.len());
        completed.extend(applied);
        return Err(partial_issue_error(
            number, &issue_url, &plan.url, &completed, &pending,
        ));
    }
    completed.extend(applied);
    read::show(provider, number).map_err(|_| {
        partial_issue_error(
            number,
            &issue_url,
            &plan.url,
            &completed,
            &["issue readback".to_owned()],
        )
    })
}

/// Validates the issue-level change and sends one `gh issue edit`.
///
/// The native fields join the same invocation as the generic patch, each as its own `gh` flag.
fn edit_issue(
    provider: &GitHubIssues,
    number: u64,
    patch: &IssuePatch,
    native: &NativeIssueEdit,
    body: Option<&str>,
) -> Result<(), AppError> {
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
    if let Some(issue_type) = &native.issue_type {
        args.extend(["--type".to_owned(), issue_type.clone()]);
    }
    if native.remove_type {
        args.push("--remove-type".to_owned());
    }
    if let Some(parent) = &native.parent {
        args.extend(["--parent".to_owned(), parent.clone()]);
    }
    if native.remove_parent {
        args.push("--remove-parent".to_owned());
    }
    push_repeated(&mut args, "--add-sub-issue", &native.sub_issues_add);
    push_repeated(&mut args, "--remove-sub-issue", &native.sub_issues_remove);
    push_repeated(&mut args, "--add-blocked-by", &native.blocked_by_add);
    push_repeated(&mut args, "--remove-blocked-by", &native.blocked_by_remove);
    push_repeated(&mut args, "--add-blocking", &native.blocking_add);
    push_repeated(&mut args, "--remove-blocking", &native.blocking_remove);
    provider.run_gh(&args, body.map(|body| body.as_bytes().to_vec()))?;
    Ok(())
}

/// True when the patch or the native fields would make `gh issue edit` change something.
///
/// `expect_updated_at` only guards a write, so it does not count on its own.
fn has_issue_writes(patch: &IssuePatch, native: &NativeIssueEdit) -> bool {
    patch.title.is_some()
        || patch.body.is_some()
        || !patch.assignees_add.is_empty()
        || !patch.assignees_remove.is_empty()
        || !patch.labels_add.is_empty()
        || !patch.labels_remove.is_empty()
        || patch.milestone.is_some()
        || patch.clear_milestone
        || !patch.projects_add.is_empty()
        || !patch.projects_remove.is_empty()
        || !patch.attachments.is_empty()
        || !native.is_empty()
}

/// The canonical issue URL that identifies the Project item to write.
///
/// Built from the resolved repository and number instead of the fetched issue URL, so the write
/// target never comes from provider response data.
fn canonical_issue_url(repo: &str, number: u64) -> String {
    format!("https://github.com/{repo}/issues/{number}")
}

fn partial_issue_error(
    number: u64,
    issue_url: &str,
    target: &str,
    completed: &[String],
    pending: &[String],
) -> AppError {
    AppError::partial_success(
        serde_json::json!({
            "type": "issue",
            "number": number,
            "url": issue_url,
            "target": target,
        }),
        completed,
        pending,
    )
}

fn push_repeated(args: &mut Vec<String>, flag: &str, values: &[String]) {
    for value in values {
        args.push(flag.to_owned());
        args.push(value.clone());
    }
}

fn parse_issue_url(output: &[u8], repo: &str) -> Result<String, AppError> {
    let output = std::str::from_utf8(output).map_err(|_| AppError::provider_response())?;
    let url = output
        .lines()
        .find(|line| line.starts_with("https://github.com/") && line.contains("/issues/"))
        .ok_or_else(AppError::provider_response)?;
    parse_issue_number_from_url(url)?;
    let expected_prefix = format!("https://github.com/{repo}/issues/");
    if !url.starts_with(&expected_prefix) {
        return Err(AppError::provider_response());
    }
    Ok(url.trim_end_matches('/').to_owned())
}

fn parse_issue_number_from_url(url: &str) -> Result<u64, AppError> {
    let number = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(AppError::provider_response)?;
    number
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(AppError::provider_response)
}
