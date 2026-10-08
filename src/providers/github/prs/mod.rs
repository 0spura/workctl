use std::time::Duration;

mod mapping;
mod read;
mod write;

use crate::domain::{AppError, CheckRun, PullRequest, PullRequestStatus, PullRequestSummary};
use crate::process::runner;
use crate::providers::{
    MergeMethod, NewPr, PrChecksOptions, PrPatch, PrQuery, PullRequestProvider, ReviewEvent,
};

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

    pub(super) fn run_gh_raw_with_deadline(
        &self,
        args: &[String],
        deadline: Duration,
    ) -> Result<runner::ProcessOutput, AppError> {
        super::run_gh_raw_with_deadline(args, deadline)
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
    fn status(&self, number: u64) -> Result<PullRequestStatus, AppError> {
        let mut status = read::status(self, number)?;
        status.required_checks = read::checks(
            self,
            number,
            &PrChecksOptions {
                required: true,
                ..PrChecksOptions::default()
            },
        )?;
        Ok(status)
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

    fn checks(&self, number: u64, options: &PrChecksOptions) -> Result<Vec<CheckRun>, AppError> {
        read::checks(self, number, options)
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

    fn update_branch(&self, number: u64, rebase: bool) -> Result<(), AppError> {
        write::update_branch(self, number, rebase)
    }
    fn checkout(&self, number: u64) -> Result<(), AppError> {
        write::checkout(self, number)
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
    fn comment(&self, number: u64, body: &str) -> Result<(), AppError> {
        write::comment(self, number, body)
    }

    fn lock(&self, number: u64, reason: Option<&str>) -> Result<(), AppError> {
        write::lock(self, number, reason)
    }

    fn unlock(&self, number: u64) -> Result<(), AppError> {
        write::unlock(self, number)
    }

    fn revert(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        draft: bool,
    ) -> Result<u64, AppError> {
        write::revert(self, number, title, body, draft)
    }
}
