use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{discover, files};
use crate::domain::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitlab,
}

impl Provider {
    /// The provider that owns `host`, if it is a public provider endpoint.
    pub fn for_host(host: &str) -> Option<Self> {
        match host.trim_end_matches('/').to_ascii_lowercase().as_str() {
            "github.com" => Some(Self::Github),
            "gitlab.com" => Some(Self::Gitlab),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Remote {
    provider: Provider,
    repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSelection {
    pub code: Option<Provider>,
    pub work_items: Option<Provider>,
}

/// Resolves repository scope for a single provider domain. A remote from another provider may
/// supply no repository scope for this command; an explicit `--repo` is then required.
pub fn resolve_domain_context(
    provider: Provider,
    explicit_repo: Option<&str>,
    cwd: &Path,
) -> Result<String, AppError> {
    let root = discover::git_root(cwd)?;
    let _config = root
        .as_deref()
        .map(files::load)
        .transpose()?
        .unwrap_or_default();
    let remote = if explicit_repo.is_none() {
        root.as_deref()
            .map(discover::origin_remote)
            .transpose()?
            .flatten()
            .map(|url| parse_remote(&url))
            .transpose()?
    } else {
        None
    };
    if let Some(remote) = remote.as_ref() {
        if remote.provider != provider && explicit_repo.is_none() {
            return Err(AppError::context(
                "the selected provider differs from the Git origin; specify --repo",
            ));
        }
    }
    let repo = match explicit_repo {
        Some(value) => validate_repo(provider, value)?,
        None => remote.map(|remote| remote.repo).ok_or(AppError::context(
            "could not determine a repository; specify --repo",
        ))?,
    };
    Ok(repo)
}

/// Resolves an explicit local target to its Git worktree root.
///
/// Relative paths are anchored to the caller's original working directory. A supplied path must
/// exist, be a directory, and belong to a Git worktree; it never falls back to the caller context.
pub fn target_worktree_root(cwd: &Path, repo_path: &Path) -> Result<PathBuf, AppError> {
    let path = if repo_path.is_absolute() {
        repo_path.to_path_buf()
    } else {
        cwd.join(repo_path)
    };
    let path = path
        .canonicalize()
        .map_err(|_| AppError::context("could not resolve the target project path"))?;
    if !fs::metadata(&path)
        .map_err(|_| AppError::context("could not inspect the target project path"))?
        .is_dir()
    {
        return Err(AppError::context(
            "the target project path must be a Git worktree directory",
        ));
    }
    discover::git_root(&path)?.ok_or(AppError::context(
        "the target project path is not inside a Git worktree",
    ))
}

/// Chooses independent providers from the selected worktree's configuration and Git origin
/// before parsing.
pub fn select_providers(cwd: &Path) -> ProviderSelection {
    let Ok(Some(root)) = discover::git_root(cwd) else {
        return ProviderSelection {
            code: None,
            work_items: None,
        };
    };
    let config = files::load(&root).ok();
    let remote = discover::origin_remote(&root)
        .ok()
        .flatten()
        .and_then(|url| parse_remote(&url).ok())
        .map(|remote| remote.provider);
    let code_config = config.as_ref().and_then(|config| config.code_provider);
    let work_item_config = config.as_ref().and_then(|config| config.work_item_provider);
    selection_from_values(code_config, work_item_config, remote)
}

fn selection_from_values(
    code_config: Option<Provider>,
    work_item_config: Option<Provider>,
    fallback: Option<Provider>,
) -> ProviderSelection {
    ProviderSelection {
        code: code_config.or(fallback),
        work_items: work_item_config.or(fallback),
    }
}

/// Validates a repository argument against the provider that owns it.
///
/// A leading known host is accepted only when it is the resolved provider's host, so a caller
/// cannot point one grammar at another provider's repository.
pub fn validate_repo(provider: Provider, value: &str) -> Result<String, AppError> {
    if provider == Provider::Gitlab && value.starts_with("https://") {
        return validate_gitlab_url(value);
    }

    let segments = value.split('/').collect::<Vec<_>>();
    let segments = match segments.split_first() {
        Some((host, rest)) if Provider::for_host(host).is_some() => {
            if Provider::for_host(host) != Some(provider) {
                return Err(AppError::invalid_input(
                    "--repo host does not match the resolved provider",
                ));
            }
            if provider == Provider::Gitlab {
                return validate_gitlab_path(host, rest);
            }
            rest
        }
        _ => segments.as_slice(),
    };
    let expected = match provider {
        Provider::Github => "repository must be OWNER/REPO",
        Provider::Gitlab => "repository must be GROUP/PROJECT or a GitLab project URL",
    };
    if segments.len() < 2 || segments.iter().any(|segment| !valid_component(segment)) {
        return Err(AppError::invalid_input(expected));
    }
    if provider == Provider::Github && segments.len() > 2 {
        return Err(AppError::invalid_input(expected));
    }
    match provider {
        Provider::Github => Ok(segments.join("/")),
        Provider::Gitlab => Ok(format!("https://gitlab.com/{}", segments.join("/"))),
    }
}

fn validate_gitlab_url(value: &str) -> Result<String, AppError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::invalid_input("GitLab project URL must be an HTTPS project URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || Provider::for_host(url.host_str().expect("host checked"))
            .is_some_and(|owner| owner != Provider::Gitlab)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::invalid_input(
            "GitLab project URL must be an HTTPS project URL",
        ));
    }
    let segments = url
        .path_segments()
        .ok_or_else(|| AppError::invalid_input("GitLab project URL must include a project path"))?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 || segments.iter().any(|segment| !valid_component(segment)) {
        return Err(AppError::invalid_input(
            "GitLab project URL must include a valid group/project path",
        ));
    }
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().expect("host checked")),
        None => url.host_str().expect("host checked").to_owned(),
    };
    Ok(format!("https://{host}/{}", segments.join("/")))
}

