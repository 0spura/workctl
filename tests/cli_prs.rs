#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

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

/// The one review-API invocation an inline review may make.
const REVIEW_POST: &str = "api --method POST repos/owner/repo/pulls/42/reviews --input -";

/// The one branch-deletion invocation: the pull request's remote head ref, with nothing local.
const REMOTE_BRANCH_DELETE: &str = "api --method DELETE repos/owner/repo/git/refs/heads/feature-x";

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
            "--closes",
            "other/repo#6",
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
        "Fixes the bug\n\nDetails.\nCloses #7\nCloses #9\nCloses other/repo#6"
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

// RF-PR.14: Check watching forwards native controls, preserves failure evidence, and times out boundedly.
#[test]
fn checks_watch_preserves_failed_reports_and_bounds_wait_time() {
    let fixture = Fixture::new();
    let watched = fixture.run_pr(
        &[
            "pr",
            "checks",
            "42",
            "--required",
            "--watch",
            "--interval",
            "4",
            "--fail-fast",
        ],
        "checks-watch-fail-fast",
    );
    let checks = success_json(&watched);
    assert_eq!(checks.as_array().expect("check report").len(), 2);
    assert_eq!(checks[1]["name"], "lint");
    assert_eq!(checks[1]["bucket"], "fail");
    assert!(gh_lines(&fixture).contains(
        &"pr checks 42 --repo owner/repo --required --watch --interval 4 --fail-fast --json name,state,bucket,description,link,workflow".to_string()
    ));
    let pending = fixture.run_pr(
        &["pr", "checks", "42", "--watch", "--watch-timeout", "3"],
        "checks-watch-pending",
    );
    let pending_checks = success_json(&pending);
    assert_eq!(pending_checks[0]["state"], "pending");
    assert_eq!(pending_checks[0]["bucket"], "pending");

    let timed_out = fixture.run_pr(
        &["pr", "checks", "42", "--watch", "--watch-timeout", "1"],
        "checks-watch-timeout",
    );
    assert!(!timed_out.status.success());
    assert!(timed_out.stdout.is_empty());
    assert_eq!(error_json(&timed_out)["code"], "timeout");
    assert!(!String::from_utf8_lossy(&timed_out.stderr).contains("sleep"));
}

#[test]
fn checks_watch_controls_require_watch_and_valid_bounds() {
    let fixture = Fixture::new();
    for args in [
        vec!["pr", "checks", "42", "--interval", "5"],
        vec!["pr", "checks", "42", "--fail-fast"],
        vec!["pr", "checks", "42", "--watch-timeout", "10"],
        vec!["pr", "checks", "42", "--watch", "--interval", "0"],
        vec!["pr", "checks", "42", "--watch", "--watch-timeout", "3601"],
    ] {
        let output = fixture.run_pr(&args, "");
        assert_eq!(output.status.code(), Some(2));
    }
    assert!(!fixture.log.exists());
}

// RF-PR.12: PR status exposes review/mergeability and required-check evidence without claiming readiness.
#[test]
fn status_combines_mergeability_review_and_required_checks() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(&["pr", "status", "42"], "");
    let status = success_json(&output);
    assert_eq!(status["number"], 42);
    assert_eq!(status["state"], "open");
    assert_eq!(status["draft"], false);
    assert_eq!(status["mergeable"], "mergeable");
    assert_eq!(status["review_decision"], serde_json::Value::Null);
    assert_eq!(status["base_ref"], "main");
    assert_eq!(status["head_ref"], "feature-x");
    assert_eq!(status["required_checks"][0]["name"], "build");
    assert_eq!(status["required_checks"][0]["bucket"], "pass");
    assert_eq!(status["required_checks"][1]["name"], "lint");
    assert_eq!(status["required_checks"][1]["bucket"], "fail");
    assert!(status.get("body").is_none());
    assert!(status.get("ready").is_none());

    let lines = gh_lines(&fixture);
    let view = lines
        .iter()
        .position(|line| {
            line == "pr view 42 --repo owner/repo --json number,title,state,isDraft,url,baseRefName,headRefName,mergeable,reviewDecision"
        })
        .expect("status view invocation");
    let checks = lines
        .iter()
        .position(|line| {
            line == "pr checks 42 --repo owner/repo --required --json name,state,bucket,description,link,workflow"
        })
        .expect("required checks invocation");
    assert!(view < checks);

    let text = fixture.run_pr(&["pr", "status", "42", "--format", "text"], "");
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).expect("text output");
    assert!(text.contains("Review decision: unknown"));
    assert!(text.contains("lint: failure (fail)"));
    assert!(!text.contains("ready for merge"));
}

