use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

use crate::config::{GithubWorkflowStage, Workflow, WorkflowAction};
use crate::process::runner;

const MAX_ACTIONS_PER_INVOCATION: usize = 32;
const STALE_ACTION_LEASE: Duration = Duration::from_secs(5);

struct ActionRow {
    id: String,
    provider: String,
    repository: String,
    target_resource: String,
    target_id: String,
    action: String,
}

#[derive(Deserialize)]
struct Membership {
    #[serde(default)]
    labels: Vec<serde_json::Value>,
    #[serde(default)]
    assignees: Vec<serde_json::Value>,
}

pub(super) fn dispatch_pending(
    connection: &mut Connection,
    workflow: &Workflow,
    github_project: Option<&crate::config::GithubProjectDefaults>,
    scope: &str,
    approval_fingerprint: &str,
    deadline: Instant,
) {
    if scope.is_empty() || approval_fingerprint.is_empty() {
        return;
    }
    recover_stale_actions(connection, scope, approval_fingerprint);
    let mut warned = false;
    for _ in 0..MAX_ACTIONS_PER_INVOCATION {
        if Instant::now() >= deadline {
            break;
        }
        let Some(row) = claim_next(connection, scope, approval_fingerprint) else {
            break;
        };
        let action = match serde_json::from_str::<WorkflowAction>(&row.action) {
            Ok(action) => action,
            Err(_) => {
                finish_action(connection, &row.id, "failed", Some("invalid stored action"));
                warned = true;
                break;
            }
        };
        let result = apply_action(&row, &action, workflow, github_project, deadline);
        match result {
            Ok(()) => finish_action(connection, &row.id, "completed", None),
            Err(failure) if is_non_idempotent(&action) => {
                finish_action(connection, &row.id, "uncertain", Some(failure));
                warned = true;
                break;
            }
            Err(failure) => {
                finish_action(connection, &row.id, "pending", Some(failure));
                warned = true;
                break;
            }
        }
    }
    if warned {
        eprintln!("workctl: warning: workflow action needs inspection or retry");
    }
}

fn claim_next(
    connection: &mut Connection,
    scope: &str,
    approval_fingerprint: &str,
) -> Option<ActionRow> {
    let transaction = connection.transaction().ok()?;
    let row = transaction
        .query_row(
            "SELECT a.id,a.provider,a.repository,a.target_resource,a.target_id,a.action
             FROM workflow_actions a
             WHERE a.state='pending' AND a.scope=?1 AND a.approval_fingerprint=?2
               AND NOT EXISTS (
                 SELECT 1 FROM workflow_actions earlier
                 WHERE earlier.event_id=a.event_id AND earlier.target_resource=a.target_resource
                   AND earlier.target_id=a.target_id AND earlier.ordinal<a.ordinal
                   AND earlier.state!='completed'
               )
             ORDER BY a.updated_at,a.created_at,a.event_id,a.ordinal LIMIT 1",
            rusqlite::params![scope, approval_fingerprint],
            |row| {
                Ok(ActionRow {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    repository: row.get(2)?,
                    target_resource: row.get(3)?,
                    target_id: row.get(4)?,
                    action: row.get(5)?,
                })
            },
        )
        .optional()
        .ok()??;
    let changed = transaction
        .execute(
            "UPDATE workflow_actions SET state='running',attempts=attempts+1,updated_at=?1
             WHERE id=?2 AND state='pending' AND scope=?3 AND approval_fingerprint=?4",
            rusqlite::params![now(), row.id, scope, approval_fingerprint],
        )
        .ok()?;
    if changed != 1 {
        return None;
    }
    transaction.commit().ok()?;
    Some(row)
}

