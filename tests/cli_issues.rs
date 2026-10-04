#![cfg(unix)]

mod common;

use std::fs;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

use common::{Fixture, error_json, success_json};

#[test]
fn create_list_view_and_edit_preserve_the_cli_contract() {
    let fixture = Fixture::new();
    let title = "quoted \" title; $(touch should-not-run)";
    let body = "first line\nsecond line; $(touch should-not-run)";

    let created = fixture.run_issue(&["issue", "create", "--title", title, "--body", body], "");
    let created = success_json(&created);
    assert_eq!(created["number"], 7);
    assert_eq!(created["state"], "open");
    assert_eq!(
        created["body"],
        "Provider body\n\n## Notes\n\noriginal notes"
    );
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read create body"),
        body
    );

    let created_without_body = fixture.run_issue(&["issue", "create", "--title", "No body"], "");
    success_json(&created_without_body);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read empty-body create"),
        ""
    );
    let listed = fixture.run_issue(&["issue", "list", "--state", "closed", "--limit", "1"], "");
    let listed = success_json(&listed);
    assert_eq!(listed.as_array().expect("issue list").len(), 1);
    assert_eq!(listed[0]["number"], 8);
    assert_eq!(listed[0]["state"], "open");
    assert!(listed[0].get("body").is_none());
    let maximum = fixture.run_issue(&["issue", "list", "--limit", "1000"], "");
    assert_eq!(
        success_json(&maximum).as_array().expect("issue list").len(),
        2
    );

    let shown = fixture.run_issue(&["issue", "view", "7"], "");
    let shown = success_json(&shown);
    assert_eq!(shown["url"], "https://github.com/owner/repo/issues/7");
    assert_eq!(shown["created_at"], "2026-01-01T00:00:00Z");

    let edited = fixture.run_issue(&["issue", "edit", "7", "--body", ""], "");
    let edited = success_json(&edited);
    assert_eq!(edited["number"], 7);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read edit body"),
        ""
    );

    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    assert!(log.contains("issue create --repo owner/repo --title"));
    assert!(log.contains(
        "issue list --repo owner/repo --state closed --limit 1 --json number,title,state,url,updatedAt"
    ));
    assert!(log.contains(
        "issue list --repo owner/repo --state open --limit 1000 --json number,title,state,url,updatedAt"
    ));
    assert!(log.contains("api repos/owner/repo/issues/7"));
    assert!(log.contains("issue edit 7 --repo owner/repo --body-file -"));
    assert!(!fixture.root.join("should-not-run").exists());
}

#[test]
fn text_output_and_provider_failures_are_safe() {
    let fixture = Fixture::new();
    let text = fixture.run_issue(&["issue", "list", "--format", "text", "--limit", "2"], "");
    assert!(text.status.success());
    assert!(String::from_utf8_lossy(&text.stdout).contains("#8 First [open]"));

    let auth = fixture.run_issue(&["issue", "view", "7"], "auth-failure");
    let error = error_json(&auth);
    assert_eq!(error["code"], "authentication");
    assert!(!String::from_utf8_lossy(&auth.stderr).contains("provider diagnostic"));

    let pull_request = fixture.run_issue(&["issue", "view", "7"], "pull-request");
    assert_eq!(error_json(&pull_request)["code"], "not_issue");
    let large_body = "x".repeat(100_000);
    let early_exit = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Closed stdin",
            "--body",
            &large_body,
        ],
        "early-exit",
    );
    assert_eq!(success_json(&early_exit)["number"], 7);
    let pull_request_edit = fixture.run_issue(
        &["issue", "edit", "7", "--title", "Must not change"],
        "pull-request",
    );
    assert_eq!(error_json(&pull_request_edit)["code"], "not_issue");
    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    assert!(!log.contains("api --method PATCH"));
}

#[test]
fn help_documents_every_flag_and_the_patch_contract() {
    let fixture = Fixture::new();
    let group = fixture.run(&["issue", "--help"], "");
    assert!(group.status.success());
    let group_help = String::from_utf8_lossy(&group.stdout);
    assert!(!group_help.contains("suggest-labels"));
    for args in [
        &["issue", "create", "--help"][..],
        &["issue", "list", "--help"][..],
        &["issue", "edit", "--help"][..],
    ] {
        let output = fixture.run(args, "");
        assert!(output.status.success());
        let help = String::from_utf8_lossy(&output.stdout);
        if args[1] == "create" || args[1] == "edit" {
            assert!(help.contains("@auto"));
            assert!(!help.contains("--auto-labels"));
        }
        if args[1] != "list" {
            assert!(help.contains("--attach"));
        }
        for line in help
            .lines()
            .filter(|line| line.starts_with("  ") && line.trim_start().starts_with("--"))
        {
            let line = line.trim();
            let described = line
                .split_once("  ")
                .is_some_and(|(_, description)| !description.trim().is_empty());
            assert!(described, "flag without a help description: {line}");
        }
    }
    // The patch failure mode is the one thing a caller cannot guess from the flag names.
    let edit = fixture.run(&["issue", "edit", "--help"], "");
    let help = String::from_utf8_lossy(&edit.stdout);
    assert!(help.contains("patch_conflict"));
    assert!(help.contains("exact context match"));
    assert!(!fixture.log.exists());
}

