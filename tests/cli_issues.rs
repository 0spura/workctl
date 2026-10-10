#![cfg(unix)]

mod common;

use std::fs;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::thread;

use common::{Fixture, error_json, success_json};

/// Exact `gh` argv lines in invocation order, so an ordering assertion cannot match a prefix of a
/// longer argument list by accident.
fn gh_lines(fixture: &Fixture) -> Vec<String> {
    fs::read_to_string(&fixture.log)
        .expect("read gh argument log")
        .lines()
        .map(str::to_owned)
        .collect()
}

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

// RF-WI.1: Removed model IDs fail before creating an issue or accessing its provider.
#[test]
fn removed_gliner_model_is_rejected_before_provider_access() {
    let fixture = Fixture::new();
    let output = fixture.run_with_model(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Unsupported model",
            "--label",
            "@auto",
        ],
        "",
        "fastino/GLiNER2.5-Decide",
        None,
    );
    assert_eq!(error_json(&output)["code"], "decision_config");
    assert!(!fixture.log.exists());
}

// RF-CFG.3: The configured candidate list, not the full repository catalog, reaches the model.
#[test]
fn auto_marker_uses_local_llm_adapter_and_keeps_manual_labels() {
    let fixture = Fixture::new();
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&fixture.root)
        .status()
        .expect("initialize config test repository");
    assert!(status.success());
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"issue":{"labelCandidates":["bug"]}}}}"#,
    )
    .expect("write label candidate config");
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
        assert!(!prompt.contains("docs"));
        let response = r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.91}]}"}}]}"#;
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
            "--remove-milestone",
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
    assert_eq!(
        commands,
        [
            "create", "list", "blockers", "view", "edit", "comment", "lock", "develop"
        ]
    );
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
            "--remove-milestone",
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
fn init_configured_project(fixture: &Fixture, fields: &str) {
    init_configured_project_with_auto_fields(fixture, fields, &[]);
}

fn init_configured_project_with_auto_fields(fixture: &Fixture, fields: &str, auto_fields: &[&str]) {
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&fixture.root)
        .status()
        .expect("initialize config test repository");
    assert!(status.success());
    let fields: serde_json::Value =
        serde_json::from_str(&format!("{{{fields}}}")).expect("parse test project fields");
    let config = serde_json::json!({
        "defaults": {
            "github": {
                "issue": {
                    "assignees": ["octocat"],
                    "labels": ["triaged"],
                    "labelCandidates": ["bug", "docs"],
                    "project": {
                        "url": "https://github.com/orgs/owner/projects/3",
                        "repositories": ["owner/repo"],
                        "fields": fields,
                        "autoSelectFields": auto_fields,
                    }
                }
            }
        }
    });
    fs::write(
        fixture.root.join(".workctl.json"),
        serde_json::to_vec(&config).expect("serialize GitHub Project config"),
    )
    .expect("write GitHub Project defaults");
}

// RF-WI.6: Model-selected Project options share one classification call with automatic labels.
#[test]
fn configured_project_fields_are_selected_and_written() {
    let fixture = Fixture::new();
    init_configured_project_with_auto_fields(&fixture, "", &["Priority"]);
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
        assert!(prompt.contains("Project priority"));
        assert!(prompt.contains("Priority=High"));
        assert!(prompt.contains("Priority=Low"));
        let response = r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"bug\",\"probability\":0.91},{\"label\":\"docs\",\"probability\":0.05},{\"label\":\"Priority=High\",\"probability\":0.92},{\"label\":\"Priority=Low\",\"probability\":0.08}]}"}}]}"#;
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .expect("write model response");
    });
    let output = fixture.run_with_model(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Project priority",
            "--body",
            "Important work",
            "--label",
            "@auto",
        ],
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
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert_eq!(invocations.matches("api graphql").count(), 1);
    assert_eq!(invocations.matches("issue create").count(), 1);
    assert!(invocations.contains("--label bug"));
    assert!(invocations.contains("--field-id PRIORITY_ID --single-select-option-id OPTION_HIGH"));
}

// RF-WI.6: Explicit Project values suppress model choice for that field and take precedence.
#[test]
fn explicit_project_field_overrides_auto_selection_without_model_call() {
    let fixture = Fixture::new();
    init_configured_project_with_auto_fields(&fixture, r#""Priority":"High""#, &["Priority"]);
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Explicit priority",
            "--project-field",
            "Priority=Low",
        ],
        "",
    );
    assert_eq!(success_json(&output)["number"], 7);
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("--field-id PRIORITY_ID --single-select-option-id OPTION_LOW"));
    assert_eq!(invocations.matches("issue create").count(), 1);
}

// RF-WI.6: Low-confidence Project field classifications leave the field unset.
#[test]
fn low_confidence_project_field_is_left_unset() {
    let fixture = Fixture::new();
    init_configured_project_with_auto_fields(&fixture, "", &["Priority"]);
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
        let response = r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"Priority=High\",\"probability\":0.61},{\"label\":\"Priority=Low\",\"probability\":0.39}]}"}}]}"#;
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .expect("write model response");
    });
    let output = fixture.run_with_model(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Uncertain priority",
        ],
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
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("issue create"));
    assert!(invocations.contains("project item-add"));
    assert!(!invocations.contains("--field-id PRIORITY_ID"));
}