// RF-PR.12: missing check reports and non-PR targets remain inconclusive/errors, never readiness.
#[test]
fn status_keeps_empty_checks_inconclusive_and_stops_for_non_pull_requests() {
    let fixture = Fixture::new();
    let no_checks = fixture.run_pr(&["pr", "status", "42"], "checks-none");
    let no_checks = success_json(&no_checks);
    assert_eq!(no_checks["required_checks"], json!([]));
    assert!(no_checks.get("ready").is_none());

    let missing_fixture = Fixture::new();
    let missing = missing_fixture.run_pr(&["pr", "status", "42"], "pr-view-failure");
    assert_eq!(error_json(&missing)["code"], "not_pull_request");
    assert!(missing.stdout.is_empty());
    assert!(
        !gh_lines(&missing_fixture)
            .iter()
            .any(|line| line.starts_with("pr checks 42 "))
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

/// RF-PR.8: closing references are body text, so an edit manages them without the caller
/// reproducing the body.
#[test]
fn edit_manages_closing_references_in_the_body() {
    let fixture = Fixture::new();
    let linked = fixture.run_pr(
        &[
            "pr",
            "edit",
            "42",
            "--closes",
            "7",
            "--closes",
            "other/repo#6",
        ],
        "",
    );
    success_json(&linked);
    assert_eq!(
        gh_input(&fixture),
        "PR body\nCloses #7\nCloses other/repo#6"
    );
    assert!(gh_lines(&fixture).contains(&"pr edit 42 --repo owner/repo --body-file -".to_string()));

    // A fetched body already carrying closing lines loses only the named reference.
    let unlinked = fixture.run_pr(
        &["pr", "edit", "42", "--remove-closes", "other/repo#6"],
        "pr-body-closes",
    );
    success_json(&unlinked);
    assert_eq!(
        gh_input(&fixture),
        "PR body\n\nCloses #7\nSee #9 for context"
    );

    // A reference this body does not close leaves the body alone, so no write is sent.
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let unchanged = fixture.run_pr(&["pr", "edit", "42", "--remove-closes", "7"], "");
    success_json(&unchanged);
    let lines = gh_lines(&fixture);
    assert!(
        !lines.iter().any(|line| line.starts_with("pr edit")),
        "actual gh calls: {lines:?}"
    );

    let conflicting = fixture.run_pr(
        &["pr", "edit", "42", "--closes", "7", "--remove-closes", "7"],
        "",
    );
    assert_eq!(error_json(&conflicting)["code"], "invalid_input");
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

// RF-PR.13: Branch updates mirror the gh operation and expose the selected update strategy.
#[test]
fn update_branch_forwards_default_and_rebase_modes_and_safely_reports_failure() {
    let fixture = Fixture::new();
    let merge_update = fixture.run_pr(&["pr", "update-branch", "42"], "");
    assert_eq!(
        success_json(&merge_update),
        json!({"number": 42, "rebase": false})
    );
    assert!(gh_lines(&fixture).contains(&"pr update-branch 42 --repo owner/repo".to_string()));

    let rebase_update = fixture.run_pr(
        &["pr", "update-branch", "42", "--rebase", "--format", "text"],
        "",
    );
    assert_eq!(
        String::from_utf8(rebase_update.stdout).expect("UTF-8 text output"),
        "pr #42 branch updated (rebase)\n"
    );
    assert!(
        gh_lines(&fixture).contains(&"pr update-branch 42 --repo owner/repo --rebase".to_string())
    );

    let failed = fixture.run_pr(
        &["pr", "update-branch", "42", "--rebase"],
        "pr-update-branch-failure",
    );
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert_eq!(error_json(&failed)["code"], "github_cli");
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private provider diagnostic"));
}

// RF-PR.15: Checkout delegates to gh without its force option and preserves safe failures.
#[test]
fn checkout_switches_to_the_pr_branch_without_force_and_reports_safe_failure() {
    let fixture = Fixture::new();
    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&fixture.root)
            .output()
            .expect("run local git fixture")
    };
    assert!(run_git(&["init", "-q"]).status.success());
    assert!(
        run_git(&["config", "user.name", "Fixture"])
            .status
            .success()
    );
    assert!(
        run_git(&["config", "user.email", "fixture@example.test"])
            .status
            .success()
    );
    fs::write(fixture.root.join("seed.txt"), "seed").expect("write initial fixture file");
    assert!(run_git(&["add", "seed.txt"]).status.success());
    assert!(run_git(&["commit", "-qm", "initial"]).status.success());

    let checked_out = fixture.run_pr(&["pr", "checkout", "42"], "pr-checkout-git");
    assert_eq!(success_json(&checked_out), json!({"number": 42}));
    assert_eq!(
        String::from_utf8(run_git(&["branch", "--show-current"]).stdout)
            .expect("branch name is UTF-8")
            .trim(),
        "pr-42"
    );
    assert!(gh_lines(&fixture).contains(&"pr checkout 42 --repo owner/repo".to_string()));
    assert!(
        !gh_lines(&fixture)
            .iter()
            .any(|line| line.contains("--force"))
    );

    let text = fixture.run_pr(&["pr", "checkout", "43", "--format", "text"], "");
    assert_eq!(
        String::from_utf8(text.stdout).expect("UTF-8 text output"),
        "pr #43 checked out\n"
    );
    assert!(gh_lines(&fixture).contains(&"pr checkout 43 --repo owner/repo".to_string()));

    let failed = fixture.run_pr(&["pr", "checkout", "42"], "pr-checkout-failure");
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert_eq!(error_json(&failed)["code"], "github_cli");
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private provider diagnostic"));

    let force = fixture.run_pr(&["pr", "checkout", "42", "--force"], "");
    assert_eq!(force.status.code(), Some(2));
    assert!(
        !gh_lines(&fixture)
            .iter()
            .any(|line| line.contains("--force"))
    );
}

/// A fixture whose `pr view` read-backs report the state a transition produced.
///
/// The shared fixture answers every read with a fixed state, so a lifecycle test could never
/// observe a closed result. This wrapper keeps a state file, delegates every write to the shared
/// mock unchanged, and answers only `pr view` with a record carrying the tracked state.
fn pr_lifecycle_fixture() -> Fixture {
    let fixture = Fixture::new();
    fs::rename(fixture.bin.join("gh"), fixture.bin.join("gh-shared"))
        .expect("keep the shared gh mock");
    let script = r#"#!/bin/sh
shared="$(dirname "$WORKCTL_GH_LOG")/bin/gh-shared"
state_file="$(dirname "$WORKCTL_GH_LOG")/pr-state"
case "$1 $2" in
    "pr close") printf 'CLOSED' > "$state_file" ;;
    "pr reopen") printf 'OPEN' > "$state_file" ;;
