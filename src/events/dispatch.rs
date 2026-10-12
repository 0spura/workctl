use std::collections::BTreeMap;
#[cfg(any(not(target_os = "windows"), test))]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::fs::OpenOptions;
#[cfg(not(target_os = "windows"))]
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::Client;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};

use crate::config::{EventSelection, MAX_WORKFLOW_PR_REFERENCES};
use crate::domain::AppError;

const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_ENDPOINTS: usize = 16;
const DELIVERY_BUDGET: Duration = Duration::from_secs(1);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalConfig {
    events: GlobalEvents,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalEvents {
    endpoints: BTreeMap<String, Endpoint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Endpoint {
    pub(crate) url: String,
    pub(crate) token: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Endpoint")
            .field("url", &"[redacted]")
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CloudEvent {
    pub specversion: &'static str,
    pub id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub subject: String,
    pub time: String,
    pub datacontenttype: &'static str,
    pub data: EventData,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EventData {
    pub schema_version: u8,
    pub repository: String,
    pub resource: String,
    pub resource_id: String,
    pub operation: String,
    pub action: String,
    pub outcome: String,
    #[serde(
        rename = "relatedIssueReferences",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub related_issue_references: Vec<RelatedIssueReference>,
    #[serde(
        rename = "removedRelatedIssueReferences",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub removed_related_issue_references: Vec<RelatedIssueReference>,
    #[serde(skip)]
    pub effective_related_issue_references: Option<Vec<RelatedIssueReference>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RelatedIssueReference {
    pub repository: String,
    pub number: u64,
}

pub(crate) struct ConfiguredEndpoints {
    pub(crate) selected: Vec<(String, Endpoint)>,
    pub(crate) workflow: Option<crate::config::Workflow>,
    pub(crate) github_project: Option<crate::config::GithubProjectDefaults>,
    pub(crate) workflow_scope: Option<String>,
    pub(crate) workflow_fingerprint: Option<String>,
    pub(crate) repository: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct PlannedAction {
    pub(crate) event_id: String,
    pub(crate) ordinal: i64,
    pub(crate) provider: String,
    pub(crate) repository: String,
    pub(crate) target_resource: String,
    pub(crate) target_id: String,
    pub(crate) action: crate::config::WorkflowAction,
    pub(crate) scope: String,
    pub(crate) approval_fingerprint: String,
    pub(crate) created_at: String,
}

pub(crate) fn validate_config(
    selection: Option<EventSelection>,
) -> Result<ConfiguredEndpoints, AppError> {
    if let Some(selection) = selection {
        if !selection.endpoints.is_empty() {
            return resolve_endpoints(selection, &global_config_path()?);
        }
    }
    Ok(ConfiguredEndpoints {
        selected: Vec::new(),
        workflow: None,
        github_project: None,
        workflow_scope: None,
        workflow_fingerprint: None,
        repository: None,
    })
}

fn resolve_endpoints(
    selection: EventSelection,
    path: &Path,
) -> Result<ConfiguredEndpoints, AppError> {
    if selection.endpoints.len() > MAX_ENDPOINTS {
        return Err(AppError::config("event endpoint selection is invalid"));
    }
    let bytes = read_private_config(path)?;
    let global: GlobalConfig = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::config("private event configuration is invalid"))?;
    if global.events.endpoints.len() > MAX_ENDPOINTS {
        return Err(AppError::config(
            "private event configuration exceeds the endpoint limit",
        ));
    }
    for (id, endpoint) in &global.events.endpoints {
        if id.trim().is_empty() {
            return Err(AppError::config("private event endpoint ID is invalid"));
        }
        validate_endpoint(endpoint)?;
    }
    let mut selected = Vec::with_capacity(selection.endpoints.len());
    for id in selection.endpoints {
        let endpoint = global
            .events
            .endpoints
            .get(&id)
            .ok_or_else(|| AppError::config("selected event endpoint is not configured"))?;
        selected.push((
            id,
            Endpoint {
                url: endpoint.url.clone(),
                token: endpoint.token.clone(),
            },
        ));
    }
    Ok(ConfiguredEndpoints {
        selected,
        workflow: None,
        github_project: None,
        workflow_scope: None,
        workflow_fingerprint: None,
        repository: None,
    })
}

fn validate_endpoint(endpoint: &Endpoint) -> Result<(), AppError> {
    let url = reqwest::Url::parse(&endpoint.url)
        .map_err(|_| AppError::config("private event endpoint URL is invalid"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || endpoint.token.trim().is_empty()
    {
        return Err(AppError::config(
            "private event endpoint definition is invalid",
        ));
    }
    let authorization = format!("Bearer {}", endpoint.token);
    if reqwest::header::HeaderValue::from_bytes(authorization.as_bytes()).is_err() {
        return Err(AppError::config(
            "private event endpoint definition is invalid",
        ));
    }
    Ok(())
}

fn global_config_path() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "macos")]
    let dir = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/workctl"));
    #[cfg(target_os = "windows")]
    let dir = std::env::var_os("LOCALAPPDATA").map(|home| PathBuf::from(home).join("workctl"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|home| home.join("workctl"));
    let path = dir
        .map(|path| path.join("config.json"))
        .ok_or_else(|| AppError::config("private event configuration path is unavailable"))?;
    if !path.is_absolute() {
        return Err(AppError::config(
            "private event configuration path is invalid",
        ));
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
fn read_private_config(path: &Path) -> Result<Vec<u8>, AppError> {
    super::private_windows::read_private_file(path, MAX_CONFIG_BYTES)
        .map_err(|_| AppError::config("private event configuration is unavailable"))
}

#[cfg(not(target_os = "windows"))]
fn read_private_config(path: &Path) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| AppError::config("private event configuration is unavailable"))?;
    if !metadata.file_type().is_file() {
        return Err(AppError::config(
            "private event configuration must be a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err(AppError::config(
                "private event configuration permissions must be 0600",
            ));
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| AppError::config("private event configuration is unavailable"))?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| AppError::config("private event configuration is unavailable"))?;
    if !opened_metadata.is_file() {
        return Err(AppError::config(
            "private event configuration must be a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if opened_metadata.permissions().mode() & 0o777 != 0o600 {
            return Err(AppError::config(
                "private event configuration permissions must be 0600",
            ));
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::config("private event configuration is unavailable"))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(AppError::config(
            "private event configuration exceeds the size limit",
        ));
    }
    Ok(bytes)
}

fn persist_pr_reference_deltas(
    connection: &rusqlite::Connection,
    event: &CloudEvent,
) -> Result<(), AppError> {
    for reference in &event.data.related_issue_references {
        if reference.repository.is_empty() || reference.number == 0 {
            return Err(AppError::config("pull request reference is invalid"));
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO workflow_pr_references(source,subject,repository,number)
                 VALUES (?1,?2,?3,?4)",
                rusqlite::params![
                    event.source.to_ascii_lowercase(),
                    event.subject,
                    reference.repository.to_ascii_lowercase(),
                    i64::try_from(reference.number)
                        .map_err(|_| AppError::config("pull request reference is invalid"))?
                ],
            )
            .map_err(|_| AppError::config("could not persist pull request references"))?;
    }
    for reference in &event.data.removed_related_issue_references {
        if reference.repository.is_empty() || reference.number == 0 {
            return Err(AppError::config("pull request reference is invalid"));
        }
        connection
            .execute(
                "DELETE FROM workflow_pr_references
                 WHERE source=?1 AND subject=?2 AND repository=?3 AND number=?4",
                rusqlite::params![
                    event.source.to_ascii_lowercase(),
                    event.subject,
                    reference.repository.to_ascii_lowercase(),
                    i64::try_from(reference.number)
                        .map_err(|_| AppError::config("pull request reference is invalid"))?
                ],
            )
            .map_err(|_| AppError::config("could not persist pull request references"))?;
    }
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_pr_references WHERE source=?1 AND subject=?2",
            rusqlite::params![event.source.to_ascii_lowercase(), event.subject],
            |row| row.get(0),
        )
        .map_err(|_| AppError::config("could not persist pull request references"))?;
    if count > MAX_WORKFLOW_PR_REFERENCES as i64 {
        return Err(AppError::config("pull request reference limit exceeded"));
    }
    Ok(())
}

pub(crate) fn enqueue_and_dispatch(
    outbox_path: &Path,
    endpoints: &ConfiguredEndpoints,
    events: &[CloudEvent],
    actions: &[PlannedAction],
    workflow_authorized: bool,
) -> Result<(), AppError> {
    if events.is_empty() {
        return Ok(());
    }
    let mut connection = super::store::open(outbox_path)?;
    let transaction = connection
        .transaction()
        .map_err(|_| AppError::config("could not persist mutation event"))?;
    for event in events {
        let payload = serde_json::to_string(event)
            .map_err(|_| AppError::config("could not encode mutation event"))?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO events(id,payload,created_at) VALUES (?1,?2,?3)",
                rusqlite::params![event.id, payload, event.time],
            )
            .map_err(|_| AppError::config("could not persist mutation event"))?;
        if event.data.resource == "pull_request" {
            persist_pr_reference_deltas(&transaction, event)?;
        }
        for (endpoint_id, _) in &endpoints.selected {
            transaction.execute("INSERT OR IGNORE INTO deliveries(event_id,endpoint_id,delivered) VALUES (?1,?2,0)",
                rusqlite::params![event.id, endpoint_id])
                .map_err(|_| AppError::config("could not persist mutation event"))?;
        }
    }
    for action in actions {
        if let crate::config::WorkflowAction::LogTimeSpent {
            session_id,
            finished_at_ms,
            ..
        } = &action.action
        {
            let changed = transaction.execute(
                "UPDATE work_tracking_sessions SET state='finishing',finished_at_ms=?1,finish_event_id=?2
                 WHERE id=?3 AND scope=?4 AND approval_fingerprint=?5 AND state='active'",
                rusqlite::params![finished_at_ms, action.event_id, session_id, action.scope, action.approval_fingerprint],
            ).map_err(|_| AppError::config("could not persist workflow action"))?;
            if changed != 1 {
                continue;
            }
        }
        let payload = serde_json::to_string(&action.action)
            .map_err(|_| AppError::config("could not encode workflow action"))?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO workflow_actions(
                    id,event_id,ordinal,provider,repository,scope,approval_fingerprint,
                    target_resource,target_id,action,state,attempts,created_at,updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',0,?11,?11)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    action.event_id,
                    action.ordinal,
                    action.provider,
                    action.repository,
                    action.scope,
                    action.approval_fingerprint,
                    action.target_resource,
                    action.target_id,
                    payload,
                    action.created_at
                ],
            )
            .map_err(|_| AppError::config("could not persist workflow action"))?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::config("could not persist mutation event"))?;
    dispatch_pending(&mut connection, endpoints, workflow_authorized);
    Ok(())
}

fn dispatch_pending(
    connection: &mut rusqlite::Connection,
    endpoints: &ConfiguredEndpoints,
    workflow_authorized: bool,
) {
    if endpoints.selected.is_empty() && (!workflow_authorized || endpoints.workflow.is_none()) {
        return;
    }
    let started = Instant::now();
    let deadline = started + DELIVERY_BUDGET;
    if workflow_authorized {
        if let Some(workflow) = endpoints.workflow.as_ref() {
            let local_deadline =
                (started + local_workflow_budget(!endpoints.selected.is_empty())).min(deadline);
            super::workflow::dispatch_pending(
                connection,
                workflow,
                endpoints.github_project.as_ref(),
                endpoints.workflow_scope.as_deref().unwrap_or_default(),
                endpoints
                    .workflow_fingerprint
                    .as_deref()
                    .unwrap_or_default(),
                local_deadline,
            );
        }
    }
    if endpoints.selected.is_empty() {
        return;
    }
    run_with_delivery_deadline(deadline, async {
        let client = match Client::builder()
            .redirect(Policy::none())
            .no_hickory_dns()
            .build()
        {
            Ok(client) => client,
            Err(_) => return,
        };
        dispatch_pending_with_client(connection, endpoints, &client, deadline).await;
    });
}

fn local_workflow_budget(has_http_endpoints: bool) -> Duration {
    if has_http_endpoints {
        DELIVERY_BUDGET / 2
    } else {
        DELIVERY_BUDGET
    }
}

#[cfg(test)]
mod workflow_budget_tests {
    use super::{DELIVERY_BUDGET, local_workflow_budget};

    #[test]
    fn local_workflow_uses_remaining_aggregate_budget() {
        assert_eq!(local_workflow_budget(false), DELIVERY_BUDGET);
        assert_eq!(local_workflow_budget(true), DELIVERY_BUDGET / 2);
    }
}

fn run_with_delivery_deadline(deadline: Instant, delivery: impl Future<Output = ()>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        if Instant::now() < deadline {
            let _ = tokio::time::timeout_at(deadline.into(), delivery).await;
        }
    });
    // System DNS uses blocking getaddrinfo work that cannot be cancelled once started.
    // Do not join it during teardown after the aggregate dispatch deadline expires.
    runtime.shutdown_background();
}

