#![cfg(unix)]

mod common;

use std::fs;

use common::{Fixture, error_json, success_json};
use serde_json::json;

/// Exact `gh` argv lines in invocation order, so assertions cannot match a prefix of a longer
/// argument list by accident.
fn gh_lines(fixture: &Fixture) -> Vec<String> {
    fs::read_to_string(&fixture.log)
        .expect("read gh argument log")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn gh_input(fixture: &Fixture) -> String {
    fs::read_to_string(&fixture.input).expect("read gh stdin capture")
}

#[test]
fn create_sends_the_body_and_flags_to_gh() {
    let fixture = Fixture::new();
    let created = fixture.run_pr(
        &[
            "pr",
            "create",
            "--title",
            "Add feature",
            "--body",
            "Fixes the bug\n\nDetails.",
            "--closes",
            "7",
            "--closes",
            "9",
            "--base",
            "main",
            "--head",
            "feature-x",
            "--draft",
            "--assignee",
            "octocat",
            "--label",
            "bug",
            "--label",
            "p1",
            "--reviewer",
            "hubot",
            "--milestone",
            "M1",
            "--project",
            "Roadmap",
        ],
        "",
    );
    let created = success_json(&created);
    assert_eq!(created["number"], 42);
    assert_eq!(created["state"], "open");
    assert_eq!(created["url"], "https://github.com/owner/repo/pull/42");

    assert_eq!(
        gh_input(&fixture),
        "Fixes the bug\n\nDetails.\nCloses #7\nCloses #9"
    );
    let lines = gh_lines(&fixture);
    assert!(
        lines.contains(
            &"pr create --repo owner/repo --title Add feature --body-file - --base main --head feature-x --draft --assignee octocat --label bug --label p1 --reviewer hubot --milestone M1 --project Roadmap"
                .to_string()
        ),
        "actual gh calls: {lines:?}"
    );
    assert!(lines.contains(
        &"pr view 42 --repo owner/repo --json number,title,body,state,isDraft,url,baseRefName,headRefName,author,createdAt,updatedAt,mergedAt,mergeable,reviewDecision,labels,assignees"
            .to_string()
    ));
}

#[test]
fn attachments_are_forwarded_and_require_a_supported_gh() {
    let fixture = Fixture::new();
    let created = fixture.run_pr(
        &[
            "pr",
            "create",
            "--title",
            "With media",
            "--attach",
            "screen.png#Screenshot",
        ],
        "",
    );
    assert_eq!(success_json(&created)["number"], 42);
    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"--version".to_string()));
    assert!(lines.contains(
        &"pr create --repo owner/repo --title With media --body-file - --attach screen.png#Screenshot"
            .to_string()
    ));

    let edited = fixture.run_pr(&["pr", "edit", "42", "--attach", "clip.mp4"], "");
    assert_eq!(success_json(&edited)["number"], 42);
    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr edit 42 --repo owner/repo --attach clip.mp4".to_string()));

    let old = Fixture::new();
    let rejected = old.run_pr(
        &[
            "pr",
            "create",
            "--title",
            "Old CLI",
            "--attach",
            "screen.png",
        ],
        "old-version",
    );
    assert_eq!(error_json(&rejected)["code"], "dependency_version");
    assert_eq!(
        gh_lines(&old),
        ["auth status --hostname github.com", "--version"]
    );
}

#[test]
fn failed_attachment_create_warns_to_check_remote_without_retrying() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(
        &["pr", "create", "--title", "Media", "--attach", "screen.png"],
        "attachment-create-failure",
    );
    assert_eq!(error_json(&output)["code"], "attachment_create_uncertain");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));
    assert_eq!(
        gh_lines(&fixture)
            .iter()
            .filter(|line| line.starts_with("pr create"))
            .count(),
        1
    );
}

#[test]
fn pr_create_keeps_the_body_explicit_and_metadata_optional() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(&["pr", "create", "--help"], "");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--title"));
    assert!(help.contains("--label"));
    assert!(help.contains("--reviewer"));
    assert!(!help.contains("--fill"));
    assert!(!fixture.log.exists());
}