esac
current="$(cat "$state_file" 2>/dev/null || printf 'OPEN')"
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    number="$3"
    printf '%s\n' "$*" >> "$WORKCTL_GH_LOG"
    printf '%s\n' "{\"number\":$number,\"title\":\"Add pull requests\",\"body\":\"PR body\",\"state\":\"$current\",\"isDraft\":false,\"url\":\"https://github.com/owner/repo/pull/$number\",\"baseRefName\":\"main\",\"headRefName\":\"feature-x\",\"author\":{\"login\":\"octocat\"},\"createdAt\":\"2026-01-01T00:00:00Z\",\"updatedAt\":\"2026-01-02T00:00:00Z\",\"mergedAt\":\"\",\"mergeable\":\"MERGEABLE\",\"reviewDecision\":\"\",\"labels\":[{\"name\":\"bug\"}],\"assignees\":[{\"login\":\"hubot\"}]}"
    exit 0
fi
exec "$shared" "$@"
"#;
    let path = fixture.bin.join("gh");
    fs::write(&path, script).expect("write stateful gh wrapper");
    let mut permissions = fs::metadata(&path)
        .expect("read wrapper permissions")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("make wrapper executable");
    fixture
}

#[test]
fn review_merge_ready_close_and_reopen_use_the_gh_commands() {
    let fixture = pr_lifecycle_fixture();

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
        &["pr", "merge", "42", "--method", "squash", "--auto"],
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
        &[
            "pr",
            "edit",
            "42",
            "--state",
            "closed",
            "--comment",
            "bye",
            "--delete-branch",
        ],
        "",
    );
    let closed = success_json(&closed);
    assert_eq!(closed["number"], 42);
    assert_eq!(closed["state"], "closed");
    assert_eq!(gh_input(&fixture), "bye");

    let reopened = fixture.run_pr(
        &["pr", "edit", "42", "--state", "open", "--comment", "back"],
        "",
    );
    let reopened = success_json(&reopened);
    assert_eq!(reopened["number"], 42);
    assert_eq!(reopened["state"], "open");
    assert_eq!(gh_input(&fixture), "back");

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr review 42 --repo owner/repo --approve --body-file -".to_string()));
    assert!(lines.contains(&"pr review 42 --repo owner/repo --request-changes".to_string()));
    assert!(lines.contains(&"pr merge 42 --repo owner/repo --squash --auto".to_string()));
    assert!(lines.contains(&"pr ready 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&"pr ready 42 --repo owner/repo --undo".to_string()));
    assert!(lines.contains(&"pr close 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&"pr reopen 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&"pr comment 42 --repo owner/repo --body-file -".to_string()));
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("pr close 42") && line.contains("--comment"))
    );
    // Branch deletion is workctl's own API call; `gh` never receives the flag, so it can never
    // touch a local branch.
    assert!(lines.contains(&REMOTE_BRANCH_DELETE.to_string()));
    assert!(!lines.iter().any(|line| line.contains("--delete-branch")));

    // Each state edit fetches the pull request, applies the transition, comments on stdin, then
    // reads back once: the pre-transition read, close, comment and final read keep that order, and
    // the reopen phase repeats it.
    let position = |expected: &str| {
        lines
            .iter()
            .position(|line| line.as_str() == expected)
            .unwrap_or_else(|| panic!("missing invocation: {expected}"))
    };
    let views: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("pr view 42 "))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        views.len(),
        4,
        "each edit fetches before and after its transition"
    );
    let close = position("pr close 42 --repo owner/repo");
    let delete_branch = position(REMOTE_BRANCH_DELETE);
    let comment = position("pr comment 42 --repo owner/repo --body-file -");
    let reopen = position("pr reopen 42 --repo owner/repo");
    assert!(views[0] < close && close < delete_branch);
    assert!(delete_branch < comment && comment < views[1]);
    assert!(views[2] < reopen && reopen < views[3]);

    // A transition comment and a branch deletion both require an explicit state.
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let orphan_comment = fixture.run_pr(&["pr", "edit", "42", "--comment", "orphan"], "");
    assert_eq!(error_json(&orphan_comment)["code"], "invalid_input");
    let orphan_delete = fixture.run_pr(&["pr", "edit", "42", "--delete-branch"], "");
    assert_eq!(error_json(&orphan_delete)["code"], "invalid_input");
    assert!(!fixture.log.exists());
}

