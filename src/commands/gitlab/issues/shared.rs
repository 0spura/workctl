use crate::cli::GlobalArgs;
use crate::commands::support;
use crate::domain::AppError;
use crate::providers::gitlab::issues::GitLabIssues;

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitLabIssues, AppError> {
    let provider = GitLabIssues::new(support::resolve_repo(
        globals.work_item_provider,
        globals.repo.as_deref(),
    )?)?;
    provider.authenticate()?;
    Ok(provider)
}
