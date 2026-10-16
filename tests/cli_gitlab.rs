//! GitLab provider contract: the grammar mirrors `glab`, and issue reads map onto the shared
//! shape without exposing provider diagnostics.

mod common;

use common::{Fixture, error_json, success_json};
use std::fs;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::thread;

const PROJECT: &str = "https://gitlab.com/group/sub/project";

#[test]
fn configured_code_and_work_item_providers_route_independently() {
    let fixture = Fixture::new();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&fixture.root)
            .status()
            .expect("initialize fixture repository")
            .success()
    );
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"github"}"#,
    )
    .expect("write mixed provider config");

    let issues = fixture.run_issue_json(&["issue", "list"], "");
    assert_eq!(success_json(&issues).as_array().unwrap().len(), 2);
    assert!(fixture.glab_invocations().is_empty());

    let merge_requests = fixture.run_gitlab_json(&["mr", "list"], "");
    assert_eq!(
        success_json(&merge_requests)[0]["url"],
        format!("{PROJECT}/-/merge_requests/5")
    );
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!("mr list --output json --repo {PROJECT} --page 1 --per-page 30")
        ]
    );
}

#[test]
fn unsupported_linear_configuration_fails_before_provider_access() {
    let fixture = Fixture::new();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&fixture.root)
            .status()
            .expect("initialize fixture repository")
            .success()
    );
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"linear"}"#,
    )
    .expect("write unsupported provider config");

    let output = fixture.run_gitlab_json(&["issue", "list"], "");
    assert_eq!(error_json(&output)["code"], "config");
    assert!(fixture.glab_invocations().is_empty());
    assert!(
        std::fs::read_to_string(&fixture.log)
            .unwrap_or_default()
            .is_empty()
    );
}

#[test]
fn gitlab_list_forwards_every_filter_to_glab() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "list",
            "--all",
            "--label",
            "bug",
            "--not-label",
            "wontfix",
            "--assignee",
            "@me",
            "--not-assignee",
            "bob",
            "--not-author",
            "mallory",
            "--confidential",
            "--issue-type",
            "incident",
            "--iteration",
            "3",
            "--in",
            "title,description",
            "--search",
            "crash",
            "--order",
            "weight",
            "--sort",
            "asc",
            "--page",
            "2",
            "--per-page",
            "5",
        ],
        "",
    );

    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "issue list --output json --repo {PROJECT} --all --confidential --label bug \
                 --not-label wontfix --not-assignee bob --not-author mallory --assignee @me \
                 --search crash --in title,description --issue-type incident --iteration 3 \
                 --order weight --sort asc --page 2 --per-page 5"
            ),
        ]
    );

    let issues = success_json(&output);
    let issues = issues.as_array().expect("issue summaries");
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0]["number"], 8);
    assert_eq!(issues[0]["title"], "First");
    assert_eq!(issues[0]["state"], "open");
    assert!(issues[0].get("body").is_none());
    assert_eq!(issues[1]["number"], 9);
    assert_eq!(issues[1]["state"], "closed");
    assert_eq!(issues[1]["updated_at"], "2026-01-04T00:00:00.000Z");
}

#[test]
fn gitlab_view_maps_the_description_onto_the_body() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(&["issue", "view", "12"], "");

    assert_eq!(
        fixture.glab_invocations().last().expect("glab invocation"),
        &format!("issue view --output json --repo {PROJECT} 12")
    );

    let issue = success_json(&output);
    assert_eq!(issue["number"], 12);
    assert_eq!(issue["title"], "Crash on startup");
    assert_eq!(issue["body"], "Steps to reproduce");
    assert_eq!(issue["state"], "open");
    assert_eq!(issue["url"], format!("{PROJECT}/-/issues/12"));
    assert_eq!(issue["created_at"], "2026-01-02T03:04:05.000Z");
}

/// RF-WI.13: a supplied slug gets the issue IID prefix and creates a remote GitLab branch.
#[test]
fn gitlab_issue_develop_prefixes_slug_and_lists_iid_branches() {
    let fixture = Fixture::new();
    let created = fixture.run_gitlab_json(
        &[
            "issue",
            "develop",
            "12",
            "--name",
            "auth-session-expiry",
            "--base",
            "main",
        ],
        "",
    );
    assert_eq!(success_json(&created)["branch"], "12-auth-session-expiry");
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!("issue view --output json --repo {PROJECT} 12"),
            "api --hostname gitlab.com --method POST projects/group%2Fsub%2Fproject/repository/branches --raw-field branch=12-auth-session-expiry --raw-field ref=main".to_owned(),
        ]
    );

    let listed = fixture.run_gitlab_json(&["issue", "develop", "12", "--list"], "develop-list");
    assert_eq!(
        success_json(&listed)["branches"],
        serde_json::json!(["12-auth-session-expiry", "12-second-fix"])
    );
    assert!(
        fixture
            .glab_invocations()
            .last()
            .unwrap()
            .contains("--field search=12- --field per_page=100 --paginate")
    );
}

