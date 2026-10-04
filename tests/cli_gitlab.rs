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
