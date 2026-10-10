pub mod github;
pub mod gitlab;

use crate::domain::{
    AppError, CheckRun, ClosingReference, Issue, IssueSummary, PullRequest, PullRequestStatus,
    PullRequestSummary,
};

use std::time::Duration;
/// Bounded query for issue summaries. Every set field maps to one provider filter.
#[derive(Debug)]
pub struct IssueQuery {
    pub state: String,
    pub limit: usize,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub mention: Option<String>,
    pub milestone: Option<String>,
    pub search: Option<String>,
    pub issue_type: Option<String>,
}

#[derive(Debug)]
pub struct NewIssue {
    pub title: String,
    pub body: String,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub milestone: Option<String>,
    pub projects: Vec<String>,
    pub attachments: Vec<String>,
}

/// A change requested without the caller having to reproduce the current body.
#[derive(Debug)]
pub enum BodyChange {
    Replace(String),
    Append(String),
    ReplaceSection {
        heading: String,
        body: String,
    },
    /// Unified diff applied to the current body.
    Patch(String),
}

#[derive(Debug)]
pub struct IssuePatch {
    pub title: Option<String>,
    pub body: Option<BodyChange>,
    pub assignees_add: Vec<String>,
    pub assignees_remove: Vec<String>,
    pub labels_add: Vec<String>,
    pub labels_remove: Vec<String>,
    pub milestone: Option<String>,
    pub clear_milestone: bool,
    pub projects_add: Vec<String>,
    pub projects_remove: Vec<String>,
    pub attachments: Vec<String>,
    pub expect_updated_at: Option<String>,
}

/// Bounded query for pull request summaries. Every set field maps to one provider filter.
#[derive(Debug)]
pub struct PrQuery {
    /// `open`, `closed`, `merged`, or `all`.
    pub state: String,
    pub limit: usize,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub search: Option<String>,
    pub draft: bool,
}

#[derive(Debug)]
pub struct NewPr {
    pub title: String,
    pub body: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub draft: bool,
    /// Issues the pull request closes when it merges.
    pub closes: Vec<ClosingReference>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub reviewers: Vec<String>,
    pub milestone: Option<String>,
    pub projects: Vec<String>,
    pub attachments: Vec<String>,
}

#[derive(Debug)]
pub struct PrPatch {
    pub title: Option<String>,
    pub body: Option<BodyChange>,
    /// Issues linked as closed on merge, written into the body.
    pub closes_add: Vec<ClosingReference>,
    /// Closing references removed from the body.
    pub closes_remove: Vec<ClosingReference>,
    pub base: Option<String>,
    pub labels_add: Vec<String>,
    pub labels_remove: Vec<String>,
    pub reviewers_add: Vec<String>,
    pub reviewers_remove: Vec<String>,
    pub assignees_add: Vec<String>,
    pub assignees_remove: Vec<String>,
    pub milestone: Option<String>,
    pub clear_milestone: bool,
    pub projects_add: Vec<String>,
    pub projects_remove: Vec<String>,
    pub attachments: Vec<String>,
    pub expect_updated_at: Option<String>,
}