/// RF-WI.13: omitting the slug follows GitLab's configured issue branch template.
#[test]
fn gitlab_issue_develop_uses_project_default_branch_template() {
    let fixture = Fixture::new();
    let created = fixture.run_gitlab_json(&["issue", "develop", "12"], "");
    assert_eq!(success_json(&created)["branch"], "12-crash-on-startup");
    let invocations = fixture.glab_invocations();
    assert!(
        invocations
            .contains(&"api --hostname gitlab.com projects/group%2Fsub%2Fproject".to_owned())
    );
    assert!(
        invocations
            .last()
            .unwrap()
            .contains("branch=12-crash-on-startup --raw-field ref=main")
    );
}

/// RF-WI.13: malformed slugs fail before provider access; remote failures never report success.
#[test]
fn gitlab_issue_develop_rejects_bad_slug_and_handles_write_failure() {
    let fixture = Fixture::new();
    let invalid =
        fixture.run_gitlab_json(&["issue", "develop", "12", "--name", "Auth/session"], "");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(fixture.glab_invocations().is_empty());

    let failed = fixture.run_gitlab_json(
        &["issue", "develop", "12", "--name", "auth-session-expiry"],
        "develop-api-failure",
    );
    assert_eq!(error_json(&failed)["code"], "gitlab_write_uncertain");
    assert!(failed.stdout.is_empty());
}

/// RF-WI.13: checkout is opt-in and requires a matching local GitLab worktree before the write.
#[test]
fn gitlab_issue_develop_checkout_preflights_the_local_remote() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "develop",
            "12",
            "--name",
            "auth-session-expiry",
            "--checkout",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert_eq!(
        fixture.glab_invocations(),
        ["auth status --hostname gitlab.com"]
    );
}

#[test]
fn gitlab_view_text_omits_provider_metadata_dump() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(&["issue", "view", "12"], "");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("text output is UTF-8");
    assert!(text.contains("Crash on startup"));
    assert!(text.contains("Steps to reproduce"));
    assert!(!text.contains("created_at"));
    assert!(!text.contains("updated_at"));
    assert!(!text.contains("url"));
    assert!(!text.contains("author"));
}

#[test]
fn gitlab_state_flags_select_the_requested_state() {
    let fixture = Fixture::new();
    fixture.run_gitlab(&["issue", "list"], "");
    fixture.run_gitlab(&["issue", "list", "--closed"], "");
    fixture.run_gitlab(&["issue", "list", "--all", "--closed"], "");
    let invocations = fixture.glab_invocations();
    assert_eq!(
        invocations[1],
        format!("issue list --output json --repo {PROJECT} --page 1 --per-page 30")
    );
    assert_eq!(
        invocations[3],
        format!("issue list --output json --repo {PROJECT} --closed --page 1 --per-page 30")
    );
    assert_eq!(
        invocations[5],
        format!("issue list --output json --repo {PROJECT} --all --closed --page 1 --per-page 30")
    );
}

#[test]
fn gitlab_issue_state_and_subscription_use_native_commands() {
    let fixture = Fixture::new();
    let closed = fixture.run_gitlab_json(&["issue", "close", "12"], "");
    assert_eq!(success_json(&closed)["state"], "closed");
    let opened = fixture.run_gitlab_json(&["issue", "reopen", "12"], "");
    assert_eq!(success_json(&opened)["state"], "open");
    let subscribed = fixture.run_gitlab_json(&["issue", "subscribe", "12"], "");
    assert_eq!(success_json(&subscribed)["state"], "subscribed");
    let unsubscribed = fixture.run_gitlab_json(&["issue", "unsubscribe", "12"], "");
    assert_eq!(success_json(&unsubscribed)["state"], "unsubscribed");
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!("issue close 12 --repo {PROJECT}"),
            "auth status --hostname gitlab.com".to_owned(),
            format!("issue reopen 12 --repo {PROJECT}"),
            "auth status --hostname gitlab.com".to_owned(),
            format!("issue subscribe 12 --repo {PROJECT}"),
            "auth status --hostname gitlab.com".to_owned(),
            format!("issue unsubscribe 12 --repo {PROJECT}"),
        ]
    );
}

