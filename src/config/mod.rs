mod discover;
mod files;
mod resolve;
pub use files::{Config, GithubIssueDefaults, GithubProjectDefaults};
pub use resolve::{Provider, resolve_context, select_provider};
pub fn load_for_cwd(cwd: &std::path::Path) -> Result<Config, crate::domain::AppError> {
    discover::git_root(cwd)?
        .as_deref()
        .map(files::load)
        .transpose()
        .map(|config| config.unwrap_or_default())
}
