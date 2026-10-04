mod mapping;
mod read;

use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::IssueQuery;

/// GitLab issue operations.
///
/// Only the reads the GitLab grammar exposes exist here: it has no create or update verb yet, so
/// this type offers no write surface instead of one that would fail at runtime.
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

    pub(super) fn run_glab(&self, args: &[String]) -> Result<Vec<u8>, AppError> {
        super::run_glab(args)
    }

    pub(super) fn repo_argument(&self) -> String {
        super::repo_argument(&self.repo)
    }
}
