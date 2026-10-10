use serde::{Deserialize, Serialize};

use crate::domain::{AppError, PullRequest, PullRequestState, body};
use crate::providers::github::prs::{GitHubPulls, read};
use crate::providers::{
    InlineReviewComment, MergeMethod, NewPr, PrPatch, PrTransition, ReviewEvent, ReviewSide,
    resolve_body_change,
};

pub(super) fn create(provider: &GitHubPulls, pr: &NewPr) -> Result<PullRequest, AppError> {
    if !pr.attachments.is_empty() {
        super::super::require_attachment_support()?;
    }
    let milestone = super::super::resolve_milestone(&provider.repo, pr.milestone.as_deref())?;
    let body = if pr.closes.is_empty() {
        pr.body.clone()
    } else {
        let block = pr
            .closes
            .iter()
            .map(|reference| format!("Closes {reference}"))
            .collect::<Vec<_>>()
            .join("\n");
        body::append(&pr.body, &block)
    };

    let mut args = vec![
        "pr".to_owned(),
        "create".to_owned(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--title".to_owned(),
        pr.title.clone(),
        "--body-file".to_owned(),
        "-".to_owned(),
    ];
    if let Some(base) = &pr.base {
        args.extend(["--base".to_owned(), base.clone()]);
    }
    if let Some(head) = &pr.head {
        args.extend(["--head".to_owned(), head.clone()]);
    }
    if pr.draft {
        args.push("--draft".to_owned());
    }
    push_repeated(&mut args, "--assignee", &pr.assignees);
    push_repeated(&mut args, "--label", &pr.labels);
    push_repeated(&mut args, "--reviewer", &pr.reviewers);
    if let Some(milestone) = milestone {
        args.extend(["--milestone".to_owned(), milestone]);
    }
    push_repeated(&mut args, "--project", &pr.projects);
    push_repeated(&mut args, "--attach", &pr.attachments);
    let output = super::super::run_gh_raw(&args, Some(body.into_bytes())).map_err(|_| {
        if pr.attachments.is_empty() {
            AppError::github_cli()
        } else {
            AppError::attachment_create_uncertain()
        }
    })?;
    if !output.success {
        return Err(if pr.attachments.is_empty() {
            AppError::github_cli()
        } else {
            AppError::attachment_create_uncertain()
        });
    }
    read::show(provider, parse_pr_number(&output.stdout)?)
}

/// Applies ordinary pull-request fields, an optional state transition and its comment in order.
///
/// The native edit runs only when the patch has a field, so a state-only edit never calls
/// `gh pr edit`. Each acknowledged write is recorded; a later failure stops without rollback or
/// retry and returns a partial-success error. The final record is read once after every requested
/// write. A merged pull request rejects an explicit state request before any write.
pub(super) fn edit(
    provider: &GitHubPulls,
    number: u64,
    patch: &PrPatch,
    transition: Option<&PrTransition>,
) -> Result<PullRequest, AppError> {
    if !patch.attachments.is_empty() {
        super::super::require_attachment_support()?;
    }
    let current = read::show(provider, number)?;
    if let Some(expected) = patch.expect_updated_at.as_deref() {
        if current.updated_at != expected {
            return Err(AppError::conflict());
        }
    }
    if transition.is_some() && matches!(current.state, PullRequestState::Merged) {
        return Err(AppError::invalid_input(
            "a merged pull request cannot change state",
        ));
    }

    let mut body = match &patch.body {
        Some(change) => Some(resolve_body_change(&current.body, change)?),
        None => None,
    };
    // Closing references are body text, so they apply to the text this request already produced.
    let closing_base = body.as_deref().unwrap_or(&current.body);
    if let Some(linked) =
        body::apply_closing_references(closing_base, &patch.closes_add, &patch.closes_remove)
    {
        body = Some(linked);
    }
    let native_edit = has_field(patch, body.is_some());
    if !native_edit && transition.is_none() {
        return Ok(current);
    }

    let mut completed: Vec<String> = Vec::new();
    if native_edit {
        let milestone =
            super::super::resolve_milestone(&provider.repo, patch.milestone.as_deref())?;
        let mut args = vec![
            "pr".to_owned(),
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
        if let Some(base) = &patch.base {
            args.extend(["--base".to_owned(), base.clone()]);
        }
        push_repeated(&mut args, "--add-label", &patch.labels_add);
        push_repeated(&mut args, "--remove-label", &patch.labels_remove);
        push_repeated(&mut args, "--add-reviewer", &patch.reviewers_add);
        push_repeated(&mut args, "--remove-reviewer", &patch.reviewers_remove);
        push_repeated(&mut args, "--add-assignee", &patch.assignees_add);
        push_repeated(&mut args, "--remove-assignee", &patch.assignees_remove);
        if let Some(milestone) = milestone {
            args.extend(["--milestone".to_owned(), milestone]);
        } else if patch.clear_milestone {
            args.push("--remove-milestone".to_owned());
        }
        push_repeated(&mut args, "--add-project", &patch.projects_add);
        push_repeated(&mut args, "--remove-project", &patch.projects_remove);
        push_repeated(&mut args, "--attach", &patch.attachments);
        provider.run_gh(&args, body.map(String::into_bytes))?;
        completed.push("pull request edit".to_owned());
    }

    if let Some(transition) = transition {
        let state_name = if transition.closed {
            "pull request close"
        } else {
            "pull request reopen"
        };
        let state_operation = if transition.closed {
            close(provider, number)
        } else {
            reopen(provider, number)
        };
        if state_operation.is_err() {
            let failure = AppError::github_write_uncertain();
            if completed.is_empty() {
                return Err(failure);
            }
            let mut pending = vec![state_name];
            if transition.comment.is_some() {
                pending.push("transition comment");
            }
            pending.push("pull request readback");
            return Err(partial_pr_transition_error(
                &provider.repo,
                number,
                &completed,
                &pending,
                &failure,
            ));
        }
        completed.push(state_name.to_owned());
        if transition.delete_branch {
            if super::branches::delete_remote_branch(provider, number).is_err() {
                let mut pending = vec!["remote branch deletion"];
                if transition.comment.is_some() {
                    pending.push("transition comment");
                }
                pending.push("pull request readback");
                return Err(partial_pr_transition_error(
                    &provider.repo,
                    number,
                    &completed,
                    &pending,
                    &AppError::github_write_uncertain(),
                ));
            }
            completed.push("remote branch deletion".to_owned());
        }
        if let Some(transition_comment) = transition.comment.as_deref() {
            if comment(provider, number, transition_comment).is_err() {
                let failure = AppError::github_write_uncertain();
                return Err(partial_pr_transition_error(
                    &provider.repo,
                    number,
                    &completed,
                    &["transition comment", "pull request readback"],
                    &failure,
                ));
            }
            completed.push("transition comment".to_owned());
        }
    }

    read::show(provider, number).map_err(|_| {
        if completed.is_empty() {
            AppError::provider_response()
        } else {
            partial_pr_transition_error(
                &provider.repo,
                number,
                &completed,
                &["pull request readback"],
                &AppError::provider_response(),
            )
        }
    })
}

/// Builds the canonical pull-request URL and reports an ordered partial result.
///
/// The URL comes from the resolved repository and number instead of the fetched URL, so the
/// reported target never comes from provider response data.
fn partial_pr_transition_error(
    repo: &str,
    number: u64,
    completed: &[String],
    pending: &[&str],
    failure: &AppError,
) -> AppError {
    let pr_url = canonical_pr_url(repo, number);
    let mut error = AppError::partial_success(
        serde_json::json!({
            "type": "pull_request",
            "number": number,
            "url": pr_url,
        }),
        completed,
        &pending
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    );
    error.details = Some(serde_json::json!({
        "resource": {
            "type": "pull_request",
            "number": number,
            "url": pr_url,
        },
        "completed": completed,
        "pending": pending,
        "failure": {
            "code": failure.code,
            "details": failure.details,
        }
    }));
    error
}

/// The canonical pull-request URL that identifies the transitioned resource.
fn canonical_pr_url(repo: &str, number: u64) -> String {
    format!("https://github.com/{repo}/pull/{number}")
}

pub(super) fn review(
    provider: &GitHubPulls,
    number: u64,
    event: ReviewEvent,
    body: Option<&str>,
    comments: &[InlineReviewComment],
) -> Result<(), AppError> {
    if comments.is_empty() {
        return review_summary(provider, number, event, body);
    }
    review_inline(provider, number, event, body, comments)
}

/// Submits one top-level review event through `gh pr review`, carrying the summary on stdin.
fn review_summary(
    provider: &GitHubPulls,
    number: u64,
    event: ReviewEvent,
    body: Option<&str>,
) -> Result<(), AppError> {
    let flag = match event {
        ReviewEvent::Approve => "--approve",
        ReviewEvent::RequestChanges => "--request-changes",
        ReviewEvent::Comment => "--comment",
    };
    let mut args = vec![
        "pr".to_owned(),
        "review".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
        flag.to_owned(),
    ];
    if body.is_some() {
        args.push("--body-file".to_owned());
        args.push("-".to_owned());
    }
    provider.run_gh(&args, body.map(|body| body.as_bytes().to_vec()))?;
    Ok(())
}

/// The review request the GitHub API receives, with the current head as its anchor.
#[derive(Serialize)]
struct ReviewRequest<'a> {
    event: &'static str,
    commit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    comments: Vec<InlineCommentRequest<'a>>,
}

#[derive(Serialize)]
struct InlineCommentRequest<'a> {
    path: &'a str,
    line: u64,
    side: &'static str,
    body: &'a str,
}

