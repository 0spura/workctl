mod common;

use std::fs;

use common::Fixture;

#[test]
fn rf_cfg_10_unknown_global_endpoint_fails_before_provider_mutation() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(
        fixture.root.join(".workctl.local.json"),
        r#"{"events":{"endpoints":["missing"]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    #[cfg(target_os = "macos")]
    let global_config = home.join("Library/Application Support/workctl/config.json");
    #[cfg(target_os = "windows")]
    let global_config = config.join("workctl/config.json");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let global_config = config.join("workctl/config.json");
    fs::create_dir_all(global_config.parent().unwrap()).unwrap();
    fs::write(&global_config, r#"{"events":{"endpoints":{}}}"#).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&global_config, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let output = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "should not write",
        ],
        "success",
        &home,
        &config,
        &state,
    );

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("selected event endpoint is not configured")
    );
    assert!(
        fs::read_to_string(&fixture.log)
            .unwrap_or_default()
            .is_empty()
    );
}
#[test]
fn rf_wf_3_workflow_only_mutation_persists_event_and_typed_actions() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();

    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        approval.status.success(),
        "{}",
        String::from_utf8_lossy(&approval.stderr)
    );
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Work item",
        ],
        "workflow-actions",
        &home,
        &config,
        &state,
    );
    assert!(
        mutation.status.success(),
        "{}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    assert!(
        fs::read_to_string(&fixture.log)
            .unwrap()
            .contains("issue create")
    );
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(&outbox).unwrap();

    let event: String = connection
        .query_row("SELECT payload FROM events", [], |row| row.get(0))
        .unwrap();
    let event: serde_json::Value = serde_json::from_str(&event).unwrap();
    assert_eq!(event["type"], "workctl.issue.created");
    assert_eq!(event["data"]["outcome"], "confirmed");
    assert!(event["data"].get("body").is_none());
    let action: (String, String, String, String, String) = connection
        .query_row(
            "SELECT id,provider,repository,target_id,action FROM workflow_actions",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        (&action.1, &action.2, &action.3, &action.4),
        (
            &"github".to_owned(),
            &"owner/repo".to_owned(),
            &"7".to_owned(),
            &r#"{"type":"addLabel","label":"triage"}"#.to_owned(),
        )
    );
    let completed_state: String = connection
        .query_row(
            "SELECT state FROM workflow_actions WHERE id=?1",
            [&action.0],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(completed_state, "completed");
    assert!(
        fs::read_to_string(&fixture.log)
            .unwrap()
            .contains("issue edit 7 --repo owner/repo --add-label triage")
    );
    connection
        .execute(
            "UPDATE workflow_actions SET state='uncertain', action=?1 WHERE id=?2",
            rusqlite::params![
                r#"{"type":"comment","body":"private comment body"}"#,
                action.0
            ],
        )
        .unwrap();
    drop(connection);
    let status = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "status"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(status.status.success());
    let status_text = String::from_utf8_lossy(&status.stdout);
    assert!(status_text.contains("workflow approved uncertain=1"));
    let inspection = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "workflow",
            "actions",
            "--limit",
            "10",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(inspection.status.success());
    let inspected = String::from_utf8_lossy(&inspection.stdout);
    assert!(inspected.contains(&action.0));
    assert!(status_text.contains("workflow approved uncertain=1"));
    assert!(inspected.contains("uncertain"));
    assert!(!inspected.contains("private comment body"));
    let resolution = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "workflow",
            "resolve",
            &action.0,
            "--applied",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(resolution.status.success());
    assert!(String::from_utf8_lossy(&resolution.stdout).contains("completed"));
    let connection = rusqlite::Connection::open(&outbox).unwrap();
    let action_state: String = connection
        .query_row(
            "SELECT state FROM workflow_actions WHERE id=?1",
            [&action.0],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(action_state, "completed");
    connection
        .execute(
            "UPDATE workflow_actions SET state='failed' WHERE id=?1",
            [&action.0],
        )
        .unwrap();
    drop(connection);
    let retry = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "workflow",
            "resolve",
            &action.0,
            "--retry",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(retry.status.success());
    assert!(String::from_utf8_lossy(&retry.stdout).contains("pending"));
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let action_state: String = connection
        .query_row(
            "SELECT state FROM workflow_actions WHERE id=?1",
            [&action.0],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(action_state, "pending");
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"changed"}]}]}}"#,
    )
    .unwrap();
    let unapproved_actions = fixture.run_with_event_dirs(
        &[
            "--format",
            "json",
            "--provider",
            "github",
            "workflow",
            "actions",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(!unapproved_actions.status.success());
    assert!(
        String::from_utf8_lossy(&unapproved_actions.stderr)
            .contains("workflow approval is required")
    );
    let unapproved_resolution = fixture.run_with_event_dirs(
        &[
            "--format",
            "json",
            "--provider",
            "github",
            "workflow",
            "resolve",
            &action.0,
            "--retry",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(!unapproved_resolution.status.success());
    assert!(
        String::from_utf8_lossy(&unapproved_resolution.stderr)
            .contains("workflow approval is required")
    );
    let status = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "status"],
        "success",
        &home,
        &config,
        &state,
    );
    let status_text = String::from_utf8_lossy(&status.stdout);
    assert!(status.status.success());
    assert!(status_text.contains("approval required"));
    assert!(!status_text.contains("pending=1"));
}
#[test]
fn rf_wf_1_provider_mutation_cannot_use_stale_workflow_approval() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());

    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"urgent"}]}]}}"#,
    )
    .unwrap();
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Work item",
        ],
        "workflow-actions",
        &home,
        &config,
        &state,
    );
    assert!(mutation.status.success());
    let invocations = fs::read_to_string(&fixture.log).unwrap();
    assert!(invocations.contains("issue create"));
    assert!(!invocations.contains("--add-label triage"));
    assert!(!invocations.contains("--add-label urgent"));
}