#[test]
fn gitlab_issue_note_sends_message_on_stdin() {
    let fixture = Fixture::new();
    let output =
        fixture.run_gitlab_json(&["issue", "note", "12", "--message", "Investigating"], "");
    assert_eq!(success_json(&output)["target"], "issue");
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            "api --method POST projects/group%2Fsub%2Fproject/issues/12/notes --hostname gitlab.com --header Content-Type: application/json --input -"
                .to_owned(),
        ]
    );
    assert_eq!(fixture.glab_input(), br#"{"body":"Investigating"}"#);
}
#[test]
fn gitlab_grammar_rejects_github_flags_and_invalid_page() {
    let fixture = Fixture::new();
    let unknown_flag = fixture.run_gitlab_json(&["issue", "list", "--limit", "5"], "");
    assert_eq!(unknown_flag.status.code(), Some(2));
    assert_eq!(error_json(&unknown_flag)["code"], "invalid_input");

    let invalid_page = fixture.run_gitlab_json(&["issue", "list", "--per-page", "101"], "");
    assert_eq!(invalid_page.status.code(), Some(2));
    assert_eq!(error_json(&invalid_page)["code"], "invalid_input");

    let pr_group = fixture.run_gitlab_json(&["pr", "list"], "");
    assert_eq!(pr_group.status.code(), Some(2));
    let error = error_json(&pr_group);
    assert_eq!(error["code"], "invalid_input");
    assert!(
        error["message"].as_str().expect("message").contains("mr"),
        "the GitLab grammar must name `mr` for merge requests, got {error}"
    );

    assert!(fixture.glab_invocations().is_empty());
}

#[test]
fn gitlab_failures_report_stable_codes_and_the_sanitized_provider_cause() {
    let fixture = Fixture::new();
    let unauthenticated = fixture.run_gitlab_json(&["issue", "list"], "auth-failure");
    assert_eq!(error_json(&unauthenticated)["code"], "authentication");
    assert!(
        !String::from_utf8_lossy(&unauthenticated.stderr).contains("private provider diagnostic"),
        "authentication failures keep provider stderr private"
    );

    let fixture = Fixture::new();
    let failed = fixture.run_gitlab_json(&["issue", "list"], "list-failure");
    let error = error_json(&failed);
    assert_eq!(error["code"], "gitlab_cli");
    assert_eq!(error["message"], "the GitLab CLI operation failed");
    assert_eq!(error["cause"], "private provider diagnostic");
}

#[test]
fn gitlab_rejects_an_invalid_issue_number_before_glab() {
    let fixture = Fixture::new();
    for number in ["0", "abc"] {
        let output = fixture.run_gitlab_json(&["issue", "view", number], "");
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(error_json(&output)["code"], "invalid_input");
    }
    assert!(fixture.glab_invocations().is_empty());
}

#[test]
fn rf_gl_4_gitlab_create_uses_native_flags_and_stdin_description() {
    let fixture = Fixture::new();
    let description = "First line\n--title=must remain data\n";
    let description_file = fixture.root.join("create-description.txt");
    std::fs::write(&description_file, description).expect("write description fixture");
    let description_path = description_file.to_str().expect("UTF-8 fixture path");
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "New issue",
            "--description-file",
            description_path,
            "--label",
            "bug",
            "--label",
            "triage",
            "--assignee",
            "alice",
            "--milestone",
            "M1",
            "--confidential",
            "--weight",
            "0",
            "--due-date",
            "2026-02-28",
            "--epic",
            "7",
            "--linked-issues",
            "8,9",
            "--link-type",
            "relates_to",
            "--linked-mr",
            "6",
            "--time-estimate",
            "1h",
            "--time-spent",
            "30m",
            "--template",
            "bug",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "issue create --repo {PROJECT} --title=New issue --description= --yes \
                 --label=bug --label=triage --assignee=alice --milestone=M1 --confidential \
                 --weight=0 --due-date=2026-02-28 --epic=7 --linked-issues=8,9 \
                 --link-type=relates_to --linked-mr=6 --time-estimate=1h --time-spent=30m \
                 --template=bug"
            ),
            "api --method PUT projects/group%2Fsub%2Fproject/issues/21 --hostname gitlab.com --header Content-Type: application/json --input -"
                .to_owned(),
            format!("issue update 21 --repo {PROJECT} --weight=0"),
            format!("issue view --output json --repo {PROJECT} 21"),
        ]
    );
    assert_eq!(
        fixture.glab_input(),
        br#"{"description":"First line\n--title=must remain data\n"}"#
    );
    let issue = success_json(&output);
    assert_eq!(issue["number"], 21);
    assert_eq!(issue["body"], "Created description");
    assert_eq!(issue["url"], format!("{PROJECT}/-/issues/21"));
}

