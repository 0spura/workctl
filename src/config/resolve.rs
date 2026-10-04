use std::path::Path;

use clap::ValueEnum;
use serde::Deserialize;

use crate::config::{discover, files};
use crate::domain::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[value(name = "github")]
    Github,
    #[value(name = "gitlab")]
    Gitlab,
}

impl Provider {
    /// The provider that owns `host`, if it is a host this build can reach.
    pub fn for_host(host: &str) -> Option<Self> {
        match host.trim_end_matches('/').to_ascii_lowercase().as_str() {
            "github.com" => Some(Self::Github),
            "gitlab.com" => Some(Self::Gitlab),
            _ => None,
        }
    }

    /// Parses a `--provider` value without the command tree.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "github" => Some(Self::Github),
            "gitlab" => Some(Self::Gitlab),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ResolvedContext {
    pub repo: String,
}

#[derive(Debug)]
struct Remote {
    provider: Provider,
    repo: String,
}

pub fn resolve_context(
    explicit_provider: Option<Provider>,
    explicit_repo: Option<&str>,
    cwd: &Path,
) -> Result<ResolvedContext, AppError> {
    let root = discover::git_root(cwd)?;
    let config = root
        .as_deref()
        .map(files::load)
        .transpose()?
        .unwrap_or_default();
    let configured_provider = config.work_item_provider.or(config.provider);
    let needs_remote =
        explicit_repo.is_none() || (explicit_provider.is_none() && configured_provider.is_none());
    let remote = if needs_remote {
        root.as_deref()
            .map(discover::origin_remote)
            .transpose()?
            .flatten()
            .map(|url| parse_remote(&url))
            .transpose()?
    } else {
        None
    };

    let provider = explicit_provider
        .or(configured_provider)
        .or_else(|| remote.as_ref().map(|remote| remote.provider))
        .ok_or(AppError::context(
            "could not determine a work-item provider; specify --provider",
        ))?;
    if remote
        .as_ref()
        .is_some_and(|remote| remote.provider != provider)
    {
        return Err(AppError::provider_mismatch());
    }

    let repo = match explicit_repo {
        Some(value) => validate_repo(provider, value)?,
        None => remote
            .map(|remote| remote.repo)
            .ok_or(AppError::context(
                "could not determine a repository; specify --repo",
            ))?,
    };
    Ok(ResolvedContext { repo })
}

/// Selects the provider for building the command grammar, before parsing.
///
/// The grammar has to exist before clap can read the arguments, so this runs without the
/// command tree and never fails: an unresolvable or invalid context falls back to GitHub, and
/// the authoritative resolution reports the real error when the command runs.
pub fn select_provider(explicit: Option<&str>, cwd: &Path) -> Provider {
    if let Some(provider) = explicit.and_then(Provider::from_name) {
        return provider;
    }
    let Ok(Some(root)) = discover::git_root(cwd) else {
        return Provider::Github;
    };
    if let Ok(config) = files::load(&root) {
        if let Some(provider) = config.work_item_provider.or(config.provider) {
            return provider;
        }
    }
    discover::origin_remote(&root)
        .ok()
        .flatten()
        .and_then(|url| parse_remote(&url).ok())
        .map(|remote| remote.provider)
        .unwrap_or(Provider::Github)
}

/// Validates a repository argument against the provider that owns it.
///
/// A leading known host is accepted only when it is the resolved provider's host, so a caller
/// cannot point one grammar at another provider's repository.
pub fn validate_repo(provider: Provider, value: &str) -> Result<String, AppError> {
    let segments = value.split('/').collect::<Vec<_>>();
    let segments = match segments.split_first() {
        Some((host, rest)) if Provider::for_host(host).is_some() => {
            if Provider::for_host(host) != Some(provider) {
                return Err(AppError::invalid_input(
                    "--repo host does not match the resolved provider",
                ));
            }
            rest
        }
        _ => segments.as_slice(),
    };
    let expected = match provider {
        Provider::Github => "repository must be OWNER/REPO",
        Provider::Gitlab => "repository must be GROUP/PROJECT or GROUP/SUBGROUP/PROJECT",
    };
    if segments.len() < 2 || segments.iter().any(|segment| !valid_component(segment)) {
        return Err(AppError::invalid_input(expected));
    }
    if provider == Provider::Github && segments.len() > 2 {
        return Err(AppError::invalid_input(expected));
    }
    Ok(segments.join("/"))
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
    let repo = validate_repo(provider, path).map_err(|_| AppError::provider_unsupported())?;
    Ok(Remote { provider, repo })
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Provider, parse_remote, resolve_context, select_provider, validate_repo};

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
    fn resolves_config_remote_and_explicit_precedence() {
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
                .args([
                    "remote",
                    "add",
                    "origin",
                    "git@github.com:remote-owner/remote-repo.git",
                ])
                .current_dir(&root.0)
                .status()
                .expect("add origin")
                .success()
        );
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"provider":"gitlab","workItemProvider":"github"}"#,
        )
        .expect("write shared configuration");

        let resolved = resolve_context(None, None, &root.0).expect("resolve GitHub context");
        assert_eq!(resolved.repo, "remote-owner/remote-repo");

        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"workItemProvider":"gitlab"}"#,
        )
        .expect("write local override");
        assert_eq!(
            resolve_context(None, None, &root.0).unwrap_err().code,
            "provider_unsupported"
        );

        let explicit = resolve_context(
            Some(Provider::Github),
            Some("explicit-owner/explicit-repo"),
            &root.0,
        )
        .expect("explicit overrides");
        assert_eq!(explicit.repo, "explicit-owner/explicit-repo");
        fs::write(root.0.join(".workctl.json"), r#"{"repo":"owner/repo"}"#)
            .expect("write invalid shared configuration");
        assert_eq!(
            resolve_context(
                Some(Provider::Github),
                Some("explicit-owner/explicit-repo"),
                &root.0,
            )
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
                .expect("valid remote")
                .repo,
            "group/subgroup/project"
        );
    }

    #[test]
    fn accepts_a_repo_host_only_for_the_matching_provider() {
        assert_eq!(
            validate_repo(Provider::Gitlab, "gitlab.com/group/sub/project").expect("matching host"),
            "group/sub/project"
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
            "group/sub/project"
        );
    }

    #[test]
    fn fails_closed_when_the_remote_belongs_to_another_provider() {
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
        assert_eq!(
            resolve_context(Some(Provider::Gitlab), None, &root.0)
                .unwrap_err()
                .code,
            "provider_unsupported"
        );
        let resolved = resolve_context(Some(Provider::Gitlab), Some("group/project"), &root.0)
            .expect("an explicit repository overrides the remote");
        assert_eq!(resolved.repo, "group/project");
    }

    #[test]
    fn selects_the_grammar_provider_before_parsing() {
        let root = TempRoot::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&root.0)
                .status()
                .expect("run git init")
                .success()
        );
        assert_eq!(select_provider(None, &root.0), Provider::Github);
        assert_eq!(select_provider(Some("gitlab"), &root.0), Provider::Gitlab);
        assert!(
            Command::new("git")
                .args(["remote", "add", "origin", "git@gitlab.com:g/p.git"])
                .current_dir(&root.0)
                .status()
                .expect("add origin")
                .success()
        );
        assert_eq!(select_provider(None, &root.0), Provider::Gitlab);
        assert_eq!(select_provider(Some("bogus"), &root.0), Provider::Gitlab);
        fs::write(root.0.join(".workctl.json"), r#"{"provider":"github"}"#)
            .expect("write shared configuration");
        assert_eq!(select_provider(None, &root.0), Provider::Github);
    }
}