#[test]
fn rf_wf_2_gitlab_issue_creation_preserves_assignees_when_adding_configured_assignee() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"},{"type":"addAssignee","assignee":"bob"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "gitlab", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        approval.status.success(),
        "{}",
        String::from_utf8_lossy(&approval.stderr)
    );
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "create",
            "--title",
            "Work item",
            "--description",
            "workflow test",
        ],
        "workflow-assignee-delta",
        &home,
        &config,
        &state,
    );
    assert!(
        mutation.status.success(),
        "{}; {:?}",
        String::from_utf8_lossy(&mutation.stderr),
        fixture.glab_invocations()
    );
    let invocations = fixture.glab_invocations();
    assert!(
        invocations.iter().any(|call| call
            == "issue update 21 --repo https://gitlab.com/group/sub/project --label=triage"),
        "{invocations:?}"
    );
    assert!(
        invocations.iter().any(|call| call
            == "issue update 21 --repo https://gitlab.com/group/sub/project --assignee=+bob"),
        "{invocations:?}"
    );
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let action_state: String = connection
        .query_row("SELECT state FROM workflow_actions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(action_state, "completed");
}

#[test]
fn rf_wf_3_ambiguous_comment_is_inspectable_without_exposing_body() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"comment","body":"private workflow text"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Work item",
        ],
        "workflow-comment-failure",
        &home,
        &config,
        &state,
    );
    assert!(mutation.status.success());
    let private_input = fs::read_to_string(&fixture.input).unwrap();
    assert_eq!(private_input, "private workflow text");
    assert!(
        String::from_utf8_lossy(&mutation.stderr)
            .contains("workflow action needs inspection or retry")
    );
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let action: (String, String) = connection
        .query_row("SELECT id,state FROM workflow_actions", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(action.1, "uncertain");
    drop(connection);
    let inspection = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "actions"],
        "success",
        &home,
        &config,
        &state,
    );
    let inspection_text = String::from_utf8_lossy(&inspection.stdout);
    assert!(inspection.status.success());
    assert!(inspection_text.contains(&action.0));
    assert!(inspection_text.contains("uncertain"));
    assert!(!inspection_text.contains("private workflow text"));
}

