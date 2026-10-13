use std::io::{self, Write};

use crate::domain::{AppError, Issue, PullRequestStatus};
use crate::output::SuccessOutput;

pub fn write(output: &SuccessOutput) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    write_to(&mut stdout, output)
}

fn write_to(stdout: &mut impl Write, output: &SuccessOutput) -> Result<(), AppError> {
    match output {
        SuccessOutput::BlockerChains(chains) => {
            for chain in chains {
                write_safe(stdout, chain, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }

        SuccessOutput::Issue(issue) => write_issue(stdout, issue)?,
        SuccessOutput::IssueMutation(mutation) => {
            write_issue_mutation(stdout, mutation)?;
        }
        SuccessOutput::IssueMutations(mutations) => {
            for mutation in mutations {
                write_issue_mutation(stdout, mutation)?;
            }
        }
        SuccessOutput::PullRequestMutation(mutation) => {
            write_pull_request_mutation(stdout, mutation)?;
        }
        SuccessOutput::GitHubIssueView(view) => {
            write_issue_header(stdout, &view.issue)?;
            if let Some(issue_type) = &view.issue_type {
                write!(stdout, "Type: ").map_err(|_| AppError::output())?;
                write_safe(stdout, issue_type, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
            if let Some(parent) = &view.parent {
                write_related_issue(stdout, "Parent", parent)?;
            }
            for issue in &view.sub_issues {
                write_related_issue(stdout, "Sub-issue", issue)?;
            }
            write_safe(stdout, &view.issue.body, true)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Issues(issues) => {
            for issue in issues {
                write!(stdout, "#{} ", issue.number).map_err(|_| AppError::output())?;
                write_safe(stdout, &issue.title, false)?;
                writeln!(stdout, " [{}]", state(issue.state)).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::PullRequest(pr) => write_pull_request(stdout, pr)?,
        SuccessOutput::PullRequestStatus(status) => write_pull_request_status(stdout, status)?,
        SuccessOutput::PullRequests(prs) => {
            for pr in prs {
                write!(stdout, "#{} ", pr.number).map_err(|_| AppError::output())?;
                write_safe(stdout, &pr.title, false)?;
                write!(stdout, " [{}]", pr_state(pr.state)).map_err(|_| AppError::output())?;
                if pr.draft {
                    write!(stdout, " [draft]").map_err(|_| AppError::output())?;
                }
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::GitLabMergeRequest(pr) => write_gitlab_pull_request(stdout, &pr.0)?,
        SuccessOutput::GitLabMergeRequests(prs) => {
            for pr in &prs.0 {
                write!(stdout, "!{} ", pr.number).map_err(|_| AppError::output())?;
                write_safe(stdout, &pr.title, false)?;
                write!(stdout, " [{}]", pr_state(pr.state)).map_err(|_| AppError::output())?;
                if pr.draft {
                    write!(stdout, " [draft]").map_err(|_| AppError::output())?;
                }
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::Checks(checks) => {
            for check in checks {
                write_safe(stdout, &check.name, false)?;
                write!(stdout, ": ").map_err(|_| AppError::output())?;
                write_safe(stdout, &check.state, false)?;
                if let Some(bucket) = &check.bucket {
                    write!(stdout, " ({bucket})").map_err(|_| AppError::output())?;
                }
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::Diff { diff, .. } => {
            write_safe(stdout, diff, true)?;
            if !diff.ends_with('\n') {
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::Merge { number, auto, .. } => {
            write!(stdout, "#{number} ").map_err(|_| AppError::output())?;
            writeln!(stdout, "{}", if *auto { "merge queued" } else { "merged" })
                .map_err(|_| AppError::output())?;
        }
        SuccessOutput::Review { number, event } => {
            write!(stdout, "#{number} reviewed: ").map_err(|_| AppError::output())?;
            write_safe(stdout, event, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Ready { number, draft } => {
            writeln!(
                stdout,
                "#{number} {}",
                if *draft {
                    "converted to draft"
                } else {
                    "ready for review"
                }
            )
            .map_err(|_| AppError::output())?;
        }
        SuccessOutput::State { number, state } | SuccessOutput::IssueState { number, state } => {
            write!(stdout, "#{number} ").map_err(|_| AppError::output())?;
            write_safe(stdout, state, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::GitLabState(state) => {
            write!(stdout, "!{} ", state.number).map_err(|_| AppError::output())?;
            write_safe(stdout, &state.state, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::LinkedBranch { number, branch } => {
            write!(stdout, "#{number} linked branch ").map_err(|_| AppError::output())?;
            write_safe(stdout, branch, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::LinkedBranches { number, branches } => {
            writeln!(stdout, "#{number} linked branches:").map_err(|_| AppError::output())?;
            for branch in branches {
                write_safe(stdout, branch, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::Comment { number, .. } => {
            writeln!(stdout, "#{number} commented").map_err(|_| AppError::output())?;
        }
        SuccessOutput::ConversationLock { number, locked, .. } => {
            writeln!(
                stdout,
                "#{number} conversation {}",
                if *locked { "locked" } else { "unlocked" }
            )
            .map_err(|_| AppError::output())?;
        }
        SuccessOutput::Revert {
            number,
            pull_request,
        } => {
            writeln!(stdout, "#{number} revert opened as #{pull_request}")
                .map_err(|_| AppError::output())?;
        }
        SuccessOutput::UpdateBranch { number, rebase } => {
            writeln!(
                stdout,
                "#{number} branch updated ({})",
                if *rebase { "rebase" } else { "merge" }
            )
            .map_err(|_| AppError::output())?;
        }
        SuccessOutput::Checkout { number } => {
            writeln!(stdout, "#{number} checked out").map_err(|_| AppError::output())?;
        }
        SuccessOutput::GitLabCheckout(checkout) => {
            writeln!(stdout, "!{} checked out", checkout.number).map_err(|_| AppError::output())?;
        }
        SuccessOutput::GitLabAction { number, action } => {
            write!(stdout, "!{number} ").map_err(|_| AppError::output())?;
            write_safe(stdout, action, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::GitLabData(data) => write_gitlab_data(stdout, data)?,
        SuccessOutput::WorkflowStatus { status, actions } => {
            write!(stdout, "workflow {status}").map_err(|_| AppError::output())?;
            for (state, count) in actions {
                write!(stdout, " {state}={count}").map_err(|_| AppError::output())?;
            }
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::WorkflowActions(actions) => {
            for action in actions {
                writeln!(
                    stdout,
                    "workflow action {} {} {}#{}",
                    action.id, action.state, action.target_resource, action.target_id
                )
                .map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::WorkflowResolution { id, state } => {
            writeln!(stdout, "workflow action {id} {state}").map_err(|_| AppError::output())?;
        }
        SuccessOutput::UpdateStatus(status) => {
            if let Some(install_command) = status.install_command {
                writeln!(
                    stdout,
                    "Update available: {} → {}. Run `{install_command}`.",
                    &status.current[..7],
                    &status.latest[..7]
                )
                .map_err(|_| AppError::output())?;
            } else {
                writeln!(stdout, "workctl is up to date ({})", &status.current[..7])
                    .map_err(|_| AppError::output())?;
            }
        }
    }
    Ok(())
}

fn write_pull_request(
    stdout: &mut impl Write,
    pr: &crate::domain::PullRequest,
) -> Result<(), AppError> {
    write!(stdout, "#{} ", pr.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.title, false)?;
    write!(stdout, " [{}]", pr_state(pr.state)).map_err(|_| AppError::output())?;
    if pr.draft {
        write!(stdout, " [draft]").map_err(|_| AppError::output())?;
    }
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Branch: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.head_ref, false)?;
    write!(stdout, " -> ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.base_ref, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    if let Some(mergeable) = &pr.mergeable {
        write!(stdout, "Mergeable: ").map_err(|_| AppError::output())?;
        write_safe(stdout, mergeable, false)?;
        writeln!(stdout).map_err(|_| AppError::output())?;
    }
    if let Some(decision) = &pr.review_decision {
        write!(stdout, "Review: ").map_err(|_| AppError::output())?;
        write_safe(stdout, decision, false)?;
        writeln!(stdout).map_err(|_| AppError::output())?;
    }
    write_names(stdout, "Labels", &pr.labels)?;
    write_names(stdout, "Assignees", &pr.assignees)?;
    write_safe(stdout, &pr.body, true)?;
    writeln!(stdout).map_err(|_| AppError::output())
}

fn write_gitlab_pull_request(
    stdout: &mut impl Write,
    pr: &crate::domain::PullRequest,
) -> Result<(), AppError> {
    write!(stdout, "!{} ", pr.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.title, false)?;
    write!(stdout, " [{}]", pr_state(pr.state)).map_err(|_| AppError::output())?;
    if pr.draft {
        write!(stdout, " [draft]").map_err(|_| AppError::output())?;
    }
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Branch: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.head_ref, false)?;
    write!(stdout, " -> ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.base_ref, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write_names(stdout, "Labels", &pr.labels)?;
    write_safe(stdout, &pr.body, true)?;
    writeln!(stdout).map_err(|_| AppError::output())
}

fn write_pull_request_status(
    stdout: &mut impl Write,
    status: &PullRequestStatus,
) -> Result<(), AppError> {
    write!(stdout, "#{} ", status.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &status.title, false)?;
    write!(stdout, " [{}]", pr_state(status.state)).map_err(|_| AppError::output())?;
    if status.draft {
        write!(stdout, " [draft]").map_err(|_| AppError::output())?;
    }
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Branch: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &status.head_ref, false)?;
    write!(stdout, " -> ").map_err(|_| AppError::output())?;
    write_safe(stdout, &status.base_ref, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Mergeable: ").map_err(|_| AppError::output())?;
    write_safe(
        stdout,
        status.mergeable.as_deref().unwrap_or("unknown"),
        false,
    )?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Review decision: ").map_err(|_| AppError::output())?;
    write_safe(
        stdout,
        status.review_decision.as_deref().unwrap_or("unknown"),
        false,
    )?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    if status.required_checks.is_empty() {
        writeln!(stdout, "Required checks: no checks reported").map_err(|_| AppError::output())?;
    } else {
        writeln!(stdout, "Required checks:").map_err(|_| AppError::output())?;
        for check in &status.required_checks {
            write_safe(stdout, &check.name, false)?;
            write!(stdout, ": ").map_err(|_| AppError::output())?;
            write_safe(stdout, &check.state, false)?;
            if let Some(bucket) = &check.bucket {
                write!(stdout, " (").map_err(|_| AppError::output())?;
                write_safe(stdout, bucket, false)?;
                write!(stdout, ")").map_err(|_| AppError::output())?;
            }
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
    }
    Ok(())
}

fn write_names(stdout: &mut impl Write, label: &str, names: &[String]) -> Result<(), AppError> {
    if names.is_empty() {
        return Ok(());
    }
    write!(stdout, "{label}: ").map_err(|_| AppError::output())?;
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            write!(stdout, ", ").map_err(|_| AppError::output())?;
        }
        write_safe(stdout, name, false)?;
    }
    writeln!(stdout).map_err(|_| AppError::output())
}

fn write_issue_header(stdout: &mut impl Write, issue: &Issue) -> Result<(), AppError> {
    write!(stdout, "#{} ", issue.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.title, false)?;
    writeln!(stdout, " [{}]", state(issue.state)).map_err(|_| AppError::output())
}

fn write_issue(stdout: &mut impl Write, issue: &Issue) -> Result<(), AppError> {
    write_issue_header(stdout, issue)?;
    write_safe(stdout, &issue.body, true)?;
    writeln!(stdout).map_err(|_| AppError::output())
}

fn write_related_issue(
    stdout: &mut impl Write,
    label: &str,
    issue: &crate::domain::RelatedIssue,
) -> Result<(), AppError> {
    let repository = issue
        .url
        .strip_prefix("https://github.com/")
        .and_then(|url| url.split_once("/issues/").map(|(repository, _)| repository));
    write!(stdout, "{label}: ").map_err(|_| AppError::output())?;
    if let Some(repository) = repository {
        write_safe(stdout, repository, false)?;
        write!(stdout, "#").map_err(|_| AppError::output())?;
    } else {
        write!(stdout, "#").map_err(|_| AppError::output())?;
    }
    write!(stdout, "{} ", issue.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.title, false)?;
    writeln!(stdout, " [{}]", state(issue.state)).map_err(|_| AppError::output())
}

pub(super) fn write_safe(
    stdout: &mut impl Write,
    value: &str,
    preserve_layout: bool,
) -> Result<(), AppError> {
    for character in value.chars() {
        if preserve_layout && matches!(character, '\n' | '\t') {
            stdout
                .write_all(character.encode_utf8(&mut [0; 4]).as_bytes())
                .map_err(|_| AppError::output())?;
        } else if character.is_control() {
            write!(stdout, "\\u{{{:x}}}", character as u32).map_err(|_| AppError::output())?;
        } else {
            stdout
                .write_all(character.encode_utf8(&mut [0; 4]).as_bytes())
                .map_err(|_| AppError::output())?;
        }
    }
    Ok(())
}

fn write_issue_mutation(
    stdout: &mut impl Write,
    mutation: &crate::output::IssueMutation,
) -> Result<(), AppError> {
    write!(stdout, "#{} ", mutation.issue.number).map_err(|_| AppError::output())?;
    writeln!(
        stdout,
        "{}",
        mutation_label(mutation.action, mutation.unchanged)
    )
    .map_err(|_| AppError::output())
}

fn write_pull_request_mutation(
    stdout: &mut impl Write,
    mutation: &crate::output::PullRequestMutation,
) -> Result<(), AppError> {
    if mutation.gitlab {
        write!(stdout, "!{} ", mutation.pull_request.number).map_err(|_| AppError::output())?;
    } else {
        write!(stdout, "#{} ", mutation.pull_request.number).map_err(|_| AppError::output())?;
    }
    writeln!(
        stdout,
        "{}",
        mutation_label(mutation.action, mutation.unchanged)
    )
    .map_err(|_| AppError::output())
}

fn mutation_label(action: crate::output::MutationAction, unchanged: bool) -> &'static str {
    if unchanged {
        "unchanged"
    } else {
        match action {
            crate::output::MutationAction::Created => "created",
            crate::output::MutationAction::Updated => "updated",
            crate::output::MutationAction::Closed => "closed",
            crate::output::MutationAction::Reopened => "reopened",
        }
    }
}

fn write_gitlab_data(
    stdout: &mut impl Write,
    data: &crate::output::GitLabData,
) -> Result<(), AppError> {
    let items = data
        .value
        .as_array()
        .ok_or_else(AppError::provider_response)?;
    if items.is_empty() {
        writeln!(stdout, "!{}: no {}", data.number, data.operation)
            .map_err(|_| AppError::output())?;
        return Ok(());
    }
    for item in items {
        match data.operation {
            "approvers" => {
                let user = item.get("user").unwrap_or(item);
                write!(stdout, "!{} approver #", data.number).map_err(|_| AppError::output())?;
                write_json_id(stdout, user.get("id").or_else(|| item.get("id")))?;
                write!(stdout, " ").map_err(|_| AppError::output())?;
                let name = user
                    .get("username")
                    .or_else(|| user.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(AppError::provider_response)?;
                write_safe(stdout, name, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
            "issues" => {
                let issue = item.get("issue").unwrap_or(item);
                write!(stdout, "!{} issue ", data.number).map_err(|_| AppError::output())?;
                if let Some(reference) = issue
                    .pointer("/references/full")
                    .and_then(serde_json::Value::as_str)
                {
                    write_safe(stdout, reference, false)?;
                } else {
                    write!(stdout, "#").map_err(|_| AppError::output())?;
                    write_json_id(stdout, issue.get("iid").or_else(|| issue.get("id")))?;
                }
                write!(stdout, " ").map_err(|_| AppError::output())?;
                let title = issue
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(AppError::provider_response)?;
                write_safe(stdout, title, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
            "notes" => {
                let discussion_id = item.get("id").ok_or_else(AppError::provider_response)?;
                let notes = item
                    .get("notes")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(AppError::provider_response)?;
                for note in notes {
                    write!(stdout, "!{} note #", data.number).map_err(|_| AppError::output())?;
                    write_json_id(stdout, Some(discussion_id))?;
                    write!(stdout, "/").map_err(|_| AppError::output())?;
                    write_json_id(stdout, note.get("id"))?;
                    write!(stdout, ": ").map_err(|_| AppError::output())?;
                    let body = note
                        .get("body")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(AppError::provider_response)?;
                    write_safe(stdout, body, true)?;
                    writeln!(stdout).map_err(|_| AppError::output())?;
                }
            }
            _ => return Err(AppError::provider_response()),
        }
    }
    Ok(())
}

fn write_json_id(
    stdout: &mut impl Write,
    value: Option<&serde_json::Value>,
) -> Result<(), AppError> {
    let value = value.ok_or_else(AppError::provider_response)?;
    if let Some(id) = value.as_str() {
        write_safe(stdout, id, false)
    } else if let Some(id) = value.as_u64() {
        write!(stdout, "{id}").map_err(|_| AppError::output())
    } else {
        Err(AppError::provider_response())
    }
}

fn state(state: crate::domain::IssueState) -> &'static str {
    match state {
        crate::domain::IssueState::Open => "open",
        crate::domain::IssueState::Closed => "closed",
    }
}

fn pr_state(state: crate::domain::PullRequestState) -> &'static str {
    match state {
        crate::domain::PullRequestState::Open => "open",
        crate::domain::PullRequestState::Closed => "closed",
        crate::domain::PullRequestState::Merged => "merged",
    }
}

#[cfg(test)]
mod tests {
    use super::{SuccessOutput, write_to};
    use crate::domain::{Issue, IssueState, IssueSummary};

    #[test]
    fn escapes_terminal_controls_and_preserves_body_layout() {
        let issue = Issue {
            number: 5,
            title: "Title\n\u{1b}]52;c=clipboard".to_owned(),
            body: "line 1\nline 2\t\u{7}".to_owned(),
            state: IssueState::Open,
            url: "https://github.com/owner/repo/issues/5".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-02T00:00:00Z".to_owned(),
        };
        let mut bytes = Vec::new();
        write_to(&mut bytes, &SuccessOutput::Issue(issue)).expect("render safe text");
        let rendered = String::from_utf8(bytes).expect("UTF-8 output");
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\u{7}'));
        assert!(rendered.contains(r"Title\u{a}\u{1b}]52;c=clipboard"));
        assert!(rendered.contains("line 1\nline 2\t\\u{7}"));

        let summary = IssueSummary {
            number: 6,
            title: "List\u{1b}[2J".to_owned(),
            state: IssueState::Closed,
            url: "https://github.com/owner/repo/issues/6".to_owned(),
            updated_at: "2026-01-02T00:00:00Z".to_owned(),
        };
        let mut bytes = Vec::new();
        write_to(&mut bytes, &SuccessOutput::Issues(vec![summary])).expect("render safe summary");
        let rendered = String::from_utf8(bytes).expect("UTF-8 output");
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.contains(r"List\u{1b}[2J"));
    }
}