fn validate_gitlab_path(host: &str, segments: &[&str]) -> Result<String, AppError> {
    if segments.len() < 2 || segments.iter().any(|segment| !valid_component(segment)) {
        return Err(AppError::invalid_input(
            "repository must be GROUP/PROJECT or a GitLab project URL",
        ));
    }
    Ok(format!("https://{host}/{}", segments.join("/")))
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn parse_remote(url: &str) -> Result<Remote, AppError> {
    let (host, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(AppError::provider_unsupported)?;
        (authority.rsplit('@').next().unwrap_or(authority), path)
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(AppError::provider_unsupported)?;
        (
            authority
                .rsplit('@')
                .next()
                .unwrap_or(authority)
                .split(':')
                .next()
                .unwrap_or(authority),
            path,
        )
    } else if let Some((authority, path)) = url.rsplit_once(':') {
        if !path.contains('/') {
            return Err(AppError::provider_unsupported());
        }
        (authority.rsplit('@').next().unwrap_or(authority), path)
    } else {
        return Err(AppError::provider_unsupported());
    };

    let provider = Provider::for_host(host).ok_or_else(AppError::provider_unsupported)?;
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let repo = validate_repo(provider, &format!("{host}/{path}"))
        .map_err(|_| AppError::provider_unsupported())?;
    Ok(Remote { provider, repo })
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Provider, parse_remote, resolve_domain_context, select_providers, validate_repo};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "workctl-resolve-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create temporary Git root");
            Self(path)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn resolves_domain_repository_from_its_matching_origin() {
        let root = TempRoot::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&root.0)
                .status()
                .expect("run git init")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["remote", "add", "origin", "git@github.com:owner/repo.git"])
                .current_dir(&root.0)
                .status()
                .expect("add origin")
                .success()
        );
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"codeProvider":"gitlab","workItemProvider":"github"}"#,
        )
        .expect("write mixed provider configuration");
        let selected = select_providers(&root.0);
        assert_eq!(selected.code, Some(Provider::Gitlab));
        assert_eq!(selected.work_items, Some(Provider::Github));
        let resolved = resolve_domain_context(selected.work_items.unwrap(), None, &root.0)
            .expect("resolve issue scope");
        assert_eq!(resolved, "owner/repo");

        assert_eq!(
            resolve_domain_context(Provider::Gitlab, None, &root.0)
                .unwrap_err()
                .code,
            "context"
        );
        let explicit = resolve_domain_context(Provider::Gitlab, Some("group/sub/project"), &root.0)
            .expect("explicit GitLab scope");
        assert_eq!(explicit, "https://gitlab.com/group/sub/project");
        assert!(
            Command::new("git")
                .args([
                    "remote",
                    "set-url",
                    "origin",
                    "https://git.internal.test/team/project.git",
                ])
                .current_dir(&root.0)
                .status()
                .expect("set self-managed origin")
                .success()
        );
        let explicit = resolve_domain_context(
            Provider::Gitlab,
            Some("https://git.internal.test/team/project"),
            &root.0,
        )
        .expect("explicit GitLab scope overrides an unknown origin host");
        assert_eq!(explicit, "https://git.internal.test/team/project");

        fs::write(root.0.join(".workctl.json"), r#"{"unknown":"value"}"#)
            .expect("write invalid shared configuration");
        assert_eq!(
            resolve_domain_context(Provider::Github, Some("owner/repo"), &root.0)
                .unwrap_err()
                .code,
            "config"
        );
    }
    #[test]
    fn recognizes_supported_https_and_ssh_remote_forms() {
        for remote in [
            "https://github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
        ] {
            let parsed = parse_remote(remote).expect("supported remote");
            assert_eq!(parsed.provider, Provider::Github);
            assert_eq!(parsed.repo, "owner/repo");
        }
    }

    #[test]
    fn rejects_unknown_hosts_and_repository_path_injection() {
        assert_eq!(
            parse_remote("https://example.com/owner/repo")
                .unwrap_err()
                .code,
            "provider_unsupported"
        );
        assert_eq!(
            validate_repo(Provider::Github, "owner/repo/extra")
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            validate_repo(Provider::Github, "owner/repo?query")
                .unwrap_err()
                .code,
            "invalid_input"
        );
    }

    #[test]
    fn resolves_gitlab_remotes_including_nested_groups() {
        for remote in [
            "git@gitlab.com:owner/repo.git",
            "https://gitlab.com/group/subgroup/project.git",
        ] {
            let parsed = parse_remote(remote).expect("valid remote");
            assert_eq!(parsed.provider, Provider::Gitlab);
        }
        assert_eq!(
            parse_remote("git@gitlab.com:group/subgroup/project.git")
                .expect("valid GitLab remote")
                .repo,
            "https://gitlab.com/group/subgroup/project"
        );
    }

    #[test]
    fn accepts_self_managed_gitlab_urls_and_rejects_unsafe_urls() {
        assert_eq!(
            validate_repo(
                Provider::Gitlab,
                "https://gitlab.example.test/group/project"
            )
            .expect("self-managed GitLab URL"),
            "https://gitlab.example.test/group/project"
        );
        assert_eq!(
            validate_repo(
                Provider::Gitlab,
                "https://gitlab.example.test:8443/group/project"
            )
            .expect("custom GitLab port"),
            "https://gitlab.example.test:8443/group/project"
        );
        for invalid in [
            "http://gitlab.example.test/group/project",
            "https://user:secret@gitlab.example.test/group/project",
            "https://gitlab.example.test/group/project?private_token=x",
            "https://gitlab.example.test/group",
            "https://github.com/owner/repo",
        ] {
            assert!(
                validate_repo(Provider::Gitlab, invalid).is_err(),
                "accepted unsafe or incomplete GitLab URL: {invalid}"
            );
        }
    }

    #[test]
    fn accepts_a_repo_host_only_for_the_matching_provider() {
        assert_eq!(
            validate_repo(Provider::Gitlab, "gitlab.com/group/sub/project").expect("matching host"),
            "https://gitlab.com/group/sub/project"
        );
        assert_eq!(
            validate_repo(Provider::Github, "github.com/owner/repo").expect("matching host"),
            "owner/repo"
        );
        assert_eq!(
            validate_repo(Provider::Github, "gitlab.com/owner/repo")
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            validate_repo(Provider::Github, "owner/repo/extra")
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            validate_repo(Provider::Gitlab, "group/sub/project").expect("nested group"),
            "https://gitlab.com/group/sub/project"
        );
    }

    #[test]
    fn selects_independent_providers_from_current_worktree_config_and_origin() {
        let root = TempRoot::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&root.0)
                .status()
                .expect("run git init")
                .success()
        );
        assert_eq!(select_providers(&root.0).code, None);
        assert_eq!(select_providers(&root.0).work_items, None);
        assert!(
            Command::new("git")
                .args(["remote", "add", "origin", "git@gitlab.com:g/p.git"])
                .current_dir(&root.0)
                .status()
                .expect("add origin")
                .success()
        );
        assert_eq!(select_providers(&root.0).code, Some(Provider::Gitlab));
        assert_eq!(select_providers(&root.0).work_items, Some(Provider::Gitlab));
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"codeProvider":"github","workItemProvider":"gitlab"}"#,
        )
        .expect("write worktree provider configuration");
        let selected = select_providers(&root.0);
        assert_eq!(selected.code, Some(Provider::Github));
        assert_eq!(selected.work_items, Some(Provider::Gitlab));
    }
    #[test]
    fn provider_selection_uses_the_current_worktree_not_another_project() {
        let root = TempRoot::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&root.0)
                .status()
                .expect("initialize second project")
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "remote",
                    "add",
                    "origin",
                    "git@github.com:other/project.git"
                ])
                .current_dir(&root.0)
                .status()
                .expect("add second project origin")
                .success()
        );
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"codeProvider":"gitlab","workItemProvider":"github"}"#,
        )
        .expect("write second project config");

        let selected = select_providers(&root.0);
        assert_eq!(selected.code, Some(Provider::Gitlab));
        assert_eq!(selected.work_items, Some(Provider::Github));
    }
}