#[test]
fn rf_wf_2_github_project_field_action_adds_item_and_sets_value() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"fields":{}}}}},"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"setProjectField","field":"Notes","value":"triaged"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Work item",
        ],
        "workflow-project-actions",
        &home,
        &config,
        &state,
    );
    assert!(
        mutation.status.success(),
        "{}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let invocations = fs::read_to_string(&fixture.log).unwrap();
    assert!(
        invocations.contains(
            "project item-add 3 --owner owner --url https://github.com/owner/repo/issues/7"
        )
    );
    assert!(invocations.contains("project item-edit --project-id PVT_owner_project --url https://github.com/owner/repo/issues/7 --field-id NOTES_ID --text triaged"));
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let action_state: String = connection
        .query_row("SELECT state FROM workflow_actions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(action_state, "completed");
}

#[test]
fn rf_wf_3_desired_state_retry_reconciles_provider_write_that_succeeded() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let mutation_args = [
        "--provider",
        "github",
        "--repo",
        "owner/repo",
        "issue",
        "create",
        "--title",
        "Work item",
    ];
    let first = fixture.run_with_event_dirs(
        &mutation_args,
        "workflow-label-applied-then-failed",
        &home,
        &config,
        &state,
    );
    assert!(first.status.success());
    assert!(
        String::from_utf8_lossy(&first.stderr)
            .contains("workflow action needs inspection or retry")
    );
    let second = fixture.run_with_event_dirs(
        &mutation_args,
        "workflow-label-applied-then-failed",
        &home,
        &config,
        &state,
    );
    assert!(second.status.success());
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let pending: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_actions WHERE state!='completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0);
    let calls = fs::read_to_string(&fixture.log).unwrap();
    assert_eq!(
        calls
            .matches("issue edit 7 --repo owner/repo --add-label triage")
            .count(),
        1
    );
}

#[test]
fn rf_wf_3_github_project_field_action_skips_already_reconciled_value() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"issue":{"project":{"url":"https://github.com/orgs/owner/projects/3","repositories":["owner/repo"],"fields":{}}}}},"workflow":{"rules":[{"on":"pullRequestCreated","actions":[{"type":"setProjectField","field":"Notes","value":"triaged"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "pr",
            "create",
            "--title",
            "Change",
            "--head",
            "feature",
            "--closes",
            "7",
        ],
        "workflow-project-reconciled",
        &home,
        &config,
        &state,
    );
    assert!(
        mutation.status.success(),
        "{}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let invocations = fs::read_to_string(&fixture.log).unwrap();
    assert!(!invocations.contains("project item-add"));
    assert!(!invocations.contains("project item-edit"));
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let action_state: String = connection
        .query_row("SELECT state FROM workflow_actions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(action_state, "completed");
}

#[test]
fn rf_wf_2_pr_event_keeps_only_explicit_issue_references() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"rules":[{"on":"pullRequestMerged","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "github", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let mutation = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "pr",
            "create",
            "--title",
            "Explicit link",
            "--head",
            "feature",
            "--closes",
            "7",
            "--closes",
            "other/repo#6",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        mutation.status.success(),
        "{}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let removed = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "pr",
            "edit",
            "42",
            "--remove-closes",
            "OTHER/REPO#6",
        ],
        "pr-body-closes",
        &home,
        &config,
        &state,
    );
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let merged = fixture.run_with_event_dirs(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "pr",
            "merge",
            "42",
        ],
        "workflow-merge-confirmed",
        &home,
        &config,
        &state,
    );
    assert!(
        merged.status.success(),
        "{}",
        String::from_utf8_lossy(&merged.stderr)
    );
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let targets = connection
        .prepare("SELECT target_id FROM workflow_actions ORDER BY target_id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(targets, ["7"]);
    let invocations = fs::read_to_string(&fixture.log).unwrap();
    assert!(invocations.contains("issue edit 7 --repo owner/repo --add-label triage"));
    assert!(!invocations.contains("pr edit 42 --repo owner/repo --add-label triage"));
    let event: String = connection
        .query_row(
            "SELECT payload FROM events WHERE json_extract(payload,'$.type')='workctl.pull_request.created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let event: serde_json::Value = serde_json::from_str(&event).unwrap();
    assert_eq!(
        event["data"]["relatedIssueReferences"],
        serde_json::json!([
            {"repository":"owner/repo","number":7},
            {"repository":"other/repo","number":6}
        ])
    );
}

#[test]
fn rf_wf_4_gitlab_work_time_is_capped_and_logged_once_on_issue_close() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"workTracking":{"idleTimeoutMinutes":10,"logGitlabTimeSpent":true}}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "gitlab", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        approval.status.success(),
        "{}",
        String::from_utf8_lossy(&approval.stderr)
    );
    let viewed = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "view",
            "12",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        viewed.status.success(),
        "{}",
        String::from_utf8_lossy(&viewed.stderr)
    );
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(&outbox).unwrap();
    connection
        .execute("UPDATE work_tracking_cursor SET observed_at_ms=0", [])
        .unwrap();
    drop(connection);
    let closed = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "close",
            "12",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        closed.status.success(),
        "{}",
        String::from_utf8_lossy(&closed.stderr)
    );
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let session: (i64, Option<i64>, i64, String) = connection.query_row(
        "SELECT started_at_ms,finished_at_ms,active_millis,state FROM work_tracking_sessions WHERE issue_id='12'",
        [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
    ).unwrap();
    assert!(session.0 > 0);
    assert!(session.1.is_some_and(|finished| finished >= session.0));
    assert!(session.2 >= 600_000);
    assert_eq!(session.3, "completed");
    let invocations = fixture.glab_invocations();
    assert_eq!(
        invocations
            .iter()
            .filter(|call| call.as_str()
                == "api --hostname gitlab.com --method POST projects/group%2Fsub%2Fproject/issues/12/add_spent_time --field duration=10m")
            .count(),
        1,
        "{invocations:?}"
    );
}

