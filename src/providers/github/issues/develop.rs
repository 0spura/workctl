use crate::domain::AppError;
use crate::providers::github::issues::GitHubIssues;

/// Creates the branch GitHub links to the issue and returns its name.
///
/// The branch is created on the remote, so the current worktree changes only when `checkout` is
/// requested.
pub(super) fn create(
    provider: &GitHubIssues,
    number: u64,
    base: Option<&str>,
    name: Option<&str>,
    checkout: bool,
) -> Result<String, AppError> {
    let mut args = vec![
        "issue".to_owned(),
        "develop".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
    ];
    if let Some(base) = base {
        args.extend(["--base".to_owned(), base.to_owned()]);
    }
    if let Some(name) = name {
        args.extend(["--name".to_owned(), name.to_owned()]);
    }
    if checkout {
        args.push("--checkout".to_owned());
    }
    let stdout = provider.run_gh(&args, None)?;
    branch_from_reference(text(&stdout)?)
}

/// Branch names already linked to the issue, in the order the provider reports them.
pub(super) fn list(provider: &GitHubIssues, number: u64) -> Result<Vec<String>, AppError> {
    let args = vec![
        "issue".to_owned(),
        "develop".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--list".to_owned(),
    ];
    let stdout = provider.run_gh(&args, None)?;
    Ok(text(&stdout)?
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The branch name inside the reference `gh` prints, as in `github.com/OWNER/REPO/tree/BRANCH`.
fn branch_from_reference(reference: &str) -> Result<String, AppError> {
    reference
        .trim()
        .split_once("/tree/")
        .map(|(_, branch)| branch.trim())
        .filter(|branch| !branch.is_empty())
        .map(str::to_owned)
        .ok_or_else(AppError::provider_response)
}

fn text(stdout: &[u8]) -> Result<&str, AppError> {
    std::str::from_utf8(stdout).map_err(|_| AppError::provider_response())
}
