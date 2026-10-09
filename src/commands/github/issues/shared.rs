use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::providers::WorkItemProvider;
use crate::providers::github::issues::GitHubIssues;

use crate::commands::support;

pub(super) fn filter_label_catalog(
    catalog: Vec<crate::domain::RepositoryLabel>,
    candidates: &[String],
) -> Result<Vec<crate::domain::RepositoryLabel>, AppError> {
    if candidates.is_empty() {
        return Ok(catalog);
    }
    let mut selected = Vec::with_capacity(candidates.len());
    for name in candidates {
        let label = catalog
            .iter()
            .find(|label| label.name == *name)
            .ok_or(AppError::config(
                "a configured label candidate does not exist in the repository",
            ))?;
        selected.push(label.clone());
    }
    Ok(selected)
}

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitHubIssues, AppError> {
    let provider = GitHubIssues::new(support::resolve_repo(
        globals.work_item_provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
