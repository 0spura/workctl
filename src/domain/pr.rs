use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Serialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
    pub merged_at: Option<String>,
    /// `mergeable`, `conflicting`, or `unknown`.
    pub mergeable: Option<String>,
    /// `approved`, `changes_requested`, or `review_required`.
    pub review_decision: Option<String>,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PullRequestStatus {
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    /// `mergeable`, `conflicting`, or `unknown`.
    pub mergeable: Option<String>,
    /// `approved`, `changes_requested`, or `review_required`.
    pub review_decision: Option<String>,
    pub required_checks: Vec<CheckRun>,
}

#[derive(Debug, Serialize)]
pub struct CheckRun {
    pub name: String,
    /// Check state, lowercased (for example `success`, `pending`, `failure`).
    pub state: String,
    /// `pass`, `fail`, `pending`, `skipping`, or `cancel`.
    pub bucket: Option<String>,
    pub description: Option<String>,
    pub link: Option<String>,
    pub workflow: Option<String>,
}