#[test]
fn rf_gl_5_gitlab_update_uses_native_flags_and_reads_back_the_issue() {
    let fixture = Fixture::new();
    let description = "Replacement\nwith exact bytes\n";
    let description_file = fixture.root.join("update-description.txt");
    std::fs::write(&description_file, description).expect("write description fixture");
    let description_path = description_file.to_str().expect("UTF-8 fixture path");
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "update",
            "12",
            "--title",
            "Updated title",
            "--description-file",
            description_path,
            "--label",
            "ready",
            "--unlabel",
            "triage",
            "--assignee=+alice",
            "--assignee=-bob",
            "--milestone",
            "",
            "--public",
            "--weight",
            "0",
            "--due-date",
            "2026-02-28",
        ],
        "description-updated",
    );

    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "issue update 12 --repo {PROJECT} --title=Updated title \
                 --label=ready --unlabel=triage --assignee=+alice --assignee=-bob --milestone= \
                 --public --weight=0 --due-date=2026-02-28"
            ),
            "api --method PUT projects/group%2Fsub%2Fproject/issues/12 --hostname gitlab.com --header Content-Type: application/json --input -"
                .to_owned(),
            format!("issue view --output json --repo {PROJECT} 12"),
        ]
    );
    assert_eq!(
        fixture.glab_input(),
        br#"{"description":"Replacement\nwith exact bytes\n"}"#
    );
    let issue = success_json(&output);
    assert_eq!(issue["number"], 12);
    assert_eq!(issue["body"], "Replacement\nwith exact bytes\n");
}

#[test]
fn gitlab_writes_reject_empty_or_invalid_changes_before_authentication() {
    let fixture = Fixture::new();
    let empty_description = fixture.root.join("empty-description.txt");
    std::fs::write(&empty_description, "").expect("write empty description fixture");
    let empty_path = empty_description.to_str().expect("UTF-8 fixture path");
    let invalid = [
        fixture.run_gitlab(&["issue", "create", "--title", "No description"], ""),
        fixture.run_gitlab(&["issue", "update", "12"], ""),
        fixture.run_gitlab(
            &["issue", "update", "12", "--description-file", empty_path],
            "",
        ),
        fixture.run_gitlab(
            &["issue", "update", "12", "--assignee", "alice", "--unassign"],
            "",
        ),
        fixture.run_gitlab(&["issue", "update", "12", "--due-date", "2026-02-30"], ""),
        fixture.run_gitlab(
            &[
                "issue",
                "create",
                "--title",
                "Bad epic",
                "--description",
                "",
                "--epic",
                "0",
            ],
            "",
        ),
        fixture.run_gitlab(
            &[
                "issue",
                "create",
                "--title",
                "Bad MR",
                "--description",
                "",
                "--linked-mr",
                "0",
            ],
            "",
        ),
    ];
    for output in invalid {
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    assert!(fixture.glab_invocations().is_empty());
}

#[test]
fn gitlab_description_dash_is_data_not_an_editor_request() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &["issue", "create", "--title", "Dash", "--description", "-"],
        "",
    );
    assert!(output.status.success());
    assert_eq!(fixture.glab_input(), br#"{"description":"-"}"#);
    assert!(
        fixture
            .glab_invocations()
            .iter()
            .any(|invocation| invocation == "api --method PUT projects/group%2Fsub%2Fproject/issues/21 --hostname gitlab.com --header Content-Type: application/json --input -")
    );
}

#[test]
fn gitlab_description_patch_failure_is_reported_as_uncertain_without_diagnostics() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "Native description",
            "--description",
            "private body",
        ],
        "description-write-failure",
    );
    assert_eq!(error_json(&output)["code"], "gitlab_write_uncertain");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));
    assert!(
        !fixture
            .glab_invocations()
            .iter()
            .any(|entry| entry.contains("private body"))
    );
}
#[test]
fn gitlab_write_uncertainty_hides_diagnostics_and_rejects_untrusted_create_url() {
    let fixture = Fixture::new();
    let description = fixture.root.join("description.txt");
    std::fs::write(&description, "create").expect("write description fixture");
    let description_path = description.to_str().expect("UTF-8 fixture path");
    let failed = fixture.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "New",
            "--description-file",
            description_path,
        ],
        "write-failure",
    );
    assert_eq!(error_json(&failed)["code"], "gitlab_write_uncertain");
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private provider diagnostic"));
    assert_eq!(fixture.glab_invocations().len(), 2);

    let fixture = Fixture::new();
    let description = fixture.root.join("description.txt");
    std::fs::write(&description, "create").expect("write description fixture");
    let description_path = description.to_str().expect("UTF-8 fixture path");
    let bad_url = fixture.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "New",
            "--description-file",
            description_path,
        ],
        "bad-create-url",
    );
    assert_eq!(error_json(&bad_url)["code"], "gitlab_write_uncertain");
    assert_eq!(fixture.glab_invocations().len(), 2);
}
#[test]
fn gitlab_issue_create_accepts_current_work_item_url() {
    let fixture = Fixture::new();
    let description = fixture.root.join("description.txt");
    std::fs::write(&description, "create").expect("write description fixture");
    let description_path = description.to_str().expect("UTF-8 fixture path");
    let output = fixture.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "New",
            "--description-file",
            description_path,
        ],
        "work-items-create-url",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let issue = success_json(&output);
    assert_eq!(issue["number"], 21);
    assert_eq!(issue["body"], "Created description");
}