/// An acknowledged review, as far as this client relies on it.
#[derive(Deserialize)]
struct SubmittedReview {
    id: u64,
    state: String,
}

/// Submits one review with inline comments in a single request, anchored to the current head.
///
/// The JSON payload travels on stdin. A failed or unconfirmable write reports an uncertain
/// mutation: the review may exist even though its result could not be read, so it is never
/// retried.
fn review_inline(
    provider: &GitHubPulls,
    number: u64,
    event: ReviewEvent,
    body: Option<&str>,
    comments: &[InlineReviewComment],
) -> Result<(), AppError> {
    let commit_id = read::head_sha(provider, number)?;
    let request = ReviewRequest {
        event: review_api_event(event),
        commit_id,
        body,
        comments: comments
            .iter()
            .map(|comment| InlineCommentRequest {
                path: &comment.path,
                line: comment.line,
                side: review_api_side(comment.side),
                body: &comment.body,
            })
            .collect(),
    };
    let payload = serde_json::to_vec(&request).map_err(|_| AppError::provider_response())?;
    let args = [
        "api".to_owned(),
        "--method".to_owned(),
        "POST".to_owned(),
        format!("repos/{}/pulls/{number}/reviews", provider.repo),
        "--input".to_owned(),
        "-".to_owned(),
    ];
    let output = provider
        .run_gh_raw(&args, Some(payload))
        .map_err(|_| AppError::github_write_uncertain())?;
    if !output.success {
        return Err(AppError::github_write_uncertain());
    }
    parse_submitted_review(&output.stdout, event)
}