fn recover_stale_actions(connection: &Connection, scope: &str, approval_fingerprint: &str) {
    let cutoff = now_before(STALE_ACTION_LEASE);
    let stale = connection
        .prepare(
            "SELECT id,action FROM workflow_actions
             WHERE state='running' AND scope=?1 AND approval_fingerprint=?2
               AND updated_at<?3 LIMIT ?4",
        )
        .and_then(|mut statement| {
            statement
                .query_map(
                    rusqlite::params![
                        scope,
                        approval_fingerprint,
                        cutoff,
                        MAX_ACTIONS_PER_INVOCATION as i64
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?
                .collect::<Result<Vec<_>, _>>()
        });
    let Ok(stale) = stale else { return };
    for (id, payload) in stale {
        let state = if serde_json::from_str::<WorkflowAction>(&payload)
            .is_ok_and(|action| is_non_idempotent(&action))
        {
            "uncertain"
        } else {
            "pending"
        };
        let _ = connection.execute(
            "UPDATE workflow_actions SET state=?1,safe_error='previous attempt did not finish',updated_at=?2
             WHERE id=?3 AND state='running' AND updated_at<?4 AND scope=?5 AND approval_fingerprint=?6",
            rusqlite::params![state, now(), id, cutoff, scope, approval_fingerprint],
        );
    }
}

fn apply_action(
    row: &ActionRow,
    action: &WorkflowAction,
    workflow: &Workflow,
    github_project: Option<&crate::config::GithubProjectDefaults>,
    deadline: Instant,
) -> Result<(), &'static str> {
    let number = row
        .target_id
        .parse::<u64>()
        .map_err(|_| "invalid target ID")?;
    if number == 0 {
        return Err("invalid target ID");
    }
    match row.provider.as_str() {
        "github" => apply_github(row, number, action, workflow, github_project, deadline),
        "gitlab" => apply_gitlab(row, number, action, workflow, deadline),
        _ => Err("unsupported provider"),
    }
}

fn apply_github(
    row: &ActionRow,
    number: u64,
    action: &WorkflowAction,
    workflow: &Workflow,
    github_project: Option<&crate::config::GithubProjectDefaults>,
    deadline: Instant,
) -> Result<(), &'static str> {
    let resource = match row.target_resource.as_str() {
        "issue" => "issue",
        "pull_request" => "pr",
        _ => return Err("unsupported target"),
    };
    if let WorkflowAction::SetStage { stage } = action {
        let mapping = workflow
            .stage_mappings
            .github
            .get(stage)
            .ok_or("stage mapping unavailable")?;
        return github_project_field(row, number, mapping, github_project, deadline);
    }
    if let WorkflowAction::SetProjectField { field, value } = action {
        return github_project_field(
            row,
            number,
            &GithubWorkflowStage {
                field: field.clone(),
                value: value.clone(),
            },
            github_project,
            deadline,
        );
    }
    if let WorkflowAction::Comment { body } = action {
        let args = [
            resource.to_owned(),
            "comment".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            row.repository.clone(),
            "--body-file".to_owned(),
            "-".to_owned(),
        ];
        return run_provider("gh", &args, Some(body.as_bytes().to_vec()), deadline, true);
    }
    let (field, value, add) = match action {
        WorkflowAction::AddLabel { label } => ("labels", label.as_str(), true),
        WorkflowAction::RemoveLabel { label } => ("labels", label.as_str(), false),
        WorkflowAction::AddAssignee { assignee } => ("assignees", assignee.as_str(), true),
        WorkflowAction::RemoveAssignee { assignee } => ("assignees", assignee.as_str(), false),
        _ => return Err("unsupported action"),
    };
    if github_membership_contains(row, number, field, value, deadline)? == add {
        return Ok(());
    }
    let flag = match (field, add) {
        ("labels", true) => "--add-label",
        ("labels", false) => "--remove-label",
        ("assignees", true) => "--add-assignee",
        _ => "--remove-assignee",
    };
    let args = [
        resource.to_owned(),
        "edit".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        row.repository.clone(),
        flag.to_owned(),
        value.to_owned(),
    ];
    run_provider("gh", &args, None, deadline, false)
}