#[test]
fn rf_gl_6_gitlab_auto_labels_use_glab_catalog_and_final_issue_text() {
    let fixture = Fixture::new();
    let (base_url, model) = local_model(
        r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.9},{\"label\":\"docs\",\"probability\":0.79}]}"}}]}"#,
    );
    let output = fixture.run_with_model(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "create",
            "--title",
            "Parser regression",
            "--description",
            "Parser crashes on nested input",
            "--label",
            "@auto",
            "--label",
            "manual",
        ],
        "",
        "local/test-model",
        Some(&base_url),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = model.join().expect("model request");
    let prompt = request["messages"][1]["content"]
        .as_str()
        .expect("model prompt");
    assert!(prompt.contains("Parser regression"));
    assert!(prompt.contains("Parser crashes on nested input"));
    assert!(prompt.contains("Broken behavior"));
    assert!(prompt.contains("Documentation"));
    let invocations = fixture.glab_invocations().join("\n");
    assert!(invocations.contains("--per-page 100 --page 1"));
    assert!(invocations.contains("--per-page 100 --page 2"));
    assert!(invocations.contains("--label=bug"));
    assert!(invocations.contains("--label=manual"));
    assert!(!invocations.contains("--label=@auto"));
}

#[test]
fn rf_gl_6_gitlab_auto_label_update_uses_final_text_and_rejects_missing_model_before_glab() {
    let fixture = Fixture::new();
    let (base_url, model) = local_model(
        r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.9},{\"label\":\"docs\",\"probability\":0.79}]}"}}]}"#,
    );
    let output = fixture.run_with_model(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "update",
            "12",
            "--title",
            "Updated parser title",
            "--description",
            "Updated parser details",
            "--label",
            "@auto",
        ],
        "",
        "local/test-model",
        Some(&base_url),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = model.join().expect("model request");
    let prompt = request["messages"][1]["content"]
        .as_str()
        .expect("model prompt");
    assert!(prompt.contains("Updated parser title"));
    assert!(prompt.contains("Updated parser details"));
    let invocations = fixture.glab_invocations().join("\n");
    assert!(invocations.contains("issue update"));
    assert!(invocations.contains("--label=bug"));
    assert!(!invocations.contains("--label=@auto"));

    let missing = Fixture::new();
    let output = missing.run_gitlab_json(
        &[
            "issue",
            "create",
            "--title",
            "Requires model configuration",
            "--description",
            "Body",
            "--label",
            "@auto",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "decision_authentication");
    assert!(missing.glab_invocations().is_empty());
}

#[test]
fn rf_gl_6_gitlab_auto_update_rejects_concurrent_changes_and_unlabel_marker() {
    let fixture = Fixture::new();
    let (base_url, model) = local_model(
        r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.9},{\"label\":\"docs\",\"probability\":0.1}]}"}}]}"#,
    );
    let output = fixture.run_with_model(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "group/sub/project",
            "issue",
            "update",
            "12",
            "--label",
            "@auto",
            "--format",
            "json",
        ],
        "update-stale",
        "local/test-model",
        Some(&base_url),
    );
    assert_eq!(error_json(&output)["code"], "conflict");
    model.join().expect("model request");
    let invocations = fixture.glab_invocations().join("\n");
    assert!(!invocations.contains("issue update"));

    let invalid = Fixture::new();
    let output = invalid.run_gitlab_json(&["issue", "update", "12", "--unlabel", "@auto"], "");
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(invalid.glab_invocations().is_empty());
}

