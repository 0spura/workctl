//! GitLab merge request commands retain glab naming, flags, and data semantics.

mod common;

use common::{Fixture, success_json};
use std::fs;
use std::process::Command;

const PROJECT: &str = "https://gitlab.com/group/sub/project";

#[test]
fn gitlab_mr_create_sends_description_on_stdin_and_forwards_native_options() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "create",
            "--title",
            "Improve provider parity",
            "--description",
            "Native workflow details",
            "--source-branch",
            "feature",
            "--target-branch",
            "main",
            "--draft",
            "--label",
            "backend",
            "--reviewer",
            "reviewer",
            "--related-issue",
            "4",
            "--allow-collaboration=false",
            "--remove-source-branch=false",
            "--squash-before-merge=true",
        ],
        "",
    );
    let result = success_json(&output);
    assert_eq!(result["number"], 6);
    assert_eq!(result["body"], "Created MR description");
    assert_eq!(
        fixture.glab_input(),
        br#"{"description":"Native workflow details"}"#
    );
    let calls = fixture.glab_invocations();
    assert!(
        calls.iter().any(|call| call == &format!(
            "mr create --repo {PROJECT} --title Improve provider parity --description= --yes --no-editor --source-branch=feature --target-branch=main --label=backend --reviewer=reviewer --draft --related-issue=4 --allow-collaboration=false --remove-source-branch=false --squash-before-merge=true"
        )),
        "glab invocations: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.contains("Native workflow details"))
    );
    let compact_fixture = Fixture::new();
    let compact = compact_fixture.run_gitlab(
        &[
            "mr",
            "create",
            "--title",
            "Private details",
            "--description",
            "long description",
        ],
        "",
    );
    assert_eq!(
        String::from_utf8(compact.stdout).expect("UTF-8 text output"),
        "!6 created\n"
    );
}

#[test]
fn gitlab_mr_update_forwards_native_fields_and_sends_description_on_stdin() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "update",
            "5",
            "--title",
            "Updated title",
            "--description",
            "Updated details",
            "--label",
            "backend",
            "--reviewer",
            "reviewer",
            "--target-branch",
            "release",
            "--remove-source-branch=false",
        ],
        "",
    );
    assert_eq!(success_json(&output)["number"], 5);
    assert_eq!(
        fixture.glab_input(),
        br#"{"description":"Updated details"}"#
    );
    let calls = fixture.glab_invocations();
    assert!(calls.iter().any(|call| call == &format!(
        "mr update 5 --repo {PROJECT} --title=Updated title --target-branch=release --label=backend --reviewer=reviewer --remove-source-branch=false"
    )));
    assert!(calls.iter().any(|call| {
        call == "api --method PUT projects/group%2Fsub%2Fproject/merge_requests/5 --hostname gitlab.com --header Content-Type: application/json --input -"
    }));
}

#[test]
fn gitlab_mr_list_uses_native_filters_and_returns_summaries() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "list",
            "--merged",
            "--label",
            "backend",
            "--not-label",
            "wip",
            "--milestone",
            "4",
            "--source-branch",
            "feature",
            "--created-after",
            "2026-01-01",
            "--environment",
            "staging",
            "--order",
            "updated_at",
            "--sort",
            "asc",
            "--page",
            "3",
            "--per-page",
            "7",
        ],
        "",
    );
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "mr list --output json --repo {PROJECT} --merged --label backend --not-label wip \
                 --milestone 4 --source-branch feature --created-after 2026-01-01 \
                 --environment staging --order updated_at --sort asc --page 3 --per-page 7"
            ),
        ]
    );
    let requests = success_json(&output);
    assert_eq!(requests[0]["number"], 5);
    assert_eq!(requests[0]["base_ref"], "main");
    assert_eq!(requests[0]["head_ref"], "feature");
    assert!(requests[0].get("body").is_none());
}