// RF-WI.6: A tied best Project option is rejected before the issue write.
#[test]
fn tied_project_field_scores_fail_before_issue_creation() {
    let fixture = Fixture::new();
    init_configured_project_with_auto_fields(&fixture, "", &["Priority"]);
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
        let response = r#"{"choices":[{"message":{"content":"{\"suggestions\":[{\"label\":\"Priority=High\",\"probability\":0.9},{\"label\":\"Priority=Low\",\"probability\":0.9}]}"}}]}"#;
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .expect("write model response");
    });
    let output = fixture.run_with_model(
        &[
            "--provider",
            "github",
            "--repo",
            "owner/repo",
            "issue",
            "create",
            "--title",
            "Tied priority",
        ],
        "",
        "local/test-model",
        Some(&format!("http://{address}/v1")),
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    model.join().expect("model fixture");
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(!invocations.contains("issue create"));
    assert!(!invocations.contains("project item-add"));
}

// RF-WI.6: Configured defaults and typed Project fields resolve before the issue write.
#[test]
fn github_project_defaults_and_dynamic_fields_apply_before_issue_readback() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, r#""Priority":"High","Notes":"from config""#);
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Project-backed issue",
            "--project",
            "Roadmap",
            "--project-field",
            "Priority=Low",
            "--project-field",
            "Effort=2",
            "--project-field",
            "Start date=2026-10-07",
            "--project-field",
            "Iteration=Sprint A",
        ],
        "",
    );
    assert_eq!(success_json(&output)["number"], 7);
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("issue create --repo owner/repo --title Project-backed issue --body-file - --assignee octocat --label triaged"));
    assert!(
        invocations.contains(
            "project item-add 3 --owner owner --url https://github.com/owner/repo/issues/7"
        )
    );
    assert!(invocations.contains("project item-edit --project-id PVT_owner_project --url https://github.com/owner/repo/issues/7 --field-id PRIORITY_ID --single-select-option-id OPTION_LOW"));
    assert!(invocations.contains("project item-edit --project-id PVT_owner_project --url https://github.com/owner/repo/issues/7 --field-id EFFORT_ID --number 2"));
    assert!(invocations.contains("project item-edit --project-id PVT_owner_project --url https://github.com/owner/repo/issues/7 --field-id START_ID --date 2026-10-07"));
    assert!(invocations.contains("project item-edit --project-id PVT_owner_project --url https://github.com/owner/repo/issues/7 --field-id ITERATION_ID --iteration-id ITERATION_A"));
    assert!(invocations.contains("--field-id NOTES_ID --text from config"));
}
// RF-CFG.3: Project membership and field defaults do not cross repository allowlists.
#[test]
fn project_defaults_do_not_apply_outside_the_repository_allowlist() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, r#""Priority":"High""#);
    let output = fixture.run(
        &[
            "--provider",
            "github",
            "--repo",
            "other/repo",
            "issue",
            "create",
            "--title",
            "Outside scope",
        ],
        "project-scope-outside",
    );
    assert_eq!(success_json(&output)["number"], 7);
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("--assignee octocat --label triaged"));
    assert!(!invocations.contains("api graphql"));
    assert!(!invocations.contains("project item-add"));
}

// RF-WI.6: Invalid configured Project values block issue creation.
#[test]
fn invalid_project_field_fails_before_issue_creation() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, r#""Priority":"Not an option""#);
    let output = fixture.run_issue(&["issue", "create", "--title", "Invalid"], "");
    let error = error_json(&output);
    assert_eq!(error["code"], "invalid_input");
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("api graphql"));
    assert!(!invocations.contains("issue create"));
}
// RF-WI.6: Ambiguous Project selection fails before provider access.
#[test]
fn multiple_projects_with_field_assignment_fail_before_remote_writes() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, "");
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Ambiguous",
            "--project",
            "Roadmap",
            "--project",
            "Other",
            "--project-field",
            "Effort=2",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!fixture.log.exists());
}
// RF-WI.6: Explicit Project selection must match the configured profile.
#[test]
fn mismatched_project_title_fails_before_issue_creation() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, "");
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Wrong board",
            "--project",
            "Other",
            "--project-field",
            "Effort=2",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert!(invocations.contains("api graphql"));
    assert!(!invocations.contains("issue create"));
}

