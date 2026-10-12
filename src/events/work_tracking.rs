use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::config::{self, WorkTracking};
use crate::output::SuccessOutput;
use crate::process::runner;

const CURSOR_SLOT: i64 = 1;

pub(crate) struct Invocation {
    started: Instant,
    started_at_ms: i64,
    provider: String,
    repository: String,
    scope: Option<String>,
    worktree: Option<String>,
    branch: String,
    explicit_issue_id: Option<String>,
    approval_fingerprint: String,
    tracking: Option<WorkTracking>,
}

impl Invocation {
    pub(crate) fn begin(
        cwd: &Path,
        provider: &str,
        repository: &str,
        issue_id: Option<u64>,
    ) -> Self {
        let worktree = config::repository_root(cwd)
            .ok()
            .and_then(|path| std::fs::canonicalize(path).ok())
            .map(|path| path.to_string_lossy().into_owned());
        let scope = worktree.clone();
        let branch = worktree
            .as_deref()
            .and_then(|root| current_branch(Path::new(root)))
            .unwrap_or_default();
        let mut approval_fingerprint = String::new();
        let mut tracking = None;
        if let (Some(root), Ok(project_config)) = (worktree.as_deref(), config::load_for_cwd(cwd)) {
            if project_config.workflow.is_some()
                && config::workflow_approval_status(&project_config, Path::new(root))
                    .unwrap_or(false)
            {
                approval_fingerprint = project_config.workflow_approval.clone().unwrap_or_default();
                tracking = project_config
                    .workflow
                    .and_then(|workflow| workflow.work_tracking);
            }
        }
        Self {
            started: Instant::now(),
            started_at_ms: now_millis(),
            provider: provider.to_owned(),
            repository: repository.to_owned(),
            scope,
            worktree,
            branch,
            explicit_issue_id: issue_id.map(|id| id.to_string()),
            approval_fingerprint,
            tracking,
        }
    }

    pub(crate) fn finish(self, output: Option<&SuccessOutput>) {
        let Ok(path) = super::state_outbox_path() else {
            return;
        };
        let Ok(mut connection) = super::store::open(&path) else {
            return;
        };
        let now_ms = now_millis();
        let branch = self
            .worktree
            .as_deref()
            .and_then(|root| current_branch(Path::new(root)))
            .unwrap_or(self.branch.clone());
        let Some(scope) = self.scope.as_deref() else {
            reset_cursor(&connection, &self, &branch, "", now_ms);
            return;
        };
        let Ok(transaction) = connection.transaction() else {
            return;
        };
        let tracking_enabled = self.tracking.is_some()
            && !self.approval_fingerprint.is_empty()
            && !self.repository.is_empty()
            && !branch.is_empty();
        let issue_id = if tracking_enabled {
            self.explicit_issue_id.clone().or_else(|| {
                mapped_issue(
                    &transaction,
                    scope,
                    &self.approval_fingerprint,
                    &self.provider,
                    &self.repository,
                    self.worktree.as_deref().unwrap_or_default(),
                    &branch,
                )
            })
        } else {
            None
        };
        if let (Some(issue_id), Some(tracking)) = (issue_id.as_deref(), self.tracking.as_ref()) {
            if tracking.log_gitlab_time_spent && self.provider == "gitlab" {
                let runtime_ms = self.started.elapsed().as_millis().min(i64::MAX as u128) as i64;
                let idle_ms = i64::from(tracking.idle_timeout_minutes) * 60_000;
                let gap_ms = capped_gap_ms(
                    read_cursor(&transaction),
                    scope,
                    &self.approval_fingerprint,
                    &self.provider,
                    &self.repository,
                    self.worktree.as_deref().unwrap_or_default(),
                    &branch,
                    issue_id,
                    self.started_at_ms,
                    idle_ms,
                );
                upsert_branch(
                    &transaction,
                    scope,
                    &self.approval_fingerprint,
                    &self.provider,
                    &self.repository,
                    self.worktree.as_deref().unwrap_or_default(),
                    &branch,
                    issue_id,
                );
                record_session_activity(
                    &transaction,
                    scope,
                    &self.approval_fingerprint,
                    &self.provider,
                    &self.repository,
                    issue_id,
                    self.worktree.as_deref().unwrap_or_default(),
                    &branch,
                    self.started_at_ms,
                    gap_ms.saturating_add(runtime_ms),
                );
            }
        }
        write_cursor(
            &transaction,
            scope,
            &self.approval_fingerprint,
            &self.provider,
            &self.repository,
            self.worktree.as_deref().unwrap_or_default(),
            &branch,
            issue_id.as_deref().unwrap_or_default(),
            now_ms,
        );
        let _ = transaction.commit();
        if let (Some(_), Some(SuccessOutput::LinkedBranch { number, branch })) =
            (self.tracking, output)
        {
            if self.provider == "github" && !self.approval_fingerprint.is_empty() {
                if let Some(worktree) = self.worktree.as_deref() {
                    map_linked_branch(
                        &path,
                        scope,
                        &self.approval_fingerprint,
                        &self.provider,
                        &self.repository,
                        worktree,
                        *number,
                        branch,
                    );
                }
            }
        }
    }
}

