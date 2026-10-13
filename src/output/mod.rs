pub mod error;
pub mod json;
pub mod text;
use crate::cli::OutputFormat;

use serde::Serialize;

use crate::domain::{
    AppError, CheckRun, GitHubIssueView, Issue, IssueSummary, PullRequest, PullRequestStatus,
    PullRequestSummary,
};
#[derive(Serialize)]
pub struct IssueMutation {
    #[serde(flatten)]
    pub issue: Issue,
    #[serde(skip)]
    pub action: MutationAction,
    #[serde(skip)]
    pub unchanged: bool,
}

#[derive(Serialize)]
pub struct PullRequestMutation {
    #[serde(flatten)]
    pub pull_request: PullRequest,
    #[serde(skip)]
    pub action: MutationAction,
    #[serde(skip)]
    pub unchanged: bool,
    #[serde(skip)]
    pub gitlab: bool,
    #[serde(skip)]
    pub related_issue_references: Vec<crate::domain::ClosingReference>,
    #[serde(skip)]
    pub removed_related_issue_references: Vec<crate::domain::ClosingReference>,
}

#[derive(Clone, Copy)]
pub enum MutationAction {
    Created,
    Updated,
    Closed,
    Reopened,
}

pub struct GitLabData {
    pub operation: &'static str,
    pub number: u64,
    pub value: serde_json::Value,
}

impl Serialize for GitLabData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl GitLabData {
    pub fn new(operation: &'static str, number: u64, value: serde_json::Value) -> Self {
        Self {
            operation,
            number,
            value,
        }
    }
}

pub struct GitLabMergeRequest(pub PullRequest);

impl Serialize for GitLabMergeRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

pub struct GitLabMergeRequests(pub Vec<PullRequestSummary>);

impl Serialize for GitLabMergeRequests {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Debug, Serialize)]
pub struct WorkflowActionSummary {
    pub id: String,
    pub state: String,
    pub action: String,
    pub target_resource: String,
    pub target_id: String,
}
#[derive(Serialize)]
pub struct GitLabState {
    pub number: u64,
    pub state: String,
}

#[derive(Serialize)]
pub struct GitLabCheckout {
    pub number: u64,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuccessOutput {
    Issue(Issue),
    IssueMutation(IssueMutation),
    GitHubIssueView(GitHubIssueView),
    BlockerChains(Vec<String>),
    Issues(Vec<IssueSummary>),
    PullRequest(PullRequest),
    GitLabMergeRequest(GitLabMergeRequest),
    PullRequestMutation(PullRequestMutation),
    PullRequestStatus(PullRequestStatus),
    PullRequests(Vec<PullRequestSummary>),
    GitLabMergeRequests(GitLabMergeRequests),
    Checks(Vec<CheckRun>),
    IssueMutations(Vec<IssueMutation>),
    Diff {
        number: u64,
        diff: String,
    },
    Merge {
        number: u64,
        method: Option<String>,
        auto: bool,
        #[serde(skip)]
        confirmed_merged: bool,
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
    GitLabState(GitLabState),
    IssueState {
        number: u64,
        state: String,
    },
    LinkedBranch {
        number: u64,
        branch: String,
    },
    LinkedBranches {
        number: u64,
        branches: Vec<String>,
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
    GitLabCheckout(GitLabCheckout),
    GitLabAction {
        number: u64,
        action: String,
    },
    GitLabData(GitLabData),
    WorkflowStatus {
        status: String,
        actions: std::collections::BTreeMap<String, u64>,
    },
    WorkflowActions(Vec<WorkflowActionSummary>),
    WorkflowResolution {
        id: String,
        state: String,
    },
    UpdateStatus(crate::update::UpdateStatus),
}

pub fn write(format: OutputFormat, output: &SuccessOutput) -> Result<(), AppError> {
    match format {
        OutputFormat::Json => json::write_success(output),
        OutputFormat::Text => text::write(output),
    }
}