#[test]
fn rf_wf_4_ambiguous_gitlab_time_entry_is_uncertain_and_not_retried() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workflow":{"workTracking":{"logGitlabTimeSpent":true}}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval = fixture.run_with_event_dirs(
        &["--provider", "gitlab", "workflow", "approve"],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(approval.status.success());
    let viewed = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "view",
            "12",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(viewed.status.success());
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(&outbox).unwrap();
    connection
        .execute("UPDATE work_tracking_cursor SET observed_at_ms=0", [])
        .unwrap();
    drop(connection);
    let closed = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "close",
            "12",
        ],
        "write-failure",
        &home,
        &config,
        &state,
    );
    assert!(
        closed.status.success(),
        "provider mutation must retain its result"
    );
    let connection = rusqlite::Connection::open(&outbox).unwrap();
    let (session_state, action_state, action_id): (String, String, String) = connection
        .query_row(
            "SELECT s.state,a.state,a.id FROM work_tracking_sessions s JOIN workflow_actions a ON json_extract(a.action,'$.session_id')=s.id",
            [],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (session_state.as_str(), action_state.as_str()),
        ("uncertain", "uncertain")
    );
    let update_calls = fixture
        .glab_invocations()
        .into_iter()
        .filter(|call| call.contains("add_spent_time"))
        .collect::<Vec<_>>();
    assert_eq!(update_calls.len(), 1, "{update_calls:?}");
    let resolved = fixture.run_with_event_dirs(
        &[
            "--provider",
            "gitlab",
            "workflow",
            "resolve",
            &action_id,
            "--applied",
        ],
        "success",
        &home,
        &config,
        &state,
    );
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved_state: String = rusqlite::Connection::open(&outbox)
        .unwrap()
        .query_row("SELECT state FROM work_tracking_sessions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(resolved_state, "completed");
}