struct Cursor {
    scope: String,
    approval_fingerprint: String,
    provider: String,
    repository: String,
    worktree: String,
    branch: String,
    issue_id: String,
    observed_at_ms: i64,
}
fn capped_gap_ms(
    cursor: Option<Cursor>,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    worktree: &str,
    branch: &str,
    issue_id: &str,
    command_started_at_ms: i64,
    idle_limit_ms: i64,
) -> i64 {
    cursor
        .filter(|cursor| {
            cursor.scope == scope
                && cursor.approval_fingerprint == fingerprint
                && cursor.provider == provider
                && cursor.repository == repository
                && cursor.worktree == worktree
                && cursor.branch == branch
                && cursor.issue_id == issue_id
        })
        .map(|cursor| {
            command_started_at_ms
                .saturating_sub(cursor.observed_at_ms)
                .clamp(0, idle_limit_ms)
        })
        .unwrap_or_default()
}

fn current_branch(root: &Path) -> Option<String> {
    let result = runner::run(
        "git",
        &["branch".to_owned(), "--show-current".to_owned()],
        None,
        Some(root),
    )
    .ok()?;
    if !result.success {
        return None;
    }
    let branch = std::str::from_utf8(&result.stdout).ok()?.trim();
    (!branch.is_empty() && branch.len() <= 1024).then(|| branch.to_owned())
}

fn read_cursor(connection: &Connection) -> Option<Cursor> {
    connection.query_row("SELECT scope,approval_fingerprint,provider,repository,worktree,branch,issue_id,observed_at_ms FROM work_tracking_cursor WHERE slot=?1", [CURSOR_SLOT], |row| Ok(Cursor { scope: row.get(0)?, approval_fingerprint: row.get(1)?, provider: row.get(2)?, repository: row.get(3)?, worktree: row.get(4)?, branch: row.get(5)?, issue_id: row.get(6)?, observed_at_ms: row.get(7)? })).optional().ok().flatten()
}

fn mapped_issue(
    connection: &Connection,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    worktree: &str,
    branch: &str,
) -> Option<String> {
    connection.query_row("SELECT issue_id FROM work_tracking_branches WHERE scope=?1 AND approval_fingerprint=?2 AND provider=?3 AND repository=?4 AND worktree=?5 AND branch=?6", rusqlite::params![scope,fingerprint,provider,repository,worktree,branch], |row| row.get(0)).optional().ok().flatten()
}

fn upsert_branch(
    connection: &Connection,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    worktree: &str,
    branch: &str,
    issue_id: &str,
) {
    let _ = connection.execute("INSERT INTO work_tracking_branches(scope,approval_fingerprint,provider,repository,worktree,branch,issue_id) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(scope,approval_fingerprint,provider,repository,worktree,branch) DO UPDATE SET issue_id=excluded.issue_id", rusqlite::params![scope,fingerprint,provider,repository,worktree,branch,issue_id]);
}