#[test]
fn gitlab_mr_view_maps_native_fields_without_fabricating_github_data() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(&["mr", "view", "5"], "");
    assert_eq!(
        fixture.glab_invocations().last().expect("glab invocation"),
        &format!("mr view --output json --repo {PROJECT} 5"),
    );
    let request = success_json(&output);
    assert_eq!(request["number"], 5);
    assert_eq!(request["state"], "merged");
    assert_eq!(request["body"], "Review details");
    assert_eq!(request["author"], "reviewer");
    assert_eq!(request["review_decision"], serde_json::Value::Null);
    assert_eq!(request["mergeable"], serde_json::Value::Null);
    assert_eq!(request["labels"], serde_json::json!(["backend"]));
}

#[test]
fn gitlab_mr_rejects_github_grammar_before_provider_access() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(&["mr", "list", "--limit", "2"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(fixture.glab_invocations().is_empty());
}

#[test]
fn gitlab_mr_state_changes_use_native_commands() {
    let fixture = Fixture::new();
    let closed = fixture.run_gitlab_json(&["mr", "close", "5"], "");
    assert_eq!(success_json(&closed)["state"], "closed");
    let reopened = fixture.run_gitlab_json(&["mr", "reopen", "5"], "");
    assert_eq!(success_json(&reopened)["state"], "open");
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!("mr close 5 --repo {PROJECT}"),
            "auth status --hostname gitlab.com".to_owned(),
            format!("mr reopen 5 --repo {PROJECT}"),
        ]
    );
}

#[test]
fn gitlab_mr_native_approval_and_subscription_actions_use_glab_verbs() {
    let fixture = Fixture::new();
    for (verb, result) in [
        ("approve", "approve"),
        ("revoke", "revoke"),
        ("rebase", "rebase"),
        ("subscribe", "subscribe"),
        ("unsubscribe", "unsubscribe"),
        ("todo", "todo"),
    ] {
        let output = fixture.run_gitlab_json(&["mr", verb, "5"], "");
        assert_eq!(success_json(&output)["action"], result);
    }
    let invocations = fixture.glab_invocations();
    for verb in [
        "approve",
        "revoke",
        "rebase",
        "subscribe",
        "unsubscribe",
        "todo",
    ] {
        assert!(invocations.contains(&format!("mr {verb} 5 --repo {PROJECT}")));
    }
}

#[test]
fn gitlab_mr_native_action_failure_is_sanitized_without_success_output() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(&["mr", "approve", "5"], "write-failure");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "gitlab_write_uncertain");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));
}

#[test]
fn gitlab_mr_note_create_sends_body_on_stdin_to_native_glab_command() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "note",
            "create",
            "5",
            "--message",
            "review notes contain details",
            "--file",
            "src/lib.rs",
            "--line",
            "10:12",
            "--resolvable=true",
        ],
        "",
    );
    assert_eq!(success_json(&output)["action"], "note added");
    assert_eq!(fixture.glab_input(), b"review notes contain details");
    assert_eq!(
        fixture.glab_invocations().last().expect("note invocation"),
        &format!(
            "mr note create 5 --repo {PROJECT} --file src/lib.rs --line 10:12 --resolvable=true"
        )
    );
}
#[test]
fn gitlab_mr_note_rejects_invalid_native_combinations_before_authentication() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "note",
            "create",
            "5",
            "--message",
            "comment",
            "--file",
            "src/lib.rs",
            "--resolvable=false",
        ],
        "",
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(fixture.glab_invocations().is_empty());
}
#[test]
fn gitlab_mr_discussions_resolve_and_reopen_by_native_identifier() {
    let fixture = Fixture::new();
    let resolved = fixture.run_gitlab_json(&["mr", "note", "resolve", "abcdef12", "5"], "");
    assert_eq!(success_json(&resolved)["action"], "discussion resolved");
    let reopened = fixture.run_gitlab_json(&["mr", "note", "reopen", "12345", "5"], "");
    assert_eq!(success_json(&reopened)["action"], "discussion reopened");
    let calls = fixture.glab_invocations();
    assert!(calls.contains(&format!("mr note resolve 5 abcdef12 --repo {PROJECT}")));
    assert!(calls.contains(&format!("mr note reopen 5 12345 --repo {PROJECT}")));
}
#[test]
fn gitlab_mr_merge_forwards_native_options_and_reports_success() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "merge",
            "5",
            "--auto-merge",
            "--message",
            "Merge feature",
            "--rebase",
            "--remove-source-branch",
            "--sha",
            "abc123",
            "--squash",
            "--squash-message",
            "Squashed feature",
            "--yes",
        ],
        "",
    );
    assert_eq!(success_json(&output)["action"], "merge queued");
    assert!(
        fixture
            .glab_invocations()
            .contains(&format!(
                "mr merge 5 --repo {PROJECT} --auto-merge=true --message Merge feature --rebase --remove-source-branch=true --sha abc123 --squash --squash-message Squashed feature --yes"
            ))
    );
    let explicit_defaults = fixture.run_gitlab_json(
        &[
            "mr",
            "merge",
            "5",
            "--auto-merge=false",
            "--remove-source-branch=false",
        ],
        "",
    );
    assert_eq!(success_json(&explicit_defaults)["action"], "merged");
    assert!(fixture.glab_invocations().contains(&format!(
        "mr merge 5 --repo {PROJECT} --auto-merge=false --remove-source-branch=false"
    )));
}
#[test]
fn gitlab_mr_diff_uses_native_command_and_raw_flag() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(&["mr", "diff", "5", "--raw"], "");
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!("mr diff 5 --repo {PROJECT} --color=never --raw"),
        ]
    );
    let result = success_json(&output);
    assert_eq!(result["number"], 5);
    assert!(result["diff"].as_str().unwrap().contains("+new"));
}

