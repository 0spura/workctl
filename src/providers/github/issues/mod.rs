mod mapping;
mod read;
mod write;

use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::{IssuePatch, IssueQuery, NewIssue, WorkItemProvider, resolve_body_change};

pub struct GitHubIssues {
    repo: String,
}

impl GitHubIssues {
    pub fn new(repo: String) -> Self {
        Self { repo }
    }

    pub(super) fn run_gh(
        &self,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, AppError> {
        super::run_gh(args, input)
    }
}

impl WorkItemProvider for GitHubIssues {
    fn authenticate(&self) -> Result<(), AppError> {
        super::authenticate()
    }
    fn labels(&self) -> Result<Vec<crate::domain::RepositoryLabel>, AppError> {
        read::labels(self)
    }

    fn create(&self, issue: &NewIssue) -> Result<Issue, AppError> {
        write::create(self, issue)
    }

    fn list(&self, query: &IssueQuery) -> Result<Vec<IssueSummary>, AppError> {
        read::list(self, query)
    }

    fn show(&self, number: u64) -> Result<Issue, AppError> {
        read::show(self, number)
    }

    /// Fetches the issue once, then applies the requested change.
    ///
    /// The fetch rejects a pull request before any mutation and provides the current body for a
    /// body change and the timestamp for the concurrency guard.
    fn edit(&self, number: u64, patch: &IssuePatch) -> Result<Issue, AppError> {
        let current = read::show(self, number)?;
        if let Some(expected) = patch.expect_updated_at.as_deref() {
            if current.updated_at != expected {
                return Err(AppError::conflict());
            }
        }
        let resolved = match &patch.body {
            None => None,
            Some(change) => Some(resolve_body_change(&current.body, change)?),
        };
        write::edit(self, number, patch, resolved.as_deref())
    }
}
