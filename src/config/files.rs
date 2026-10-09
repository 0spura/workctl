use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::config::Provider;
use crate::domain::AppError;

const SHARED_FILE: &str = ".workctl.json";
const LOCAL_FILE: &str = ".workctl.local.json";

const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub provider: Option<Provider>,
    pub code_provider: Option<Provider>,
    pub work_item_provider: Option<Provider>,
    pub defaults: Option<Defaults>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub github: Option<GithubDefaults>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubDefaults {
    pub issue: Option<GithubIssueDefaults>,
    pub pr: Option<GithubPrDefaults>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GithubMergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubPrDefaults {
    pub merge_method: Option<GithubMergeMethod>,
    #[serde(default)]
    pub delete_branch: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubIssueDefaults {
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub label_candidates: Vec<String>,
    pub project: Option<GithubProjectDefaults>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubProjectDefaults {
    pub url: String,
    pub repositories: Vec<String>,
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub auto_select_fields: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigFile {
    provider: Option<Provider>,
    code_provider: Option<Provider>,
    work_item_provider: Option<Provider>,
    defaults: Option<Defaults>,
}

pub fn load(root: &Path) -> Result<Config, AppError> {
    let mut config = Config::default();
    merge_file(root.join(SHARED_FILE), &mut config)?;
    merge_file(root.join(LOCAL_FILE), &mut config)?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &Config) -> Result<(), AppError> {
    let Some(issue) = config
        .defaults
        .as_ref()
        .and_then(|defaults| defaults.github.as_ref())
        .and_then(|github| github.issue.as_ref())
    else {
        return Ok(());
    };
    if issue.assignees.iter().any(|value| value.trim().is_empty())
        || issue
            .labels
            .iter()
            .any(|value| value.trim().is_empty() || value == "@auto")
        || issue
            .label_candidates
            .iter()
            .any(|value| value.trim().is_empty() || value == "@auto")
        || has_duplicates(&issue.label_candidates)
    {
        return Err(AppError::config(
            "GitHub issue defaults contain an invalid value",
        ));
    }
    if let Some(project) = issue.project.as_ref() {
        if !valid_project_url(&project.url)
            || project.repositories.is_empty()
            || project
                .repositories
                .iter()
                .any(|repo| !valid_repository_path(repo))
            || project.fields.iter().any(|(name, value)| {
                name.trim().is_empty() || name.contains('=') || value.trim().is_empty()
            })
            || project
                .auto_select_fields
                .iter()
                .any(|name| name.trim().is_empty() || name.contains('='))
            || has_duplicates(&project.auto_select_fields)
        {
            return Err(AppError::config("GitHub Project defaults are invalid"));
        }
    }
    Ok(())
}

fn has_duplicates(values: &[String]) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(values.len());
    values.iter().any(|value| !seen.insert(value))
}
fn valid_project_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
    parts.len() == 4
        && matches!(parts[0], "orgs" | "users")
        && !parts[1].is_empty()
        && parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && parts[2] == "projects"
        && parts[3].parse::<u64>().is_ok_and(|number| number > 0)
        && !url.contains('?')
        && !url.contains('#')
}

fn valid_repository_path(repo: &str) -> bool {
    let parts = repo.split('/').collect::<Vec<_>>();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(valid_repo_byte))
}

fn valid_repo_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

fn merge_file(path: impl AsRef<Path>, config: &mut Config) -> Result<(), AppError> {
    let Some(contents) = read_config(path.as_ref())? else {
        return Ok(());
    };
    let file: ConfigFile = serde_json::from_slice(&contents)
        .map_err(|_| AppError::config("project configuration is invalid"))?;
    if file.provider.is_some() {
        config.provider = file.provider;
    }
    if file.code_provider.is_some() {
        config.code_provider = file.code_provider;
    }
    if file.work_item_provider.is_some() {
        config.work_item_provider = file.work_item_provider;
    }
    if file.defaults.is_some() {
        config.defaults = file.defaults;
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<Option<Vec<u8>>, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AppError::config("could not inspect project configuration")),
    };
    if !metadata.file_type().is_file() {
        return Err(AppError::config(
            "project configuration must be a regular file",
        ));
    }

    let file =
        open_config(path).map_err(|_| AppError::config("could not read project configuration"))?;
    if !file
        .metadata()
        .map_err(|_| AppError::config("could not inspect project configuration"))?
        .is_file()
    {
        return Err(AppError::config(
            "project configuration must be a regular file",
        ));
    }
    let mut limited = file.take(MAX_CONFIG_BYTES as u64 + 1);
    let mut contents = Vec::new();
    limited
        .read_to_end(&mut contents)
        .map_err(|_| AppError::config("could not read project configuration"))?;
    if contents.len() > MAX_CONFIG_BYTES {
        return Err(AppError::config(
            "project configuration exceeds the size limit",
        ));
    }
    Ok(Some(contents))
}

