use crate::cli::GlobalArgs;
use crate::commands::support;
use crate::domain::AppError;
use crate::providers::gitlab::issues::GitLabIssues;

pub(super) fn validate_description(description: &str) -> Result<(), AppError> {
    if description == "-" {
        return Err(AppError::invalid_input(
            "a description consisting only of '-' cannot be passed to glab",
        ));
    }
    Ok(())
}

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitLabIssues, AppError> {
    let provider = GitLabIssues::new(support::resolve_repo(
        globals.provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