// RF-PR.10: a merged pull request rejects an explicit state request before any write.
#[test]
fn merged_pull_request_state_edit_fails_before_any_write() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(
        &["pr", "edit", "42", "--state", "open", "--comment", "reopen"],
        "pr-merged",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr view 42 --repo owner/repo --json number,title,body,state,isDraft,url,baseRefName,headRefName,author,createdAt,updatedAt,mergedAt,mergeable,reviewDecision,labels,assignees".to_string()));
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("pr close 42") || line.starts_with("pr reopen 42"))
    );
    assert!(!lines.iter().any(|line| line.starts_with("pr edit 42")));
    assert!(!lines.iter().any(|line| line.starts_with("pr comment 42")));
}

// RF-PR.7: Configured merge defaults apply only when omitted and preserve the explicit CLI method.
#[test]
fn merge_uses_configured_method_and_boolean_branch_default() {
    let fixture = Fixture::new();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&fixture.root)
            .status()
            .expect("initialize config test repository")
            .success()
    );
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"pr":{"mergeMethod":"squash","deleteBranch":false}}}}"#,
    )
    .expect("write repository merge defaults");

    let configured = fixture.run_pr(&["pr", "merge", "42"], "");
    assert_eq!(
        success_json(&configured),
        json!({"number":42,"method":"squash","auto":false})
    );
    assert!(gh_lines(&fixture).contains(&"pr merge 42 --repo owner/repo --squash".to_string()));
    assert!(!gh_lines(&fixture)
        .iter()
        .any(|line| line.contains("git/refs/heads")));

    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"pr":{"mergeMethod":"merge","deleteBranch":true}}}}"#,
    )
    .expect("update repository merge defaults");
    let overridden = fixture.run_pr(&["pr", "merge", "42", "--method", "rebase"], "");
    assert_eq!(
        success_json(&overridden),
        json!({"number":42,"method":"rebase","auto":false})
    );
    assert!(
        gh_lines(&fixture)
            .contains(&"pr merge 42 --repo owner/repo --rebase".to_string())
    );
    assert!(gh_lines(&fixture).contains(&REMOTE_BRANCH_DELETE.to_string()));
    assert!(!gh_lines(&fixture)
        .iter()
        .any(|line| line.contains("--delete-branch")));

    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"pr":{"deleteBranch":false}}}}"#,
    )
    .expect("disable configured branch deletion");
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let explicit_delete = fixture.run_pr(&["pr", "merge", "42", "--delete-branch"], "");
    assert_eq!(
        success_json(&explicit_delete),
        json!({"number":42,"method":null,"auto":false})
    );
    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr merge 42 --repo owner/repo".to_string()));
    assert!(lines.contains(&REMOTE_BRANCH_DELETE.to_string()));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("git/refs/heads"))
            .count(),
        1
    );
}