#[test]
fn rf_evt_1_url_issue_edit_uses_the_write_repository_with_or_without_origin() {
    for with_origin in [true, false] {
        let fixture = Fixture::new();
        fixture.init_git();
        if with_origin {
            assert!(
                std::process::Command::new("git")
                    .args([
                        "remote",
                        "add",
                        "origin",
                        "https://github.com/owner/origin.git"
                    ])
                    .current_dir(&fixture.root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
        fs::write(
            fixture.root.join(".workctl.json"),
            r#"{"workItemProvider":"github","workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .unwrap();
        let home = fixture.root.join("home");
        let config = fixture.root.join("xdg-config");
        let state = fixture.root.join("xdg-state");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&state).unwrap();
        let approval = fixture.run_with_event_dirs(
            &["workflow", "approve"],
            "success",
            &home,
            &config,
            &state,
        );
        assert!(approval.status.success());
        let mutation = fixture.run_with_event_dirs(
            &[
                "--repo-path",
                fixture.root.to_str().unwrap(),
                "issue",
                "edit",
                "https://github.com/owner/repo/issues/7",
                "--title",
                "Updated",
            ],
            "success",
            &home,
            &config,
            &state,
        );
        assert!(
            mutation.status.success(),
            "{}",
            String::from_utf8_lossy(&mutation.stderr)
        );
        let calls = fs::read_to_string(&fixture.log).unwrap();
        assert!(calls.contains("issue edit 7 --repo owner/repo --title Updated"));
        assert!(!calls.contains("owner/origin"));
        #[cfg(target_os = "macos")]
        let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
        #[cfg(target_os = "windows")]
        let outbox = config.join("workctl/state/events.sqlite3");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let outbox = state.join("workctl/events.sqlite3");
        let connection = rusqlite::Connection::open(outbox).unwrap();
        let payload: String = connection
            .query_row("SELECT payload FROM events", [], |row| row.get(0))
            .unwrap();
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["source"], "urn:workctl:github:owner/repo");
        assert_eq!(event["subject"], "/issue/7");
        assert_eq!(event["data"]["repository"], "owner/repo");
        assert_eq!(event["data"]["outcome"], "confirmed");
    }
}

#[test]
fn rf_evt_1_partial_url_issue_edit_keeps_the_confirmed_write_repository() {
    let fixture = Fixture::new();
    fixture.init_git();
    assert!(
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/owner/origin.git"
            ])
            .current_dir(&fixture.root)
            .status()
            .unwrap()
            .success()
    );
    fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"workItemProvider":"github","workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
    )
    .unwrap();
    let home = fixture.root.join("home");
    let config = fixture.root.join("xdg-config");
    let state = fixture.root.join("xdg-state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    let approval =
        fixture.run_with_event_dirs(&["workflow", "approve"], "success", &home, &config, &state);
    assert!(approval.status.success());
    let mutation = fixture.run_with_event_dirs(
        &[
            "--format",
            "json",
            "issue",
            "edit",
            "https://github.com/owner/repo/issues/7",
            "--title",
            "Updated",
            "--state",
            "closed",
            "--comment",
            "Transition note",
        ],
        "workflow-comment-failure",
        &home,
        &config,
        &state,
    );
    assert!(!mutation.status.success());
    let error: serde_json::Value = serde_json::from_slice(&mutation.stderr).unwrap();
    assert_eq!(error["code"], "partial_success");
    assert_eq!(
        error["details"]["resource"]["url"],
        "https://github.com/owner/repo/issues/7"
    );
    let calls = fs::read_to_string(&fixture.log).unwrap();
    assert!(calls.contains("issue edit 7 --repo owner/repo --title Updated"));
    assert!(calls.contains("issue close 7 --repo owner/repo"));
    assert!(!calls.contains("owner/origin"));
    #[cfg(target_os = "macos")]
    let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
    #[cfg(target_os = "windows")]
    let outbox = config.join("workctl/state/events.sqlite3");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let outbox = state.join("workctl/events.sqlite3");
    let connection = rusqlite::Connection::open(outbox).unwrap();
    let payload: String = connection
        .query_row("SELECT payload FROM events", [], |row| row.get(0))
        .unwrap();
    let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(event["source"], "urn:workctl:github:owner/repo");
    assert_eq!(event["subject"], "/issue/7");
    assert_eq!(event["data"]["repository"], "owner/repo");
    assert_eq!(event["data"]["outcome"], "partial");
}

#[test]
fn rf_evt_1_url_issue_edit_rejects_mixed_or_conflicting_repositories_before_io() {
    for targets in [
        vec![
            "https://github.com/owner/repo/issues/7",
            "https://github.com/owner/other/issues/8",
        ],
        vec!["https://github.com/owner/other/issues/7"],
    ] {
        let fixture = Fixture::new();
        let mut args = vec![
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "edit",
        ];
        args.extend(targets);
        args.extend(["--title", "Updated"]);
        let output = fixture.run(&args, "success");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("all edit targets must belong to the selected repository")
        );
        assert!(
            fs::read_to_string(&fixture.log)
                .unwrap_or_default()
                .is_empty()
        );
    }
}