#[test]
fn list_forwards_filters_and_normalizes_summaries() {
    let fixture = Fixture::new();
    let filtered = fixture.run_pr(
        &[
            "pr",
            "list",
            "--state",
            "merged",
            "--limit",
            "2",
            "--label",
            "bug",
            "--assignee",
            "octocat",
            "--author",
            "hubot",
            "--base",
            "main",
            "--head",
            "feature-x",
            "--search",
            "in:title change",
            "--draft",
        ],
        "",
    );
    let filtered = success_json(&filtered);
    let filtered = filtered.as_array().expect("pr list array");
    // The shim returns three summaries; `--limit 2` truncates client-side.
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0]["state"], "open");
    assert_eq!(filtered[1]["draft"], true);

    let all = fixture.run_pr(&["pr", "list"], "");
    let all = success_json(&all);
    let all = all.as_array().expect("pr list array");
    assert_eq!(all.len(), 3);
    assert_eq!(all[2]["state"], "merged");

    let lines = gh_lines(&fixture);
    assert!(lines.contains(
        &"pr list --repo owner/repo --state merged --limit 2 --label bug --assignee octocat --author hubot --base main --head feature-x --search in:title change --draft --json number,title,state,isDraft,url,baseRefName,headRefName,updatedAt"
            .to_string()
    ));
    assert!(lines.contains(
        &"pr list --repo owner/repo --state open --limit 30 --json number,title,state,isDraft,url,baseRefName,headRefName,updatedAt"
            .to_string()
    ));
}

#[test]
fn view_returns_a_pull_request_or_reports_it_is_not_one() {
    let fixture = Fixture::new();
    let shown = fixture.run_pr(&["pr", "view", "42"], "");
    let shown = success_json(&shown);
    assert_eq!(shown["number"], 42);
    assert_eq!(shown["body"], "PR body");
    assert_eq!(shown["author"], "octocat");
    assert_eq!(shown["base_ref"], "main");
    assert_eq!(shown["head_ref"], "feature-x");
    assert_eq!(shown["mergeable"], "mergeable");
    assert_eq!(shown["labels"], json!(["bug"]));
    assert_eq!(shown["assignees"], json!(["hubot"]));

    let missing = fixture.run_pr(&["pr", "view", "42"], "pr-view-failure");
    assert_eq!(error_json(&missing)["code"], "not_pull_request");
}

#[test]
fn checks_parse_failing_reports_and_treat_missing_checks_as_empty() {
    let fixture = Fixture::new();
    // `gh pr checks` exits 1 for failing checks while still printing a valid report.
    let failing = fixture.run_pr(&["pr", "checks", "42", "--required"], "checks-failing");
    let failing = success_json(&failing);
    let checks = failing.as_array().expect("checks array");
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0]["name"], "build");
    assert_eq!(checks[0]["state"], "success");
    assert_eq!(checks[0]["bucket"], "pass");
    assert_eq!(checks[1]["name"], "lint");
    assert_eq!(checks[1]["state"], "failure");

    // `no checks reported` on stderr with exit 1 is an empty report, not a failure.
    let none = fixture.run_pr(&["pr", "checks", "42"], "checks-none");
    assert_eq!(success_json(&none), json!([]));

    let lines = gh_lines(&fixture);
    assert!(lines.contains(
        &"pr checks 42 --repo owner/repo --required --json name,state,bucket,description,link,workflow"
            .to_string()
    ));
    assert!(
        lines.contains(
            &"pr checks 42 --repo owner/repo --json name,state,bucket,description,link,workflow"
                .to_string()
        )
    );
}

#[test]
fn edit_applies_a_body_change_and_rejects_a_stale_write() {
    let fixture = Fixture::new();
    let appended = fixture.run_pr(&["pr", "edit", "42", "--append-body", "extra"], "");
    let appended = success_json(&appended);
    assert_eq!(appended["number"], 42);
    // The body is appended to the fetched body, not replaced.
    assert_eq!(gh_input(&fixture), "PR body\nextra");

    let stale = fixture.run_pr(
        &[
            "pr",
            "edit",
            "42",
            "--title",
            "Stale",
            "--expect-updated-at",
            "2020-01-01T00:00:00Z",
        ],
        "",
    );
    assert_eq!(error_json(&stale)["code"], "conflict");

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr edit 42 --repo owner/repo --body-file -".to_string()));
    // The stale guard refuses before any write: one edit for the append, none for the stale call.
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("pr edit"))
            .count(),
        1
    );
}