// RF-WI.6: Post-create Project failure reports a generic, non-retriable partial result.
#[test]
fn project_field_failure_returns_created_issue_without_retrying() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, r#""Priority":"High""#);
    let output = fixture.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Partial",
            "--project-field",
            "Effort=2",
        ],
        "project-edit-failure",
    );
    let error = error_json(&output);
    assert_eq!(error["code"], "partial_success");
    assert!(
        !error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("GitHub")
    );
    assert_eq!(error["details"]["resource"]["type"], "issue");
    assert_eq!(error["details"]["resource"]["number"], 7);
    assert_eq!(
        error["details"]["resource"]["url"],
        "https://github.com/owner/repo/issues/7"
    );
    assert_eq!(
        error["details"]["completed"],
        serde_json::json!(["issue creation", "project membership"])
    );
    assert_eq!(
        error["details"]["pending"],
        serde_json::json!(["Effort", "Priority"])
    );
    let invocations = fs::read_to_string(&fixture.log).expect("read gh invocations");
    assert_eq!(invocations.matches("issue create").count(), 1);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private project diagnostic"));
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
    let output = edited.run_issue(
        &["issue", "edit", "7", "--milestone", "@current"],
        "milestone-nearest",
    );
    success_json(&output);
    let log = fs::read_to_string(&edited.log).expect("read GitHub calls");
    assert!(log.contains("issue edit 7 --repo owner/repo --milestone Sooner"));

    let assigned = Fixture::new();
    let created = assigned.run_issue(
        &[
            "issue",
            "create",
            "--title",
            "Mine",
            "--assignee",
            "@me",
            "--milestone",
            "@current",
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
        &[
            "issue",
            "create",
            "--title",
            "Explicit",
            "--milestone",
            "M1",
        ],
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
        &[
            "issue",
            "create",
            "--title",
            "None eligible",
            "--milestone",
            "@current",
        ],
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
        &[
            "issue",
            "create",
            "--title",
            "Ambiguous",
            "--milestone",
            "@current",
        ],
        "milestone-tie",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    let log = fs::read_to_string(&fixture.log).expect("read GitHub calls");
    assert!(!log.contains("issue create"));
}

// RF-WI.6: Edits apply only explicit typed fields, without creation defaults or model selection.
#[test]
fn project_edit_sets_and_clears_without_issue_or_membership_writes() {
    let fixture = Fixture::new();
    init_configured_project_with_auto_fields(&fixture, r#""Notes":"creation only""#, &["Priority"]);
    let output = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--project-field",
            "Priority=Low",
            "--project-field",
            "Effort=2",
            "--project-field",
            "Start date=2026-10-07",
            "--project-field",
            "Iteration=Sprint A",
            "--clear-project-field",
            "Notes",
        ],
        "",
    );
    assert_eq!(success_json(&output)["number"], 7);
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert!(log.contains("--field-id PRIORITY_ID --single-select-option-id OPTION_LOW"));
    assert!(log.contains("--field-id EFFORT_ID --number 2"));
    assert!(log.contains("--field-id START_ID --date 2026-10-07"));
    assert!(log.contains("--field-id ITERATION_ID --iteration-id ITERATION_A"));
    assert!(log.contains("--field-id NOTES_ID --clear"));
    assert!(!log.contains("issue edit"));
    assert!(!log.contains("item-add"));
    assert!(!log.contains("creation only"));
}

// RF-WI.6: Every requested field validates before either issue or Project mutation.
#[test]
fn project_edit_rejects_invalid_requests_before_writes() {
    for flags in [
        vec!["--project-field", "Priority=Missing"],
        vec!["--clear-project-field", "Missing"],
        vec!["--project-field", "Effort=NaN"],
        vec![
            "--project-field",
            "Priority=High",
            "--clear-project-field",
            "Priority",
        ],
        vec![
            "--project-field",
            "Priority=High",
            "--remove-project",
            "Roadmap",
        ],
        vec!["--project-field", "Priority=High", "--add-project", "Other"],
    ] {
        let fixture = Fixture::new();
        init_configured_project(&fixture, "");
        let mut args = vec!["issue", "edit", "7", "--title", "Must not write"];
        args.extend(flags);
        assert_eq!(
            error_json(&fixture.run_issue(&args, ""))["code"],
            "invalid_input"
        );
        let log = fs::read_to_string(&fixture.log).unwrap_or_default();
        assert!(!log.contains("issue edit"));
        assert!(!log.contains("item-edit"));
        assert!(!log.contains("item-add"));
    }
}

// RF-WI.6 / RF-WI.4: Repository scope and issue guards reject Project mutations.
#[test]
fn project_edit_preserves_scope_and_issue_guard() {
    for (repo, mode, extra, code) in [
        ("other/repo", "", None, "invalid_input"),
        ("owner/repo", "", Some("stale"), "conflict"),
        ("owner/repo", "pull-request", None, "not_issue"),
    ] {
        let fixture = Fixture::new();
        init_configured_project(&fixture, "");
        let mut args = vec![
            "--provider",
            "github",
            "--repo",
            repo,
            "issue",
            "edit",
            "7",
            "--project-field",
            "Priority=High",
        ];
        if let Some(timestamp) = extra {
            args.extend(["--expect-updated-at", timestamp]);
        }
        assert_eq!(error_json(&fixture.run(&args, mode))["code"], code);
        let log = fs::read_to_string(&fixture.log).unwrap_or_default();
        assert!(!log.contains("issue edit"));
        assert!(!log.contains("item-edit"));
    }
}

// RF-WI.6: A successful field followed by failure remains partial success, even without issue edits.
#[test]
fn project_field_only_edit_reports_completed_operations_on_later_failure() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, "");
    let output = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--project-field",
            "Priority=High",
            "--clear-project-field",
            "Notes",
        ],
        "project-second-edit-failure",
    );
    let error = error_json(&output);
    assert_eq!(error["code"], "partial_success");
    assert_eq!(
        error["details"]["resource"]["url"],
        "https://github.com/owner/repo/issues/7"
    );
    assert_eq!(
        error["details"]["completed"],
        serde_json::json!(["Priority"])
    );
    assert_eq!(
        error["details"]["pending"],
        serde_json::json!(["Notes", "issue readback"])
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private project diagnostic"));
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert_eq!(log.matches("project item-edit").count(), 2);
    assert!(!log.contains("issue edit"));
}