/// The review event name the GitHub API expects.
fn review_api_event(event: ReviewEvent) -> &'static str {
    match event {
        ReviewEvent::Approve => "APPROVE",
        ReviewEvent::RequestChanges => "REQUEST_CHANGES",
        ReviewEvent::Comment => "COMMENT",
    }
}

/// The diff side the GitHub API expects for an inline comment.
fn review_api_side(side: ReviewSide) -> &'static str {
    match side {
        ReviewSide::Left => "LEFT",
        ReviewSide::Right => "RIGHT",
    }
}

fn parse_submitted_review(bytes: &[u8], event: ReviewEvent) -> Result<(), AppError> {
    let review: SubmittedReview =
        serde_json::from_slice(bytes).map_err(|_| AppError::github_write_uncertain())?;
    let expected_state = match event {
        ReviewEvent::Approve => "APPROVED",
        ReviewEvent::RequestChanges => "CHANGES_REQUESTED",
        ReviewEvent::Comment => "COMMENTED",
    };
    if review.id == 0 || !review.state.eq_ignore_ascii_case(expected_state) {
        return Err(AppError::github_write_uncertain());
    }
    Ok(())
}

pub(super) fn merge(
    provider: &GitHubPulls,
    number: u64,
    method: Option<MergeMethod>,
    delete_branch: bool,
    auto: bool,
) -> Result<(), AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "merge".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if let Some(method) = method {
        args.push(format!("--{}", method.as_str()));
    }
    if auto {
        args.push("--auto".to_owned());
    }
    provider.run_gh(&args, None)?;
    if delete_branch {
        // The merge is complete, so the branch is no longer needed. Only the remote ref is
        // removed: a local branch may still be checked out or carry unmerged work.
        super::branches::delete_remote_branch(provider, number).map_err(|_| {
            AppError::partial_success(
                serde_json::json!({"type": "pull_request", "number": number}),
                &["pull request merge".to_owned()],
                &["remote branch deletion".to_owned()],
            )
        })?;
    }
    Ok(())
}

/// Checks out the PR branch without enabling gh's destructive --force option.
pub(super) fn checkout(provider: &GitHubPulls, number: u64) -> Result<(), AppError> {
    let args = [
        "pr".to_owned(),
        "checkout".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    provider.run_gh(&args, None)?;
    Ok(())
}

pub(super) fn update_branch(
    provider: &GitHubPulls,
    number: u64,
    rebase: bool,
) -> Result<(), AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "update-branch".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if rebase {
        args.push("--rebase".to_owned());
    }
    provider.run_gh(&args, None)?;
    Ok(())
}
pub(super) fn set_ready(provider: &GitHubPulls, number: u64, draft: bool) -> Result<(), AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "ready".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if draft {
        args.push("--undo".to_owned());
    }
    provider.run_gh(&args, None)?;
    Ok(())
}

