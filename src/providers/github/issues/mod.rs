mod mapping;
mod read;
mod write;

use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::{IssuePatch, IssueQuery, NewIssue, WorkItemProvider, resolve_body_change};
#[derive(Debug)]
pub struct GitHubIssues {
    repo: String,
}

/// Native GitHub issue fields that `gh issue edit` sets outside the generic patch.
///
/// Every request is forwarded verbatim as its `gh` flag in the same `gh issue edit` as the
/// generic patch, so one call changes the generic fields and the native ones together. Setting
/// and clearing the same single-valued field in one request is rejected by the command layer
/// before it reaches the provider; the provider sends whatever it is given.
#[derive(Debug, Default)]
pub struct NativeIssueEdit {
    pub issue_type: Option<String>,
    pub remove_type: bool,
    pub parent: Option<String>,
    pub remove_parent: bool,
    pub sub_issues_add: Vec<String>,
    pub sub_issues_remove: Vec<String>,
    pub blocked_by_add: Vec<String>,
    pub blocked_by_remove: Vec<String>,
    pub blocking_add: Vec<String>,
    pub blocking_remove: Vec<String>,
}

impl NativeIssueEdit {
    /// True when no native field would make `gh issue edit` change something.
    pub fn is_empty(&self) -> bool {
        self.issue_type.is_none()
            && !self.remove_type
            && self.parent.is_none()
            && !self.remove_parent
            && self.sub_issues_add.is_empty()
            && self.sub_issues_remove.is_empty()
            && self.blocked_by_add.is_empty()
            && self.blocked_by_remove.is_empty()
            && self.blocking_add.is_empty()
            && self.blocking_remove.is_empty()
    }
}

/// A validated GitHub Project plan for one issue creation.
///
/// Built by [`GitHubIssues::plan_project`] before the remote write. The plan exposes the
/// allowlisted fields the caller may resolve with a decision model and then executes the
/// resulting membership and field writes.
pub struct ProjectCreatePlan {
    pub(super) plan: super::projects::ProjectPlan,
}

impl ProjectCreatePlan {
    /// One candidate per option of every auto-selected field, in configured field order.
    pub fn candidates(&self) -> Vec<crate::domain::DecisionCandidate> {
        self.plan.option_candidates()
    }

    /// Records the model's chosen option for every auto-selected field.
    pub fn choose_from_scores(
        &mut self,
        scores: &[crate::domain::DecisionScore],
        threshold: f64,
    ) -> Result<(), AppError> {
        self.plan.choose_from_scores(scores, threshold)
    }
}

/// A validated GitHub Project plan for one issue edit.
///
/// Built by [`GitHubIssues::plan_project_edit`] before the remote write. It carries only the
/// fields the caller explicitly sets or clears, so an edit can never reapply configured defaults
/// or automatically selected fields.
pub struct ProjectEditPlan {
    pub(super) plan: super::projects::ProjectPlan,
}

impl GitHubIssues {
    pub fn new(repo: String) -> Self {
        Self { repo }
    }

    /// Discovers the configured Project and validates every requested field value.
    ///
    /// This performs no remote write: it queries the Project schema, rejects unknown or invalid
    /// fields, and returns the model-choosable options, so a schema failure happens before the
    /// issue exists.
    pub fn plan_project(
        &self,
        profile: &crate::config::GithubProjectDefaults,
        fields: Vec<(String, String)>,
        title: Option<String>,
    ) -> Result<ProjectCreatePlan, AppError> {
        let plan = super::projects::prepare(profile, &fields, title.as_deref())?;
        Ok(ProjectCreatePlan { plan })
    }

    /// Discovers the configured Project and validates every field an edit sets or clears.
    ///
    /// This performs no remote write: one schema query rejects unknown, unsupported, or invalid
    /// fields before the issue is touched. Only the fields named here are planned; configured
    /// defaults and `autoSelectFields` are never reapplied by an edit. `title`, when set, is the
    /// explicit `--add-project` title and must match the discovered Project.
    pub fn plan_project_edit(
        &self,
        profile: &crate::config::GithubProjectDefaults,
        fields: Vec<(String, String)>,
        clears: Vec<String>,
        title: Option<String>,
    ) -> Result<ProjectEditPlan, AppError> {
        let plan = super::projects::prepare_edit(profile, &fields, &clears, title.as_deref())?;
        Ok(ProjectEditPlan { plan })
    }

    pub fn create_with_project(
        &self,
        issue: &NewIssue,
        plan: &ProjectCreatePlan,
    ) -> Result<Issue, AppError> {
        write::create(self, issue, Some(&plan.plan))
    }

    /// Applies one issue edit — the generic patch plus the native fields — together with the
    /// validated Project field changes.
    ///
    /// The issue is fetched once for the concurrency guard and body resolution, so both the
    /// issue-level change and the Project field writes share one consistent read. `plan` is
    /// `None` for an edit that does not touch Project fields.
    pub fn edit_native(
        &self,
        number: u64,
        patch: &IssuePatch,
        native: &NativeIssueEdit,
        plan: Option<&ProjectEditPlan>,
    ) -> Result<Issue, AppError> {
        let body = self.fetch_for_edit(number, patch)?;
        write::edit_native(
            self,
            number,
            patch,
            native,
            body.as_deref(),
            plan.map(|plan| &plan.plan),
        )
    }

    /// Fetches the issue once and resolves the requested body change.
    ///
    /// The fetch rejects a pull request before any mutation and provides the current body for a
    /// body change and the timestamp for the concurrency guard.
    fn fetch_for_edit(&self, number: u64, patch: &IssuePatch) -> Result<Option<String>, AppError> {
        let current = read::show(self, number)?;
        if let Some(expected) = patch.expect_updated_at.as_deref() {
            if current.updated_at != expected {
                return Err(AppError::conflict());
            }
        }
        match &patch.body {
            None => Ok(None),
            Some(change) => Ok(Some(resolve_body_change(&current.body, change)?)),
        }
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
        write::create(self, issue, None)
    }

    fn list(&self, query: &IssueQuery) -> Result<Vec<IssueSummary>, AppError> {
        read::list(self, query)
    }

    fn show(&self, number: u64) -> Result<Issue, AppError> {
        read::show(self, number)
    }
}