#[test]
fn rf_gl_6_gitlab_auto_labels_fail_closed_on_invalid_or_oversized_catalogs() {
    for (mode, expected_code) in [
        ("label-malformed", "provider_response"),
        ("label-overflow", "decision_input_limit"),
    ] {
        let fixture = Fixture::new();
        let output = fixture.run_with_model(
            &[
                "--provider",
                "gitlab",
                "--repo",
                "group/sub/project",
                "issue",
                "create",
                "--title",
                "Catalog boundary",
                "--description",
                "Body",
                "--label",
                "@auto",
                "--format",
                "json",
            ],
            mode,
            "local/test-model",
            None,
        );
        assert_eq!(error_json(&output)["code"], expected_code);
        let invocations = fixture.glab_invocations().join("\n");
        assert!(invocations.contains("label list"));
        assert!(!invocations.contains("issue create"));
    }
}
// RF-CFG.6: configured GitLab issue defaults merge with the explicit flags before the provider runs.
#[test]
fn rf_cfg_6_gitlab_issue_create_merges_configured_defaults_with_explicit_labels() {
    let fixture = Fixture::new();
    fixture.init_git();
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"gitlab":{"issue":{"labels":["bug","triage"],"assignees":["default-user"]}}}}"#,
    )
    .expect("write GitLab issue defaults");

    let output = fixture.run_gitlab(
        &[
            "issue",
            "create",
            "--title",
            "Defaults",
            "--description",
            "Configured defaults",
            "--label",
            "triage",
            "--label",
            "docs",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "issue create --repo {PROJECT} --title=Defaults --description= --yes \
                 --label=bug --label=triage --label=docs --assignee=default-user"
            ),
            "api --method PUT projects/group%2Fsub%2Fproject/issues/21 --hostname gitlab.com --header Content-Type: application/json --input -"
                .to_owned(),
            format!("issue view --output json --repo {PROJECT} 21"),
        ]
    );
}

// RF-CFG.6: an explicit `--assignee` replaces the configured GitLab issue assignees.
#[test]
fn rf_cfg_6_gitlab_issue_create_explicit_assignee_replaces_configured_defaults() {
    let fixture = Fixture::new();
    fixture.init_git();
    std::fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"gitlab":{"issue":{"labels":["bug"],"assignees":["default-user"]}}}}"#,
    )
    .expect("write GitLab issue defaults");

    let output = fixture.run_gitlab(
        &[
            "issue",
            "create",
            "--title",
            "Override",
            "--description",
            "Explicit assignee",
            "--assignee",
            "alice",
        ],
        "",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invocations = fixture.glab_invocations();
    assert!(
        invocations.iter().any(|call| call
            == &format!(
                "issue create --repo {PROJECT} --title=Override --description= --yes \
             --label=bug --assignee=alice"
            )),
        "glab invocations: {invocations:?}"
    );
    assert!(
        !invocations
            .iter()
            .any(|call| call.contains("--assignee=default-user"))
    );
}

fn local_model(response: &'static str) -> (String, thread::JoinHandle<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local model");
    let address = listener.local_addr().expect("model address");
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept model request");
        let mut reader = std::io::BufReader::new(stream);
        let mut line = String::new();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).expect("read model header");
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().expect("content length");
            }
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).expect("read model request");
        let request = serde_json::from_slice(&bytes).expect("parse model request");
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .expect("write model response");
        request
    });
    (format!("http://{address}/v1"), handle)
}
/// RF-CLI.1: root help reflects the current worktree provider configuration; provider selectors are removed.
#[test]
fn root_help_uses_worktree_provider_config_and_rejects_removed_selectors() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github"}"#,
    )
    .expect("write GitHub worktree config");

    let github = fixture.run_raw(&["--help"], "");
    assert!(github.status.success());
    let github_help = String::from_utf8_lossy(&github.stdout);
    assert!(github_help.starts_with("Manage work items and code-host requests"));
    assert!(github_help.contains("Provider resolution happens before command parsing"));
    assert!(github_help.contains("target project's Git worktree"));
    assert!(github_help.contains("--repo-path PATH"));
    assert!(github_help.contains("`--repo` overrides only the remote repository scope"));
    assert!(!github_help.contains("--provider"));
    assert!(!github_help.contains("--code-provider"));
    assert!(!github_help.contains("--work-item-provider"));
    assert!(github_help.lines().any(|line| {
        line.trim_start().starts_with("issue ") && line.contains("Manage GitHub issues")
    }));
    assert!(github_help.lines().any(|line| {
        line.trim_start().starts_with("pr ") && line.contains("Manage GitHub pull requests")
    }));

    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"github"}"#,
    )
    .expect("write mixed worktree config");
    let mixed = fixture.run_raw(&["--help"], "");
    assert!(mixed.status.success());
    let mixed_help = String::from_utf8_lossy(&mixed.stdout);
    assert!(mixed_help.lines().any(|line| {
        line.trim_start().starts_with("issue ") && line.contains("Manage GitHub issues")
    }));
    assert!(mixed_help.lines().any(|line| {
        line.trim_start().starts_with("mr ") && line.contains("Manage GitLab merge requests")
    }));
    assert!(
        !mixed_help
            .lines()
            .any(|line| line.trim_start().starts_with("pr "))
    );
    assert!(
        !mixed_help
            .lines()
            .any(|line| line.trim_start().starts_with("gitlab "))
    );

    for removed in [
        ["--provider", "github", "issue", "list"],
        ["--code-provider", "gitlab", "pr", "list"],
        ["--work-item-provider", "github", "issue", "list"],
    ] {
        let output = fixture.run_raw(&removed, "");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_input"));
    }
    assert!(fixture.glab_invocations().is_empty());
}