// RF-WI.6: Combined writes stop on failure and retain the completed issue edit.
#[test]
fn project_edit_reports_issue_success_and_stops_after_field_failure() {
    for (title, code, completed) in [
        (
            Some("Updated title"),
            "partial_success",
            serde_json::json!(["issue edit"]),
        ),
        (None, "github_cli", serde_json::Value::Null),
    ] {
        let fixture = Fixture::new();
        init_configured_project(&fixture, "");
        let mut args = vec![
            "issue",
            "edit",
            "7",
            "--project-field",
            "Notes=updated",
            "--clear-project-field",
            "Priority",
        ];
        if let Some(title) = title {
            args.extend(["--title", title, "--state", "closed", "--comment", "Done"]);
        }
        let output = fixture.run_issue(&args, "project-edit-failure");
        let error = error_json(&output);
        assert_eq!(error["code"], code);
        if title.is_some() {
            assert_eq!(error["details"]["completed"], completed);
            assert_eq!(
                error["details"]["pending"],
                serde_json::json!([
                    "Notes",
                    "Priority",
                    "issue close",
                    "transition comment",
                    "issue readback"
                ])
            );
        }
        let log = fs::read_to_string(&fixture.log).unwrap();
        assert_eq!(
            log.matches("issue edit").count(),
            usize::from(title.is_some())
        );
        assert_eq!(log.matches("project item-edit").count(), 1);
        assert!(log.contains("--field-id NOTES_ID --text updated"));
        assert!(!log.contains("item-add"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private project diagnostic"));
    }
}

fn related_issue(number: u64, repo: &str, state: &str) -> serde_json::Value {
    serde_json::json!({
        "number": number, "title": format!("Related {number}"),
        "state": state, "url": format!("https://github.com/{repo}/issues/{number}")
    })
}

fn relation_connection(
    nodes: Vec<serde_json::Value>,
    total: usize,
    cursor: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "nodes": nodes, "totalCount": total,
        "pageInfo": {"hasNextPage": cursor.is_some(), "endCursor": cursor}
    })
}

fn empty_relationships() -> serde_json::Value {
    serde_json::json!({
        "issueType": null, "parent": null,
        "subIssues": relation_connection(vec![], 0, None),
        "blockedBy": relation_connection(vec![], 0, None),
        "blocking": relation_connection(vec![], 0, None)
    })
}

fn relationship_response(fixture: &Fixture, file: &str, issue: serde_json::Value) {
    fs::write(
        fixture.root.join(file),
        serde_json::to_vec(&serde_json::json!({"data":{"repository":{"issue":issue}}})).unwrap(),
    )
    .expect("write relationship response");
}

// RF-WI.3: View preserves hierarchy and body without repeating blocker information.
#[test]
fn view_exposes_native_relationship_direction_and_safe_text() {
    let fixture = Fixture::new();
    let mut graph = empty_relationships();
    graph["issueType"] = serde_json::json!({"name":"Bug"});
    graph["parent"] = related_issue(1, "owner/epics", "OPEN");
    graph["subIssues"] =
        relation_connection(vec![related_issue(8, "owner/repo", "CLOSED")], 1, None);
    graph["blockedBy"] =
        relation_connection(vec![related_issue(20, "other/deps", "OPEN")], 1, None);
    graph["blocking"] =
        relation_connection(vec![related_issue(21, "owner/repo", "CLOSED")], 1, None);
    graph["parent"]["title"] = serde_json::json!("Parent\u{1b}[2J");
    relationship_response(&fixture, "relationship-initial.json", graph);
    let view = success_json(&fixture.run_issue(&["issue", "view", "7"], ""));
    assert_eq!(view["body"], "Provider body\n\n## Notes\n\noriginal notes");
    assert_eq!(view["issue_type"], "Bug");
    assert_eq!(
        view["parent"]["url"],
        "https://github.com/owner/epics/issues/1"
    );
    assert_eq!(view["parent"]["state"], "open");
    assert_eq!(view["sub_issues"][0]["number"], 8);
    assert_eq!(view["sub_issues"][0]["state"], "closed");
    assert!(view.get("blocked_by").is_none());
    assert!(view.get("blocking").is_none());

    let text = fixture.run_issue(&["issue", "view", "7", "--format", "text"], "");
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(
        text.contains("Parent: #1 Parent\\u{1b}[2J [open] https://github.com/owner/epics/issues/1")
    );
    assert!(!text.contains("Blocked by:"));
    assert!(!text.contains("Blocking:"));
    assert!(!text.contains('\u{1b}'));

    relationship_response(&fixture, "relationship-initial.json", empty_relationships());
    let empty = success_json(&fixture.run_issue(&["issue", "view", "7"], ""));
    assert!(empty["parent"].is_null());
    assert!(empty["issue_type"].is_null());
    assert_eq!(empty["sub_issues"], serde_json::json!([]));
}