#[test]
fn removed_suggestion_command_is_rejected_before_github_access() {
    let fixture = Fixture::new();
    let output = fixture.run_issue(&["issue", "suggest-labels", "7"], "");
    assert!(!output.status.success());
    assert!(!fixture.log.exists());
}

#[test]
fn automatic_label_markers_require_key_before_github_access() {
    let fixture = Fixture::new();
    let create = fixture.run_issue(
        &["issue", "create", "--title", "Bug", "--label", "@auto"],
        "",
    );
    assert_eq!(error_json(&create)["code"], "decision_authentication");
    let edit = fixture.run_issue(&["issue", "edit", "7", "--add-label", "@auto"], "");
    assert_eq!(error_json(&edit)["code"], "decision_authentication");
    let remove = fixture.run_issue(&["issue", "edit", "7", "--remove-label", "@auto"], "");
    assert_eq!(error_json(&remove)["code"], "invalid_input");
    assert!(!fixture.log.exists());
}

#[test]
fn auto_marker_uses_local_llm_adapter_and_keeps_manual_labels() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local model fixture");
    let address = listener.local_addr().expect("fixture address");
    let model = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept model request");
        let mut reader = std::io::BufReader::new(stream);
        let mut line = String::new();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).expect("read HTTP header");
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().expect("content length");
            }
        }
        let mut request = vec![0; length];
        reader.read_exact(&mut request).expect("read model request");
        let request: serde_json::Value = serde_json::from_slice(&request).expect("model JSON");
        let prompt = request["messages"][1]["content"]
            .as_str()
            .expect("user prompt");
        assert!(prompt.contains("Parser regression"));
        assert!(prompt.contains("Parser crashes"));
        assert!(prompt.contains("Broken behavior"));
        let response = r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.91},{\"label\":\"docs\",\"probability\":0.2}]}"}}]}"#;
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        ).expect("write model response");
    });

    let args = [
        "--provider",
        "github",
        "--repo",
        "owner/repo",
        "issue",
        "create",
        "--title",
        "Parser regression",
        "--body",
        "Parser crashes",
        "--label",
        "@auto",
        "--label",
        "manual",
    ];
    let output = fixture.run_with_model(
        &args,
        "",
        "local/test-model",
        Some(&format!("http://{address}/v1")),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    model.join().expect("model fixture");
    let log = fs::read_to_string(&fixture.log).expect("read GitHub fixture log");
    assert!(log.contains("issue create"));
    assert!(log.contains("--label bug"));
    assert!(log.contains("--label manual"));
    assert!(!log.contains("--label @auto"));
}

#[test]
fn invalid_inputs_and_unsupported_provider_fail_before_gh() {
    let fixture = Fixture::new();
    let blank_title = fixture.run_issue(&["issue", "create", "--title", "   "], "");
    assert_eq!(error_json(&blank_title)["code"], "invalid_input");
    let invalid = [
        &["issue", "list", "--limit", "0"][..],
        &["issue", "list", "--limit", "1001"][..],
        &["issue", "view", "0"][..],
        &["issue", "view", "not-a-number"][..],
        &["issue", "edit", "7"][..],
        &["issue", "edit", "7", "--title", "  "][..],
        &["issue", "create", "--title", "Valid", "--assignee", "   "][..],
        &["issue", "list", "--search", "   "][..],
        &["issue", "edit", "7", "--add-label", "   "][..],
        &[
            "issue",
            "edit",
            "7",
            "--milestone",
            "M1",
            "--clear-milestone",
        ][..],
        &["issue", "delete", "7"][..],
    ];
    for args in invalid {
        let output = fixture.run_issue(args, "");
        assert_eq!(error_json(&output)["code"], "invalid_input");
    }
    assert!(!fixture.log.exists());

    // GitLab is a supported provider, so the grammar — not a runtime check — has to reject the
    // other provider's verbs, before any provider command runs.
    let wrong_grammar = fixture.run(
        &[
            "--provider",
            "gitlab",
            "--repo",
            "owner/repo",
            "issue",
            "edit",
            "1",
        ],
        "",
    );
    assert_eq!(error_json(&wrong_grammar)["code"], "invalid_input");
    assert!(!fixture.log.exists());

    let help = fixture.run(&["issue", "--help"], "");
    assert!(help.status.success());
    let help_text = String::from_utf8_lossy(&help.stdout);
    let commands: Vec<_> = help_text
        .lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(commands, ["create", "list", "view", "edit"]);
    assert!(!fixture.log.exists());
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&fixture.root)
            .status()
            .expect("initialize test Git root")
            .success()
    );
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"repo":"owner/repo"}"#,
    )
    .expect("write invalid project config");
    let invalid_config = fixture.run_issue(&["issue", "create", "--title", "Must not mutate"], "");
    assert_eq!(error_json(&invalid_config)["code"], "config");
    assert!(!fixture.log.exists());
}