/// Closes the pull request through the native command.
///
/// The transition comment never travels here: it is posted last through the stdin comment path.
fn close(provider: &GitHubPulls, number: u64) -> Result<(), AppError> {
    let args = vec![
        "pr".to_owned(),
        "close".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    provider.run_gh(&args, None)?;
    Ok(())
}

fn reopen(provider: &GitHubPulls, number: u64) -> Result<(), AppError> {
    let args = vec![
        "pr".to_owned(),
        "reopen".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    provider.run_gh(&args, None)?;
    Ok(())
}

pub(super) fn comment(provider: &GitHubPulls, number: u64, body: &str) -> Result<(), AppError> {
    let args = vec![
        "pr".to_owned(),
        "comment".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--body-file".to_owned(),
        "-".to_owned(),
    ];
    provider.run_gh(&args, Some(body.as_bytes().to_vec()))?;
    Ok(())
}

pub(super) fn lock(
    provider: &GitHubPulls,
    number: u64,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "lock".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if let Some(reason) = reason {
        args.extend(["--reason".to_owned(), reason.to_owned()]);
    }
    provider.run_gh(&args, None)?;
    Ok(())
}

pub(super) fn unlock(provider: &GitHubPulls, number: u64) -> Result<(), AppError> {
    let args = vec![
        "pr".to_owned(),
        "unlock".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    provider.run_gh(&args, None)?;
    Ok(())
}

/// Creates a revert pull request for a merged pull request.
///
/// The body travels on stdin, never in argv. The number printed by `gh pr revert` identifies the
/// new pull request; an unreadable result is reported as an invalid provider response.
pub(super) fn revert(
    provider: &GitHubPulls,
    number: u64,
    title: Option<&str>,
    body: Option<&str>,
    draft: bool,
) -> Result<u64, AppError> {
    let mut args = vec![
        "pr".to_owned(),
        "revert".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if let Some(title) = title {
        args.extend(["--title".to_owned(), title.to_owned()]);
    }
    if draft {
        args.push("--draft".to_owned());
    }
    let input = body.map(|body| {
        args.extend(["--body-file".to_owned(), "-".to_owned()]);
        body.as_bytes().to_vec()
    });
    parse_pr_number(&provider.run_gh(&args, input)?)
}

fn push_repeated(args: &mut Vec<String>, flag: &str, values: &[String]) {
    for value in values {
        args.push(flag.to_owned());
        args.push(value.clone());
    }
}

/// Whether the patch requests any mutation; `expect_updated_at` only guards a write.
fn has_field(patch: &PrPatch, has_body: bool) -> bool {
    has_body
        || patch.title.is_some()
        || patch.base.is_some()
        || patch.milestone.is_some()
        || patch.clear_milestone
        || !patch.projects_add.is_empty()
        || !patch.projects_remove.is_empty()
        || !patch.attachments.is_empty()
        || !patch.labels_add.is_empty()
        || !patch.labels_remove.is_empty()
        || !patch.reviewers_add.is_empty()
        || !patch.reviewers_remove.is_empty()
        || !patch.assignees_add.is_empty()
        || !patch.assignees_remove.is_empty()
}

/// Extracts the pull request number from the URL printed by `gh pr create`.
fn parse_pr_number(output: &[u8]) -> Result<u64, AppError> {
    let text = std::str::from_utf8(output).map_err(|_| AppError::provider_response())?;
    text.split_whitespace()
        .filter_map(|token| token.rsplit_once("/pull/"))
        .filter_map(|(_, number)| number.trim_end_matches('/').parse::<u64>().ok())
        .next()
        .ok_or(AppError::provider_response())
}

#[cfg(test)]
mod tests {
    use super::parse_pr_number;

    #[test]
    fn reads_the_pull_request_number_from_created_output() {
        let output = b"https://github.com/owner/repo/pull/21\n";
        assert_eq!(parse_pr_number(output).expect("number"), 21);
        let noisy = b"warning: something\nhttps://github.com/owner/repo/pull/7\n";
        assert_eq!(parse_pr_number(noisy).expect("number"), 7);
    }

    #[test]
    fn rejects_output_without_a_pull_request_url() {
        assert_eq!(
            parse_pr_number(b"nothing here").unwrap_err().code,
            "provider_response"
        );
    }
}