// RF-WI.3: Every connection is paginated rather than silently omitting relationships after page one.
#[test]
fn view_paginates_each_relationship_connection() {
    for (native_field, field) in [("subIssues", "sub_issues")] {
        let fixture = Fixture::new();
        let mut graph = empty_relationships();
        let nodes = (100..200)
            .map(|number| related_issue(number, "owner/repo", "OPEN"))
            .collect();
        graph[native_field] = relation_connection(nodes, 101, Some("page-one"));
        relationship_response(&fixture, "relationship-initial.json", graph);
        relationship_response(
            &fixture,
            "relationship-page.json",
            serde_json::json!({
                "relations": relation_connection(vec![related_issue(200, "other/repo", "CLOSED")], 101, None)
            }),
        );
        let view = success_json(&fixture.run_issue(&["issue", "view", "7"], ""));
        let expected: Vec<_> = (100..200)
            .map(|number| {
                serde_json::json!({
                    "number":number, "title":format!("Related {number}"), "state":"open",
                    "url":format!("https://github.com/owner/repo/issues/{number}")
                })
            })
            .chain(std::iter::once(serde_json::json!({
                "number":200, "title":"Related 200", "state":"closed",
                "url":"https://github.com/other/repo/issues/200"
            })))
            .collect();
        assert_eq!(view[field], serde_json::json!(expected));
    }
}