fn open_config(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{GithubMergeMethod, load};
    use crate::config::Provider;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "workctl-config-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create isolated config root");
            Self(path)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn local_config_overrides_each_shared_field_independently() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"provider":"github","codeProvider":"github","workItemProvider":"gitlab"}"#,
        )
        .expect("write shared config");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"provider":"github","codeProvider":"gitlab"}"#,
        )
        .expect("write local config");

        let config = load(&root.0).expect("load valid config");
        assert_eq!(config.provider, Some(Provider::Github));
        assert_eq!(config.code_provider, Some(Provider::Gitlab));
        assert_eq!(config.work_item_provider, Some(Provider::Gitlab));
    }
    #[test]
    fn local_defaults_replace_shared_defaults_as_one_object() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"github":{"issue":{"labels":["shared"],"labelCandidates":["bug"]}}}}"#,
        )
        .expect("write shared config");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"defaults":{"github":{"issue":{"labelCandidates":["docs"]}}}}"#,
        )
        .expect("write local config");

        let config = load(&root.0).expect("load valid config");
        let issue = config
            .defaults
            .expect("defaults")
            .github
            .expect("GitHub defaults")
            .issue
            .expect("issue defaults");
        assert!(issue.labels.is_empty());
        assert_eq!(issue.label_candidates, ["docs"]);
    }

    #[test]
    fn github_pr_merge_defaults_parse_method_and_boolean_branch_policy() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"github":{"pr":{"mergeMethod":"squash"}}}}"#,
        )
        .expect("write valid PR defaults");
        let config = load(&root.0).expect("load valid PR defaults");
        let pr = config.defaults.unwrap().github.unwrap().pr.unwrap();
        assert!(matches!(pr.merge_method, Some(GithubMergeMethod::Squash)));
        assert!(!pr.delete_branch);

        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"github":{"pr":{"deleteBranch":true}}}}"#,
        )
        .expect("write branch deletion default");
        let config = load(&root.0).expect("load branch deletion default");
        assert!(
            config
                .defaults
                .unwrap()
                .github
                .unwrap()
                .pr
                .unwrap()
                .delete_branch
        );

        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"github":{"pr":{"mergeMethod":"fast"}}}}"#,
        )
        .expect("write unsupported merge method");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[test]
    fn malformed_and_unknown_configuration_fails_closed() {
        let root = TempRoot::new();
        for contents in [
            r#"{"defaults":{"github":{"issue":{"labelCandidates":["@auto"]}}}}"#,
            r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"autoSelectFields":["Priority","Priority"]}}}}}"#,
            r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"autoSelectFields":[""]}}}}}"#,
        ] {
            fs::write(root.0.join(".workctl.json"), contents).expect("write invalid config");
            assert_eq!(load(&root.0).unwrap_err().code, "config");
        }
    }

    #[test]
    fn rejects_configuration_over_the_fixed_size_limit() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            vec![b' '; super::MAX_CONFIG_BYTES + 1],
        )
        .expect("write oversized config");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[test]
    fn rejects_directories_as_configuration_files() {
        let root = TempRoot::new();
        fs::create_dir(root.0.join(".workctl.json")).expect("create invalid config directory");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_configuration_without_reading_its_target() {
        let root = TempRoot::new();
        std::os::unix::fs::symlink("/dev/zero", root.0.join(".workctl.json"))
            .expect("create device symlink");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }
}