#[test]
fn edit_forwards_metadata_flags_and_requires_a_change() {
    let fixture = Fixture::new();
    let labeled = fixture.run_pr(
        &[
            "pr",
            "edit",
            "42",
            "--base",
            "release",
            "--add-label",
            "bug",
            "--remove-label",
            "stale",
            "--add-reviewer",
            "hubot",
            "--remove-reviewer",
            "octocat",
            "--add-assignee",
            "hubot",
            "--remove-assignee",
            "octocat",
            "--milestone",
            "v1",
            "--add-project",
            "Roadmap",
            "--remove-project",
            "Backlog",
        ],
        "",
    );
    success_json(&labeled);
    let lines = gh_lines(&fixture);
    assert!(lines.contains(
        &"pr edit 42 --repo owner/repo --base release --add-label bug --remove-label stale --add-reviewer hubot --remove-reviewer octocat --add-assignee hubot --remove-assignee octocat --milestone v1 --add-project Roadmap --remove-project Backlog"
            .to_string()
    ));

    let unmilestoned = fixture.run_pr(&["pr", "edit", "42", "--remove-milestone"], "");
    success_json(&unmilestoned);
    assert!(
        gh_lines(&fixture).contains(&"pr edit 42 --repo owner/repo --remove-milestone".to_string())
    );

    let empty = fixture.run_pr(&["pr", "edit", "42"], "");
    let conflicting = fixture.run_pr(
        &[
            "pr",
            "edit",
            "42",
            "--milestone",
            "M1",
            "--remove-milestone",
        ],
        "",
    );
    assert_eq!(error_json(&conflicting)["code"], "invalid_input");
    assert_eq!(error_json(&empty)["code"], "invalid_input");
    // The empty update is rejected before any provider call.
    assert_eq!(
        gh_lines(&fixture)
            .iter()
            .filter(|line| line.starts_with("pr edit"))
            .count(),
        2
    );
}

#[test]
fn review_merge_ready_close_and_reopen_use_the_gh_commands() {
    let fixture = Fixture::new();

    let approved = fixture.run_pr(
        &["pr", "review", "42", "--approve", "--body", "Looks good"],
        "",
    );
    assert_eq!(
        success_json(&approved),
        json!({"number": 42, "event": "approve"})
    );
    assert_eq!(gh_input(&fixture), "Looks good");

    let changes = fixture.run_pr(&["pr", "review", "42", "--request-changes"], "");
    assert_eq!(
        success_json(&changes),
        json!({"number": 42, "event": "request_changes"})
    );

    let merged = fixture.run_pr(
        &[
            "pr",
            "merge",
            "42",
            "--method",
            "squash",
            "--delete-branch",
            "--auto",
        ],
        "",
    );
    assert_eq!(
        success_json(&merged),
        json!({"number": 42, "method": "squash", "auto": true})
    );

    let ready = fixture.run_pr(&["pr", "ready", "42"], "");
    assert_eq!(success_json(&ready), json!({"number": 42, "draft": false}));

    let draft = fixture.run_pr(&["pr", "ready", "42", "--undo"], "");
    assert_eq!(success_json(&draft), json!({"number": 42, "draft": true}));

    let closed = fixture.run_pr(
        &["pr", "close", "42", "--comment", "bye", "--delete-branch"],
        "",
    );
    assert_eq!(
        success_json(&closed),
        json!({"number": 42, "state": "closed"})
    );

    let reopened = fixture.run_pr(&["pr", "reopen", "42", "--comment", "back"], "");
    assert_eq!(
        success_json(&reopened),
        json!({"number": 42, "state": "open"})
    );

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr review 42 --repo owner/repo --approve --body-file -".to_string()));
    assert!(lines.contains(&"pr review 42 --repo owner/repo --request-changes".to_string()));
    assert!(
        lines
            .contains(&"pr merge 42 --repo owner/repo --squash --delete-branch --auto".to_string())
    );
    assert!(lines.contains(&"pr ready 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&"pr ready 42 --repo owner/repo --undo".to_string()));
    assert!(lines.contains(&"pr close 42 --repo owner/repo -c bye --delete-branch".to_string()));
    assert!(lines.contains(&"pr reopen 42 --repo owner/repo -c back".to_string()));
}

#[test]
fn review_without_an_event_is_a_usage_error() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(&["pr", "review", "42"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(!fixture.log.exists());
}

#[test]
fn diff_prints_the_patch_as_json_or_raw_text() {
    let fixture = Fixture::new();
    let as_json = fixture.run_pr(&["pr", "diff", "42"], "");
    let as_json = success_json(&as_json);
    assert_eq!(as_json["number"], 42);
    let patch = as_json["diff"].as_str().expect("diff string");
    assert!(patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));
    assert!(patch.contains("-old line\n+new line"));

    let as_text = fixture.run_pr(&["--format", "text", "pr", "diff", "42", "--name-only"], "");
    assert!(as_text.status.success());
    assert!(as_text.stderr.is_empty());
    let rendered = String::from_utf8_lossy(&as_text.stdout);
    assert!(rendered.contains("diff --git a/src/lib.rs b/src/lib.rs"));
    assert!(rendered.contains("-old line\n+new line"));

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr diff 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&"pr diff 42 --repo owner/repo --name-only".to_string()));
}