// RF-WI.3: An unavailable, malformed, or truncated graph cannot masquerade as an empty graph.
#[test]
fn view_rejects_incomplete_relationship_graphs_without_leaking_diagnostics() {
    let fixture = Fixture::new();
    let failure = fixture.run_issue(&["issue", "view", "7"], "relationship-failure");
    assert_eq!(error_json(&failure)["code"], "github_cli");
    assert!(failure.stdout.is_empty());
    assert!(
        !String::from_utf8_lossy(&failure.stderr).contains("private relationship query diagnostic")
    );

    let mut missing = empty_relationships();
    missing.as_object_mut().unwrap().remove("subIssues");
    let mut missing_parent = empty_relationships();
    missing_parent.as_object_mut().unwrap().remove("parent");
    let mut oversized = empty_relationships();
    oversized["subIssues"] = relation_connection(vec![], 1_001, None);
    let mut count_mismatch = empty_relationships();
    count_mismatch["subIssues"] = relation_connection(vec![], 1, None);
    let mut zero_number = empty_relationships();
    zero_number["subIssues"] =
        relation_connection(vec![related_issue(0, "owner/repo", "OPEN")], 1, None);
    let mut unknown_state = empty_relationships();
    unknown_state["subIssues"] =
        relation_connection(vec![related_issue(9, "owner/repo", "UNKNOWN")], 1, None);
    let mut duplicate = empty_relationships();
    duplicate["subIssues"] =
        relation_connection(vec![related_issue(9, "owner/repo", "OPEN"); 2], 2, None);
    let mut missing_cursor = empty_relationships();
    missing_cursor["subIssues"] = serde_json::json!({"nodes":[],"totalCount":1,"pageInfo":{"hasNextPage":true,"endCursor":null}});

    for (graph, expected_code) in [
        (missing, "provider_response"),
        (missing_parent, "provider_response"),
        (oversized, "relationship_limit"),
        (count_mismatch, "provider_response"),
        (zero_number, "provider_response"),
        (unknown_state, "provider_response"),
        (duplicate, "provider_response"),
        (missing_cursor, "provider_response"),
    ] {
        relationship_response(&fixture, "relationship-initial.json", graph);
        let output = fixture.run_issue(&["issue", "view", "7"], "");
        assert_eq!(error_json(&output)["code"], expected_code);
        assert!(output.stdout.is_empty());
    }

    fs::write(
        fixture.root.join("relationship-initial.json"),
        serde_json::to_vec(&serde_json::json!({
            "data":{"repository":{"issue":empty_relationships()}},
            "errors":[{"message":"private GraphQL error"}]
        }))
        .unwrap(),
    )
    .unwrap();
    let error = fixture.run_issue(&["issue", "view", "7"], "");
    assert_eq!(error_json(&error)["code"], "provider_response");
    assert!(!String::from_utf8_lossy(&error.stderr).contains("private GraphQL error"));

    let mut graph = empty_relationships();
    graph["subIssues"] = relation_connection(
        vec![related_issue(10, "owner/repo", "OPEN")],
        3,
        Some("same"),
    );
    relationship_response(&fixture, "relationship-initial.json", graph);
    relationship_response(
        &fixture,
        "relationship-page.json",
        serde_json::json!({
            "relations":relation_connection(vec![related_issue(11, "owner/repo", "OPEN")], 3, Some("same"))
        }),
    );
    let output = fixture.run_issue(&["issue", "view", "7"], "");
    assert_eq!(error_json(&output)["code"], "provider_response");
    assert!(output.stdout.is_empty());
}
// RF-WI.7: blocker chains are complete, compact, ordered, and fail closed.
#[test]
fn blockers_print_open_chains_and_omit_closed_edges() {
    let fixture = Fixture::new();
    let write_graph = |number: u64, nodes: Vec<serde_json::Value>| {
        let count = nodes.len();
        let graph = serde_json::json!({
            "data":{"repository":{"issue":{"relations":{
                "nodes":nodes,
                "totalCount":count,
                "pageInfo":{"hasNextPage":false,"endCursor":null}
            }}}}
        });
        fs::write(
            fixture.root.join(format!("blocker-{number}.json")),
            serde_json::to_vec(&graph).expect("serialize graph"),
        )
        .expect("write graph");
    };
    let related = |number: u64, repo: &str, state: &str| {
        serde_json::json!({
            "number":number,
            "title":format!("Issue {number}"),
            "state":state,
            "url":format!("https://github.com/{repo}/issues/{number}")
        })
    };
    write_graph(
        30,
        vec![
            related(29, "owner/repo", "OPEN"),
            related(17, "platform/api", "OPEN"),
            related(40, "owner/repo", "CLOSED"),
        ],
    );
    write_graph(29, vec![related(18, "owner/repo", "OPEN")]);
    write_graph(18, vec![]);
    write_graph(17, vec![]);

    let output = fixture.run_issue(&["issue", "blockers", "30"], "blocker-graph");
    let chains = success_json(&output);
    assert_eq!(
        chains,
        serde_json::json!(["#18 -> #29 -> #30", "platform/api#17 -> #30"])
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Issue 18"));

    let text = fixture.run_issue(
        &["issue", "blockers", "30", "--format", "text"],
        "blocker-graph",
    );
    assert!(text.status.success());
    assert_eq!(
        String::from_utf8(text.stdout).expect("text chains"),
        "#18 -> #29 -> #30\nplatform/api#17 -> #30\n"
    );
}

#[test]
fn blockers_empty_closed_and_failure_results_are_unambiguous() {
    let fixture = Fixture::new();
    let empty = fixture.run_issue(&["issue", "blockers", "30"], "blocker-empty");
    assert_eq!(success_json(&empty), serde_json::json!([]));
    let empty_text = fixture.run_issue(
        &["issue", "blockers", "30", "--format", "text"],
        "blocker-empty",
    );
    assert!(empty_text.status.success());
    assert!(empty_text.stdout.is_empty());

    fs::write(&fixture.log, "").expect("reset provider log");
    let closed = fixture.run_issue(&["issue", "blockers", "30"], "blocker-closed-target");
    assert_eq!(success_json(&closed), serde_json::json!([]));
    assert!(
        !fs::read_to_string(&fixture.log)
            .expect("read provider log")
            .contains("graphql")
    );

    let failed = fixture.run_issue(&["issue", "blockers", "30"], "blocker-failure");
    assert_eq!(error_json(&failed)["code"], "github_cli");
    assert!(failed.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private blocker query diagnostic"));
}

#[test]
fn blockers_reject_cycles_and_depth_limits_without_partial_output() {
    let fixture = Fixture::new();
    let cycle = fixture.run_issue(&["issue", "blockers", "30"], "blocker-cycle");
    assert_eq!(error_json(&cycle)["code"], "provider_response");
    assert!(cycle.stdout.is_empty());

    let deep = fixture.run_issue(&["issue", "blockers", "125"], "blocker-depth");
    assert_eq!(error_json(&deep)["code"], "relationship_limit");
    assert!(deep.stdout.is_empty());
}

// RF-WI.6: Explicit auto edits preserve fields on uncertainty and never reapply create defaults.
#[test]
fn project_auto_edit_selects_only_requested_fields_and_preserves_uncertainty() {
    for (high, low, expected) in [
        (0.8, 0.2, Some("OPTION_HIGH")),
        (0.79, 0.2, None),
        (0.9, 0.9, None),
    ] {
        let fixture = Fixture::new();
        init_configured_project_with_auto_fields(
            &fixture,
            r#""Priority":"Low","Notes":"creation default""#,
            &["Iteration"],
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind model");
        let address = listener.local_addr().unwrap();
        let model = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("model request");
            let mut reader = std::io::BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
            let prompt = request["messages"][1]["content"].as_str().unwrap();
            assert!(prompt.contains("Final title"));
            assert!(prompt.contains("Added acceptance"));
            assert!(prompt.contains("original notes"));
            assert!(!prompt.contains("Iteration=Sprint A"));
            let suggestions = serde_json::json!({"suggestions":[
                {"label":"Priority=High","probability":high},
                {"label":"Priority=Low","probability":low},
                {"label":"bug","probability":0.9},
                {"label":"docs","probability":0.1}
            ]});
            let response =
                serde_json::json!({"choices":[{"message":{"content":suggestions.to_string()}}]})
                    .to_string();
            write!(reader.into_inner(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        });
        let result = fixture.run_with_model(
            &[
                "--provider",
                "github",
                "--repo",
                "owner/repo",
                "issue",
                "edit",
                "7",
                "--title",
                "Final title",
                "--append-body",
                "Added acceptance",
                "--add-label",
                "@auto",
                "--project-field",
                "Priority=@auto",
            ],
            "",
            "local/test-model",
            Some(&format!("http://{address}/v1")),
        );
        model.join().unwrap();
        success_json(&result);
        let log = fs::read_to_string(&fixture.log).unwrap();
        assert!(log.contains("--add-label bug"));
        assert!(!log.contains("--add-label Priority="));
        assert!(!log.contains("--field-id NOTES_ID"));
        assert!(!log.contains("--field-id ITERATION_ID"));
        assert!(!log.contains("project item-add"));
        match expected {
            Some(option) => assert!(log.contains(&format!(
                "--field-id PRIORITY_ID --single-select-option-id {option}"
            ))),
            None => assert!(!log.contains("project item-edit")),
        }
    }
}

// RF-WI.6: Missing credentials and invalid/unsupported automatic fields never write.
#[test]
fn project_auto_edit_fails_closed_before_writes() {
    let fixture = Fixture::new();
    init_configured_project(&fixture, "");
    let missing = fixture.run_issue(
        &["issue", "edit", "7", "--project-field", "Priority=@auto"],
        "",
    );
    assert_eq!(error_json(&missing)["code"], "decision_authentication");
    assert!(!fixture.log.exists());
    for value in ["Effort=@auto", "Unknown=@auto"] {
        let result = fixture.run_with_model(
            &[
                "--provider",
                "github",
                "--repo",
                "owner/repo",
                "issue",
                "edit",
                "7",
                "--project-field",
                value,
            ],
            "",
            "local/test-model",
            Some("http://127.0.0.1:1/v1"),
        );
        assert_eq!(error_json(&result)["code"], "invalid_input");
        let log = fs::read_to_string(&fixture.log).unwrap();
        assert!(!log.contains("issue edit"));
        assert!(!log.contains("project item-edit"));
    }
}
/// A fixture whose read-backs report the state a transition produced.
///
/// The shared fixture answers every read with a fixed state, so this wrapper keeps a state file,
/// delegates every write to the shared mock unchanged, and answers only the read-back endpoint
/// with a record carrying the tracked state.
fn issue_lifecycle_fixture() -> Fixture {
    let fixture = Fixture::new();
    fs::rename(fixture.bin.join("gh"), fixture.bin.join("gh-shared"))
        .expect("keep the shared gh mock");
    let script = r#"#!/bin/sh
shared="$(dirname "$WORKCTL_GH_LOG")/bin/gh-shared"
state_file="$(dirname "$WORKCTL_GH_LOG")/issue-state"
case "$1 $2" in
    "issue close") printf 'CLOSED' > "$state_file" ;;
    "issue reopen") printf 'OPEN' > "$state_file" ;;
esac
current="$(cat "$state_file" 2>/dev/null || printf 'OPEN')"
if [ "$1" = "api" ]; then
    case "$2" in
        repos/owner/repo/issues/*)
            number="${2##*/}"
            printf '%s\n' "$*" >> "$WORKCTL_GH_LOG"
            printf '%s\n' "{\"number\":$number,\"title\":\"Provider title\",\"body\":\"Provider body\",\"state\":\"$current\",\"html_url\":\"https://github.com/owner/repo/issues/$number\",\"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-02T00:00:00Z\"}"
            exit 0
            ;;
    esac
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

// RF-WI.8-RF-WI.12: Native state transitions, comments, and conversation locks.
#[test]
fn issue_lifecycle_comments_and_conversation_lock_use_gh_native_commands() {
    let fixture = issue_lifecycle_fixture();

    // A state-only edit closes through the native command, comments on stdin, and reads back once.
    fs::write(&fixture.log, "").expect("reset gh argument log");
    let closed = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--state",
            "closed",
            "--comment",
            "Fixed",
            "--reason",
            "completed",
        ],
        "",
    );
    let closed = success_json(&closed);
    assert_eq!(closed["number"], 7);
    assert_eq!(closed["state"], "closed");
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read transition comment"),
        "Fixed"
    );
    assert_eq!(
        gh_lines(&fixture).join("\n"),
        concat!(
            "auth status --hostname github.com\n",
            "api repos/owner/repo/issues/7\n",
            "issue close 7 --repo owner/repo --reason completed\n",
            "issue comment 7 --repo owner/repo --body-file -\n",
            "api repos/owner/repo/issues/7"
        )
    );

    // A duplicate target reaches the native close without a comment.
    fs::write(&fixture.log, "").expect("reset gh argument log");
    let duplicate = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--state",
            "closed",
            "--duplicate-of",
            "https://github.com/owner/repo/issues/9",
        ],
        "",
    );
    assert_eq!(success_json(&duplicate)["state"], "closed");
    assert_eq!(
        gh_lines(&fixture).join("\n"),
        concat!(
            "auth status --hostname github.com\n",
            "api repos/owner/repo/issues/7\n",
            "issue close 7 --repo owner/repo --duplicate-of https://github.com/owner/repo/issues/9\n",
            "api repos/owner/repo/issues/7"
        )
    );

    // Reopening reopens first and posts the stdin comment afterwards.
    fs::write(&fixture.log, "").expect("reset gh argument log");
    let reopened = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--state",
            "open",
            "--comment",
            "Reopened",
        ],
        "",
    );
    assert_eq!(success_json(&reopened)["state"], "open");
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read transition comment"),
        "Reopened"
    );
    assert_eq!(
        gh_lines(&fixture).join("\n"),
        concat!(
            "auth status --hostname github.com\n",
            "api repos/owner/repo/issues/7\n",
            "issue reopen 7 --repo owner/repo\n",
            "issue comment 7 --repo owner/repo --body-file -\n",
            "api repos/owner/repo/issues/7"
        )
    );

    let comment = fixture.run_issue(&["issue", "comment", "7", "--body", "Comment body"], "");
    assert_eq!(success_json(&comment)["target"], "issue");
    assert_eq!(
        fs::read_to_string(&fixture.input).expect("read comment input"),
        "Comment body"
    );

    let locked = fixture.run_issue(&["issue", "lock", "7", "--reason", "resolved"], "");
    assert_eq!(success_json(&locked)["locked"], true);
    let unlocked = fixture.run_issue(&["issue", "lock", "7", "--undo"], "");
    assert_eq!(success_json(&unlocked)["locked"], false);

    let log = fs::read_to_string(&fixture.log).expect("read gh arguments");
    assert!(log.contains("issue comment 7 --repo owner/repo --body-file -"));
    assert!(log.contains("issue lock 7 --repo owner/repo --reason resolved"));
    assert!(log.contains("issue unlock 7 --repo owner/repo"));

    let invalid = fixture.run_issue(&["issue", "comment", "7"], "");
    assert_eq!(error_json(&invalid)["code"], "invalid_input");
    let invalid_lock = fixture.run_issue(&["issue", "lock", "7", "--reason", "unknown"], "");
    assert!(!invalid_lock.status.success());

    // Transition modifiers that need `--state` and conflicting lock flags fail before provider access.
    fs::remove_file(&fixture.log).expect("clear gh argument log");
    let orphan_comment = fixture.run_issue(&["issue", "edit", "7", "--comment", "orphan"], "");
    assert_eq!(error_json(&orphan_comment)["code"], "invalid_input");
    let orphan_reason = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--state",
            "open",
            "--reason",
            "completed",
        ],
        "",
    );
    assert_eq!(error_json(&orphan_reason)["code"], "invalid_input");
    let conflicting_lock = fixture.run_issue(
        &["issue", "lock", "7", "--undo", "--reason", "resolved"],
        "",
    );
    assert_eq!(conflicting_lock.status.code(), Some(2));
    assert!(!fixture.log.exists());
}