#[test]
fn gitlab_mr_checkout_changes_the_local_branch_without_force() {
    let fixture = Fixture::new();
    let run_git = |args: &[&str]| {
        let result = Command::new("git")
            .args(args)
            .current_dir(&fixture.root)
            .output()
            .expect("run git fixture setup");
        assert!(
            result.status.success(),
            "git setup failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    run_git(&["init", "-q"]);
    run_git(&["config", "user.name", "Workctl Test"]);
    run_git(&["config", "user.email", "workctl-test@example.invalid"]);
    std::fs::write(fixture.root.join("file.txt"), "content").expect("create committed file");
    run_git(&["add", "file.txt"]);
    run_git(&["commit", "-qm", "initial"]);

    let output = fixture.run_gitlab_json(&["mr", "checkout", "5"], "mr-checkout-git");
    assert_eq!(success_json(&output)["number"], 5);
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&fixture.root)
        .output()
        .expect("read checked-out branch");
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "mr-5");
    assert_eq!(
        fixture.glab_invocations().last().expect("glab invocation"),
        &format!("mr checkout 5 --repo {PROJECT}")
    );
}
/// RF-GL.1: `mr issues` lists GitLab-related issues, including responses without `references`.
#[test]
fn gitlab_mr_approvers_and_related_issues_use_native_queries() {
    let fixture = Fixture::new();
    let approvers = fixture.run_gitlab_json(&["mr", "approvers", "5"], "");
    assert_eq!(success_json(&approvers)[0]["id"], 101);
    assert_eq!(success_json(&approvers)[0]["username"], "reviewer");
    let issues = fixture.run_gitlab_json(&["mr", "issues", "5"], "");
    assert_eq!(success_json(&issues)[0]["iid"], 21);
    assert_eq!(success_json(&issues)[0]["id"], 2101);
    assert!(success_json(&issues)[0].get("references").is_none());
    assert_eq!(success_json(&issues)[0]["title"], "Related issue");
    let calls = fixture.glab_invocations();
    assert!(calls.contains(&format!("mr approvers 5 --repo {PROJECT} --output json")));
    assert!(calls.iter().any(|call| {
        call == "api --hostname gitlab.com projects/group%2Fsub%2Fproject/merge_requests/5/related_issues"
    }));
    let text_fixture = Fixture::new();
    let approvers = text_fixture.run_gitlab(&["mr", "approvers", "5"], "");
    assert_eq!(
        String::from_utf8(approvers.stdout).expect("UTF-8 text output"),
        "!5 approver #101 reviewer\n"
    );
    let issues = text_fixture.run_gitlab(&["mr", "issues", "5"], "");
    assert_eq!(
        String::from_utf8(issues.stdout).expect("UTF-8 text output"),
        "!5 issue #21 Related issue\n"
    );
}
#[test]
fn gitlab_mr_note_list_filters_and_update_use_native_subcommands() {
    let fixture = Fixture::new();
    let listed = fixture.run_gitlab_json(
        &[
            "mr",
            "note",
            "list",
            "5",
            "--state",
            "unresolved",
            "--type",
            "diff",
            "--file",
            "src/lib.rs",
        ],
        "",
    );
    assert_eq!(success_json(&listed)[0]["id"], "abcdef123456");
    let updated = fixture.run_gitlab_json(
        &[
            "mr",
            "note",
            "update",
            "5",
            "12345",
            "--message",
            "Revised note",
        ],
        "",
    );
    assert_eq!(success_json(&updated)["action"], "note updated");
    assert_eq!(fixture.glab_input(), b"Revised note");
    let calls = fixture.glab_invocations();
    assert!(calls.contains(&format!(
        "mr note list 5 --repo {PROJECT} --output json --state unresolved --type diff --file src/lib.rs"
    )));
    assert!(calls.contains(&format!("mr note update 5 12345 --repo {PROJECT}")));
    let text_fixture = Fixture::new();
    let notes = text_fixture.run_gitlab(&["mr", "note", "list", "5"], "");
    assert_eq!(
        String::from_utf8(notes.stdout).expect("UTF-8 text output"),
        "!5 note #abcdef123456/12345: Review note\n"
    );
}

