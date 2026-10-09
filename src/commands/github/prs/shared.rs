use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::providers::PullRequestProvider;
use crate::providers::github::prs::GitHubPulls;

use crate::commands::support;

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitHubPulls, AppError> {
    let provider = GitHubPulls::new(support::resolve_repo(
        globals.code_provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