/// RF-WI.13: linked branches are created on the remote and reported by name.
#[test]
fn develop_creates_and_lists_linked_branches() {
    let fixture = Fixture::new();
    let created = fixture.run_issue(
        &[
            "issue",
            "develop",
            "7",
            "--name",
            "feature-x",
            "--base",
            "main",
            "--checkout",
        ],
        "",
    );
    let created = success_json(&created);
    assert_eq!(created["number"], 7);
    assert_eq!(created["branch"], "feature-x");

    let listed = fixture.run_issue(&["issue", "develop", "7", "--list"], "");
    let listed = success_json(&listed);
    assert_eq!(listed["number"], 7);
    assert_eq!(listed["branches"][0], "feature-x");
    assert_eq!(listed["branches"][1], "feature-y");

    let text = fixture.run_issue(&["issue", "develop", "7", "--list", "--format", "text"], "");
    assert!(text.status.success());
    assert_eq!(
        String::from_utf8_lossy(&text.stdout),
        "issue #7 linked branches:\nfeature-x\nfeature-y\n"
    );

    let lines = gh_lines(&fixture);
    assert!(
        lines.contains(
            &"issue develop 7 --repo owner/repo --base main --name feature-x --checkout"
                .to_string()
        ),
        "actual gh calls: {lines:?}"
    );
    assert!(lines.contains(&"issue develop 7 --repo owner/repo --list".to_string()));

    let failure = fixture.run_issue(
        &["issue", "develop", "7", "--name", "feature-x"],
        "develop-failure",
    );
    assert_eq!(error_json(&failure)["code"], "github_cli");

    // `--list` reports instead of creating, so the creating flags conflict with it.
    let conflicting = fixture.run_issue(
        &["issue", "develop", "7", "--list", "--name", "feature-x"],
        "",
    );
    assert_eq!(conflicting.status.code(), Some(2));
}

