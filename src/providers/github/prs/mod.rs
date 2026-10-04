mod mapping;
mod read;
mod write;

use crate::domain::{AppError, CheckRun, PullRequest, PullRequestSummary};
use crate::process::runner;
use crate::providers::{MergeMethod, NewPr, PrPatch, PrQuery, PullRequestProvider, ReviewEvent};

pub struct GitHubPulls {
    repo: String,
}

impl GitHubPulls {
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

    /// Runs `gh` and returns the raw result, including a non-zero exit status.
    pub(super) fn run_gh_raw(
        &self,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<runner::ProcessOutput, AppError> {
        super::run_gh_raw(args, input)
    }
}

impl PullRequestProvider for GitHubPulls {
    fn authenticate(&self) -> Result<(), AppError> {
        super::authenticate()
    }

    fn create(&self, pr: &NewPr) -> Result<PullRequest, AppError> {
        write::create(self, pr)
    }

    fn list(&self, query: &PrQuery) -> Result<Vec<PullRequestSummary>, AppError> {
        read::list(self, query)
    }

    fn show(&self, number: u64) -> Result<PullRequest, AppError> {
        read::show(self, number)
    }

    /// Fetches the pull request once, then applies the requested change.
    ///
    /// The fetch provides the current body for a body change, the timestamp for the concurrency
    /// guard, and the result returned when the patch carries no field.
    fn edit(&self, number: u64, patch: &PrPatch) -> Result<PullRequest, AppError> {
        write::edit(self, number, patch)
    }

    fn diff(&self, number: u64, name_only: bool) -> Result<String, AppError> {
        read::diff(self, number, name_only)
    }

    fn checks(&self, number: u64, required: bool) -> Result<Vec<CheckRun>, AppError> {
        read::checks(self, number, required)
    }

    fn review(&self, number: u64, event: ReviewEvent, body: Option<&str>) -> Result<(), AppError> {
        write::review(self, number, event, body)
    }

    fn merge(
        &self,
        number: u64,
        method: Option<MergeMethod>,
        delete_branch: bool,
        auto: bool,
    ) -> Result<(), AppError> {
        write::merge(self, number, method, delete_branch, auto)
    }

    fn set_ready(&self, number: u64, draft: bool) -> Result<(), AppError> {
        write::set_ready(self, number, draft)
    }

    fn close(
        &self,
        number: u64,
        comment: Option<&str>,
        delete_branch: bool,
    ) -> Result<(), AppError> {
        write::close(self, number, comment, delete_branch)
    }

    fn reopen(&self, number: u64, comment: Option<&str>) -> Result<(), AppError> {
        write::reopen(self, number, comment)
    }
}