fn apply_gitlab(
    row: &ActionRow,
    number: u64,
    action: &WorkflowAction,
    workflow: &Workflow,
    deadline: Instant,
) -> Result<(), &'static str> {
    let resource = match row.target_resource.as_str() {
        "issue" => "issue",
        "pull_request" => "mr",
        _ => return Err("unsupported target"),
    };
    if let WorkflowAction::SetStage { stage } = action {
        let mapping = workflow
            .stage_mappings
            .gitlab
            .get(stage)
            .ok_or("stage mapping unavailable")?;
        let current = gitlab_labels(row, number, resource, deadline)?;
        let mut args = vec![
            resource.to_owned(),
            "update".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            row.repository.clone(),
        ];
        for prior in workflow
            .stage_mappings
            .gitlab
            .values()
            .map(|mapping| mapping.label.as_str())
        {
            if prior != mapping.label && current.iter().any(|label| label == prior) {
                args.push(format!("--unlabel={prior}"));
            }
        }
        if !current.iter().any(|label| label == &mapping.label) {
            args.push(format!("--label={}", mapping.label));
        }
        if args.len() == 5 {
            return Ok(());
        }
        return run_provider("glab", &args, None, deadline, false);
    }
    if let WorkflowAction::LogTimeSpent { seconds, .. } = action {
        if *seconds < 60 {
            return Ok(());
        }
        let project = encode_gitlab_project(&row.repository)?;
        let endpoint = format!("projects/{project}/issues/{number}/add_spent_time");
        let args = [
            "api".to_owned(),
            "--hostname".to_owned(),
            gitlab_hostname(&row.repository)?,
            "--method".to_owned(),
            "POST".to_owned(),
            endpoint,
            "--field".to_owned(),
            format!("duration={}m", seconds / 60),
        ];
        return run_provider("glab", &args, None, deadline, true);
    }
    let (field, value, add) = match action {
        WorkflowAction::Comment { body } => {
            let kind = if resource == "mr" {
                "merge_requests"
            } else {
                "issues"
            };
            let project = encode_gitlab_project(&row.repository)?;
            let endpoint = format!("projects/{project}/{kind}/{number}/notes");
            let args = vec![
                "api".to_owned(),
                "--hostname".to_owned(),
                gitlab_hostname(&row.repository)?,
                "--method".to_owned(),
                "POST".to_owned(),
                endpoint,
                "--header".to_owned(),
                "Content-Type: application/json".to_owned(),
                "--input".to_owned(),
                "-".to_owned(),
            ];
            let payload = serde_json::to_vec(&serde_json::json!({"body": body}))
                .map_err(|_| "could not encode provider request")?;
            return run_provider("glab", &args, Some(payload), deadline, true);
        }
        WorkflowAction::SetProjectField { .. } => return Err("unsupported action"),
        WorkflowAction::AddLabel { label } => ("labels", label.as_str(), true),
        WorkflowAction::RemoveLabel { label } => ("labels", label.as_str(), false),
        WorkflowAction::AddAssignee { assignee } => ("assignees", assignee.as_str(), true),
        WorkflowAction::RemoveAssignee { assignee } => ("assignees", assignee.as_str(), false),
        WorkflowAction::SetStage { .. } | WorkflowAction::LogTimeSpent { .. } => {
            return Err("unsupported action");
        }
    };
    if gitlab_membership_contains(row, number, resource, field, value, deadline)? == add {
        return Ok(());
    }
    let mut args = vec![
        resource.to_owned(),
        "update".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        row.repository.clone(),
    ];
    if field == "assignees" {
        let prefix = if add { "+" } else { "-" };
        args.push(format!("--assignee={prefix}{value}"));
    } else {
        let flag = if add { "--label" } else { "--unlabel" };
        args.push(format!("{flag}={value}"));
    }
    run_provider("glab", &args, None, deadline, false)
}
fn gitlab_hostname(repository: &str) -> Result<String, &'static str> {
    let url = reqwest::Url::parse(repository).map_err(|_| "invalid GitLab repository")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("invalid GitLab repository");
    }
    let host = url.host_str().ok_or("invalid GitLab repository")?;
    let mut hostname = host.to_owned();
    if let Some(port) = url.port() {
        hostname.push(':');
        hostname.push_str(&port.to_string());
    }
    Ok(hostname)
}