/// RF-CLI.1: --repo-path selects provider grammar and execution context from a second checkout.
#[test]
fn repo_path_uses_target_worktree_config_and_rejects_non_worktrees() {
    let caller = Fixture::new();
    caller.init_git();
    fs::write(
        caller.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github"}"#,
    )
    .expect("write caller provider config");

    let target = Fixture::new();
    target.init_git();
    fs::write(
        target.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","defaults":{"output":{"format":"json"}}}"#,
    )
    .expect("write target provider config");
    assert!(
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://gitlab.com/group/sub/project.git",
            ])
            .current_dir(&target.root)
            .status()
            .expect("add target GitLab origin")
            .success()
    );

    let relative_path = format!(
        "../{}",
        target
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .expect("target directory name is UTF-8")
    );
    let option = format!("--repo-path={relative_path}");
    let output = caller.run_raw(&["issue", "list", &option], "");
    assert_eq!(success_json(&output).as_array().unwrap().len(), 2);
    assert!(!caller.glab_invocations().is_empty());
    assert!(!caller.log.exists());
    let github_caller = Fixture::new();
    github_caller.init_git();
    fs::write(
        github_caller.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab"}"#,
    )
    .expect("write second caller provider config");
    let github_target = Fixture::new();
    github_target.init_git();
    fs::write(
        github_target.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github","defaults":{"output":{"format":"json"}}}"#,
    )
    .expect("write target GitHub config");
    assert!(
        std::process::Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:owner/repo.git"])
            .current_dir(&github_target.root)
            .status()
            .expect("add target GitHub origin")
            .success()
    );
    let relative_path = format!(
        "../{}",
        github_target
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .expect("target directory name is UTF-8")
    );
    let option = format!("--repo-path={relative_path}");
    let output = github_caller.run_raw(&["issue", "list", &option], "");
    assert!(success_json(&output).is_array());
    assert!(github_caller.log.exists());
    assert!(github_caller.glab_invocations().is_empty());

    let outside = caller.root.join("not-a-worktree");
    fs::create_dir(&outside).expect("create non-worktree directory");
    let outside = outside.to_str().expect("fixture path is UTF-8");
    let rejected = caller.run_raw(
        &["--repo-path", outside, "--format", "json", "issue", "list"],
        "",
    );
    assert!(!rejected.status.success());
    assert_eq!(error_json(&rejected)["code"], "context");
    assert_eq!(caller.glab_invocations().len(), 2);
}

/// RF-CLI.1/RF-CFG.7: parse failures use the selected target's defaults, never the caller's.
#[test]
fn target_config_selects_parse_error_format_until_explicit_format_overrides_it() {
    let caller = Fixture::new();
    caller.init_git();
    fs::write(
        caller.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github","defaults":{"output":{"format":"text"}}}"#,
    )
    .expect("write caller output config");
    let target = Fixture::new();
    target.init_git();
    fs::write(
        target.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","defaults":{"output":{"format":"json"}}}"#,
    )
    .expect("write target output config");
    let target_path = target.root.to_str().expect("fixture path is UTF-8");

    for invalid in [
        vec!["issue", "unknown"],
        vec!["issue", "list", "--unknown"],
        vec!["issue", "view"],
    ] {
        let mut args = vec!["--repo-path", target_path];
        args.extend(invalid);
        let output = caller.run_raw(&args, "");
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(error_json(&output)["code"], "invalid_input");
    }
    let explicit_text = caller.run_raw(
        &[
            "--repo-path",
            target_path,
            "issue",
            "unknown",
            "--format",
            "text",
        ],
        "",
    );
    assert_eq!(explicit_text.status.code(), Some(2));
    assert!(explicit_text.stdout.is_empty());
    assert!(String::from_utf8_lossy(&explicit_text.stderr).starts_with("invalid_input:"));

    fs::write(
        caller.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github","defaults":{"output":{"format":"json"}}}"#,
    )
    .expect("write JSON caller output config");
    fs::write(
        target.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","defaults":{"output":{"format":"text"}}}"#,
    )
    .expect("write text target output config");
    let target_text = caller.run_raw(&["--repo-path", target_path, "issue", "unknown"], "");
    assert_eq!(target_text.status.code(), Some(2));
    assert!(target_text.stdout.is_empty());
    assert!(String::from_utf8_lossy(&target_text.stderr).starts_with("invalid_input:"));

    fs::write(
        target.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","defaults":{"output":{"format":"invalid"}}}"#,
    )
    .expect("write invalid target output config");
    let fallback = caller.run_raw(&["--repo-path", target_path, "issue", "unknown"], "");
    assert_eq!(fallback.status.code(), Some(2));
    assert!(fallback.stdout.is_empty());
    assert!(String::from_utf8_lossy(&fallback.stderr).starts_with("invalid_input:"));
    let explicit_json = caller.run_raw(
        &[
            "--repo-path",
            target_path,
            "issue",
            "unknown",
            "--format=json",
        ],
        "",
    );
    assert_eq!(error_json(&explicit_json)["code"], "invalid_input");
    assert!(!caller.log.exists());
    assert!(caller.glab_invocations().is_empty());
}