async fn dispatch_pending_with_client(
    connection: &mut rusqlite::Connection,
    endpoints: &ConfiguredEndpoints,
    client: &Client,
    deadline: Instant,
) {
    for (endpoint_id, endpoint) in &endpoints.selected {
        if Instant::now() >= deadline {
            break;
        }
        let mut statement = match connection.prepare("SELECT e.id,e.payload FROM events e JOIN deliveries d ON e.id=d.event_id WHERE d.endpoint_id=?1 AND d.delivered=0 ORDER BY e.id LIMIT 100") { Ok(statement) => statement, Err(_) => return };
        let rows = match statement.query_map([endpoint_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return,
        };
        let pending = rows.collect::<Result<Vec<_>, _>>();
        drop(statement);
        let Ok(pending) = pending else { return };
        for (event_id, payload) in pending {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let request = client
                .post(&endpoint.url)
                .bearer_auth(&endpoint.token)
                .header("content-type", "application/cloudevents+json")
                .body(payload)
                .timeout(remaining);
            if request
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                let _ = connection.execute(
                    "UPDATE deliveries SET delivered=1 WHERE event_id=?1 AND endpoint_id=?2",
                    rusqlite::params![event_id, endpoint_id],
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[cfg(unix)]
    use std::io::{BufRead, BufReader, Read};
    #[cfg(unix)]
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    #[cfg(unix)]
    use std::thread;

    fn private_file(contents: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("workctl-events-{}", uuid::Uuid::new_v4()));
        #[cfg(windows)]
        super::super::private_windows::PrivateDirectory::open_or_create(&dir).unwrap();
        #[cfg(not(windows))]
        fs::create_dir(&dir).unwrap();
        let path = dir.join("config.json");
        #[cfg(windows)]
        let mut file = super::super::private_windows::create_private_file(&path).unwrap();
        #[cfg(not(windows))]
        let mut file = {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(&path).unwrap()
        };
        file.write_all(contents.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn rf_cfg_10_accepts_private_https_endpoint_and_redacts_secrets() {
        let (dir, path) = private_file(
            r#"{"events":{"endpoints":{"sink":{"url":"https://events.example.test/v1","token":"top-secret"}}}}"#,
        );
        let selected = resolve_endpoints(
            EventSelection {
                endpoints: vec!["sink".into()],
            },
            &path,
        )
        .unwrap();
        assert_eq!(selected.selected.len(), 1);
        let debug = format!("{:?}", selected.selected[0].1);
        assert!(!debug.contains("top-secret"));
        assert!(!debug.contains("events.example.test"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_cfg_10_rejects_non_https_endpoint_before_mutation() {
        let (dir, path) = private_file(
            r#"{"events":{"endpoints":{"sink":{"url":"http://events.example.test/v1","token":"secret"}}}}"#,
        );
        let error = resolve_endpoints(
            EventSelection {
                endpoints: vec!["sink".into()],
            },
            &path,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "config");
        assert!(!error.message.contains("secret"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rf_cfg_10_rejects_invalid_bearer_header_tokens_before_dispatch() {
        for token in [r"bad\rvalue", r"bad\nvalue", r"bad\u0000value"] {
            let contents = format!(
                r#"{{"events":{{"endpoints":{{"sink":{{"url":"https://events.example.test","token":"{token}"}}}}}}}}"#
            );
            let (dir, path) = private_file(&contents);
            let error = resolve_endpoints(
                EventSelection {
                    endpoints: vec!["sink".into()],
                },
                &path,
            )
            .err()
            .unwrap();
            assert_eq!(error.code, "config");
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn rf_cfg_10_rejects_private_config_without_exact_0600_mode() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, path) = private_file(r#"{"events":{"endpoints":{}}}"#);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let error = read_private_config(&path).unwrap_err();
        assert_eq!(error.code, "config");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rf_cfg_10_rejects_configuration_over_64_kibibytes() {
        let contents = " ".repeat(MAX_CONFIG_BYTES + 1);
        let (dir, path) = private_file(&contents);
        let error = read_private_config(&path).unwrap_err();
        assert_eq!(error.code, "config");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_cfg_10_rejects_more_than_sixteen_global_endpoints() {
        let endpoints = (0..17)
            .map(|index| {
                (
                    format!("e{index}"),
                    serde_json::json!({
                        "url": "https://events.example.test", "token": "secret"
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let contents = serde_json::json!({"events":{"endpoints":endpoints}}).to_string();
        let (dir, path) = private_file(&contents);
        let error = resolve_endpoints(
            EventSelection {
                endpoints: vec!["e0".into()],
            },
            &path,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "config");
        fs::remove_dir_all(dir).unwrap();
    }

    const FIXTURE_TIMEOUT: Duration = Duration::from_secs(4);
    const DEADLINE_TOLERANCE: Duration = Duration::from_millis(250);

    #[cfg(unix)]
    enum HttpReply {
        Status(u16),
        DelayedStatus(u16, Duration),
        Stall,
    }

    #[cfg(unix)]
    struct HttpRequest {
        request_line: String,
        headers: BTreeMap<String, String>,
        payload: serde_json::Value,
    }

    #[cfg(unix)]
    fn http_fixture(replies: Vec<HttpReply>) -> (String, thread::JoinHandle<Vec<HttpRequest>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server =
            thread::spawn(move || {
                let mut requests = Vec::new();
                for reply in replies {
                    let accept_deadline = Instant::now() + FIXTURE_TIMEOUT;
                    let stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(
                                    Instant::now() < accept_deadline,
                                    "fixture request never arrived"
                                );
                                thread::sleep(Duration::from_millis(2));
                            }
                            Err(error) => panic!("fixture accept failed: {error}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(FIXTURE_TIMEOUT)).unwrap();
                    stream.set_write_timeout(Some(FIXTURE_TIMEOUT)).unwrap();
                    let mut reader = BufReader::new(stream);
                    let mut request_line = String::new();
                    assert!(reader.read_line(&mut request_line).unwrap() > 0);
                    let mut headers = BTreeMap::new();
                    loop {
                        let mut line = String::new();
                        assert!(reader.read_line(&mut line).unwrap() > 0);
                        if line == "\r\n" {
                            break;
                        }
                        let (name, value) = line.split_once(':').unwrap();
                        headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
                    }
                    let length: usize = headers.get("content-length").unwrap().parse().unwrap();
                    assert!(length <= MAX_CONFIG_BYTES);
                    let mut payload = vec![0; length];
                    reader.read_exact(&mut payload).unwrap();
                    requests.push(HttpRequest {
                        request_line: request_line.trim().to_owned(),
                        headers,
                        payload: serde_json::from_slice(&payload).unwrap(),
                    });
                    let status = match reply {
                        HttpReply::Status(status) => status,
                        HttpReply::DelayedStatus(status, delay) => {
                            thread::sleep(delay);
                            status
                        }
                        HttpReply::Stall => {
                            let mut byte = [0];
                            assert_eq!(
                                reader.read(&mut byte).unwrap(),
                                0,
                                "timed-out request was not closed"
                            );
                            continue;
                        }
                    };
                    write!(reader.get_mut(),
                    "HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ).unwrap();
                }
                requests
            });
        (format!("http://localhost:{}", address.port()), server)
    }

    #[cfg(unix)]
    fn fixture_endpoints(base_url: &str, ids: &[&str]) -> ConfiguredEndpoints {
        ConfiguredEndpoints {
            selected: ids
                .iter()
                .map(|id| {
                    (
                        (*id).to_owned(),
                        Endpoint {
                            url: format!("{base_url}/{id}"),
                            token: format!("fixture-{id}-token"),
                        },
                    )
                })
                .collect(),
            workflow: None,
            github_project: None,
            workflow_scope: None,
            workflow_fingerprint: None,
            repository: None,
        }
    }

    #[cfg(unix)]
    fn dispatch_to_fixture(connection: &mut rusqlite::Connection, endpoints: &ConfiguredEndpoints) {
        let deadline = Instant::now() + DELIVERY_BUDGET;
        run_with_delivery_deadline(deadline, async {
            // RF-CFG.10: HTTP and disabled proxies are confined to the loopback fixture.
            let client = Client::builder()
                .redirect(Policy::none())
                .no_hickory_dns()
                .no_proxy()
                .build()
                .unwrap();
            dispatch_pending_with_client(connection, endpoints, &client, deadline).await;
        });
    }

    #[cfg(unix)]
    fn pending_outbox(ids: &[&str]) -> (PathBuf, PathBuf, rusqlite::Connection) {
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("workctl-events-http-{}", uuid::Uuid::new_v4()));
        let path = dir.join("outbox.sqlite3");
        let connection = super::super::store::open(&path).unwrap();
        let event = CloudEvent {
            specversion: "1.0",
            id: "stable-fixture-event".into(),
            source: "urn:workctl:github:example/repo".into(),
            event_type: "workctl.issue.updated".into(),
            subject: "/issue/42".into(),
            time: "2026-10-10T12:00:00Z".into(),
            datacontenttype: "application/json",
            data: EventData {
                schema_version: 1,
                repository: "example/repo".into(),
                resource: "issue".into(),
                resource_id: "42".into(),
                operation: "updated".into(),
                action: "updated".into(),
                outcome: "confirmed".into(),
                related_issue_references: Vec::new(),
                removed_related_issue_references: Vec::new(),
                effective_related_issue_references: None,
            },
        };
        connection
            .execute(
                "INSERT INTO events(id,payload,created_at) VALUES (?1,?2,?3)",
                rusqlite::params![event.id, serde_json::to_string(&event).unwrap(), event.time],
            )
            .unwrap();
        for id in ids {
            connection
                .execute(
                    "INSERT INTO deliveries(event_id,endpoint_id,delivered) VALUES (?1,?2,0)",
                    rusqlite::params![event.id, id],
                )
                .unwrap();
        }
        (dir, path, connection)
    }

    #[cfg(unix)]
    fn delivered(connection: &rusqlite::Connection, endpoint_id: &str) -> i64 {
        connection
            .query_row(
                "SELECT delivered FROM deliveries WHERE endpoint_id=?1",
                [endpoint_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn rf_evt_1_http_payload_bearer_ack_pending_and_restart_replay() {
        let (base_url, server) = http_fixture(vec![
            HttpReply::Status(204),
            HttpReply::Status(503),
            HttpReply::Status(200),
        ]);
        let endpoints = fixture_endpoints(&base_url, &["accepted", "retry"]);
        let (dir, path, mut connection) = pending_outbox(&["accepted", "retry"]);
        dispatch_to_fixture(&mut connection, &endpoints);
        assert_eq!(delivered(&connection, "accepted"), 1);
        assert_eq!(delivered(&connection, "retry"), 0);
        drop(connection);

        let mut connection = super::super::store::open(&path).unwrap();
        dispatch_to_fixture(&mut connection, &endpoints);
        assert_eq!(delivered(&connection, "accepted"), 1);
        assert_eq!(delivered(&connection, "retry"), 1);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        let expected = serde_json::json!({
            "specversion": "1.0",
            "id": "stable-fixture-event",
            "source": "urn:workctl:github:example/repo",
            "type": "workctl.issue.updated",
            "subject": "/issue/42",
            "time": "2026-10-10T12:00:00Z",
            "datacontenttype": "application/json",
            "data": {
                "schema_version": 1,
                "repository": "example/repo", "resource": "issue", "resource_id": "42",
                "operation": "updated", "action": "updated", "outcome": "confirmed",
            },
        });
        for (request, endpoint_id) in requests.iter().zip(["accepted", "retry", "retry"]) {
            assert_eq!(
                request.request_line,
                format!("POST /{endpoint_id} HTTP/1.1")
            );
            assert_eq!(
                request.headers["authorization"],
                format!("Bearer fixture-{endpoint_id}-token")
            );
            assert_eq!(
                request.headers["content-type"],
                "application/cloudevents+json"
            );
            assert_eq!(request.payload, expected);
        }
        assert_eq!(requests[1].payload["id"], requests[2].payload["id"]);
        drop(connection);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rf_evt_1_http_deadline_is_aggregate_and_cancels_stalled_request() {
        let (base_url, server) = http_fixture(vec![
            HttpReply::DelayedStatus(204, Duration::from_millis(600)),
            HttpReply::Stall,
        ]);
        let endpoints = fixture_endpoints(&base_url, &["accepted", "stalled", "unattempted"]);
        let (dir, _, mut connection) = pending_outbox(&["accepted", "stalled", "unattempted"]);
        let started = Instant::now();
        dispatch_to_fixture(&mut connection, &endpoints);
        let elapsed = started.elapsed();
        let requests = server.join().unwrap();
        assert!(
            elapsed >= DELIVERY_BUDGET - Duration::from_millis(100),
            "{elapsed:?}"
        );
        assert!(
            elapsed < DELIVERY_BUDGET + DEADLINE_TOLERANCE,
            "{elapsed:?}"
        );
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].request_line, "POST /accepted HTTP/1.1");
        assert_eq!(requests[1].request_line, "POST /stalled HTTP/1.1");
        assert_eq!(delivered(&connection, "accepted"), 1);
        assert_eq!(delivered(&connection, "stalled"), 0);
        assert_eq!(delivered(&connection, "unattempted"), 0);
        drop(connection);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_deadline_cancels_future_without_joining_blocking_work() {
        struct OnDrop(Arc<AtomicBool>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let started = Instant::now();
        run_with_delivery_deadline(started + DELIVERY_BUDGET, async {
            let _on_drop = OnDrop(dropped.clone());
            // Reqwest's system resolver awaits spawn_blocking(getaddrinfo). Keep that
            // worker shape under control without replacing DNS or endpoint resolution.
            let worker = tokio::task::spawn_blocking(move || {
                started_tx.send(()).unwrap();
                let _ = release_rx.recv_timeout(FIXTURE_TIMEOUT);
                finished_tx.send(()).unwrap();
            });
            worker.await.unwrap();
        });
        let elapsed = started.elapsed();
        let finished_before_release = finished_rx.try_recv().is_ok();
        let _ = release_tx.send(());
        if !finished_before_release {
            finished_rx.recv_timeout(FIXTURE_TIMEOUT).unwrap();
        }
        started_rx.recv_timeout(FIXTURE_TIMEOUT).unwrap();
        assert!(dropped.load(Ordering::SeqCst));
        assert!(
            !finished_before_release,
            "runtime teardown joined uncancellable blocking work"
        );
        assert!(
            elapsed >= DELIVERY_BUDGET - Duration::from_millis(100),
            "{elapsed:?}"
        );
        assert!(
            elapsed < DELIVERY_BUDGET + DEADLINE_TOLERANCE,
            "{elapsed:?}"
        );
    }
    #[test]
    fn rf_wf_3_workflow_only_receipt_persists_event_and_actions_atomically() {
        let (dir, _) = private_file("{}");
        let path = dir.join("outbox.sqlite3");
        let event = CloudEvent {
            specversion: "1.0",
            id: "event-1".into(),
            source: "urn:workctl:github:owner/repo".into(),
            event_type: "workctl.issue.created".into(),
            subject: "/issue/7".into(),
            time: "2026-10-11T12:00:00Z".into(),
            datacontenttype: "application/json",
            data: EventData {
                schema_version: 1,
                repository: "owner/repo".into(),
                resource: "issue".into(),
                resource_id: "7".into(),
                operation: "created".into(),
                action: "created".into(),
                outcome: "confirmed".into(),
                related_issue_references: Vec::new(),
                removed_related_issue_references: Vec::new(),
                effective_related_issue_references: None,
            },
        };
        let consumers = ConfiguredEndpoints {
            selected: Vec::new(),
            workflow: None,
            github_project: None,
            workflow_scope: None,
            workflow_fingerprint: None,
            repository: Some("owner/repo".into()),
        };
        let action = PlannedAction {
            event_id: event.id.clone(),
            ordinal: 0,
            provider: "github".into(),
            repository: "owner/repo".into(),
            target_resource: "issue".into(),
            target_id: "7".into(),
            action: crate::config::WorkflowAction::AddLabel {
                label: "triage".into(),
            },
            scope: "/tmp/project".into(),
            approval_fingerprint: "a".repeat(64),
            created_at: event.time.clone(),
        };
        enqueue_and_dispatch(
            &path,
            &consumers,
            &[event.clone()],
            &[action.clone()],
            false,
        )
        .expect("persist workflow-only event");
        let connection = super::super::store::open(&path).unwrap();
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        let actions: i64 = connection
            .query_row("SELECT COUNT(*) FROM workflow_actions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((events, actions), (1, 1));
        drop(connection);

        let (failed_dir, _) = private_file("{}");
        let failed_path = failed_dir.join("outbox.sqlite3");
        let mut unlinked_action = action;
        unlinked_action.event_id = "missing-event".into();
        assert!(
            enqueue_and_dispatch(
                &failed_path,
                &consumers,
                &[event],
                &[unlinked_action],
                false
            )
            .is_err()
        );
        let connection = super::super::store::open(&failed_path).unwrap();
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        let actions: i64 = connection
            .query_row("SELECT COUNT(*) FROM workflow_actions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((events, actions), (0, 0));
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(failed_dir).unwrap();
    }
}