#[derive(Debug, Default)]
pub struct PrTransition {
    pub closed: bool,
    pub comment: Option<String>,
    pub delete_branch: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ReviewSide {
    Left,
    Right,
}

#[derive(Debug)]
pub struct InlineReviewComment {
    pub path: String,
    pub line: u64,
    pub side: ReviewSide,
    pub body: String,
}

impl PrPatch {
    /// True when no field would be written; `expect_updated_at` only guards a write.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.body.is_none()
            && self.closes_add.is_empty()
            && self.closes_remove.is_empty()
            && self.base.is_none()
            && self.labels_add.is_empty()
            && self.labels_remove.is_empty()
            && self.reviewers_add.is_empty()
            && self.reviewers_remove.is_empty()
            && self.assignees_add.is_empty()
            && self.assignees_remove.is_empty()
            && self.milestone.is_none()
            && !self.clear_milestone
            && self.projects_add.is_empty()
            && self.projects_remove.is_empty()
            && self.attachments.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

impl ReviewEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request_changes",
            Self::Comment => "comment",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

pub trait WorkItemProvider {
    fn authenticate(&self) -> Result<(), AppError>;
    fn labels(&self) -> Result<Vec<crate::domain::RepositoryLabel>, AppError>;
    fn create(&self, issue: &NewIssue) -> Result<Issue, AppError>;
    fn list(&self, query: &IssueQuery) -> Result<Vec<IssueSummary>, AppError>;
    fn show(&self, number: u64) -> Result<Issue, AppError>;
    fn comment(&self, number: u64, body: &str) -> Result<(), AppError>;
    fn lock(&self, number: u64, reason: Option<&str>) -> Result<(), AppError>;
    fn unlock(&self, number: u64) -> Result<(), AppError>;
}

#[derive(Debug, Default)]
pub struct PrChecksOptions {
    pub required: bool,
    pub watch: bool,
    pub interval: Option<u32>,
    pub fail_fast: bool,
    pub watch_timeout: Option<Duration>,
}

/// Pull request operations, kept separate from `WorkItemProvider` so each seam stays narrow.
pub trait PullRequestProvider {
    fn authenticate(&self) -> Result<(), AppError>;
    fn create(&self, pr: &NewPr) -> Result<PullRequest, AppError>;
    fn list(&self, query: &PrQuery) -> Result<Vec<PullRequestSummary>, AppError>;
    fn show(&self, number: u64) -> Result<PullRequest, AppError>;
    /// Reads concise review/mergeability metadata and all required check evidence.
    fn status(&self, number: u64) -> Result<PullRequestStatus, AppError>;
    /// Fetches the pull request once, then applies the requested change.
    fn edit(
        &self,
        number: u64,
        patch: &PrPatch,
        transition: Option<&PrTransition>,
    ) -> Result<PullRequest, AppError>;
    fn diff(&self, number: u64, name_only: bool) -> Result<String, AppError>;
    fn checks(&self, number: u64, options: &PrChecksOptions) -> Result<Vec<CheckRun>, AppError>;
    /// Submits one review; inline comments are submitted together in a single anchored request.
    fn review(
        &self,
        number: u64,
        event: ReviewEvent,
        body: Option<&str>,
        comments: &[InlineReviewComment],
    ) -> Result<(), AppError>;
    fn merge(
        &self,
        number: u64,
        method: Option<MergeMethod>,
        delete_branch: bool,
        auto: bool,
    ) -> Result<(), AppError>;
    /// Merge the latest base branch into the pull request branch, or rebase when requested.
    fn update_branch(&self, number: u64, rebase: bool) -> Result<(), AppError>;
    /// Checks out the pull request branch in the current local worktree.
    fn checkout(&self, number: u64) -> Result<(), AppError>;
    /// Marks the pull request ready for review, or back to draft when `draft` is set.
    fn set_ready(&self, number: u64, draft: bool) -> Result<(), AppError>;
    fn comment(&self, number: u64, body: &str) -> Result<(), AppError>;
    fn lock(&self, number: u64, reason: Option<&str>) -> Result<(), AppError>;
    fn unlock(&self, number: u64) -> Result<(), AppError>;
    fn revert(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        draft: bool,
    ) -> Result<u64, AppError>;
}

/// Applies a requested body change to the body currently stored by the provider.
pub fn resolve_body_change(current: &str, change: &BodyChange) -> Result<String, AppError> {
    match change {
        BodyChange::Replace(text) => Ok(text.clone()),
        BodyChange::Append(text) => Ok(crate::domain::body::append(current, text)),
        BodyChange::ReplaceSection { heading, body } => {
            crate::domain::body::replace_section(current, heading, body)
        }
        BodyChange::Patch(patch) => crate::domain::patch::apply(current, patch),
    }
}