#[test]
fn pr_create_and_edit_resolve_the_current_milestone_only_when_requested() {
    let fixture = Fixture::new();
    let created = fixture.run_pr(
        &["pr", "create", "--title", "No milestone"],
        "milestone-nearest",
    );
    assert_eq!(success_json(&created)["number"], 42);
    assert!(!gh_lines(&fixture).iter().any(|line| line.contains("milestones")));
    assert!(!gh_lines(&fixture).iter().any(|line| line.contains("--milestone")));

    let current = Fixture::new();
    let created = current.run_pr(
        &["pr", "create", "--title", "Current", "--milestone", "@current"],
        "milestone-nearest",
    );
    assert_eq!(success_json(&created)["number"], 42);
    let lines = gh_lines(&current);
    assert!(lines.iter().any(|line| line.contains("milestones")));
    assert!(lines.iter().any(|line| {
        line == "pr create --repo owner/repo --title Current --body-file - --milestone Sooner"
    }));

    let updated = Fixture::new();
    success_json(&updated.run_pr(
        &["pr", "edit", "42", "--milestone", "@current"],
        "milestone-nearest",
    ));
    let lines = gh_lines(&updated);
    assert!(lines.iter().any(|line| line.contains("milestones")));
    assert!(lines.contains(&"pr edit 42 --repo owner/repo --milestone Sooner".to_string()));

    let explicit = Fixture::new();
    let created = explicit.run_pr(
        &["pr", "create", "--title", "Explicit", "--milestone", "M1"],
        "milestone-tie",
    );
    assert_eq!(success_json(&created)["number"], 42);
    let lines = gh_lines(&explicit);
    assert!(!lines.iter().any(|line| line.contains("milestones")));
    assert!(lines.iter().any(|line| {
        line == "pr create --repo owner/repo --title Explicit --body-file - --milestone M1"
    }));

    let missing = Fixture::new();
    let output = missing.run_pr(
        &["pr", "create", "--title", "None eligible", "--milestone", "@current"],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!gh_lines(&missing).iter().any(|line| line.starts_with("pr create")));
}

#[test]
fn whitespace_only_names_and_filters_fail_during_clap_parsing() {
    let fixture = Fixture::new();
    for args in [
        &["pr", "create", "--title", "Valid", "--reviewer", "   "][..],
        &["pr", "list", "--author", "   "][..],
        &["pr", "edit", "42", "--add-project", "   "][..],
    ] {
        let output = fixture.run_pr(args, "");
        assert_eq!(error_json(&output)["code"], "invalid_input");
    }
    assert!(!fixture.log.exists());
}
