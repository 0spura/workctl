pub mod json;
pub mod text;
use crate::cli::OutputFormat;

use serde::Serialize;

use crate::domain::{
    AppError, CheckRun, GitHubIssueView, Issue, IssueSummary, PullRequest, PullRequestStatus,
    PullRequestSummary,
};

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuccessOutput {
    Issue(Issue),
    GitHubIssueView(GitHubIssueView),
    BlockerChains(Vec<String>),
    IssueEdits(Vec<Issue>),
    Issues(Vec<IssueSummary>),
    PullRequest(PullRequest),
    PullRequestStatus(PullRequestStatus),
    PullRequests(Vec<PullRequestSummary>),
    Checks(Vec<CheckRun>),
    Diff {
        number: u64,
        diff: String,
    },
    Merge {
        number: u64,
        method: Option<String>,
        auto: bool,
    },
    Review {
        number: u64,
        event: String,
    },
    Ready {
        number: u64,
        draft: bool,
    },
    State {
        number: u64,
        state: String,
    },
    IssueState {
        number: u64,
        state: String,
    },
    Comment {
        number: u64,
        target: String,
    },
    ConversationLock {
        number: u64,
        target: String,
        locked: bool,
    },
    Revert {
        number: u64,
        pull_request: u64,
    },
    UpdateBranch {
        number: u64,
        rebase: bool,
    },
    Checkout {
        number: u64,
    },
    GitLabAction {
        number: u64,
        action: String,
    },
    ProviderData(serde_json::Value),
}

pub fn write(format: OutputFormat, output: &SuccessOutput) -> Result<(), AppError> {
    match format {
        OutputFormat::Json => json::write_success(output),
        OutputFormat::Text => text::write(output),
    }
}
