//! GitLab merge request commands retain glab naming, flags, and data semantics.

mod common;

use common::{Fixture, success_json};
use std::process::Command;

const PROJECT: &str = "https://gitlab.com/group/sub/project";

#[test]
fn gitlab_mr_create_sends_description_on_stdin_and_forwards_native_options() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
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
}
#[test]
fn gitlab_mr_update_forwards_native_fields_and_sends_description_on_stdin() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
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
        call.starts_with("api --method PUT projects/group%2Fsub%2Fproject/merge_requests/5 ")
    }));
}

#[test]
fn gitlab_mr_list_uses_native_filters_and_returns_summaries() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
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
    let output = fixture.run_gitlab(&["mr", "view", "5"], "");
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
    let closed = fixture.run_gitlab(&["mr", "close", "5"], "");
    assert_eq!(success_json(&closed)["state"], "closed");
    let reopened = fixture.run_gitlab(&["mr", "reopen", "5"], "");
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
        let output = fixture.run_gitlab(&["mr", verb, "5"], "");
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
    let output = fixture.run_gitlab(&["mr", "approve", "5"], "write-failure");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "gitlab_write_uncertain");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));
}

#[test]
fn gitlab_mr_note_create_sends_body_on_stdin_to_native_glab_command() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
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
    let output = fixture.run_gitlab(
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
    let resolved = fixture.run_gitlab(&["mr", "note", "resolve", "abcdef12", "5"], "");
    assert_eq!(success_json(&resolved)["action"], "discussion resolved");
    let reopened = fixture.run_gitlab(&["mr", "note", "reopen", "12345", "5"], "");
    assert_eq!(success_json(&reopened)["action"], "discussion reopened");
    let calls = fixture.glab_invocations();
    assert!(calls.contains(&format!("mr note resolve abcdef12 5 --repo {PROJECT}")));
    assert!(calls.contains(&format!("mr note reopen 12345 5 --repo {PROJECT}")));
}
#[test]
fn gitlab_mr_merge_forwards_native_options_and_reports_success() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
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
    let explicit_defaults = fixture.run_gitlab(
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
    let output = fixture.run_gitlab(&["mr", "diff", "5", "--raw"], "");
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

    let output = fixture.run_gitlab(&["mr", "checkout", "5"], "mr-checkout-git");
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
#[test]
fn gitlab_mr_approvers_and_linked_issues_use_native_queries() {
    let fixture = Fixture::new();
    let approvers = fixture.run_gitlab(&["mr", "approvers", "5"], "");
    assert_eq!(success_json(&approvers)[0]["username"], "reviewer");
    let issues = fixture.run_gitlab(&["mr", "issues", "5"], "");
    assert_eq!(success_json(&issues)[0]["username"], "reviewer");
    let calls = fixture.glab_invocations();
    assert!(calls.contains(&format!("mr approvers 5 --repo {PROJECT} --output json")));
    assert!(calls.contains(&format!("mr issues 5 --repo {PROJECT} --output json")));
}
#[test]
fn gitlab_mr_note_list_filters_and_update_use_native_subcommands() {
    let fixture = Fixture::new();
    let listed = fixture.run_gitlab(
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
    let updated = fixture.run_gitlab(
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
}