// RF-PR.7, RF-PR.10: `--delete-branch` removes the remote head ref and never a local branch.
#[test]
fn delete_branch_removes_only_the_remote_ref() {
    let fixture = Fixture::new();

    let merged = fixture.run_pr(
        &["pr", "merge", "42", "--method", "merge", "--delete-branch"],
        "",
    );
    assert_eq!(
        success_json(&merged),
        json!({"number":42,"method":"merge","auto":false})
    );
    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr merge 42 --repo owner/repo --merge".to_string()));
    assert!(lines.contains(&"api repos/owner/repo/pulls/42".to_string()));
    assert!(lines.contains(&REMOTE_BRANCH_DELETE.to_string()));
    assert!(!lines.iter().any(|line| line.contains("--delete-branch")));

    // A fork pull request has its head branch in another repository; this repository's branch of
    // the same name is not the one being deleted.
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let fork = fixture.run_pr(
        &["pr", "edit", "42", "--state", "closed", "--delete-branch"],
        "pr-fork-head",
    );
    assert_eq!(success_json(&fork)["number"], 42);
    assert!(!gh_lines(&fixture)
        .iter()
        .any(|line| line.contains("git/refs/heads")));

    // The head repository of a fork pull request may no longer exist.
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let gone = fixture.run_pr(
        &["pr", "merge", "42", "--delete-branch"],
        "pr-head-repo-gone",
    );
    assert_eq!(success_json(&gone)["number"], 42);
    assert!(!gh_lines(&fixture)
        .iter()
        .any(|line| line.contains("git/refs/heads")));

    // A branch that is already gone is the state deletion is trying to reach.
    let missing = fixture.run_pr(
        &["pr", "merge", "42", "--delete-branch"],
        "branch-delete-missing",
    );
    assert_eq!(success_json(&missing)["number"], 42);

    // Any other deletion failure leaves the completed merge reported as partial progress.
    let failed = fixture.run_pr(
        &["pr", "merge", "42", "--delete-branch"],
        "branch-delete-failure",
    );
    let error = error_json(&failed);
    assert_eq!(error["code"], "partial_success");
    assert_eq!(error["details"]["resource"]["number"], 42);
    assert_eq!(
        error["details"]["completed"],
        json!(["pull request merge"])
    );
    assert_eq!(
        error["details"]["pending"],
        json!(["remote branch deletion"])
    );
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private provider diagnostic"));

    // The same failure after a close reports the close and lists the skipped comment as pending.
    let close_failed = fixture.run_pr(
        &[
            "pr",
            "edit",
            "42",
            "--state",
            "closed",
            "--comment",
            "bye",
            "--delete-branch",
        ],
        "branch-delete-failure",
    );
    let error = error_json(&close_failed);
    assert_eq!(error["code"], "partial_success");
    assert_eq!(
        error["details"]["completed"],
        json!(["pull request close"])
    );
    assert_eq!(
        error["details"]["pending"],
        json!([
            "remote branch deletion",
            "transition comment",
            "pull request readback"
        ])
    );
}