// RF-CFG.6: configured GitLab merge-request defaults fill the options the caller omitted.
#[test]
fn rf_cfg_6_gitlab_mr_create_applies_configured_defaults() {
    let fixture = Fixture::new();
    fixture.init_git();
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"gitlab":{"mr":{"labels":["backend","triage"],"assignees":["default-user"],"reviewers":["default-reviewer"],"targetBranch":"develop","draft":true}}}}"#,
    )
    .expect("write GitLab merge-request defaults");

    let output = fixture.run_gitlab_json(
        &[
            "mr", "create", "--title", "Defaults", "--label", "triage", "--label", "docs",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fixture.glab_invocations();
    assert!(
        calls.iter().any(|call| call
            == &format!(
                "mr create --repo {PROJECT} --title Defaults --description= --yes --no-editor \
             --target-branch=develop --assignee=default-user --label=backend --label=triage \
             --label=docs --reviewer=default-reviewer --draft"
            )),
        "glab invocations: {calls:?}"
    );
}

// RF-CFG.6: explicit assignee, reviewer, and target-branch flags replace the configured defaults;
// no inverse flag exists for the configured draft, so it still applies.
#[test]
fn rf_cfg_6_gitlab_mr_create_explicit_flags_replace_configured_defaults() {
    let fixture = Fixture::new();
    fixture.init_git();
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"gitlab":{"mr":{"labels":["backend"],"assignees":["default-user"],"reviewers":["default-reviewer"],"targetBranch":"develop","draft":true}}}}"#,
    )
    .expect("write GitLab merge-request defaults");

    let output = fixture.run_gitlab_json(
        &[
            "mr",
            "create",
            "--title",
            "Override",
            "--target-branch",
            "main",
            "--assignee",
            "alice",
            "--reviewer",
            "bob",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fixture.glab_invocations();
    assert!(
        calls.iter().any(|call| call
            == &format!(
                "mr create --repo {PROJECT} --title Override --description= --yes --no-editor \
             --target-branch=main --assignee=alice --label=backend --reviewer=bob --draft"
            )),
        "glab invocations: {calls:?}"
    );
    assert!(!calls.iter().any(|call| call.contains("default-user")
        || call.contains("default-reviewer")
        || call.contains("develop")));
}

#[test]
fn rf_wf_2_gitlab_mr_explicit_related_issue_drives_actions_and_persisted_references() {
    for related_issue in [Some("12"), None] {
        let fixture = Fixture::new();
        fixture.init_git();
        fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
        fs::write(
            fixture.root.join(".workctl.json"),
            r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","workflow":{"rules":[{"on":"pullRequestCreated","actions":[{"type":"addLabel","label":"triage"}]},{"on":"pullRequestMerged","actions":[{"type":"addAssignee","assignee":"alice"}]}]}}"#,
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
        assert!(
            approval.status.success(),
            "{}",
            String::from_utf8_lossy(&approval.stderr)
        );
        let mut args = vec![
            "--repo",
            "group/sub/project",
            "mr",
            "create",
            "--title",
            "Explicit workflow target",
        ];
        if let Some(issue) = related_issue {
            args.extend(["--related-issue", issue]);
        }
        let created = fixture.run_with_event_dirs(&args, "success", &home, &config, &state);
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        #[cfg(target_os = "macos")]
        let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
        #[cfg(target_os = "windows")]
        let outbox = config.join("workctl/state/events.sqlite3");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let outbox = state.join("workctl/events.sqlite3");
        let connection = rusqlite::Connection::open(&outbox).unwrap();
        let payload: String = connection
            .query_row("SELECT payload FROM events", [], |row| row.get(0))
            .unwrap();
        let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(event["type"], "workctl.pull_request.created");
        assert_eq!(event["source"], "urn:workctl:gitlab:group/sub/project");
        assert_eq!(event["subject"], "/pull_request/6");
        let references = connection
            .prepare("SELECT source,subject,repository,number FROM workflow_pr_references")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if related_issue.is_some() {
            assert_eq!(
                event["data"]["relatedIssueReferences"],
                serde_json::json!([{"repository": PROJECT, "number": 12}])
            );
            assert_eq!(
                references,
                vec![(
                    "urn:workctl:gitlab:group/sub/project".to_owned(),
                    "/pull_request/6".to_owned(),
                    PROJECT.to_owned(),
                    12,
                )]
            );
        } else {
            assert!(event["data"].get("relatedIssueReferences").is_none());
            assert!(references.is_empty());
        }
        drop(connection);
        let merged = fixture.run_with_event_dirs(
            &["--repo", "group/sub/project", "mr", "merge", "6"],
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
        let connection = rusqlite::Connection::open(&outbox).unwrap();
        let actions = connection
            .prepare(
                "SELECT provider,repository,target_resource,target_id,state FROM workflow_actions ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let calls = fixture.glab_invocations();
        assert!(!calls.iter().any(|call| call.contains("related_issues")));
        assert!(!calls.iter().any(|call| call.starts_with("mr issues")));
        if related_issue.is_some() {
            assert_eq!(actions.len(), 2, "{actions:?}");
            for action in actions {
                assert_eq!(
                    (
                        action.0.as_str(),
                        action.1.as_str(),
                        action.2.as_str(),
                        action.3.as_str(),
                    ),
                    ("gitlab", PROJECT, "issue", "12")
                );
                // Opportunistic dispatch is bounded; a persisted pending action is replayable.
                assert!(
                    matches!(action.4.as_str(), "pending" | "completed"),
                    "{action:?}"
                );
            }
        } else {
            assert!(actions.is_empty());
            assert_eq!(
                calls,
                vec![
                    "auth status --hostname gitlab.com".to_owned(),
                    format!(
                        "mr create --repo {PROJECT} --title Explicit workflow target --description= --yes --no-editor"
                    ),
                    format!("mr view --output json --repo {PROJECT} 6"),
                    "auth status --hostname gitlab.com".to_owned(),
                    format!("mr merge 6 --repo {PROJECT}"),
                    format!("mr view --output json --repo {PROJECT} 6"),
                ]
            );
        }
    }
}

// RF-WF.2: merge and approval workflows follow the confirmed provider outcome, not the verb.
#[test]
fn rf_wf_2_gitlab_mr_transition_receipts_drive_only_the_matching_workflow() {
    let cases: &[(&[&str], &str, &str, &str, usize)] = &[
        (
            &["merge", "6"],
            "workflow-merge-unconfirmed",
            "merge queued",
            "updated",
            0,
        ),
        (
            &["merge", "6", "--auto-merge=false"],
            "workflow-merge-unconfirmed",
            "merge queued",
            "updated",
            0,
        ),
        (
            &["merge", "6", "--auto-merge"],
            "workflow-merge-confirmed",
            "merge queued",
            "updated",
            0,
        ),
        (
            &["merge", "6"],
            "workflow-merge-confirmed",
            "merged",
            "merged",
            1,
        ),
        (
            &["merge", "6"],
            "workflow-merge-read-failure",
            "merge queued",
            "updated",
            0,
        ),
        (
            &["merge", "6"],
            "workflow-merge-malformed",
            "merge queued",
            "updated",
            0,
        ),
        (
            &["approve", "6"],
            "success",
            "approve",
            "review_approved",
            1,
        ),
        (&["revoke", "6"], "success", "revoke", "updated", 0),
    ];
    for &(command, mode, expected_action, expected_operation, expected_actions) in cases {
        let fixture = Fixture::new();
        fixture.init_git();
        fs::write(fixture.root.join(".gitignore"), ".workctl.local.json\n").unwrap();
        fs::write(
            fixture.root.join(".workctl.json"),
            r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","workflow":{"rules":[{"on":"pullRequestMerged","actions":[{"type":"addLabel","label":"merged"}]},{"on":"reviewApproved","actions":[{"type":"addLabel","label":"approved"}]}]}}"#,
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
        assert!(
            approval.status.success(),
            "{}",
            String::from_utf8_lossy(&approval.stderr)
        );
        let created = fixture.run_with_event_dirs(
            &[
                "--repo",
                "group/sub/project",
                "mr",
                "create",
                "--title",
                "Transition workflow target",
                "--related-issue",
                "12",
            ],
            "success",
            &home,
            &config,
            &state,
        );
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        fs::write(&fixture.glab_log, "").unwrap();
        let mut args = vec!["--repo", "group/sub/project", "--format", "json", "mr"];
        args.extend_from_slice(command);
        let transitioned = fixture.run_with_event_dirs(&args, mode, &home, &config, &state);
        assert_eq!(
            success_json(&transitioned)["action"],
            expected_action,
            "{mode}"
        );
        #[cfg(target_os = "macos")]
        let outbox = home.join("Library/Application Support/workctl/state/events.sqlite3");
        #[cfg(target_os = "windows")]
        let outbox = config.join("workctl/state/events.sqlite3");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let outbox = state.join("workctl/events.sqlite3");
        let connection = rusqlite::Connection::open(&outbox).unwrap();
        let payloads = connection
            .prepare(
                "SELECT payload FROM events WHERE json_extract(payload,'$.subject')='/pull_request/6' AND json_extract(payload,'$.type') <> 'workctl.pull_request.created'",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(payloads.len(), 1, "{command:?} {mode}");
        let event: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
        assert_eq!(
            event["type"],
            format!("workctl.pull_request.{expected_operation}")
        );
        assert_eq!(event["source"], "urn:workctl:gitlab:group/sub/project");
        assert_eq!(event["data"]["operation"], expected_operation);
        assert_eq!(event["data"]["outcome"], "confirmed");
        let actions = connection
            .prepare(
                "SELECT event_id,provider,repository,target_resource,target_id,action FROM workflow_actions",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(actions.len(), expected_actions, "{command:?} {mode}");
        for (event_id, provider, repository, resource, target_id, action) in actions {
            assert_eq!(event_id, event["id"].as_str().unwrap());
            assert_eq!(provider, "gitlab");
            assert_eq!(repository, PROJECT);
            assert_eq!(resource, "issue");
            assert_eq!(target_id, "12");
            let action: serde_json::Value = serde_json::from_str(&action).unwrap();
            assert_eq!(action["type"], "addLabel");
            assert_eq!(
                action["label"],
                if expected_operation == "merged" {
                    "merged"
                } else {
                    "approved"
                }
            );
        }
        let calls = fixture.glab_invocations();
        let views = calls
            .iter()
            .filter(|call| call.starts_with("mr view "))
            .collect::<Vec<_>>();
        let needs_confirmation = command[0] == "merge" && !command.contains(&"--auto-merge");
        assert_eq!(views.len(), usize::from(needs_confirmation), "{calls:?}");
        if needs_confirmation {
            assert_eq!(
                *views[0],
                format!("mr view --output json --repo {PROJECT} 6")
            );
        }
        if expected_actions == 0 {
            assert!(
                !calls.iter().any(|call| call.starts_with("issue update ")),
                "{calls:?}"
            );
        }
    }
}