fn github_membership_contains(
    row: &ActionRow,
    number: u64,
    field: &str,
    expected: &str,
    deadline: Instant,
) -> Result<bool, &'static str> {
    let resource = if row.target_resource == "issue" {
        "issue"
    } else {
        "pr"
    };
    let args = [
        resource.to_owned(),
        "view".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        row.repository.clone(),
        "--json".to_owned(),
        "labels,assignees".to_owned(),
    ];
    let bytes = run_provider_output("gh", &args, None, deadline)?;
    let value: Membership =
        serde_json::from_slice(&bytes).map_err(|_| "invalid provider response")?;
    let members = if field == "labels" {
        &value.labels
    } else {
        &value.assignees
    };
    Ok(members.iter().any(|member| {
        member.as_str() == Some(expected)
            || member.get("name").and_then(serde_json::Value::as_str) == Some(expected)
            || member.get("login").and_then(serde_json::Value::as_str) == Some(expected)
    }))
}

fn gitlab_membership_contains(
    row: &ActionRow,
    number: u64,
    resource: &str,
    field: &str,
    expected: &str,
    deadline: Instant,
) -> Result<bool, &'static str> {
    if field == "assignees" {
        return Ok(gitlab_assignees(row, number, resource, deadline)?
            .iter()
            .any(|value| value == expected));
    }
    Ok(gitlab_labels(row, number, resource, deadline)?
        .iter()
        .any(|label| label == expected))
}