// RF-WI.13: output that carries no branch reference is a provider error, not an invented name.
#[test]
fn develop_reports_unreadable_provider_output() {
    let fixture = Fixture::new();
    let create = fixture.run_issue(
        &["issue", "develop", "7", "--name", "feature-x"],
        "develop-unreadable-create",
    );
    assert_eq!(error_json(&create)["code"], "provider_response");
    assert!(create.stdout.is_empty());

    let list = fixture.run_issue(&["issue", "develop", "7", "--list"], "develop-unreadable-list");
    assert_eq!(error_json(&list)["code"], "provider_response");
    assert!(list.stdout.is_empty());
}

// RF-CFG.8: `defaults.github.listLimit` supplies the listing limit until `--limit` overrides it.
#[test]
fn configured_list_limit_applies_until_the_flag_overrides_it() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"github":{"listLimit":7}}}"#,
    )
    .expect("write list limit config");

    assert_eq!(
        success_json(&fixture.run_issue(&["issue", "list"], ""))
            .as_array()
            .expect("issue list")
            .len(),
        2
    );
    success_json(&fixture.run_issue(&["issue", "list", "--limit", "3"], ""));

    let log = gh_lines(&fixture);
    assert!(
        log.iter().any(|line| line.contains("--limit 7 --json")),
        "{log:?}"
    );
    assert!(
        log.iter().any(|line| line.contains("--limit 3 --json")),
        "{log:?}"
    );
}

// RF-CFG.7: `defaults.output.format` selects the success format until `--format` overrides it.
#[test]
fn configured_output_format_applies_until_the_flag_overrides_it() {
    let fixture = Fixture::new();
    fixture.init_git();
    fs::write(
        fixture.root.join(".workctl.json"),
        r#"{"defaults":{"output":{"format":"text"}}}"#,
    )
    .expect("write output format config");

    let text = fixture.run_issue(&["issue", "view", "7"], "");
    assert!(
        text.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&text.stderr)
    );
    let text = String::from_utf8(text.stdout).expect("text output is UTF-8");
    assert!(text.starts_with("#7 Provider title [open]\n"), "{text:?}");

    let json = fixture.run_issue(&["issue", "view", "7", "--format", "json"], "");
    assert_eq!(success_json(&json)["url"], "https://github.com/owner/repo/issues/7");
}
