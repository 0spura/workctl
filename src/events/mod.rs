mod dispatch;
#[cfg(windows)]
mod private_windows;
mod store;
pub(crate) mod work_tracking;
mod workflow;
#[cfg(windows)]
pub(crate) use private_windows::create_private_file_new;

use rusqlite::OptionalExtension;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::GlobalArgs;
use crate::config;
use crate::domain::AppError;
use crate::output::{MutationAction, SuccessOutput};
use dispatch::{CloudEvent, ConfiguredEndpoints, EventData};

pub(crate) fn validate_for_cwd(
    cwd: &std::path::Path,
    provider: &str,
    explicit_repo: Option<&str>,
) -> Result<ConfiguredEndpoints, AppError> {
    let project_config = config::load_for_cwd(cwd)?;
    let mut consumers = dispatch::validate_config(project_config.events.clone())?;
    let root = config::repository_root(cwd).ok();
    let provider_kind = match provider {
        "github" => config::Provider::Github,
        "gitlab" => config::Provider::Gitlab,
        _ => return Ok(consumers),
    };
    consumers.repository = config::resolve_domain_context(provider_kind, explicit_repo, cwd).ok();
    if project_config.workflow.is_some() {
        let root = root
            .as_deref()
            .ok_or_else(|| AppError::context("could not determine repository scope"))?;
        if config::workflow_approval_status(&project_config, root)? {
            let workflow = project_config
                .workflow
                .as_ref()
                .ok_or_else(|| AppError::config("workflow approval is invalid"))?;
            validate_workflow_provider(workflow, provider, &project_config)?;
            consumers.github_project = project_config
                .defaults
                .as_ref()
                .and_then(|defaults| defaults.github.as_ref())
                .and_then(|github| github.issue.as_ref())
                .and_then(|issue| issue.project.clone());
            consumers.workflow = Some(workflow.clone());
            consumers.workflow_scope = Some(
                std::fs::canonicalize(root)
                    .map_err(|_| AppError::context("could not determine repository scope"))?
                    .to_string_lossy()
                    .into_owned(),
            );
            consumers.workflow_fingerprint = project_config.workflow_approval.clone();
        }
    }
    Ok(consumers)
}