#[test]
fn body_changes_reach_github_without_rewriting_the_whole_body() {
    let fixture = Fixture::new();
    let patch_path = fixture.root.join("body.patch");
    fs::write(
        &patch_path,
        "@@ -5,1 +5,1 @@\n-original notes\n+revised notes\n",
    )
    .expect("write patch file");

    let appended = fixture.run_issue(&["issue", "edit", "7", "--append-body", "extra"], "");
    success_json(&appended);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read append body"),
        "Provider body\n\n## Notes\n\noriginal notes\nextra"
    );

    let sectioned = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--replace-section",
            "## Notes",
            "--section-body",
            "replaced",
        ],
        "",
    );
    success_json(&sectioned);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read section body"),
        "Provider body\n\n## Notes\nreplaced"
    );

    let patched = fixture.run_issue(&["issue", "edit", "7", "--patch-file", "body.patch"], "");
    success_json(&patched);
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read patched body"),
        "Provider body\n\n## Notes\n\nrevised notes"
    );

    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    // One fetch per edit: the body is read once and patched locally.
    assert_eq!(log.matches("api repos/owner/repo/issues/7").count(), 6);
    assert_eq!(log.matches("issue edit 7").count(), 3);
}

#[test]
fn issue_metadata_and_attachments_follow_optional_gh_flags() {
    let fixture = Fixture::new();
    let created = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Triaged",
            "--label",
            "bug",
            "--label",
            "p1",
            "--assignee",
            "octocat",
            "--milestone",
            "M1",
            "--project",
            "Roadmap",
            "--attach",
            "screen.png#Screenshot",
        ],
        "",
    );
    assert_eq!(success_json(&created)["number"], 7);
    let log = fs::read_to_string(&fixture.log).expect("read create flags");
    assert!(log.lines().any(|line| line == "--version"));
    assert!(!log.contains("milestones"));
    assert!(log.lines().any(|line| {
        line == "issue create --repo owner/repo --title Triaged --body-file - --assignee octocat --label bug --label p1 --milestone M1 --project Roadmap --attach screen.png#Screenshot"
    }));

    let edited = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--add-label",
            "triaged",
            "--add-assignee",
            "hubot",
            "--remove-project",
            "Backlog",
            "--clear-milestone",
            "--attach",
            "clip.mp4",
        ],
        "",
    );
    assert_eq!(success_json(&edited)["number"], 7);
    let log = fs::read_to_string(&fixture.log).expect("read edit flags");
    assert!(log.lines().any(|line| {
        line == "issue edit 7 --repo owner/repo --add-assignee hubot --add-label triaged --remove-milestone --remove-project Backlog --attach clip.mp4"
    }));
}

#[test]
fn failed_attachment_create_reports_possible_remote_creation_without_retrying() {
    let fixture = Fixture::new();
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Media",
            "--attach",
            "screen.png",
        ],
        "attachment-create-failure",
    );
    assert_eq!(error_json(&output)["code"], "attachment_create_uncertain");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider diagnostic"));
    let log = fs::read_to_string(&fixture.log).expect("read create attempt");
    assert_eq!(log.matches("issue create ").count(), 1);
}

#[test]
fn attachments_require_supported_gh_without_attempting_the_write() {
    let fixture = Fixture::new();
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "With image",
            "--attach",
            "screen.png",
        ],
        "old-version",
    );
    assert_eq!(error_json(&output)["code"], "dependency_version");
    let lines = fs::read_to_string(&fixture.log).expect("read version check");
    assert_eq!(
        lines.lines().collect::<Vec<_>>(),
        ["auth status --hostname github.com", "--version"]
    );
}