// RF-PR.7: a queued merge has not happened, so it has no branch to delete.
#[test]
fn delete_branch_with_auto_is_rejected() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(&["pr", "merge", "42", "--auto", "--delete-branch"], "");
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!fixture.log.exists());

    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&fixture.root)
            .status()
            .expect("initialize config test repository")
            .success()
    );
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"pr":{"deleteBranch":true}}}}"#,
    )
    .expect("write configured branch deletion default");
    let queued = fixture.run_pr(&["pr", "merge", "42", "--auto"], "");
    assert_eq!(
        success_json(&queued),
        json!({"number":42,"method":null,"auto":true})
    );
    assert!(!gh_lines(&fixture)
        .iter()
        .any(|line| line.contains("git/refs/heads")));
}

#[test]
fn review_without_an_event_is_a_usage_error() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(&["pr", "review", "42"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(!fixture.log.exists());
}

// RF-PR.6: a summary-only review keeps the native command and never touches the review API.
#[test]
fn summary_only_review_uses_the_native_command() {
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
    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr review 42 --repo owner/repo --approve --body-file -".to_owned()));
    assert!(!lines.iter().any(|line| line.starts_with("api ")));
    assert!(!lines.iter().any(|line| line.contains("headRefOid")));
}

// RF-PR.6: inline comments travel as one review request anchored to the observed head.
#[test]
fn inline_review_posts_one_request_anchored_to_the_head() {
    let fixture = Fixture::new();
    let file = fixture.root.join("inline.txt");
    fs::write(&file, "from file").expect("write inline body file");

    let reviewed = fixture.run_pr(
        &[
            "pr",
            "review",
            "42",
            "--approve",
            "--body",
            "Ship it",
            "--inline",
            "src/lib.rs:10",
            "right side",
            "--inline-file",
            "src/a:b.rs:12:left",
            file.to_str().expect("utf-8 path"),
        ],
        "review-inline",
    );
    assert_eq!(
        success_json(&reviewed),
        json!({"number": 42, "event": "approve"})
    );

    let lines = gh_lines(&fixture);
    assert!(lines.contains(&"pr view 42 --repo owner/repo --json headRefOid".to_owned()));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.as_str() == REVIEW_POST)
            .count(),
        1
    );
    let payload: serde_json::Value =
        serde_json::from_str(&gh_input(&fixture)).expect("inline review payload");
    assert_eq!(
        payload,
        json!({
            "event": "APPROVE",
            "commit_id": "9f8e7d6c5b4a3210cafebabe00112233445566",
            "body": "Ship it",
            "comments": [
                {"path": "src/lib.rs", "line": 10, "side": "RIGHT", "body": "right side"},
                {"path": "src/a:b.rs", "line": 12, "side": "LEFT", "body": "from file"},
            ],
        })
    );
}

// RF-PR.6: every inline input is validated before the provider is reached.
#[test]
fn inline_review_validation_fails_before_provider_access() {
    let fixture = Fixture::new();
    for args in [
        vec![
            "pr",
            "review",
            "42",
            "--approve",
            "--inline",
            "src/lib.rs:0",
            "note",
        ],
        vec![
            "pr",
            "review",
            "42",
            "--approve",
            "--inline",
            "src/lib.rs:1:sideways",
            "note",
        ],
        vec![
            "pr",
            "review",
            "42",
            "--approve",
            "--inline",
            "src/lib.rs:1",
            "   ",
        ],
        vec![
            "pr",
            "review",
            "42",
            "--comment",
            "--inline",
            "src/lib.rs:1",
            "note",
        ],
        vec![
            "pr",
            "review",
            "42",
            "--request-changes",
            "--body",
            "  ",
            "--inline",
            "src/lib.rs:1",
            "note",
        ],
        vec![
            "pr",
            "review",
            "42",
            "--approve",
            "--body-file",
            "-",
            "--inline-file",
            "src/lib.rs:1",
            "-",
        ],
    ] {
        let output = fixture.run_pr(&args, "");
        assert_eq!(error_json(&output)["code"], "invalid_input", "{args:?}");
    }
    assert!(!fixture.log.exists());
}

