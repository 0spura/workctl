use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Provider;
use crate::domain::AppError;
use crate::process::runner;

const SHARED_FILE: &str = ".workctl.json";
const LOCAL_FILE: &str = ".workctl.local.json";

const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub code_provider: Option<Provider>,
    pub work_item_provider: Option<Provider>,
    pub decision_model: Option<DecisionModelSettings>,
    pub defaults: Option<Defaults>,
    pub events: Option<EventSelection>,
    pub workflow: Option<Workflow>,
    pub workflow_approval: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workflow {
    #[serde(default)]
    pub rules: Vec<WorkflowRule>,
    #[serde(default)]
    pub stage_mappings: WorkflowStageMappings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_tracking: Option<WorkTracking>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkTracking {
    #[serde(default = "default_idle_timeout_minutes")]
    pub idle_timeout_minutes: u8,
    #[serde(default = "default_log_gitlab_time_spent")]
    pub log_gitlab_time_spent: bool,
}

fn default_idle_timeout_minutes() -> u8 {
    10
}
fn default_log_gitlab_time_spent() -> bool {
    false
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStageMappings {
    #[serde(default)]
    pub github: std::collections::BTreeMap<String, GithubWorkflowStage>,
    #[serde(default)]
    pub gitlab: std::collections::BTreeMap<String, GitlabWorkflowStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubWorkflowStage {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitlabWorkflowStage {
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRule {
    pub on: WorkflowTrigger,
    pub actions: Vec<WorkflowAction>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowTrigger {
    IssueCreated,
    LinkedBranchCreated,
    PullRequestCreated,
    PullRequestMerged,
    ReviewApproved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkflowAction {
    SetStage {
        stage: String,
    },
    SetProjectField {
        field: String,
        value: String,
    },
    AddLabel {
        label: String,
    },
    RemoveLabel {
        label: String,
    },
    AddAssignee {
        assignee: String,
    },
    RemoveAssignee {
        assignee: String,
    },
    Comment {
        body: String,
    },
    LogTimeSpent {
        seconds: u64,
        session_id: String,
        finished_at_ms: i64,
    },
}

pub const MAX_WORKFLOW_PR_REFERENCES: usize = 32;
pub const MAX_WORKFLOW_ACTIONS_PER_TRIGGER: usize = 512;
pub const MAX_PR_WORKFLOW_ACTIONS_PER_TRIGGER: usize = 16;

pub fn workflow_fingerprint(
    workflow: &Workflow,
    repository_scope: &str,
    github_project: Option<&GithubProjectDefaults>,
) -> Result<String, AppError> {
    let rules = serde_json::to_vec(&(workflow, github_project))
        .map_err(|_| AppError::config("workflow rules could not be fingerprinted"))?;
    let mut input = Vec::with_capacity(repository_scope.len() + rules.len() + 32);
    input.extend_from_slice(b"workctl-workflow-v2\0");
    input.extend_from_slice(repository_scope.as_bytes());
    input.push(0);
    input.extend_from_slice(&rules);
    let digest = ring::digest::digest(&ring::digest::SHA256, &input);
    let mut fingerprint = String::with_capacity(digest.as_ref().len() * 2);
    for byte in digest.as_ref() {
        use std::fmt::Write;
        let _ = write!(fingerprint, "{byte:02x}");
    }
    Ok(fingerprint)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSelection {
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigFile {
    code_provider: Option<Provider>,
    work_item_provider: Option<Provider>,
    decision_model: Option<DecisionModelSettings>,
    defaults: Option<Defaults>,
    events: Option<EventSelection>,
    workflow: Option<Workflow>,
    workflow_approval: Option<String>,
}

/// Decision-model settings. `apiKey` is accepted only in `.workctl.local.json`, so a shared
/// configuration file can never commit a hosted credential.
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionModelSettings {
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// Redacts the credential so diagnostics can never print it.
impl std::fmt::Debug for DecisionModelSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DecisionModelSettings")
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub github: Option<GithubDefaults>,
    pub gitlab: Option<GitlabDefaults>,
    pub output: Option<OutputDefaults>,
}

/// Success output defaults shared by both providers.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputDefaults {
    pub format: Option<OutputFormatSetting>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormatSetting {
    Json,
    Text,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitlabDefaults {
    pub issue: Option<GitlabIssueDefaults>,
    pub mr: Option<GitlabMrDefaults>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitlabIssueDefaults {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitlabMrDefaults {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    pub target_branch: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubDefaults {
    pub issue: Option<GithubIssueDefaults>,
    pub pr: Option<GithubPrDefaults>,
    /// Default `--limit` for GitHub issue and pull-request listings.
    pub list_limit: Option<usize>,
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
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    pub base: Option<String>,
    #[serde(default)]
    pub draft: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubProjectDefaults {
    pub url: String,
    pub repositories: Vec<String>,
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub auto_select_fields: Vec<String>,
}

pub fn load(root: &Path) -> Result<Config, AppError> {
    let mut config = Config::default();
    merge_file(root.join(SHARED_FILE), false, &mut config)?;
    merge_file(root.join(LOCAL_FILE), true, &mut config)?;
    validate(&config)?;
    Ok(config)
}
pub fn approve_workflow(root: &Path) -> Result<String, AppError> {
    let config = load(root)?;
    let workflow = config
        .workflow
        .as_ref()
        .ok_or_else(|| AppError::config("no shared workflow rules are configured"))?;
    ensure_workflow_local_config(root)?;
    let scope = fs::canonicalize(root)
        .map_err(|_| AppError::context("could not determine repository scope"))?;
    let github_project = config
        .defaults
        .as_ref()
        .and_then(|defaults| defaults.github.as_ref())
        .and_then(|github| github.issue.as_ref())
        .and_then(|issue| issue.project.as_ref());
    let fingerprint = workflow_fingerprint(workflow, &scope.to_string_lossy(), github_project)?;
    let path = root.join(LOCAL_FILE);
    let mut local = match read_config(&path)? {
        Some(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|_| AppError::config("project configuration is invalid"))?,
        None => serde_json::Value::Object(serde_json::Map::new()),
    };
    let object = local
        .as_object_mut()
        .ok_or_else(|| AppError::config("project configuration must be a JSON object"))?;
    object.insert(
        "workflowApproval".to_owned(),
        serde_json::Value::String(fingerprint.clone()),
    );
    let bytes = serde_json::to_vec_pretty(&local)
        .map_err(|_| AppError::config("project configuration could not be written"))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(AppError::config(
            "project configuration exceeds the size limit",
        ));
    }
    let temporary = root.join(format!(".workctl.local.{}.tmp", uuid::Uuid::new_v4()));
    let mut file = create_workflow_approval_file(&temporary)
        .map_err(|_| AppError::config("could not write local workflow approval"))?;
    let write_result = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| AppError::config("could not write local workflow approval"));
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if fs::rename(&temporary, &path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::config("could not write local workflow approval"));
    }
    Ok(fingerprint)
}

fn create_workflow_approval_file(path: &Path) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        crate::events::create_private_file_new(path)
    }
    #[cfg(not(windows))]
    {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}

pub fn workflow_approval_status(config: &Config, root: &Path) -> Result<bool, AppError> {
    let Some(workflow) = config.workflow.as_ref() else {
        return Ok(false);
    };
    let Some(approval) = config.workflow_approval.as_deref() else {
        return Ok(false);
    };
    let scope = std::fs::canonicalize(root)
        .map_err(|_| AppError::context("could not determine repository scope"))?;
    let github_project = config
        .defaults
        .as_ref()
        .and_then(|defaults| defaults.github.as_ref())
        .and_then(|github| github.issue.as_ref())
        .and_then(|issue| issue.project.as_ref());
    Ok(workflow_fingerprint(workflow, &scope.to_string_lossy(), github_project)? == approval)
}

fn ensure_workflow_local_config(root: &Path) -> Result<(), AppError> {
    if [
        "GIT_INDEX_FILE",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Err(AppError::config(
            "could not verify local workflow configuration provenance",
        ));
    }
    let path = root.join(LOCAL_FILE);
    let tracked = runner::run(
        "git",
        &["ls-files", "--cached", "-z"].map(str::to_owned),
        None,
        Some(root),
    )
    .map_err(|_| AppError::config("could not verify local workflow configuration provenance"))?;
    if !tracked.success {
        return Err(AppError::config(
            "could not verify local workflow configuration provenance",
        ));
    }
    let metadata = fs::metadata(&path).ok();
    if tracked.stdout.split(|byte| *byte == 0).any(|entry| {
        entry.eq_ignore_ascii_case(LOCAL_FILE.as_bytes())
            || metadata
                .as_ref()
                .is_some_and(|metadata| tracked_path_aliases_local(&path, entry, metadata))
    }) {
        return Err(AppError::config(
            "local workflow approval requires an untracked .workctl.local.json",
        ));
    }
    let ignored = runner::run(
        "git",
        &["check-ignore", "--quiet", "--", LOCAL_FILE].map(str::to_owned),
        None,
        Some(root),
    )
    .map_err(|_| AppError::config("could not verify local workflow ignore rule"))?;
    if !ignored.success {
        return Err(AppError::config(
            ".workctl.local.json must be ignored before local workflow approval",
        ));
    }
    Ok(())
}

fn validate(config: &Config) -> Result<(), AppError> {
    if let Some(github) = config.defaults.as_ref().and_then(|d| d.github.as_ref()) {
        if let Some(limit) = github.list_limit {
            if !(1..=1000).contains(&limit) {
                return Err(AppError::config(
                    "GitHub list limit must be between 1 and 1000",
                ));
            }
        }
        if let Some(pr) = github.pr.as_ref() {
            if pr
                .labels
                .iter()
                .any(|value| value.trim().is_empty() || value == "@auto")
                || pr
                    .assignees
                    .iter()
                    .chain(pr.reviewers.iter())
                    .any(|value| value.trim().is_empty())
                || pr.base.as_ref().is_some_and(|base| base.trim().is_empty())
            {
                return Err(AppError::config(
                    "GitHub pull-request defaults contain an invalid value",
                ));
            }
        }
    }
    if let Some(gitlab) = config.defaults.as_ref().and_then(|d| d.gitlab.as_ref()) {
        let invalid = |values: &[String]| {
            values
                .iter()
                .any(|value| value.trim().is_empty() || value == "@auto")
        };
        let issue_invalid = gitlab
            .issue
            .as_ref()
            .is_some_and(|issue| invalid(&issue.labels) || invalid(&issue.assignees));
        let mr_invalid = gitlab.mr.as_ref().is_some_and(|mr| {
            invalid(&mr.labels)
                || invalid(&mr.assignees)
                || invalid(&mr.reviewers)
                || mr
                    .target_branch
                    .as_ref()
                    .is_some_and(|branch| branch.trim().is_empty())
        });
        if issue_invalid || mr_invalid {
            return Err(AppError::config("GitLab defaults contain an invalid value"));
        }
    }
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

fn workflow_expansion_is_valid(workflow: &Workflow) -> bool {
    [
        WorkflowTrigger::IssueCreated,
        WorkflowTrigger::LinkedBranchCreated,
        WorkflowTrigger::PullRequestCreated,
        WorkflowTrigger::PullRequestMerged,
        WorkflowTrigger::ReviewApproved,
    ]
    .into_iter()
    .all(|trigger| {
        let actions = workflow
            .rules
            .iter()
            .filter(|rule| rule.on == trigger)
            .map(|rule| rule.actions.len())
            .sum::<usize>();
        let limit = if matches!(
            trigger,
            WorkflowTrigger::PullRequestCreated
                | WorkflowTrigger::PullRequestMerged
                | WorkflowTrigger::ReviewApproved
        ) {
            MAX_PR_WORKFLOW_ACTIONS_PER_TRIGGER
        } else {
            MAX_WORKFLOW_ACTIONS_PER_TRIGGER
        };
        actions <= limit
    })
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

/// Merges one project configuration file. Credentials and endpoint selection are local-only.
fn merge_file(
    path: impl AsRef<Path>,
    allow_credentials: bool,
    config: &mut Config,
) -> Result<(), AppError> {
    let Some(contents) = read_config(path.as_ref())? else {
        return Ok(());
    };
    let file: ConfigFile = serde_json::from_slice(&contents)
        .map_err(|_| AppError::config("project configuration is invalid"))?;
    if file.workflow_approval.is_some() && !allow_credentials {
        return Err(AppError::config(
            "workflow approval is only allowed in .workctl.local.json",
        ));
    }
    if file.workflow.is_some() && !allow_credentials {
        if file.workflow.as_ref().is_some_and(|workflow| {
            (workflow.rules.is_empty() && workflow.work_tracking.is_none())
                || workflow
                    .work_tracking
                    .as_ref()
                    .is_some_and(|tracking| !(1..=60).contains(&tracking.idle_timeout_minutes))
                || workflow.rules.len() > 32
                || workflow.stage_mappings.github.len() > 64
                || workflow.stage_mappings.gitlab.len() > 64
                || workflow
                    .stage_mappings
                    .github
                    .iter()
                    .any(|(stage, mapping)| {
                        stage.trim().is_empty()
                            || mapping.field.trim().is_empty()
                            || mapping.value.trim().is_empty()
                    })
                || workflow
                    .stage_mappings
                    .gitlab
                    .iter()
                    .any(|(stage, mapping)| {
                        stage.trim().is_empty() || mapping.label.trim().is_empty()
                    })
                || workflow.rules.iter().any(|rule| {
                    rule.actions.is_empty()
                        || rule.actions.len() > 16
                        || rule.actions.iter().any(|action| match action {
                            WorkflowAction::SetStage { stage } => stage.trim().is_empty(),
                            WorkflowAction::SetProjectField { field, value } => {
                                field.trim().is_empty() || value.trim().is_empty()
                            }
                            WorkflowAction::AddLabel { label }
                            | WorkflowAction::RemoveLabel { label } => label.trim().is_empty(),
                            WorkflowAction::AddAssignee { assignee }
                            | WorkflowAction::RemoveAssignee { assignee } => {
                                let value = assignee.trim();
                                value.is_empty()
                                    || value != assignee
                                    || value.contains(',')
                                    || value.starts_with('+')
                                    || value.starts_with('-')
                                    || value.starts_with('!')
                                    || value.chars().any(char::is_whitespace)
                            }
                            WorkflowAction::Comment { body } => {
                                body.trim().is_empty() || body.len() > 16 * 1024
                            }
                            WorkflowAction::LogTimeSpent { .. } => true,
                        })
                })
                || !workflow_expansion_is_valid(workflow)
        }) {
            return Err(AppError::config("workflow rules are invalid"));
        }
        config.workflow = file.workflow;
    } else if file.workflow.is_some() {
        return Err(AppError::config(
            "workflow rules are only allowed in .workctl.json",
        ));
    }
    if let Some(approval) = file.workflow_approval {
        if approval.len() != 64 || !approval.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AppError::config("local workflow approval is invalid"));
        }
        ensure_untracked_event_config(path.as_ref())?;
        ensure_ignored_local_config(path.as_ref())?;
        config.workflow_approval = Some(approval);
    }
    if let Some(events) = file.events {
        if !allow_credentials {
            return Err(AppError::config(
                "event endpoint selection is only allowed in .workctl.local.json",
            ));
        }
        ensure_untracked_event_config(path.as_ref())?;
        if events.endpoints.len() > 16
            || events.endpoints.iter().any(|id| id.trim().is_empty())
            || has_duplicates(&events.endpoints)
        {
            return Err(AppError::config("event endpoint selection is invalid"));
        }
        config.events = Some(events);
    }
    if file.code_provider.is_some() {
        config.code_provider = file.code_provider;
    }
    if file.work_item_provider.is_some() {
        config.work_item_provider = file.work_item_provider;
    }
    if let Some(decision_model) = file.decision_model {
        if decision_model.api_key.is_some() && !allow_credentials {
            return Err(AppError::config(
                "the decision-model API key is only allowed in .workctl.local.json",
            ));
        }
        // Each decision-model key merges independently, so a local file can add its key without
        // discarding a shared model or base URL.
        let resolved = config.decision_model.get_or_insert_default();
        if decision_model.model.is_some() {
            resolved.model = decision_model.model;
        }
        if decision_model.api_key.is_some() {
            resolved.api_key = decision_model.api_key;
        }
        if decision_model.base_url.is_some() {
            resolved.base_url = decision_model.base_url;
        }
    }
    if file.defaults.is_some() {
        config.defaults = file.defaults;
    }
    Ok(())
}

fn ensure_untracked_event_config(path: &Path) -> Result<(), AppError> {
    if [
        "GIT_INDEX_FILE",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Err(AppError::config(
            "could not verify local event configuration provenance",
        ));
    }
    let output = runner::run(
        "git",
        &["ls-files", "--cached", "-z"].map(str::to_owned),
        None,
        path.parent(),
    )
    .map_err(|_| AppError::config("could not verify local event configuration provenance"))?;
    if !output.success {
        return Err(AppError::config(
            "could not verify local event configuration provenance",
        ));
    }
    let local_metadata = fs::metadata(path)
        .map_err(|_| AppError::config("could not inspect local event configuration"))?;
    if output.stdout.split(|byte| *byte == 0).any(|tracked| {
        tracked.eq_ignore_ascii_case(LOCAL_FILE.as_bytes())
            || tracked_path_aliases_local(path, tracked, &local_metadata)
    }) {
        return Err(AppError::config(
            "event endpoint selection requires an untracked .workctl.local.json",
        ));
    }
    Ok(())
}
fn ensure_ignored_local_config(path: &Path) -> Result<(), AppError> {
    let ignored = runner::run(
        "git",
        &["check-ignore", "--quiet", "--", LOCAL_FILE].map(str::to_owned),
        None,
        path.parent(),
    )
    .map_err(|_| AppError::config("could not verify local workflow ignore rule"))?;
    if !ignored.success {
        return Err(AppError::config(
            ".workctl.local.json must be ignored before local workflow approval",
        ));
    }
    Ok(())
}

#[cfg_attr(windows, allow(unused_variables))]
fn tracked_path_aliases_local(path: &Path, tracked: &[u8], local: &fs::Metadata) -> bool {
    if tracked.is_empty() || tracked.contains(&b'/') || tracked.contains(&b'\\') {
        return false;
    }
    #[cfg(unix)]
    {
        let candidate = path
            .parent()
            .map(|parent| parent.join(std::ffi::OsStr::from_bytes(tracked)));
        let Some(candidate) = candidate else {
            return false;
        };
        let Ok(metadata) = fs::metadata(candidate) else {
            return false;
        };
        local.dev() == metadata.dev() && local.ino() == metadata.ino()
    }
    #[cfg(windows)]
    {
        let Some(name) = std::str::from_utf8(tracked).ok() else {
            return false;
        };
        // Git accepts stream-qualified, trailing-dot/space, and short-name spellings on
        // Windows. Stable std lacks file-ID lookup here, so reject possible aliases.
        !name.is_ascii()
            || name.contains(':')
            || name.ends_with('.')
            || name.ends_with(' ')
            || name.contains('~')
    }
}

#[cfg(test)]
mod event_config_provenance_tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn tracked_case_variant_of_local_config_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "workctl-event-provenance-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(root.join(".WORKCTL.LOCAL.JSON"), "{}").unwrap();
        let status = Command::new("git")
            .args(["add", "--", ".WORKCTL.LOCAL.JSON"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(status.success());

        let error = ensure_untracked_event_config(&root.join(LOCAL_FILE)).unwrap_err();
        assert_eq!(
            error.message,
            "event endpoint selection requires an untracked .workctl.local.json"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tracked_files_sharing_local_config_identity_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "workctl-event-alias-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let local = root.join(LOCAL_FILE);
        fs::write(&local, "{}").unwrap();
        let alias = ".worKctl.local.json";
        if !root.join(alias).exists() {
            fs::hard_link(&local, root.join(alias)).unwrap();
        }
        let metadata = fs::metadata(&local).unwrap();
        assert!(tracked_path_aliases_local(
            &local,
            alias.as_bytes(),
            &metadata
        ));
        fs::remove_dir_all(root).unwrap();
    }
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
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        GithubMergeMethod, GithubProjectDefaults, OutputFormatSetting, Workflow, approve_workflow,
        load, workflow_approval_status, workflow_fingerprint,
    };
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

        fn git(&self, args: &[&str]) {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&self.0)
                    .status()
                    .expect("run fixture Git command")
                    .success()
            );
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn local_domain_provider_settings_override_matching_shared_values() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"codeProvider":"github","workItemProvider":"gitlab"}"#,
        )
        .expect("write shared config");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"codeProvider":"gitlab"}"#,
        )
        .expect("write local config");

        let config = load(&root.0).expect("load valid config");
        assert_eq!(config.code_provider, Some(Provider::Gitlab));
        assert_eq!(config.work_item_provider, Some(Provider::Gitlab));
    }

    #[test]
    fn decision_model_keys_merge_per_key_with_a_local_only_api_key() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"decisionModel":{"model":"laya","baseUrl":"http://127.0.0.1:11434/v1"}}"#,
        )
        .expect("write shared decision-model settings");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"decisionModel":{"model":"local/test-model","apiKey":"local-secret-token"}}"#,
        )
        .expect("write local decision-model settings");

        let settings = load(&root.0)
            .expect("load valid decision-model settings")
            .decision_model
            .expect("decision-model settings");
        assert_eq!(settings.model.as_deref(), Some("local/test-model"));
        assert_eq!(
            settings.base_url.as_deref(),
            Some("http://127.0.0.1:11434/v1")
        );
        assert_eq!(settings.api_key.as_deref(), Some("local-secret-token"));
        assert!(!format!("{settings:?}").contains("local-secret-token"));
    }

    #[test]
    fn shared_decision_model_api_key_is_rejected_without_echoing_it() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"decisionModel":{"model":"laya","apiKey":"shared-secret-token"}}"#,
        )
        .expect("write shared decision-model key");

        let error = load(&root.0).expect_err("shared API key must fail closed");
        assert_eq!(error.code, "config");
        assert!(!error.message.contains("shared-secret-token"));
    }

    #[test]
    fn unknown_decision_model_configuration_fails_closed() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"decisionModel":{"mode":"laya"}}"#,
        )
        .expect("write unknown decision-model key");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
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
    fn gitlab_and_output_defaults_parse_typed_values() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"gitlab":{"issue":{"labels":["bug"],"assignees":["alice"]},
"mr":{"labels":["backend"],"reviewers":["bob"],"targetBranch":"develop","draft":true}},
"output":{"format":"text"}}}"#,
        )
        .expect("write GitLab and output defaults");

        let config = load(&root.0).expect("load valid defaults");
        let defaults = config.defaults.expect("defaults");
        let gitlab = defaults.gitlab.expect("GitLab defaults");
        let issue = gitlab.issue.expect("issue defaults");
        assert_eq!(issue.labels, ["bug"]);
        assert_eq!(issue.assignees, ["alice"]);
        let mr = gitlab.mr.expect("merge-request defaults");
        assert_eq!(mr.labels, ["backend"]);
        assert_eq!(mr.reviewers, ["bob"]);
        assert_eq!(mr.target_branch.as_deref(), Some("develop"));
        assert!(mr.draft);
        assert!(matches!(
            defaults.output.expect("output defaults").format,
            Some(OutputFormatSetting::Text)
        ));
    }

    #[test]
    fn github_pr_create_defaults_and_list_limit_parse_typed_values() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"defaults":{"github":{"pr":{"labels":["bug"],"assignees":["octocat"],
"reviewers":["hubot"],"base":"develop","draft":true},"listLimit":50}}}"#,
        )
        .expect("write create defaults and list limit");

        let config = load(&root.0).expect("load valid defaults");
        let github = config
            .defaults
            .expect("defaults")
            .github
            .expect("GitHub defaults");
        assert_eq!(github.list_limit, Some(50));
        let pr = github.pr.expect("pull-request defaults");
        assert_eq!(pr.labels, ["bug"]);
        assert_eq!(pr.assignees, ["octocat"]);
        assert_eq!(pr.reviewers, ["hubot"]);
        assert_eq!(pr.base.as_deref(), Some("develop"));
        assert!(pr.draft);
        assert!(pr.merge_method.is_none());
        assert!(!pr.delete_branch);
    }

    #[test]
    fn out_of_range_and_blank_new_defaults_fail_closed() {
        let root = TempRoot::new();
        for contents in [
            r#"{"defaults":{"github":{"listLimit":0}}}"#,
            r#"{"defaults":{"github":{"listLimit":1001}}}"#,
            r#"{"defaults":{"github":{"pr":{"labels":[" "]}}}}"#,
            r#"{"defaults":{"github":{"pr":{"labels":["@auto"]}}}}"#,
            r#"{"defaults":{"github":{"pr":{"base":""}}}}"#,
            r#"{"defaults":{"gitlab":{"issue":{"assignees":[" "]}}}}"#,
            r#"{"defaults":{"gitlab":{"issue":{"labels":["@auto"]}}}}"#,
            r#"{"defaults":{"gitlab":{"mr":{"reviewers":["@auto"]}}}}"#,
            r#"{"defaults":{"gitlab":{"mr":{"targetBranch":" "}}}}"#,
            r#"{"defaults":{"output":{"format":"yaml"}}}"#,
            r#"{"defaults":{"output":{"style":"text"}}}"#,
            r#"{"defaults":{"gitlab":{"mergeRequests":{}}}}"#,
        ] {
            fs::write(root.0.join(".workctl.json"), contents).expect("write invalid config");
            assert_eq!(
                load(&root.0).expect_err(contents).code,
                "config",
                "config accepted: {contents}"
            );
        }
    }

    #[test]
    fn malformed_and_unknown_configuration_fails_closed() {
        let root = TempRoot::new();
        for contents in [
            r#"{"provider":"github"}"#,
            r#"{"defaults":{"github":{"issue":{"labelCandidates":["@auto"]}}}}"#,
            r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"fields":{"priority":42}}}}}}"#,
            r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"autoSelectFields":["Priority","Priority"]}}}}}"#,
            r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"autoSelectFields":[""]}}}}}"#,
        ] {
            fs::write(root.0.join(".workctl.json"), contents).expect("write invalid config");
            assert_eq!(
                load(&root.0).expect_err(contents).code,
                "config",
                "config accepted: {contents}"
            );
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
    #[test]
    fn rf_cfg_10_local_event_endpoint_selection_is_read_only_ids() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"events":{"endpoints":["sink"]}}"#,
        )
        .expect("write local event selection");
        let config = load(&root.0).expect("load local event selection");
        assert_eq!(config.events.unwrap().endpoints, vec!["sink".to_owned()]);
    }

    #[test]
    fn rf_cfg_10_staged_and_committed_local_event_selection_is_rejected() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"events":{"endpoints":["sink"]}}"#,
        )
        .expect("write tracked event selection");
        root.git(&["add", "--", ".workctl.local.json"]);
        let error = load(&root.0).expect_err("staged event selection must fail closed");
        assert_eq!(error.code, "config");
        assert!(error.message.contains("untracked .workctl.local.json"));
        root.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "Track local config",
        ]);
        assert_eq!(
            load(&root.0)
                .expect_err("committed event selection must fail closed")
                .code,
            "config"
        );
    }

    #[test]
    fn tracked_ordinary_project_configuration_remains_accepted() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workItemProvider":"github"}"#,
        )
        .expect("write shared config");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"codeProvider":"gitlab"}"#,
        )
        .expect("write local config without event selection");
        root.git(&["add", "--", ".workctl.json", ".workctl.local.json"]);
        let config = load(&root.0).expect("ordinary tracked config remains valid");
        assert_eq!(config.work_item_provider, Some(Provider::Github));
        assert_eq!(config.code_provider, Some(Provider::Gitlab));
        assert!(config.events.is_none());
    }

    #[test]
    fn rf_cfg_10_local_event_selection_fails_closed_without_git_provenance() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"events":{"endpoints":["sink"]}}"#,
        )
        .expect("write event selection outside a worktree");
        assert_eq!(
            load(&root.0)
                .expect_err("unverifiable opt-in must fail closed")
                .code,
            "config"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rf_cfg_10_tracked_local_event_config_cannot_reach_provider_mutation() {
        // Run real command dispatch in an isolated child so cwd and PATH never affect other tests.
        const CHILD_ROOT: &str = "WORKCTL_EVENT_PROVENANCE_TEST_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            use crate::cli::{ActiveCommand, GlobalArgs};
            use crate::config::ProviderSelection;
            use clap::FromArgMatches;

            std::env::set_current_dir(root).expect("enter isolated command worktree");
            let providers = ProviderSelection {
                code: Some(Provider::Github),
                work_items: Some(Provider::Github),
            };
            let matches = crate::cli::command_tree(providers)
                .try_get_matches_from([
                    "workctl",
                    "--repo",
                    "owner/repo",
                    "issue",
                    "create",
                    "--title",
                    "Must not mutate",
                ])
                .expect("parse mutation command");
            let globals = GlobalArgs::from_arg_matches(&matches).expect("parse global options");
            let command = ActiveCommand::from_matches(providers, &matches).expect("parse command");
            let error = crate::commands::execute(globals, command)
                .expect_err("tracked opt-in must stop command dispatch");
            assert_eq!(error.code, "config");
            assert!(error.message.contains("untracked .workctl.local.json"));
            return;
        }

        use std::os::unix::fs::PermissionsExt;

        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"events":{"endpoints":["sink"]}}"#,
        )
        .expect("write repository-controlled event selection");
        root.git(&["add", "--", ".workctl.local.json"]);
        let bin = root.0.join("bin");
        fs::create_dir(&bin).expect("create isolated provider directory");
        let provider = bin.join("gh");
        fs::write(
            &provider,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$WORKCTL_EVENT_PROVENANCE_PROVIDER_LOG\"\nexit 1\n",
        )
        .expect("write provider call sentinel");
        fs::set_permissions(&provider, fs::Permissions::from_mode(0o700))
            .expect("make provider sentinel executable");
        let log = root.0.join("provider.log");
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let output = Command::new(std::env::current_exe().expect("locate unit test executable"))
            .args([
                "--exact",
                "config::files::tests::rf_cfg_10_tracked_local_event_config_cannot_reach_provider_mutation",
                "--nocapture",
            ])
            .env(CHILD_ROOT, &root.0)
            .env("PATH", std::env::join_paths(paths).expect("compose isolated PATH"))
            .env("WORKCTL_EVENT_PROVENANCE_PROVIDER_LOG", &log)
            .output()
            .expect("execute isolated mutation command regression");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            !log.exists(),
            "tracked event selection reached the provider"
        );
    }

    #[test]
    fn rf_cfg_10_shared_configuration_cannot_enable_events() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"events":{"endpoints":["sink"]}}"#,
        )
        .expect("write shared event selection");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }
    #[test]
    fn rf_wf_1_shared_workflow_rules_parse_actions_and_provider_stage_mappings() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"stageMappings":{"github":{"Ready":{"field":"Status","value":"Ready"}},"gitlab":{"Ready":{"label":"stage::ready"}}},"rules":[{"on":"issueCreated","actions":[{"type":"setStage","stage":"Ready"},{"type":"setProjectField","field":"Priority","value":"High"},{"type":"addLabel","label":"triage"},{"type":"removeLabel","label":"backlog"},{"type":"addAssignee","assignee":"octocat"},{"type":"removeAssignee","assignee":"hubot"},{"type":"comment","body":"Reviewed"}]}]}}"#,
        )
        .expect("write workflow rules");
        let workflow = load(&root.0)
            .expect("parse approved workflow actions")
            .workflow
            .expect("workflow");
        assert_eq!(workflow.rules.len(), 1);
        assert_eq!(workflow.rules[0].actions.len(), 7);
        assert_eq!(workflow.stage_mappings.github["Ready"].field, "Status");
        assert_eq!(workflow.stage_mappings.github["Ready"].value, "Ready");
        assert_eq!(
            workflow.stage_mappings.gitlab["Ready"].label,
            "stage::ready"
        );
    }

    #[test]
    fn rf_wf_1_workflow_rules_reject_unknown_actions_and_empty_targets() {
        let root = TempRoot::new();
        for contents in [
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"runShell","command":"echo no"}]}]}}"#,
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":" "}]}]}}"#,
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[]}]}}"#,
        ] {
            fs::write(root.0.join(".workctl.json"), contents).unwrap();
            assert_eq!(load(&root.0).unwrap_err().code, "config");
        }
    }
    #[test]
    fn rf_wf_1_rejects_pr_workflow_expansion_over_bounded_reference_fanout() {
        let root = TempRoot::new();
        let actions = vec![serde_json::json!({"type":"addLabel","label":"triage"}); 16];
        let workflow = serde_json::json!({
            "workflow": {
                "rules": [
                    {"on":"pullRequestMerged","actions":actions},
                    {"on":"pullRequestMerged","actions":actions}
                ]
            }
        });
        fs::write(
            root.0.join(".workctl.json"),
            serde_json::to_vec(&workflow).unwrap(),
        )
        .unwrap();
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[test]
    fn rf_wf_1_shared_config_cannot_store_workflow_approval() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflowApproval":"approved"}"#,
        )
        .unwrap();
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }
    #[test]
    fn rf_wf_1_workflow_fingerprint_tracks_rules_and_repository_scope() {
        let first: Workflow = serde_json::from_str(
            r#"{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}"#,
        )
        .unwrap();
        let changed: Workflow = serde_json::from_str(
            r#"{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"urgent"}]}]}"#,
        )
        .unwrap();
        let approved = workflow_fingerprint(&first, "github:owner/repo", None).unwrap();
        assert_eq!(
            approved,
            workflow_fingerprint(&first, "github:owner/repo", None).unwrap()
        );
        assert_ne!(
            approved,
            workflow_fingerprint(&first, "github:owner/other", None).unwrap()
        );
        assert_ne!(
            approved,
            workflow_fingerprint(&changed, "github:owner/repo", None).unwrap()
        );
        let project = GithubProjectDefaults {
            url: "https://github.com/orgs/owner/projects/3".into(),
            repositories: vec!["owner/repo".into()],
            fields: std::collections::BTreeMap::new(),
            auto_select_fields: Vec::new(),
        };
        assert_ne!(
            approved,
            workflow_fingerprint(&first, "github:owner/repo", Some(&project)).unwrap()
        );
        assert_eq!(approved.len(), 64);
    }
    #[test]
    fn rf_wf_1_local_approval_is_invalidated_by_shared_rule_edits() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(root.0.join(".gitignore"), ".workctl.local.json\n").expect("ignore local config");
        let rules = |label: &str| {
            format!(
                r#"{{"workflow":{{"rules":[{{"on":"issueCreated","actions":[{{"type":"addLabel","label":"{label}"}}]}}]}}}}"#
            )
        };
        fs::write(root.0.join(".workctl.json"), rules("triage")).expect("write shared workflow");
        approve_workflow(&root.0).expect("approve workflow");
        let config = load(&root.0).expect("load approved workflow");
        assert!(workflow_approval_status(&config, &root.0).unwrap());

        fs::write(root.0.join(".workctl.json"), rules("urgent")).expect("edit shared workflow");
        let config = load(&root.0).expect("load changed workflow");
        assert!(!workflow_approval_status(&config, &root.0).unwrap());
    }

    #[test]
    fn rf_wf_1_approval_preserves_existing_synthetic_credentials() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(root.0.join(".gitignore"), ".workctl.local.json\n").expect("ignore local config");
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .expect("write shared workflow");
        let path = root.0.join(".workctl.local.json");
        fs::write(
            &path,
            r#"{"decisionModel":{"apiKey":"synthetic-approval-key"}}"#,
        )
        .expect("write synthetic local credential");

        let fingerprint = approve_workflow(&root.0).expect("approve workflow");
        let local: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read fixture config"))
                .expect("parse approved fixture config");
        assert_eq!(local["decisionModel"]["apiKey"], "synthetic-approval-key");
        assert_eq!(local["workflowApproval"], fingerprint);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn rf_wf_1_approval_file_creation_never_reopens_an_existing_path() {
        let root = TempRoot::new();
        let path = root.0.join(".workctl.local.fixture.tmp");
        let bytes = br#"{"decisionModel":{"apiKey":"synthetic-approval-key"}}"#;
        fs::write(&path, bytes).expect("write existing synthetic fixture");

        let error = super::create_workflow_approval_file(&path)
            .expect_err("approval temporary file must be new");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(path).expect("read untouched fixture"), bytes);
    }

    #[test]
    fn rf_wf_1_local_approval_requires_a_sha256_fingerprint() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(root.0.join(".gitignore"), ".workctl.local.json\n").expect("ignore local config");
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .expect("write shared workflow");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"workflowApproval":"approved"}"#,
        )
        .expect("write malformed approval");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }
    #[test]
    fn rf_wf_1_approval_fails_without_ignore_rule_and_does_not_write_local_opt_in() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .expect("write shared workflow");
        assert_eq!(approve_workflow(&root.0).unwrap_err().code, "config");
        assert!(!root.0.join(".workctl.local.json").exists());
    }
    #[test]
    fn rf_wf_1_tracked_local_config_cannot_authorize_workflow() {
        let root = TempRoot::new();
        root.git(&["init", "--quiet"]);
        fs::write(root.0.join(".gitignore"), ".workctl.local.json\n").expect("ignore local config");
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .expect("write shared workflow");
        fs::write(root.0.join(".workctl.local.json"), "{}").expect("write local config");
        root.git(&["add", "--force", "--", ".workctl.local.json"]);
        assert_eq!(approve_workflow(&root.0).unwrap_err().code, "config");
        assert_eq!(
            fs::read_to_string(root.0.join(".workctl.local.json")).unwrap(),
            "{}"
        );
    }
    #[test]
    fn rf_wf_1_approval_is_bound_to_repository_scope() {
        let first = TempRoot::new();
        let second = TempRoot::new();
        for root in [&first, &second] {
            root.git(&["init", "--quiet"]);
            fs::write(root.0.join(".gitignore"), ".workctl.local.json\n")
                .expect("ignore local config");
            fs::write(
                root.0.join(".workctl.json"),
                r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
            )
            .expect("write shared workflow");
        }
        approve_workflow(&first.0).expect("approve first repository");
        let first_config = load(&first.0).expect("load first approval");
        let approval = first_config.workflow_approval.unwrap();
        fs::write(
            second.0.join(".workctl.local.json"),
            serde_json::json!({"workflowApproval": approval}).to_string(),
        )
        .expect("copy approval into second repository");
        let second_config = load(&second.0).expect("load second repository");
        assert!(!workflow_approval_status(&second_config, &second.0).unwrap());
    }
}
