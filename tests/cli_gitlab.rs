//! GitLab provider contract: the grammar mirrors `glab`, and issue reads map onto the shared
//! shape without exposing provider diagnostics.

mod common;

use common::{Fixture, error_json, success_json};

const PROJECT: &str = "https://gitlab.com/group/sub/project";

#[test]
fn gitlab_list_forwards_every_filter_to_glab() {
    let fixture = Fixture::new();
    let output = fixture.run_gitlab(
        &[
            "issue",
            "list",
            "--all",
            "--label",
            "bug",
            "--assignee",
            "@me",
            "--search",
            "crash",
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
                "issue list --output json --repo {PROJECT} --all --label bug --assignee @me \
                 --search crash --per-page 5"
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
    let output = fixture.run_gitlab(&["issue", "view", "12"], "");

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

#[test]
fn gitlab_state_flags_select_the_requested_state() {
    let fixture = Fixture::new();
    fixture.run_gitlab(&["issue", "list"], "");
    fixture.run_gitlab(&["issue", "list", "--closed"], "");

    let invocations = fixture.glab_invocations();
    assert_eq!(
        invocations[1],
        format!("issue list --output json --repo {PROJECT} --per-page 30")
    );
    assert_eq!(
        invocations[3],
        format!("issue list --output json --repo {PROJECT} --closed --per-page 30")
    );
}

#[test]
fn gitlab_grammar_rejects_github_flags_and_conflicts() {
    let fixture = Fixture::new();
    let unknown_flag = fixture.run_gitlab(&["issue", "list", "--limit", "5"], "");
    assert_eq!(unknown_flag.status.code(), Some(2));
    assert_eq!(error_json(&unknown_flag)["code"], "invalid_input");

    let conflicting = fixture.run_gitlab(&["issue", "list", "--all", "--closed"], "");
    assert_eq!(conflicting.status.code(), Some(2));
    assert_eq!(error_json(&conflicting)["code"], "invalid_input");

    let pr_group = fixture.run_gitlab(&["pr", "list"], "");
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
fn gitlab_failures_report_stable_codes_without_provider_output() {
    let fixture = Fixture::new();
    let unauthenticated = fixture.run_gitlab(&["issue", "list"], "auth-failure");
    assert_eq!(error_json(&unauthenticated)["code"], "authentication");

    let fixture = Fixture::new();
    let failed = fixture.run_gitlab(&["issue", "list"], "list-failure");
    let error = error_json(&failed);
    assert_eq!(error["code"], "gitlab_cli");
    assert_eq!(error["message"], "the GitLab CLI operation failed");

    for output in [unauthenticated, failed] {
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"),
            "provider stderr must not reach the caller"
        );
    }
}

#[test]
fn gitlab_rejects_an_invalid_issue_number_before_glab() {
    let fixture = Fixture::new();
    for number in ["0", "abc"] {
        let output = fixture.run_gitlab(&["issue", "view", number], "");
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
    let output = fixture.run_gitlab(
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
                "issue create --repo {PROJECT} --title=New issue --description-file=- \
                 --label=bug --label=triage --assignee=alice --milestone=M1 --confidential \
                 --weight=0 --due-date=2026-02-28"
            ),
            format!("issue update 21 --repo {PROJECT} --weight=0"),
            format!("issue view --output json --repo {PROJECT} 21"),
        ]
    );
    assert_eq!(fixture.glab_input(), description.as_bytes());
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
    let output = fixture.run_gitlab(
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
        "",
    );

    assert_eq!(
        fixture.glab_invocations(),
        vec![
            "auth status --hostname gitlab.com".to_owned(),
            format!(
                "issue update 12 --repo {PROJECT} --title=Updated title --description-file=- \
                 --label=ready --unlabel=triage --assignee=+alice --assignee=-bob --milestone= \
                 --public --weight=0 --due-date=2026-02-28"
            ),
            format!("issue view --output json --repo {PROJECT} 12"),
        ]
    );
    assert_eq!(fixture.glab_input(), description.as_bytes());
    let issue = success_json(&output);
    assert_eq!(issue["number"], 12);
    assert_eq!(issue["body"], "Steps to reproduce");
}

#[test]
fn gitlab_writes_reject_empty_or_invalid_changes_before_authentication() {
    let fixture = Fixture::new();
    let empty_description = fixture.root.join("empty-description.txt");
    std::fs::write(&empty_description, "").expect("write empty description fixture");
    let empty_path = empty_description.to_str().expect("UTF-8 fixture path");
    let dash_description = fixture.root.join("dash-description.txt");
    std::fs::write(&dash_description, "-").expect("write dash description fixture");
    let dash_path = dash_description.to_str().expect("UTF-8 fixture path");
    let invalid = [
        fixture.run_gitlab(
            &["issue", "create", "--title", "Dash", "--description", "-"],
            "",
        ),
        fixture.run_gitlab(
            &["issue", "update", "12", "--description-file", dash_path],
            "",
        ),
        fixture.run_gitlab(&["issue", "create", "--title", "No description"], ""),
        fixture.run_gitlab(&["issue", "update", "12"], ""),
        fixture.run_gitlab(
            &["issue", "update", "12", "--description-file", empty_path],
            "",
        ),
        fixture.run_gitlab(
            &[
                "issue",
                "update",
                "12",
                "--assignee",
                "alice",
                "--unassign",
            ],
            "",
        ),
        fixture.run_gitlab(
            &["issue", "update", "12", "--due-date", "2026-02-30"],
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
fn gitlab_write_uncertainty_hides_diagnostics_and_rejects_untrusted_create_url() {
    let fixture = Fixture::new();
    let description = fixture.root.join("description.txt");
    std::fs::write(&description, "create").expect("write description fixture");
    let description_path = description.to_str().expect("UTF-8 fixture path");
    let failed = fixture.run_gitlab(
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
    let bad_url = fixture.run_gitlab(
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