fn gitlab_labels(
    row: &ActionRow,
    number: u64,
    resource: &str,
    deadline: Instant,
) -> Result<Vec<String>, &'static str> {
    let args = [
        resource.to_owned(),
        "view".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        row.repository.clone(),
        "--output".to_owned(),
        "json".to_owned(),
    ];
    let bytes = run_provider_output("glab", &args, None, deadline)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid provider response")?;
    Ok(value
        .get("labels")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect())
}
fn gitlab_assignees(
    row: &ActionRow,
    number: u64,
    resource: &str,
    deadline: Instant,
) -> Result<Vec<String>, &'static str> {
    let args = [
        resource.to_owned(),
        "view".to_owned(),
        number.to_string(),
        "--repo".to_owned(),
        row.repository.clone(),
        "--output".to_owned(),
        "json".to_owned(),
    ];
    let bytes = run_provider_output("glab", &args, None, deadline)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid provider response")?;
    let assignees = value
        .get("assignees")
        .and_then(serde_json::Value::as_array)
        .ok_or("invalid provider response")?;
    assignees
        .iter()
        .map(|assignee| {
            assignee
                .get("username")
                .and_then(serde_json::Value::as_str)
                .or_else(|| assignee.get("name").and_then(serde_json::Value::as_str))
                .filter(|username| !username.is_empty())
                .map(str::to_owned)
                .ok_or("invalid provider response")
        })
        .collect()
}
fn github_project_field(
    row: &ActionRow,
    number: u64,
    mapping: &GithubWorkflowStage,
    profile: Option<&crate::config::GithubProjectDefaults>,
    deadline: Instant,
) -> Result<(), &'static str> {
    let profile = profile.ok_or("GitHub Project profile unavailable")?;
    if !profile
        .repositories
        .iter()
        .any(|repo| repo == &row.repository)
    {
        return Err("GitHub Project is not configured for this repository");
    }
    let target_path = if row.target_resource == "issue" {
        "issues"
    } else {
        "pull"
    };
    let target_url = format!(
        "https://github.com/{}/{target_path}/{number}",
        row.repository
    );
    let (owner, project_number) = profile
        .url
        .strip_prefix("https://github.com/")
        .and_then(|path| {
            let parts = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
            if parts.len() == 4 && matches!(parts[0], "orgs" | "users") && parts[2] == "projects" {
                parts[3]
                    .parse::<u64>()
                    .ok()
                    .map(|number| (parts[1].to_owned(), number))
            } else {
                None
            }
        })
        .ok_or("invalid GitHub Project profile")?;
    let query = "query($owner:String!,$number:Int!){repositoryOwner(login:$owner){... on Organization{projectV2(number:$number){id fields(first:100){nodes{__typename ... on ProjectV2Field{id name dataType} ... on ProjectV2SingleSelectField{id name dataType options{id name}} ... on ProjectV2IterationField{id name dataType configuration{iterations{id title}}}} pageInfo{hasNextPage}} items(first:100){nodes{content{... on Issue{url} ... on PullRequest{url}} fieldValues(first:100){nodes{... on ProjectV2ItemFieldTextValue{text field{id}} ... on ProjectV2ItemFieldNumberValue{number field{id}} ... on ProjectV2ItemFieldDateValue{date field{id}} ... on ProjectV2ItemFieldSingleSelectValue{name field{id}} ... on ProjectV2ItemFieldIterationValue{title field{id}}}}} pageInfo{hasNextPage}}}} ... on User{projectV2(number:$number){id fields(first:100){nodes{__typename ... on ProjectV2Field{id name dataType} ... on ProjectV2SingleSelectField{id name dataType options{id name}} ... on ProjectV2IterationField{id name dataType configuration{iterations{id title}}}} pageInfo{hasNextPage}} items(first:100){nodes{content{... on Issue{url} ... on PullRequest{url}} fieldValues(first:100){nodes{... on ProjectV2ItemFieldTextValue{text field{id}} ... on ProjectV2ItemFieldNumberValue{number field{id}} ... on ProjectV2ItemFieldDateValue{date field{id}} ... on ProjectV2ItemFieldSingleSelectValue{name field{id}} ... on ProjectV2ItemFieldIterationValue{title field{id}}}}} pageInfo{hasNextPage}}}}}}";
    let args = [
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={query}"),
        "-F".to_owned(),
        format!("owner={owner}"),
        "-F".to_owned(),
        format!("number={project_number}"),
    ];
    let bytes = run_provider_output("gh", &args, None, deadline)?;
    let response: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid GitHub Project response")?;
    let project = response
        .pointer("/data/repositoryOwner/projectV2")
        .ok_or("GitHub Project unavailable")?;
    let project_id = project
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or("GitHub Project unavailable")?;
    let fields = project
        .pointer("/fields/nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or("GitHub Project fields unavailable")?;
    if project
        .pointer("/fields/pageInfo/hasNextPage")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Err("GitHub Project has too many fields");
    }
    let field = fields
        .iter()
        .find(|field| {
            field.get("name").and_then(serde_json::Value::as_str) == Some(mapping.field.as_str())
        })
        .ok_or("GitHub Project field unavailable")?;
    let field_id = field
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or("GitHub Project field unavailable")?;
    let data_type = field
        .get("dataType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let (flag, value) = match data_type {
        "TEXT" => ("--text", mapping.value.clone()),
        "NUMBER" => {
            mapping
                .value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or("invalid Project number")?;
            ("--number", mapping.value.clone())
        }
        "DATE" => ("--date", mapping.value.clone()),
        "SINGLE_SELECT" => {
            let id = field
                .get("options")
                .and_then(serde_json::Value::as_array)
                .and_then(|options| {
                    options.iter().find(|option| {
                        option.get("name").and_then(serde_json::Value::as_str)
                            == Some(mapping.value.as_str())
                    })
                })
                .and_then(|option| option.get("id"))
                .and_then(serde_json::Value::as_str)
                .ok_or("Project option unavailable")?;
            ("--single-select-option-id", id.to_owned())
        }
        "ITERATION" => {
            let id = field
                .pointer("/configuration/iterations")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| {
                    items.iter().find(|item| {
                        item.get("title").and_then(serde_json::Value::as_str)
                            == Some(mapping.value.as_str())
                    })
                })
                .and_then(|item| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .ok_or("Project iteration unavailable")?;
            ("--iteration-id", id.to_owned())
        }
        _ => return Err("unsupported GitHub Project field type"),
    };
    let items = project
        .pointer("/items/nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or("GitHub Project items unavailable")?;
    if project
        .pointer("/items/pageInfo/hasNextPage")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Err("GitHub Project has too many items to reconcile");
    }
    let target_item = items.iter().find(|item| {
        item.pointer("/content/url")
            .and_then(serde_json::Value::as_str)
            == Some(target_url.as_str())
    });
    if let Some(item) = target_item {
        let values = item
            .pointer("/fieldValues/nodes")
            .and_then(serde_json::Value::as_array)
            .ok_or("GitHub Project item values unavailable")?;
        if values.len() == 100 {
            return Err("GitHub Project item has too many field values");
        }
        let current = values.iter().find(|value| {
            value
                .pointer("/field/id")
                .and_then(serde_json::Value::as_str)
                == Some(field_id)
        });
        let already_set = current.is_some_and(|current| match data_type {
            "TEXT" => {
                current.get("text").and_then(serde_json::Value::as_str)
                    == Some(mapping.value.as_str())
            }
            "NUMBER" => {
                current.get("number").and_then(serde_json::Value::as_f64)
                    == mapping.value.parse::<f64>().ok()
            }
            "DATE" => {
                current.get("date").and_then(serde_json::Value::as_str)
                    == Some(mapping.value.as_str())
            }
            "SINGLE_SELECT" => {
                current.get("name").and_then(serde_json::Value::as_str)
                    == Some(mapping.value.as_str())
            }
            "ITERATION" => {
                current.get("title").and_then(serde_json::Value::as_str)
                    == Some(mapping.value.as_str())
            }
            _ => false,
        });
        if already_set {
            return Ok(());
        }
    } else {
        let args = [
            "project".to_owned(),
            "item-add".to_owned(),
            project_number.to_string(),
            "--owner".to_owned(),
            owner.clone(),
            "--url".to_owned(),
            target_url.clone(),
        ];
        run_provider("gh", &args, None, deadline, false)?;
    }
    let args = [
        "project".to_owned(),
        "item-edit".to_owned(),
        "--project-id".to_owned(),
        project_id.to_owned(),
        "--url".to_owned(),
        target_url,
        "--field-id".to_owned(),
        field_id.to_owned(),
        flag.to_owned(),
        value,
    ];
    run_provider("gh", &args, None, deadline, false)
}