fn record_session_activity(
    connection: &Connection,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    issue_id: &str,
    worktree: &str,
    branch: &str,
    started_at_ms: i64,
    add_millis: i64,
) {
    let active_id = connection.query_row("SELECT id FROM work_tracking_sessions WHERE scope=?1 AND approval_fingerprint=?2 AND provider=?3 AND repository=?4 AND issue_id=?5 AND state='active' ORDER BY started_at_ms DESC LIMIT 1", rusqlite::params![scope,fingerprint,provider,repository,issue_id], |row| row.get::<_,String>(0)).optional().ok().flatten();
    if let Some(id) = active_id {
        let _ = connection.execute("UPDATE work_tracking_sessions SET worktree=?1,branch=?2,active_millis=active_millis+?3 WHERE id=?4 AND state='active'", rusqlite::params![worktree,branch,add_millis.max(0),id]);
    } else {
        let _ = connection.execute("INSERT INTO work_tracking_sessions(id,scope,approval_fingerprint,provider,repository,issue_id,worktree,branch,started_at_ms,active_millis,state) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active')", rusqlite::params![uuid::Uuid::new_v4().to_string(),scope,fingerprint,provider,repository,issue_id,worktree,branch,started_at_ms.max(0),add_millis.max(0)]);
    }
}

fn map_linked_branch(
    path: &Path,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    worktree: &str,
    issue_id: u64,
    branch: &str,
) {
    let Ok(connection) = super::store::open(path) else {
        return;
    };
    let issue = issue_id.to_string();
    upsert_branch(
        &connection,
        scope,
        fingerprint,
        provider,
        repository,
        worktree,
        branch,
        &issue,
    );
}

fn write_cursor(
    connection: &Connection,
    scope: &str,
    fingerprint: &str,
    provider: &str,
    repository: &str,
    worktree: &str,
    branch: &str,
    issue_id: &str,
    observed_at_ms: i64,
) {
    let _ = connection.execute("INSERT INTO work_tracking_cursor(slot,scope,approval_fingerprint,provider,repository,worktree,branch,issue_id,observed_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(slot) DO UPDATE SET scope=excluded.scope,approval_fingerprint=excluded.approval_fingerprint,provider=excluded.provider,repository=excluded.repository,worktree=excluded.worktree,branch=excluded.branch,issue_id=excluded.issue_id,observed_at_ms=excluded.observed_at_ms", rusqlite::params![CURSOR_SLOT,scope,fingerprint,provider,repository,worktree,branch,issue_id,observed_at_ms]);
}

fn reset_cursor(
    connection: &Connection,
    invocation: &Invocation,
    branch: &str,
    issue_id: &str,
    now: i64,
) {
    write_cursor(
        connection,
        invocation.scope.as_deref().unwrap_or_default(),
        &invocation.approval_fingerprint,
        &invocation.provider,
        &invocation.repository,
        invocation.worktree.as_deref().unwrap_or_default(),
        branch,
        issue_id,
        now,
    );
}

pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::{Cursor, capped_gap_ms};

    fn cursor() -> Cursor {
        Cursor {
            scope: "/repo".into(),
            approval_fingerprint: "approved".into(),
            provider: "gitlab".into(),
            repository: "https://gitlab.example/group/project".into(),
            worktree: "/repo".into(),
            branch: "feature/task".into(),
            issue_id: "42".into(),
            observed_at_ms: 1_000,
        }
    }

    #[test]
    fn glfl_01_idle_timeout_default_is_ten_minutes() {
        let tracking: super::WorkTracking = serde_json::from_str("{}").unwrap();
        assert_eq!(tracking.idle_timeout_minutes, 10);
        assert!(!tracking.log_gitlab_time_spent);
    }

    #[test]
    fn glfl_02_interval_requires_identical_attribution_and_caps_idle_gap() {
        let timeout = 10 * 60_000;
        let attributed = |cursor| {
            capped_gap_ms(
                Some(cursor),
                "/repo",
                "approved",
                "gitlab",
                "https://gitlab.example/group/project",
                "/repo",
                "feature/task",
                "42",
                900_000,
                timeout,
            )
        };
        assert_eq!(attributed(cursor()), timeout);
        let mut switched = cursor();
        switched.branch = "other".into();
        assert_eq!(attributed(switched), 0);
        let mut another_issue = cursor();
        another_issue.issue_id = "43".into();
        assert_eq!(attributed(another_issue), 0);
        let mut another_project = cursor();
        another_project.repository = "https://gitlab.example/other/project".into();
        assert_eq!(attributed(another_project), 0);
    }
}