/// RF-CLI.1: an inline native-byte path selects the target rather than falling back to the caller.
#[cfg(unix)]
#[test]
fn non_utf8_inline_repo_path_selects_target_worktree() {
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    let caller = Fixture::new();
    caller.init_git();
    fs::write(
        caller.root.join(".workctl.json"),
        r#"{"codeProvider":"github","workItemProvider":"github"}"#,
    )
    .expect("write caller provider config");
    assert!(
        Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:owner/caller.git"])
            .current_dir(&caller.root)
            .status()
            .expect("add caller GitHub origin")
            .success()
    );
    let target = Fixture::new();
    target.init_git();
    fs::write(
        target.root.join(".workctl.json"),
        r#"{"codeProvider":"gitlab","workItemProvider":"gitlab","defaults":{"output":{"format":"json"}}}"#,
    )
    .expect("write target provider config");
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://gitlab.com/group/sub/project.git",
            ])
            .current_dir(&target.root)
            .status()
            .expect("add target GitLab origin")
            .success()
    );
    let native_path = target
        .root
        .join(OsString::from_vec(b"native-\xff".to_vec()));
    // Darwin filesystems reject invalid UTF-8 filenames; still exercise fail-closed native input.
    #[cfg(not(target_os = "macos"))]
    fs::create_dir(&native_path).expect("create native-byte target directory");
    let mut option = OsString::from("--repo-path=");
    option.push(&native_path);
    assert!(option.to_str().is_none());

    let mut paths = vec![caller.bin.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).expect("compose isolated PATH");
    let run = |option: &OsStr, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_workctl"))
            .arg(option)
            .args(args)
            .current_dir(&caller.root)
            .env("PATH", &path)
            .env("WORKCTL_GH_LOG", &caller.log)
            .env("WORKCTL_GH_INPUT", &caller.input)
            .env("WORKCTL_GH_CWD_ROOT", &caller.root)
            .env("WORKCTL_GH_MODE", "")
            .env("WORKCTL_GLAB_LOG", &caller.glab_log)
            .env("WORKCTL_GLAB_INPUT", &caller.glab_input)
            .env("WORKCTL_GLAB_MODE", "")
            .output()
            .expect("run workctl with a native-byte target")
    };

    #[cfg(target_os = "macos")]
    {
        let rejected = run(&option, &["issue", "list", "--format", "json"]);
        assert_eq!(error_json(&rejected)["code"], "context");
        assert!(caller.glab_invocations().is_empty());
        assert!(!caller.log.exists());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let output = run(&option, &["issue", "list"]);
        assert_eq!(success_json(&output).as_array().unwrap().len(), 2);
        assert_eq!(
            caller.glab_invocations(),
            [
                "auth status --hostname gitlab.com".to_owned(),
                format!("issue list --output json --repo {PROJECT} --page 1 --per-page 30"),
            ]
        );
        assert!(!caller.log.exists());

        let explicit = run(&option, &["issue", "unknown", "--format", "text"]);
        assert_eq!(explicit.status.code(), Some(2));
        assert!(explicit.stdout.is_empty());
        assert!(String::from_utf8_lossy(&explicit.stderr).starts_with("invalid_input:"));

        option.push("/missing");
        let invalid = run(&option, &["issue", "list", "--format", "json"]);
        assert_eq!(error_json(&invalid)["code"], "context");
        assert_eq!(caller.glab_invocations().len(), 2);
        assert!(!caller.log.exists());
    }
}