fn encode_gitlab_project(repository: &str) -> Result<String, &'static str> {
    let path = if let Some(url) = repository.strip_prefix("https://") {
        url.split_once('/')
            .map(|(_, path)| path)
            .ok_or("invalid GitLab repository")?
    } else {
        repository
    };
    if path.is_empty()
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("invalid GitLab repository");
    }
    Ok(path.replace('/', "%2F"))
}

fn run_provider(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    deadline: Instant,
    uncertain_on_failure: bool,
) -> Result<(), &'static str> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(if uncertain_on_failure {
            "comment outcome uncertain"
        } else {
            "provider deadline elapsed"
        });
    }
    let result =
        runner::run_with_deadline(program, args, input, None, remaining).map_err(|_| {
            if uncertain_on_failure {
                "comment outcome uncertain"
            } else {
                "provider action failed"
            }
        })?;
    if result.success {
        Ok(())
    } else {
        Err(if uncertain_on_failure {
            "comment outcome uncertain"
        } else {
            "provider action failed"
        })
    }
}

fn run_provider_output(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    deadline: Instant,
) -> Result<Vec<u8>, &'static str> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("provider deadline elapsed");
    }
    let result = runner::run_with_deadline(program, args, input, None, remaining)
        .map_err(|_| "provider read failed")?;
    if result.success {
        Ok(result.stdout)
    } else {
        Err("provider read failed")
    }
}

fn is_non_idempotent(action: &WorkflowAction) -> bool {
    matches!(
        action,
        WorkflowAction::Comment { .. } | WorkflowAction::LogTimeSpent { .. }
    )
}

