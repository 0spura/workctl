use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::providers::github::prs::GitHubPulls;
use crate::providers::PullRequestProvider;

use crate::commands::support;

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitHubPulls, AppError> {
    let provider = GitHubPulls::new(support::resolve_repo(
        globals.provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