#[test]
fn list_filters_and_edit_guards_are_enforced() {
    let fixture = Fixture::new();
    let listed = fixture.run_issue(
        &[
            "issue",
            "list",
            "--label",
            "bug",
            "--label",
            "p1",
            "--assignee",
            "me",
            "--search",
            "in:title fix",
            "--milestone",
            "v1",
        ],
        "",
    );
    success_json(&listed);
    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    assert!(log.contains(
        "issue list --repo owner/repo --state open --limit 30 --json number,title,state,url,updatedAt --label bug --label p1 --assignee me --milestone v1 --search in:title fix"
    ));

    let guarded = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--title",
            "Stale",
            "--expect-updated-at",
            "2020-01-01T00:00:00Z",
        ],
        "",
    );
    assert_eq!(error_json(&guarded)["code"], "conflict");
    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    assert!(!log.contains("issue edit 7"));

    let accepted = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--title",
            "Fresh",
            "--expect-updated-at",
            "2026-01-02T00:00:00Z",
        ],
        "",
    );
    success_json(&accepted);

    let bad_patch = fixture.root.join("bad.patch");
    fs::write(&bad_patch, "@@ -1,1 +1,1 @@\n-not the body\n+replacement\n")
        .expect("write non-applying patch");
    let rejected = [
        &["issue", "edit", "7", "--body", "a", "--append-body", "b"][..],
        &["issue", "edit", "7", "--replace-section", "## Notes"][..],
        &["issue", "edit", "7", "--section-body", "orphan"][..],
        &["issue", "edit", "7", "--patch-file", "bad.patch"][..],
    ];
    for args in rejected {
        let output = fixture.run_issue(args, "");
        let code = error_json(&output)["code"].clone();
        assert!(
            code == "invalid_input" || code == "patch_conflict",
            "unexpected code {code} for {args:?}"
        );
    }
    let log = fs::read_to_string(&fixture.log).expect("read gh argument log");
    assert_eq!(log.matches("issue edit 7").count(), 1);
}

#[test]
fn issue_create_omits_the_milestone_unless_current_is_requested() {
    let fixture = Fixture::new();
    let created = fixture.run_issue(
        &["issue", "create", "--title", "No milestone"],
        "milestone-nearest",
    );
    assert_eq!(success_json(&created)["number"], 7);
    let log = fs::read_to_string(&fixture.log).expect("read GitHub calls");
    assert!(!log.contains("milestones"));
    assert!(!log.contains("--milestone"));

    let current = Fixture::new();
    let created = current.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Current milestone",
            "--milestone",
            "@current",
        ],
        "milestone-nearest",
    );
    assert_eq!(success_json(&created)["number"], 7);
    let log = fs::read_to_string(&current.log).expect("read GitHub calls");
    assert!(log.contains("milestones"));
    assert!(log.contains("--milestone Sooner"));

    let edited = Fixture::new();
    let output = edited.run_issue(&["issue", "edit", "7", "--milestone", "@current"], "milestone-nearest");
    success_json(&output);
    let log = fs::read_to_string(&edited.log).expect("read GitHub calls");
    assert!(log.contains("issue edit 7 --repo owner/repo --milestone Sooner"));

    let assigned = Fixture::new();
    let created = assigned.run_issue(
        &[
            "issue", "create", "--title", "Mine", "--assignee", "@me", "--milestone", "@current",
        ],
        "milestone-nearest",
    );
    assert_eq!(success_json(&created)["number"], 7);
    let log = fs::read_to_string(&assigned.log).expect("read GitHub calls");
    assert!(log.contains(
        "issue create --repo owner/repo --title Mine --body-file - --assignee @me --milestone Sooner"
    ));

    let explicit = Fixture::new();
    let created = explicit.run_issue(
        &["issue", "create", "--title", "Explicit", "--milestone", "M1"],
        "milestone-tie",
    );
    assert_eq!(success_json(&created)["number"], 7);
    let log = fs::read_to_string(&explicit.log).expect("read GitHub calls");
    assert!(!log.contains("milestones"));
    assert!(log.contains("--milestone M1"));
}

#[test]
fn issue_create_rejects_current_without_an_eligible_milestone() {
    let fixture = Fixture::new();
    let output = fixture.run_issue(
        &["issue", "create", "--title", "None eligible", "--milestone", "@current"],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    let log = fs::read_to_string(&fixture.log).expect("read GitHub calls");
    assert!(!log.contains("issue create"));
}

#[test]
fn issue_create_rejects_tied_nearest_milestones_without_writing() {
    let fixture = Fixture::new();
    let output = fixture.run_issue(
        &["issue", "create", "--title", "Ambiguous", "--milestone", "@current"],
        "milestone-tie",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    let log = fs::read_to_string(&fixture.log).expect("read GitHub calls");
    assert!(!log.contains("issue create"));
}