fn finish_action(connection: &Connection, id: &str, state: &str, error: Option<&str>) {
    let action: Option<String> = connection
        .query_row(
            "SELECT action FROM workflow_actions WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if let Some(action) =
        action.and_then(|payload| serde_json::from_str::<WorkflowAction>(&payload).ok())
    {
        if let WorkflowAction::LogTimeSpent { session_id, .. } = action {
            let session_state = if state == "completed" {
                "completed"
            } else if state == "uncertain" {
                "uncertain"
            } else {
                "active"
            };
            let _ = connection.execute(
                "UPDATE work_tracking_sessions SET state=?1 WHERE id=?2 AND state='finishing'",
                rusqlite::params![session_state, session_id],
            );
        }
    }
    let _ = connection.execute(
        "UPDATE workflow_actions SET state=?1,safe_error=?2,updated_at=?3 WHERE id=?4 AND state='running'",
        rusqlite::params![state, error, now(), id],
    );
}

fn now() -> String {
    super::utc_timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}

fn now_before(duration: Duration) -> String {
    super::utc_timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(duration.as_secs()),
    )
}

#[cfg(test)]
mod tests {
    use super::{claim_next, gitlab_hostname};
    use rusqlite::Connection;

    #[test]
    fn workflow_claim_is_bound_to_repository_scope_and_approval() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE workflow_actions (
                    id TEXT PRIMARY KEY, event_id TEXT, ordinal INTEGER, provider TEXT,
                    repository TEXT, target_resource TEXT, target_id TEXT, action TEXT,
                    state TEXT, attempts INTEGER, updated_at TEXT, scope TEXT,
                    approval_fingerprint TEXT, created_at TEXT
                );
                INSERT INTO workflow_actions VALUES
                  ('a','event-a',0,'github','owner/a','issue','1','{}','pending',0,'now','/repo-a','fingerprint-a','now'),
                  ('b','event-b',0,'github','owner/b','issue','2','{}','pending',0,'now','/repo-b','fingerprint-b','now');",
            )
            .unwrap();
        let claimed = claim_next(&mut connection, "/repo-a", "fingerprint-a").unwrap();
        assert_eq!(claimed.repository, "owner/a");
        let foreign_state: String = connection
            .query_row(
                "SELECT state FROM workflow_actions WHERE id='b'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_state, "pending");
        assert!(claim_next(&mut connection, "/repo-b", "wrong-approval").is_none());
    }

    #[test]
    fn failed_action_retry_yields_to_new_independent_work_without_reordering_target() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE workflow_actions (
                    id TEXT PRIMARY KEY, event_id TEXT, ordinal INTEGER, provider TEXT,
                    repository TEXT, target_resource TEXT, target_id TEXT, action TEXT,
                    state TEXT, attempts INTEGER, updated_at TEXT, scope TEXT,
                    approval_fingerprint TEXT, created_at TEXT
                );
                INSERT INTO workflow_actions VALUES
                  ('retry','event-old',0,'github','owner/repo','issue','1','{}','pending',1,'2026-01-03','/repo','fp','2026-01-01'),
                  ('blocked','event-old',1,'github','owner/repo','issue','1','{}','pending',0,'2026-01-02','/repo','fp','2026-01-01'),
                  ('fresh','event-new',0,'github','owner/repo','issue','2','{}','pending',0,'2026-01-02','/repo','fp','2026-01-02');",
            )
            .unwrap();

        let claimed = claim_next(&mut connection, "/repo", "fp").unwrap();
        assert_eq!(claimed.id, "fresh");

        let claimed = claim_next(&mut connection, "/repo", "fp").unwrap();
        assert_eq!(claimed.id, "retry");
        assert!(claim_next(&mut connection, "/repo", "fp").is_none());
    }

    #[test]
    fn workflow_api_uses_repository_host_for_gitlab_notes() {
        assert_eq!(
            gitlab_hostname("https://gitlab.example.test:8443/group/sub/project").unwrap(),
            "gitlab.example.test:8443"
        );
        assert_eq!(
            gitlab_hostname("https://gitlab.com/group/project").unwrap(),
            "gitlab.com"
        );
        assert!(gitlab_hostname("http://gitlab.example.test/group/project").is_err());
        assert!(gitlab_hostname("https://user:secret@gitlab.example.test/group/project").is_err());
    }
}
