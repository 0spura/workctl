mod mapping;
mod read;
mod write;

use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::IssueQuery;

/// Fields accepted by the native `glab issue create` command.
#[derive(Debug)]
pub struct GitLabIssueCreate {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub confidential: bool,
    pub weight: Option<u64>,
    pub due_date: Option<String>,
}

/// Fields accepted by the native `glab issue update` command.
#[derive(Debug)]
pub struct GitLabIssueUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub labels_add: Vec<String>,
    pub labels_remove: Vec<String>,
    pub assignees: Vec<String>,
    pub unassign: bool,
    /// `Some("")` and `Some("0")` both clear the current milestone.
    pub milestone: Option<String>,
    pub confidential: Option<bool>,
    pub weight: Option<u64>,
    pub due_date: Option<String>,
}

/// GitLab issue operations. Native write fields stay provider-owned; successful writes are mapped
/// back to the shared `Issue` result through `glab issue view --output json`.
pub struct GitLabIssues {
    repo: String,
}

impl GitLabIssues {
    pub fn new(repo: String) -> Self {
        Self { repo }
    }

    pub fn authenticate(&self) -> Result<(), AppError> {
        super::authenticate()
    }

    pub fn list(&self, query: &IssueQuery) -> Result<Vec<IssueSummary>, AppError> {
        read::list(self, query)
    }

    pub fn show(&self, number: u64) -> Result<Issue, AppError> {
        read::show(self, number)
    }

    pub fn create(&self, issue: &GitLabIssueCreate) -> Result<Issue, AppError> {
        write::create(self, issue)
    }

    pub fn update(&self, number: u64, patch: &GitLabIssueUpdate) -> Result<Issue, AppError> {
        write::update(self, number, patch)
    }

    pub(super) fn run_glab(&self, args: &[String]) -> Result<Vec<u8>, AppError> {
        super::run_glab(args)
    }

    pub(super) fn run_glab_mutation(
        &self,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, AppError> {
        super::run_glab_mutation(args, input)
    }

    pub(super) fn repo_argument(&self) -> String {
        super::repo_argument(&self.repo)
    }
}