fn validate_workflow_provider(
    workflow: &config::Workflow,
    provider: &str,
    project_config: &config::Config,
) -> Result<(), AppError> {
    for rule in &workflow.rules {
        for action in &rule.actions {
            match action {
                config::WorkflowAction::SetStage { stage } => {
                    let mapped = match provider {
                        "github" => workflow.stage_mappings.github.contains_key(stage),
                        "gitlab" => workflow.stage_mappings.gitlab.contains_key(stage),
                        _ => false,
                    };
                    if !mapped {
                        return Err(AppError::config(
                            "workflow stage has no mapping for the selected provider",
                        ));
                    }
                }
                config::WorkflowAction::SetProjectField { .. } if provider != "github" => {
                    return Err(AppError::config(
                        "workflow Project fields are unsupported by the selected provider",
                    ));
                }
                config::WorkflowAction::SetProjectField { .. }
                    if project_config
                        .defaults
                        .as_ref()
                        .and_then(|defaults| defaults.github.as_ref())
                        .and_then(|github| github.issue.as_ref())
                        .and_then(|issue| issue.project.as_ref())
                        .is_none() =>
                {
                    return Err(AppError::config(
                        "workflow Project field requires a configured GitHub Project",
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(())
}
pub(crate) fn disabled() -> ConfiguredEndpoints {
    ConfiguredEndpoints {
        selected: Vec::new(),
        workflow: None,
        github_project: None,
        workflow_scope: None,
        workflow_fingerprint: None,
        repository: None,
    }
}

pub(crate) fn record_success(
    _globals: &GlobalArgs,
    provider: &str,
    output: &SuccessOutput,
    endpoints: &ConfiguredEndpoints,
) {
    let Some(provider_repository) = endpoints.repository.as_deref() else {
        return;
    };
    let Some(repository) = canonical_repository(provider_repository) else {
        return;
    };
    let workflow_authorized = workflow_approval_is_current(endpoints);
    let mut events = output_events(output, provider, &repository);
    if workflow_authorized {
        hydrate_pr_references(&mut events);
    }
    let actions = plan_workflow_actions(
        &events,
        endpoints,
        provider,
        provider_repository,
        workflow_authorized,
    );
    enqueue(&events, &actions, endpoints, workflow_authorized);
}

pub(crate) fn record_partial(
    _globals: &GlobalArgs,
    provider: &str,
    error: &AppError,
    endpoints: &ConfiguredEndpoints,
) {
    if error.code != "partial_success" {
        enqueue(&[], &[], endpoints, false);
        return;
    }
    let Some(provider_repository) = endpoints.repository.as_deref() else {
        enqueue(&[], &[], endpoints, false);
        return;
    };
    let Some(repository) = canonical_repository(provider_repository) else {
        enqueue(&[], &[], endpoints, false);
        return;
    };
    let mut events = Vec::new();
    let mut emitted = std::collections::HashSet::new();
    if let Some(fact) = error.partial_event {
        if let Some(event) = build_event(
            provider,
            &repository,
            fact.resource,
            fact.number.to_string(),
            fact.operation,
            "partial",
        ) {
            events.push(event);
        }
    } else if let Some(details) = error.details.as_ref() {
        collect_partial_events(
            details,
            provider,
            &repository,
            &mut emitted,
            &mut events,
            "updated",
        );
    }
    if let (Some(fact), Some(details)) = (error.partial_event, error.details.as_ref()) {
        if fact.resource == "pull_request" {
            attach_partial_pr_references(details, &repository, fact.number, &mut events);
        }
    }
    let workflow_authorized = workflow_approval_is_current(endpoints);
    if workflow_authorized {
        hydrate_pr_references(&mut events);
    }
    let actions = plan_workflow_actions(
        &events,
        endpoints,
        provider,
        provider_repository,
        workflow_authorized,
    );
    enqueue(&events, &actions, endpoints, workflow_authorized);
}

fn attach_partial_pr_references(
    details: &serde_json::Value,
    repository: &str,
    number: u64,
    events: &mut [CloudEvent],
) {
    let Some(event) = events.iter_mut().find(|event| {
        event.data.resource == "pull_request" && event.data.resource_id == number.to_string()
    }) else {
        return;
    };
    let collect = |key: &str| {
        details
            .get(key)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter_map(|value| value.parse::<crate::domain::ClosingReference>().ok())
            .map(|reference| dispatch::RelatedIssueReference {
                repository: reference
                    .repo
                    .unwrap_or_else(|| repository.to_owned())
                    .to_ascii_lowercase(),
                number: reference.number,
            })
            .collect::<Vec<_>>()
    };
    event.data.related_issue_references = collect("relatedIssueReferences");
    event.data.removed_related_issue_references = collect("removedRelatedIssueReferences");
}

fn collect_partial_events(
    value: &serde_json::Value,
    provider: &str,
    repository: &str,
    emitted: &mut std::collections::HashSet<(String, String)>,
    events: &mut Vec<CloudEvent>,
    inherited_action: &str,
) {
    let action = inherited_action;
    if let Some(resource) = value.get("resource") {
        collect_partial_events(resource, provider, repository, emitted, events, action);
    }
    let kind = match value.get("type").and_then(serde_json::Value::as_str) {
        Some("pull_request") => "pull_request",
        Some("issue") | Some("issue_batch") => "issue",
        _ => return,
    };
    if let Some(items) = value.get("issues").and_then(serde_json::Value::as_array) {
        for item in items {
            let id = item
                .get("issue")
                .and_then(|issue| issue.get("number"))
                .and_then(serde_json::Value::as_u64)
                .or_else(|| item.get("number").and_then(serde_json::Value::as_u64));
            if let Some(id) = id {
                push_partial(provider, repository, kind, id, action, emitted, events);
            }
        }
    } else if let Some(id) = value.get("number").and_then(serde_json::Value::as_u64) {
        push_partial(provider, repository, kind, id, action, emitted, events);
    }
    if let Some(details) = value
        .get("failure")
        .and_then(|failure| failure.get("details"))
    {
        collect_partial_events(details, provider, repository, emitted, events, action);
    }
}

fn push_partial(
    provider: &str,
    repository: &str,
    kind: &str,
    id: u64,
    action: &str,
    emitted: &mut std::collections::HashSet<(String, String)>,
    events: &mut Vec<CloudEvent>,
) {
    if emitted.insert((kind.to_owned(), id.to_string())) {
        if let Some(event) = build_event(
            provider,
            repository,
            kind,
            id.to_string(),
            action,
            "partial",
        ) {
            events.push(event);
        }
    }
}

fn workflow_approval_is_current(endpoints: &ConfiguredEndpoints) -> bool {
    let (Some(scope), Some(expected_fingerprint)) = (
        endpoints.workflow_scope.as_deref(),
        endpoints.workflow_fingerprint.as_deref(),
    ) else {
        return false;
    };
    if endpoints.workflow.is_none() || expected_fingerprint.is_empty() {
        return false;
    }
    let root = std::path::Path::new(scope);
    let current = config::load_for_cwd(root)
        .and_then(|project_config| {
            if config::workflow_approval_status(&project_config, root)?
                && project_config.workflow_approval.as_deref() == Some(expected_fingerprint)
            {
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .unwrap_or(false);
    if !current {
        eprintln!("workctl: warning: workflow approval changed; local actions paused");
    }
    current
}

fn enqueue(
    events: &[CloudEvent],
    actions: &[dispatch::PlannedAction],
    endpoints: &ConfiguredEndpoints,
    workflow_authorized: bool,
) {
    if events.is_empty() || endpoints.is_empty() {
        return;
    }
    let path = match state_outbox_path() {
        Ok(path) => path,
        Err(_) => {
            safe_warning();
            return;
        }
    };
    if dispatch::enqueue_and_dispatch(&path, endpoints, events, actions, workflow_authorized)
        .is_err()
    {
        safe_warning();
    }
}

fn output_events(output: &SuccessOutput, provider: &str, repository: &str) -> Vec<CloudEvent> {
    let mut events = Vec::new();
    let mut add = |resource: &str, id: String, action: &str| {
        if let Some(event) = build_event(provider, repository, resource, id, action, "confirmed") {
            events.push(event);
        }
    };
    match output {
        SuccessOutput::IssueMutation(m) if !m.unchanged => add(
            "issue",
            m.issue.number.to_string(),
            mutation_action(m.action),
        ),
        SuccessOutput::IssueMutations(ms) => {
            for m in ms {
                if !m.unchanged {
                    add(
                        "issue",
                        m.issue.number.to_string(),
                        mutation_action(m.action),
                    );
                }
            }
        }
        SuccessOutput::PullRequestMutation(m) if !m.unchanged => add(
            "pull_request",
            m.pull_request.number.to_string(),
            mutation_action(m.action),
        ),
        SuccessOutput::IssueState { number, state } => add(
            "issue",
            number.to_string(),
            if state == "closed" {
                "closed"
            } else {
                "reopened"
            },
        ),
        SuccessOutput::Comment { number, target } => add(
            if target == "pr" {
                "pull_request"
            } else {
                "issue"
            },
            number.to_string(),
            "commented",
        ),
        SuccessOutput::Merge {
            number,
            confirmed_merged,
            ..
        } => add(
            "pull_request",
            number.to_string(),
            if *confirmed_merged {
                "merged"
            } else {
                "updated"
            },
        ),
        SuccessOutput::Review { number, event } => add(
            "pull_request",
            number.to_string(),
            if event == "approve" {
                "review_approved"
            } else {
                "review_submitted"
            },
        ),
        SuccessOutput::Ready { number, draft } => add(
            "pull_request",
            number.to_string(),
            if *draft { "updated" } else { "ready" },
        ),
        SuccessOutput::UpdateBranch { number, .. } => {
            add("pull_request", number.to_string(), "updated")
        }
        SuccessOutput::ConversationLock { number, target, .. } => add(
            if target == "pr" {
                "pull_request"
            } else {
                "issue"
            },
            number.to_string(),
            "updated",
        ),
        SuccessOutput::Revert { pull_request, .. } => {
            add("pull_request", pull_request.to_string(), "created")
        }
        SuccessOutput::State { number, .. } => add("issue", number.to_string(), "updated"),
        SuccessOutput::GitLabState(s) => add(
            "pull_request",
            s.number.to_string(),
            if s.state == "closed" {
                "closed"
            } else {
                "reopened"
            },
        ),
        SuccessOutput::GitLabAction { number, action } => {
            let action = match action.as_str() {
                "merged" => "merged",
                "approve" => "review_approved",
                "note added" => "commented",
                "note updated" => "updated",
                _ => "updated",
            };
            add("pull_request", number.to_string(), action);
        }
        SuccessOutput::LinkedBranch { number, .. } => {
            add("issue", number.to_string(), "linked_branch_created")
        }
        _ => {}
    }
    if let SuccessOutput::PullRequestMutation(mutation) = output {
        if !mutation.unchanged {
            if let Some(event) = events.last_mut() {
                event.data.related_issue_references = mutation
                    .related_issue_references
                    .iter()
                    .map(|reference| dispatch::RelatedIssueReference {
                        repository: reference
                            .repo
                            .clone()
                            .unwrap_or_else(|| repository.to_owned()),
                        number: reference.number,
                    })
                    .collect();
                event.data.removed_related_issue_references = mutation
                    .removed_related_issue_references
                    .iter()
                    .map(|reference| dispatch::RelatedIssueReference {
                        repository: reference
                            .repo
                            .clone()
                            .unwrap_or_else(|| repository.to_owned()),
                        number: reference.number,
                    })
                    .collect();
            }
        }
    }
    events
}
fn hydrate_pr_references(events: &mut [CloudEvent]) {
    let Some(source) = events
        .iter()
        .find(|event| event.data.resource == "pull_request")
        .map(|event| event.source.to_ascii_lowercase())
    else {
        return;
    };
    let Ok(path) = state_outbox_path() else {
        return;
    };
    let Ok(connection) = store::open(&path) else {
        return;
    };
    for event in events
        .iter_mut()
        .filter(|event| event.data.resource == "pull_request")
    {
        let refs = connection
            .prepare(
                "SELECT repository,number FROM workflow_pr_references
                 WHERE source=?1 AND subject=?2 ORDER BY repository,number LIMIT 33",
            )
            .and_then(|mut statement| {
                statement
                    .query_map(rusqlite::params![source, event.subject], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            u64::try_from(row.get::<_, i64>(1)?).map_err(|_| {
                                rusqlite::Error::IntegralValueOutOfRange(1, i64::MIN)
                            })?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()
            });
        let Ok(refs) = refs else {
            event.data.effective_related_issue_references = Some(Vec::new());
            continue;
        };
        if refs.len() > config::MAX_WORKFLOW_PR_REFERENCES {
            event.data.effective_related_issue_references = Some(Vec::new());
            continue;
        }
        let mut active: std::collections::BTreeSet<_> = refs
            .into_iter()
            .map(|(repository, number)| (repository.to_ascii_lowercase(), number))
            .collect();
        let additions = serde_json::Value::Array(
            event
                .data
                .related_issue_references
                .iter()
                .map(|reference| {
                    serde_json::json!({
                        "repository": reference.repository,
                        "number": reference.number
                    })
                })
                .collect(),
        );
        let removals = serde_json::Value::Array(
            event
                .data
                .removed_related_issue_references
                .iter()
                .map(|reference| {
                    serde_json::json!({
                        "repository": reference.repository,
                        "number": reference.number
                    })
                })
                .collect(),
        );
        apply_reference_delta(&mut active, Some(&additions), true);
        apply_reference_delta(&mut active, Some(&removals), false);
        if active.len() > config::MAX_WORKFLOW_PR_REFERENCES {
            event.data.effective_related_issue_references = Some(Vec::new());
            continue;
        }
        event.data.effective_related_issue_references = Some(
            active
                .into_iter()
                .map(|(repository, number)| dispatch::RelatedIssueReference { repository, number })
                .collect(),
        );
    }
}

fn apply_reference_delta(
    active: &mut std::collections::BTreeSet<(String, u64)>,
    values: Option<&serde_json::Value>,
    add: bool,
) {
    let Some(values) = values.and_then(serde_json::Value::as_array) else {
        return;
    };
    for value in values {
        let (Some(repository), Some(number)) = (
            value
                .get("repository")
                .and_then(serde_json::Value::as_str)
                .filter(|repository| !repository.is_empty()),
            value
                .get("number")
                .and_then(serde_json::Value::as_u64)
                .filter(|number| *number > 0),
        ) else {
            continue;
        };
        let key = (repository.to_ascii_lowercase(), number);
        if add {
            active.insert(key);
        } else {
            active.remove(&key);
        }
    }
}

fn plan_workflow_actions(
    events: &[CloudEvent],
    consumers: &ConfiguredEndpoints,
    provider: &str,
    repository: &str,
    workflow_authorized: bool,
) -> Vec<dispatch::PlannedAction> {
    if !workflow_authorized {
        return Vec::new();
    }
    let Some(workflow) = consumers.workflow.as_ref() else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    if provider == "gitlab"
        && workflow
            .work_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.log_gitlab_time_spent)
    {
        let path = state_outbox_path().ok();
        if let Some(path) = path {
            if let Ok(connection) = store::open(&path) {
                for event in events.iter().filter(|event| {
                    event.data.resource == "issue"
                        && event.data.operation == "closed"
                        && event.data.outcome == "confirmed"
                }) {
                    let session = connection.query_row(
                        "SELECT id,active_millis FROM work_tracking_sessions WHERE scope=?1 AND approval_fingerprint=?2 AND provider='gitlab' AND repository=?3 AND issue_id=?4 AND state='active' ORDER BY started_at_ms DESC LIMIT 1",
                        rusqlite::params![consumers.workflow_scope.as_deref().unwrap_or_default(), consumers.workflow_fingerprint.as_deref().unwrap_or_default(), repository, event.data.resource_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                    ).optional().ok().flatten();
                    if let Some((session_id, millis)) = session {
                        actions.push(dispatch::PlannedAction {
                            event_id: event.id.clone(),
                            ordinal: actions.len() as i64,
                            provider: "gitlab".to_owned(),
                            repository: repository.to_owned(),
                            target_resource: "issue".to_owned(),
                            target_id: event.data.resource_id.clone(),
                            action: crate::config::WorkflowAction::LogTimeSpent {
                                seconds: u64::try_from(millis.max(0) / 1000).unwrap_or_default(),
                                session_id,
                                finished_at_ms: work_tracking::now_millis(),
                            },
                            scope: consumers.workflow_scope.clone().unwrap_or_default(),
                            approval_fingerprint: consumers
                                .workflow_fingerprint
                                .clone()
                                .unwrap_or_default(),
                            created_at: event.time.clone(),
                        });
                    }
                }
            }
        }
    }
    for event in events {
        if !matches!(event.data.outcome.as_str(), "confirmed" | "partial") {
            continue;
        }
        let trigger = match (event.data.resource.as_str(), event.data.operation.as_str()) {
            ("issue", "created") => Some(crate::config::WorkflowTrigger::IssueCreated),
            ("issue", "linked_branch_created") => {
                Some(crate::config::WorkflowTrigger::LinkedBranchCreated)
            }
            ("pull_request", "created") => Some(crate::config::WorkflowTrigger::PullRequestCreated),
            ("pull_request", "merged") => Some(crate::config::WorkflowTrigger::PullRequestMerged),
            ("pull_request", "review_approved") => {
                Some(crate::config::WorkflowTrigger::ReviewApproved)
            }
            _ => None,
        };
        let Some(trigger) = trigger else {
            continue;
        };
        let targets: Vec<(&str, String, String)> = if event.data.resource == "pull_request" {
            event
                .data
                .effective_related_issue_references
                .as_ref()
                .unwrap_or(&event.data.related_issue_references)
                .iter()
                .filter(|reference| reference.number > 0 && !reference.repository.is_empty())
                .map(|reference| {
                    (
                        "issue",
                        reference.repository.clone(),
                        reference.number.to_string(),
                    )
                })
                .collect()
        } else {
            vec![(
                event.data.resource.as_str(),
                repository.to_owned(),
                event.data.resource_id.clone(),
            )]
        };
        for rule in workflow.rules.iter().filter(|rule| rule.on == trigger) {
            for (target_resource, target_repository, target_id) in &targets {
                for action in &rule.actions {
                    actions.push(dispatch::PlannedAction {
                        event_id: event.id.clone(),
                        ordinal: actions.len() as i64,
                        provider: provider.to_owned(),
                        repository: target_repository.clone(),
                        target_resource: (*target_resource).to_owned(),
                        target_id: target_id.clone(),
                        action: action.clone(),
                        scope: consumers.workflow_scope.clone().unwrap_or_default(),
                        approval_fingerprint: consumers
                            .workflow_fingerprint
                            .clone()
                            .unwrap_or_default(),
                        created_at: event.time.clone(),
                    });
                }
            }
        }
    }
    actions
}

fn mutation_action(action: MutationAction) -> &'static str {
    match action {
        MutationAction::Created => "created",
        MutationAction::Updated => "updated",
        MutationAction::Closed => "closed",
        MutationAction::Reopened => "reopened",
    }
}

fn build_event(
    provider: &str,
    repo: &str,
    resource: &str,
    id: String,
    action: &str,
    outcome: &str,
) -> Option<CloudEvent> {
    let repository = canonical_repository(repo)?;
    let time = utc_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs());
    Some(CloudEvent {
        specversion: "1.0",
        id: uuid::Uuid::new_v4().to_string(),
        source: format!("urn:workctl:{provider}:{repository}"),
        event_type: format!("workctl.{resource}.{action}"),
        subject: format!("/{resource}/{id}"),
        time,
        datacontenttype: "application/json",
        data: EventData {
            schema_version: 1,
            repository,
            resource: resource.to_owned(),
            resource_id: id,
            operation: action.to_owned(),
            action: action.to_owned(),
            outcome: outcome.to_owned(),
            related_issue_references: Vec::new(),
            removed_related_issue_references: Vec::new(),
            effective_related_issue_references: None,
        },
    })
}

fn canonical_repository(repository: &str) -> Option<String> {
    if let Some(rest) = repository.strip_prefix("https://") {
        let (_, path) = rest.split_once('/')?;
        return Some(
            path.trim_end_matches('/')
                .strip_suffix(".git")
                .unwrap_or(path.trim_end_matches('/'))
                .to_owned(),
        );
    }
    Some(repository.to_owned())
}

fn utc_timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3600,
        day_seconds % 3600 / 60,
        day_seconds % 60
    )
}

fn state_outbox_path() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "macos")]
    let dir = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/workctl/state"));
    #[cfg(target_os = "windows")]
    let dir =
        std::env::var_os("LOCALAPPDATA").map(|home| PathBuf::from(home).join("workctl/state"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|home| home.join("workctl"));
    let path = dir
        .map(|dir| dir.join("events.sqlite3"))
        .ok_or_else(|| AppError::config("mutation event state path is unavailable"))?;
    if !path.is_absolute() {
        return Err(AppError::config("mutation event state path is invalid"));
    }
    Ok(path)
}
pub(crate) fn validate_pr_reference_delta(
    endpoints: &mut ConfiguredEndpoints,
    repository: &str,
    number: Option<u64>,
    additions: &[crate::domain::ClosingReference],
    removals: &[crate::domain::ClosingReference],
) -> Result<(), AppError> {
    if endpoints.workflow.is_none() || (additions.is_empty() && removals.is_empty()) {
        return Ok(());
    }
    if additions.len() > config::MAX_WORKFLOW_PR_REFERENCES
        || removals.len() > config::MAX_WORKFLOW_PR_REFERENCES
    {
        return Err(AppError::invalid_input(
            "pull request reference limit exceeded",
        ));
    }
    let canonical_repo = canonical_repository(repository)
        .ok_or_else(|| AppError::invalid_input("pull request repository is invalid"))?
        .to_ascii_lowercase();
    let mut active = std::collections::BTreeSet::new();
    if let Some(number) = number {
        let path =
            match state_outbox_path().and_then(|path| store::open(&path).map(|db| (path, db))) {
                Ok((_, connection)) => connection,
                Err(_) => {
                    endpoints.workflow = None;
                    endpoints.workflow_scope = None;
                    endpoints.workflow_fingerprint = None;
                    return Ok(());
                }
            };
        let subject = format!("/pull_request/{number}");
        let source = format!("urn:workctl:github:{canonical_repo}");
        let refs = path
            .prepare(
                "SELECT repository,number FROM workflow_pr_references
                 WHERE source=?1 AND subject=?2 ORDER BY repository,number LIMIT 33",
            )
            .and_then(|mut statement| {
                statement
                    .query_map(rusqlite::params![source, subject], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            u64::try_from(row.get::<_, i64>(1)?).map_err(|_| {
                                rusqlite::Error::IntegralValueOutOfRange(1, i64::MIN)
                            })?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()
            });
        let Ok(refs) = refs else {
            endpoints.workflow = None;
            endpoints.workflow_scope = None;
            endpoints.workflow_fingerprint = None;
            return Ok(());
        };
        active.extend(
            refs.into_iter()
                .map(|(repository, number)| (repository.to_ascii_lowercase(), number)),
        );
    }
    for reference in additions {
        active.insert((
            reference
                .repo
                .as_deref()
                .unwrap_or(&canonical_repo)
                .to_ascii_lowercase(),
            reference.number,
        ));
    }
    for reference in removals {
        active.remove(&(
            reference
                .repo
                .as_deref()
                .unwrap_or(&canonical_repo)
                .to_ascii_lowercase(),
            reference.number,
        ));
    }
    if active.len() > config::MAX_WORKFLOW_PR_REFERENCES {
        return Err(AppError::invalid_input(
            "pull request reference limit exceeded",
        ));
    }
    Ok(())
}

fn safe_warning() {
    eprintln!("workctl: warning: mutation event could not be queued");
}
pub(crate) fn workflow_action_counts(
    scope: &str,
    approval_fingerprint: &str,
) -> Result<std::collections::BTreeMap<String, u64>, AppError> {
    if approval_fingerprint.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let path = state_outbox_path()?;
    let connection = store::open(&path)?;
    let mut statement = connection
        .prepare(
            "SELECT state,COUNT(*) FROM workflow_actions
             WHERE scope=?1 AND approval_fingerprint=?2 GROUP BY state",
        )
        .map_err(|_| AppError::config("could not inspect workflow status"))?;
    let rows = statement
        .query_map(rusqlite::params![scope, approval_fingerprint], |row| {
            Ok((
                row.get::<_, String>(0)?,
                u64::try_from(row.get::<_, i64>(1)?).unwrap_or_default(),
            ))
        })
        .map_err(|_| AppError::config("could not inspect workflow status"))?;
    rows.collect::<Result<std::collections::BTreeMap<_, _>, _>>()
        .map_err(|_| AppError::config("could not inspect workflow status"))
}

pub(crate) fn list_workflow_actions(
    scope: &str,
    approval_fingerprint: &str,
    limit: u8,
) -> Result<Vec<crate::output::WorkflowActionSummary>, AppError> {
    let path = state_outbox_path()?;
    let connection = store::open(&path)?;
    let mut statement = connection
        .prepare(
            "SELECT id,state,action,target_resource,target_id
             FROM workflow_actions WHERE scope=?1 AND approval_fingerprint=?2
             ORDER BY created_at,event_id,ordinal
             LIMIT ?3",
        )
        .map_err(|_| AppError::config("could not inspect workflow actions"))?;
    let rows = statement
        .query_map(
            rusqlite::params![scope, approval_fingerprint, i64::from(limit)],
            |row| {
                let action: String = row.get(2)?;
                let action_type = serde_json::from_str::<serde_json::Value>(&action)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "unknown".to_owned());
                Ok(crate::output::WorkflowActionSummary {
                    id: row.get(0)?,
                    state: row.get(1)?,
                    action: action_type,
                    target_resource: row.get(3)?,
                    target_id: row.get(4)?,
                })
            },
        )
        .map_err(|_| AppError::config("could not inspect workflow actions"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::config("could not inspect workflow actions"))
}

#[derive(Clone, Copy)]
pub(crate) enum ActionResolution {
    Applied,
    Retry,
}

pub(crate) fn resolve_workflow_action(
    id: &str,
    resolution: ActionResolution,
    scope: &str,
    approval_fingerprint: &str,
) -> Result<String, AppError> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::invalid_input("workflow action ID is invalid"));
    }
    let path = state_outbox_path()?;
    let mut connection = store::open(&path)?;
    let transaction = connection
        .transaction()
        .map_err(|_| AppError::config("could not resolve workflow action"))?;
    let now = utc_timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
    let (state, query) = match resolution {
        ActionResolution::Applied => (
            "completed",
            "UPDATE workflow_actions SET state=?1,safe_error=NULL,updated_at=?2
             WHERE id=?3 AND scope=?4 AND approval_fingerprint=?5 AND state='uncertain'",
        ),
        ActionResolution::Retry => (
            "pending",
            "UPDATE workflow_actions SET state=?1,safe_error=NULL,updated_at=?2
             WHERE id=?3 AND scope=?4 AND approval_fingerprint=?5 AND state IN ('failed','uncertain')",
        ),
    };
    let changed = transaction
        .execute(
            query,
            rusqlite::params![state, now, id, scope, approval_fingerprint],
        )
        .map_err(|_| AppError::config("could not resolve workflow action"))?;
    if changed != 1 {
        return Err(AppError::invalid_input(
            "workflow action is missing or cannot be resolved from its current state",
        ));
    }
    let action: String = transaction
        .query_row(
            "SELECT action FROM workflow_actions WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::config("could not resolve workflow action"))?;
    if let Ok(crate::config::WorkflowAction::LogTimeSpent { session_id, .. }) =
        serde_json::from_str(&action)
    {
        let session_state = match resolution {
            ActionResolution::Applied => "completed",
            ActionResolution::Retry => "finishing",
        };
        transaction
            .execute(
                "UPDATE work_tracking_sessions SET state=?1 WHERE id=?2 AND scope=?3 AND approval_fingerprint=?4 AND state IN ('uncertain','finishing')",
                rusqlite::params![session_state, session_id, scope, approval_fingerprint],
            )
            .map_err(|_| AppError::config("could not resolve workflow action"))?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::config("could not resolve workflow action"))?;
    Ok(state.to_owned())
}

impl ConfiguredEndpoints {
    fn is_empty(&self) -> bool {
        self.selected.is_empty() && self.workflow.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rf_evt_1_event_has_safe_cloudevent_projection() {
        let event = build_event(
            "github",
            "owner/repo",
            "issue",
            "7".into(),
            "created",
            "confirmed",
        )
        .unwrap();
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["specversion"], "1.0");
        assert_eq!(value["source"], "urn:workctl:github:owner/repo");
        assert_eq!(value["type"], "workctl.issue.created");
        assert_eq!(value["subject"], "/issue/7");
        assert!(value["data"].get("body").is_none());
        assert_eq!(value["data"]["outcome"], "confirmed");
    }
    #[test]
    fn rf_evt_1_partial_confirmed_target_is_recorded_without_resource_body() {
        let error = AppError::partial_success(
            serde_json::json!({"type":"issue","number":7}),
            &["issue close".to_owned()],
            &["issue readback".to_owned()],
        );
        let details = error.details.unwrap();
        let mut emitted = std::collections::HashSet::new();
        let mut events = Vec::new();
        collect_partial_events(
            &details,
            "github",
            "owner/repo",
            &mut emitted,
            &mut events,
            "updated",
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "workctl.issue.updated");
        assert_eq!(events[0].data.outcome, "partial");
        assert_eq!(events[0].data.resource_id, "7");
    }
    #[test]
    fn rf_evt_1_confirmed_event_omits_issue_body() {
        let output = SuccessOutput::IssueMutation(crate::output::IssueMutation {
            issue: crate::domain::Issue {
                number: 4,
                title: "title".into(),
                body: "private body".into(),
                state: crate::domain::IssueState::Open,
                url: "https://github.com/owner/repo/issues/4".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            },
            action: MutationAction::Created,
            unchanged: false,
        });
        let event = output_events(&output, "github", "owner/repo")
            .pop()
            .unwrap();
        let payload = serde_json::to_string(&event).unwrap();
        assert!(payload.contains("workctl.issue.created"));
        assert!(!payload.contains("private body"));
    }
    #[test]
    fn rf_wf_2_plans_typed_triggers_and_confirmed_partial_progress() {
        let workflow: crate::config::Workflow = serde_json::from_str(
            r#"{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"created"}]},{"on":"linkedBranchCreated","actions":[{"type":"addLabel","label":"branch"}]},{"on":"pullRequestCreated","actions":[{"type":"addLabel","label":"pr"}]},{"on":"pullRequestMerged","actions":[{"type":"addLabel","label":"merged"}]},{"on":"reviewApproved","actions":[{"type":"addLabel","label":"approved"}]}]}"#,
        )
        .unwrap();
        let consumers = ConfiguredEndpoints {
            selected: Vec::new(),
            workflow: Some(workflow),
            github_project: None,
            workflow_scope: Some("/tmp/project".into()),
            workflow_fingerprint: Some("a".repeat(64)),
            repository: Some("owner/repo".into()),
        };
        let mut events = Vec::new();
        for (resource, operation) in [
            ("issue", "created"),
            ("issue", "linked_branch_created"),
            ("pull_request", "created"),
            ("pull_request", "merged"),
            ("pull_request", "review_approved"),
            ("pull_request", "review_submitted"),
        ] {
            let mut event = build_event(
                "github",
                "owner/repo",
                resource,
                "7".into(),
                operation,
                "confirmed",
            )
            .unwrap();
            if resource == "pull_request" {
                event.data.related_issue_references = vec![dispatch::RelatedIssueReference {
                    repository: "owner/repo".into(),
                    number: 7,
                }];
            }
            events.push(event);
        }
        events.push(
            build_event(
                "github",
                "owner/repo",
                "issue",
                "8".into(),
                "created",
                "partial",
            )
            .unwrap(),
        );
        let actions = plan_workflow_actions(&events, &consumers, "github", "owner/repo", true);
        assert_eq!(actions.len(), 6);
        assert_eq!(
            actions
                .iter()
                .map(|action| action.target_id.as_str())
                .collect::<Vec<_>>(),
            ["7", "7", "7", "7", "7", "8"]
        );
        assert_eq!(actions[0].ordinal, 0);
        assert_eq!(actions[5].ordinal, 5);
    }
    #[test]
    fn rf_wf_2_rejects_unmapped_or_unsupported_actions_before_provider_write() {
        let workflow: crate::config::Workflow = serde_json::from_str(
            r#"{"rules":[{"on":"issueCreated","actions":[{"type":"setStage","stage":"Ready"}]}]}"#,
        )
        .unwrap();
        assert_eq!(
            validate_workflow_provider(&workflow, "gitlab", &crate::config::Config::default())
                .unwrap_err()
                .code,
            "config"
        );
        assert_eq!(
            validate_workflow_provider(&workflow, "github", &crate::config::Config::default())
                .unwrap_err()
                .code,
            "config"
        );
        let github_field: crate::config::Workflow = serde_json::from_str(
            r#"{"rules":[{"on":"issueCreated","actions":[{"type":"setProjectField","field":"Priority","value":"High"}]}]}"#,
        )
        .unwrap();
        assert_eq!(
            validate_workflow_provider(&github_field, "gitlab", &crate::config::Config::default())
                .unwrap_err()
                .code,
            "config"
        );
    }
    #[test]
    fn rf_wf_2_queued_merge_does_not_trigger_confirmed_merge_workflow() {
        let output = SuccessOutput::Merge {
            number: 42,
            method: None,
            auto: true,
            confirmed_merged: false,
        };
        let events = output_events(&output, "github", "owner/repo");
        assert_eq!(events[0].data.operation, "updated");
        let workflow = serde_json::from_str(
            r#"{"rules":[{"on":"pullRequestMerged","actions":[{"type":"addLabel","label":"merged"}]}]}"#,
        )
        .unwrap();
        let endpoints = ConfiguredEndpoints {
            selected: Vec::new(),
            workflow: Some(workflow),
            github_project: None,
            workflow_scope: Some("/tmp/project".into()),
            workflow_fingerprint: Some("a".repeat(64)),
            repository: Some("owner/repo".into()),
        };
        assert!(
            plan_workflow_actions(&events, &endpoints, "github", "owner/repo", true).is_empty()
        );
    }
    #[test]
    fn rf_wf_2_partial_pr_edit_preserves_only_confirmed_reference_deltas() {
        let mut event = build_event(
            "github",
            "owner/repo",
            "pull_request",
            "42".into(),
            "updated",
            "partial",
        )
        .unwrap();
        attach_partial_pr_references(
            &serde_json::json!({
                "relatedIssueReferences": ["Other/Repo#7"],
                "removedRelatedIssueReferences": ["#8"]
            }),
            "owner/repo",
            42,
            std::slice::from_mut(&mut event),
        );
        assert_eq!(
            event.data.related_issue_references[0].repository,
            "other/repo"
        );
        assert_eq!(event.data.related_issue_references[0].number, 7);
        assert_eq!(
            event.data.removed_related_issue_references[0].repository,
            "owner/repo"
        );
        assert_eq!(event.data.removed_related_issue_references[0].number, 8);

        let mut unconfirmed = build_event(
            "github",
            "owner/repo",
            "pull_request",
            "42".into(),
            "updated",
            "partial",
        )
        .unwrap();
        attach_partial_pr_references(
            &serde_json::json!({"relatedIssueReferences": ["other/repo#7"]}),
            "owner/repo",
            41,
            std::slice::from_mut(&mut unconfirmed),
        );
        assert!(unconfirmed.data.related_issue_references.is_empty());
    }
}