// RF-PR.6: each text source and the aggregate review payload are bounded.
#[test]
fn inline_review_sources_and_payload_are_bounded() {
    let fixture = Fixture::new();
    let oversized = fixture.root.join("oversized.txt");
    fs::write(&oversized, "x".repeat(1024 * 1024 + 1)).expect("write oversized body");

    let output = fixture.run_pr(
        &[
            "pr",
            "review",
            "42",
            "--approve",
            "--inline-file",
            "src/lib.rs:1",
            oversized.to_str().expect("utf-8 path"),
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");

    let half = fixture.root.join("half.txt");
    fs::write(&half, "x".repeat(600 * 1024)).expect("write half-size body");
    let output = fixture.run_pr(
        &[
            "pr",
            "review",
            "42",
            "--approve",
            "--body-file",
            half.to_str().expect("utf-8 path"),
            "--inline-file",
            "src/lib.rs:1",
            half.to_str().expect("utf-8 path"),
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!fixture.log.exists());
}

// RF-PR.6: an unreadable head aborts before any review request is written.
#[test]
fn inline_review_without_a_readable_head_never_writes() {
    let fixture = Fixture::new();
    let output = fixture.run_pr(
        &[
            "pr",
            "review",
            "42",
            "--approve",
            "--inline",
            "src/lib.rs:1",
            "note",
        ],
        "review-head-missing",
    );
    assert_eq!(error_json(&output)["code"], "provider_response");
    assert!(
        !gh_lines(&fixture)
            .iter()
            .any(|line| line.starts_with("api "))
    );
}

// RF-PR.6: a failed or unreadable submission is an uncertain write, never a retry.
#[test]
fn interrupted_inline_review_reports_an_uncertain_write_without_retrying() {
    for mode in ["review-failure", "review-unconfirmed", "review-pending"] {
        let fixture = Fixture::new();
        let output = fixture.run_pr(
            &[
                "pr",
                "review",
                "42",
                "--comment",
                "--body",
                "Summary",
                "--inline",
                "src/lib.rs:1",
                "note",
            ],
            mode,
        );
        assert_eq!(
            error_json(&output)["code"],
            "github_write_uncertain",
            "{mode}"
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private review diagnostic"));
        let review_posts = gh_lines(&fixture)
            .iter()
            .filter(|line| line.as_str() == REVIEW_POST)
            .count();
        assert_eq!(review_posts, 1, "{mode}");
    }
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
    assert!(
        !gh_lines(&fixture)
            .iter()
            .any(|line| line.contains("milestones"))
    );
    assert!(
        !gh_lines(&fixture)
            .iter()
            .any(|line| line.contains("--milestone"))
    );

    let current = Fixture::new();
    let created = current.run_pr(
        &[
            "pr",
            "create",
            "--title",
            "Current",
            "--milestone",
            "@current",
        ],
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
        &[
            "pr",
            "create",
            "--title",
            "None eligible",
            "--milestone",
            "@current",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(
        !gh_lines(&missing)
            .iter()
            .any(|line| line.starts_with("pr create"))
    );
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
// RF-PR.16-RF-PR.19: PR comments, conversation locks, and revert creation.
#[test]
fn pr_comment_lock_and_revert_use_native_gh_commands() {
    let fixture = Fixture::new();

    let comment = fixture.run_pr(&["pr", "comment", "42", "--body", "Looks good"], "");
    assert_eq!(success_json(&comment)["target"], "pr");
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read comment input"),
        "Looks good"
    );

    let locked = fixture.run_pr(&["pr", "lock", "42", "--reason", "too_heated"], "");
    assert_eq!(success_json(&locked)["locked"], true);
    let unlocked = fixture.run_pr(&["pr", "lock", "42", "--undo"], "");
    assert_eq!(success_json(&unlocked)["locked"], false);

    let reverted = fixture.run_pr(
        &[
            "pr",
            "revert",
            "42",
            "--title",
            "Revert feature",
            "--body",
            "Revert details",
            "--draft",
        ],
        "",
    );
    let reverted = success_json(&reverted);
    assert_eq!(reverted["number"], 42);
    assert_eq!(reverted["pull_request"], 90);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read revert body"),
        "Revert details"
    );

    let log = fs::read_to_string(&fixture.log).expect("read gh arguments");
    assert!(log.contains("pr comment 42 --repo owner/repo --body-file -"));
    assert!(log.contains("pr lock 42 --repo owner/repo --reason too_heated"));
    assert!(log.contains("pr unlock 42 --repo owner/repo"));
    assert!(
        log.contains("pr revert 42 --repo owner/repo --title Revert feature --draft --body-file -")
    );

    let invalid = fixture.run_pr(&["pr", "comment", "42"], "");
    assert_eq!(error_json(&invalid)["code"], "invalid_input");
    let invalid_lock = fixture.run_pr(&["pr", "lock", "42", "--reason", "unknown"], "");
    assert!(!invalid_lock.status.success());
}
