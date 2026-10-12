mod discover;
mod files;
mod resolve;
pub use files::{
    Config, DecisionModelSettings, EventSelection, GithubIssueDefaults, GithubMergeMethod,
    GithubPrDefaults, GithubProjectDefaults, GitlabIssueDefaults, GitlabMrDefaults,
    OutputFormatSetting,
};
pub(crate) use files::{
    GithubWorkflowStage, MAX_WORKFLOW_PR_REFERENCES, WorkTracking, Workflow, WorkflowAction,
    WorkflowTrigger, approve_workflow, workflow_approval_status,
};
pub use resolve::{
    Provider, ProviderSelection, resolve_domain_context, select_providers, target_worktree_root,
};
pub fn repository_root(
    cwd: &std::path::Path,
) -> Result<std::path::PathBuf, crate::domain::AppError> {
    discover::git_root(cwd)?.ok_or_else(|| {
        crate::domain::AppError::context("current directory is not in a Git repository")
    })
}
pub fn load_for_cwd(cwd: &std::path::Path) -> Result<Config, crate::domain::AppError> {
    discover::git_root(cwd)?
        .as_deref()
        .map(files::load)
        .transpose()
        .map(|config| config.unwrap_or_default())
}
