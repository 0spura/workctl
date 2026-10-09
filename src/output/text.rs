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
        SuccessOutput::IssueEdits(issues) => {
            for issue in issues {
                write_issue(stdout, issue)?;
            }
        }
        SuccessOutput::Issues(issues) => {
            for issue in issues {
                write!(stdout, "#{} ", issue.number).map_err(|_| AppError::output())?;
                write_safe(stdout, &issue.title, false)?;
                write!(stdout, " [{}] ", state(issue.state)).map_err(|_| AppError::output())?;
                write_safe(stdout, &issue.url, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::PullRequest(pr) => write_pull_request(stdout, pr)?,
        SuccessOutput::PullRequests(prs) => {
            for pr in prs {
                write!(stdout, "#{} ", pr.number).map_err(|_| AppError::output())?;
                write_safe(stdout, &pr.title, false)?;
                write!(stdout, " [{}]", pr_state(pr.state)).map_err(|_| AppError::output())?;
                if pr.draft {
                    write!(stdout, " [draft]").map_err(|_| AppError::output())?;
                }
                write!(stdout, " ").map_err(|_| AppError::output())?;
                write_safe(stdout, &pr.url, false)?;
                writeln!(stdout).map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::PullRequestStatus(status) => write_pull_request_status(stdout, status)?,
        SuccessOutput::Checks(checks) => {
            for check in checks {
                write_safe(stdout, &check.name, false)?;
                write!(stdout, ": ").map_err(|_| AppError::output())?;
                write_safe(stdout, &check.state, false)?;
                if let Some(bucket) = &check.bucket {
                    write!(stdout, " ({bucket})").map_err(|_| AppError::output())?;
                }
                if let Some(link) = &check.link {
                    write!(stdout, " ").map_err(|_| AppError::output())?;
                    write_safe(stdout, link, false)?;
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
        SuccessOutput::Merge {
            number,
            method,
            auto,
        } => {
            write!(stdout, "pr #{number} ").map_err(|_| AppError::output())?;
            if *auto {
                write!(stdout, "auto-merge queued").map_err(|_| AppError::output())?;
            } else {
                write!(stdout, "merged").map_err(|_| AppError::output())?;
            }
            if let Some(method) = method {
                write!(stdout, " ({method})").map_err(|_| AppError::output())?;
            }
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Review { number, event } => {
            write!(stdout, "pr #{number} reviewed: ").map_err(|_| AppError::output())?;
            write_safe(stdout, event, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Ready { number, draft } => {
            write!(stdout, "pr #{number} ").map_err(|_| AppError::output())?;
            if *draft {
                writeln!(stdout, "converted to draft").map_err(|_| AppError::output())?;
            } else {
                writeln!(stdout, "ready for review").map_err(|_| AppError::output())?;
            }
        }
        SuccessOutput::State { number, state } => {
            write!(stdout, "pr #{number} ").map_err(|_| AppError::output())?;
            write_safe(stdout, state, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::IssueState { number, state } => {
            write!(stdout, "issue #{number} ").map_err(|_| AppError::output())?;
            write_safe(stdout, state, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Comment { number, target } => {
            writeln!(stdout, "{target} #{number} commented").map_err(|_| AppError::output())?;
        }
        SuccessOutput::ConversationLock {
            number,
            target,
            locked,
        } => {
            writeln!(
                stdout,
                "{target} #{number} conversation {}",
                if *locked { "locked" } else { "unlocked" }
            )
            .map_err(|_| AppError::output())?;
        }
        SuccessOutput::Revert {
            number,
            pull_request,
        } => {
            writeln!(stdout, "pr #{number} revert opened as pr #{pull_request}")
                .map_err(|_| AppError::output())?;
        }
        SuccessOutput::UpdateBranch { number, rebase } => {
            write!(
                stdout,
                "pr #{number} branch updated ({})",
                if *rebase { "rebase" } else { "merge" }
            )
            .map_err(|_| AppError::output())?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::Checkout { number } => {
            writeln!(stdout, "pr #{number} checked out").map_err(|_| AppError::output())?;
        }
        SuccessOutput::GitLabAction { number, action } => {
            write!(stdout, "merge request !{number} ").map_err(|_| AppError::output())?;
            write_safe(stdout, action, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
        }
        SuccessOutput::ProviderData(value) => {
            let json = serde_json::to_string(value).map_err(|_| AppError::output())?;
            write_safe(stdout, &json, false)?;
            writeln!(stdout).map_err(|_| AppError::output())?;
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
    write_safe(stdout, &pr.url, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Branch: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.head_ref, false)?;
    write!(stdout, " -> ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.base_ref, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Author: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.author, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Created: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.created_at, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Updated: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &pr.updated_at, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    if let Some(merged_at) = &pr.merged_at {
        write!(stdout, "Merged: ").map_err(|_| AppError::output())?;
        write_safe(stdout, merged_at, false)?;
        writeln!(stdout).map_err(|_| AppError::output())?;
    }
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
    writeln!(stdout).map_err(|_| AppError::output())?;
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
    write_safe(stdout, &status.url, false)?;
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
            if let Some(link) = &check.link {
                write!(stdout, " ").map_err(|_| AppError::output())?;
                write_safe(stdout, link, false)?;
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
    writeln!(stdout, " [{}]", state(issue.state)).map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.url, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Created: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.created_at, false)?;
    writeln!(stdout).map_err(|_| AppError::output())?;
    write!(stdout, "Updated: ").map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.updated_at, false)?;
    writeln!(stdout).map_err(|_| AppError::output())
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
    write!(stdout, "{label}: #{} ", issue.number).map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.title, false)?;
    write!(stdout, " [{}] ", state(issue.state)).map_err(|_| AppError::output())?;
    write_safe(stdout, &issue.url, false)?;
    writeln!(stdout).map_err(|_| AppError::output())
}

fn write_safe(stdout: &mut impl Write, value: &str, preserve_layout: bool) -> Result<(), AppError> {
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
