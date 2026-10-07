#![cfg(unix)]

mod common;

use common::{Fixture, error_json, success_json};
use std::fs;
use std::os::unix::fs::PermissionsExt;

// Stateful provider fixture: writes change records that subsequent reads and assertions observe.
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    let script = r#"#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
root = pathlib.Path(os.environ['WORKCTL_GH_LOG']).parent
with open(os.environ['WORKCTL_GH_LOG'], 'a') as log:
    log.write(' '.join(args) + '\n')
mode = os.environ.get('WORKCTL_GH_MODE', '')
if args[0] in ('--version', 'auth'):
    print('gh version 2.102.0')
    sys.exit(0)
if args[:2] == ['issue', 'edit']:
    number = int(args[2])
    if mode == 'second-target-failure' and number == 8:
        print('private diagnostic withheld', file=sys.stderr)
        sys.exit(1)
    path = root / ('state-' + str(number) + '.json')
    state = json.loads(path.read_text()) if path.exists() else {'title':'Original', 'body':'Original body', 'parent':'200', 'type':'Task', 'sub-issue':['201'], 'blocked-by':['202'], 'blocking':['203'], 'milestone':'M1', 'label':['old'], 'assignee':[]}
    rest = iter(args[3:])
    for flag in rest:
        if flag.startswith('--remove-') and flag in ('--remove-parent', '--remove-type', '--remove-milestone'):
            state[flag.removeprefix('--remove-')] = None
            continue
        value = next(rest)
        if flag == '--repo':
            continue
        if flag == '--body-file':
            state['body'] = sys.stdin.read() if value == '-' else pathlib.Path(value).read_text()
        elif flag.startswith('--add-'):
            key = flag.removeprefix('--add-')
            values = state.setdefault(key, [])
            if value not in values:
                values.append(value)
        elif flag.startswith('--remove-'):
            key = flag.removeprefix('--remove-')
            state[key] = [entry for entry in state.get(key, []) if entry != value]
        else:
            state[flag.removeprefix('--')] = value
    path.write_text(json.dumps(state))
    print('https://github.com/owner/repo/issues/' + str(number))
    sys.exit(0)
if args[0] == 'api':
    endpoint = args[1].split('/')
    number = int(endpoint[-1])
    path = root / ('state-' + str(number) + '.json')
    if mode == 'readback-failure' and path.exists():
        print('private diagnostic withheld', file=sys.stderr)
        sys.exit(1)
    state = json.loads(path.read_text()) if path.exists() else {'title':'Original', 'body':'Original body'}
    print(json.dumps({'number':number,'title':state['title'],'body':state['body'],'state':'open','html_url':'https://github.com/' + '/'.join(endpoint[1:3]) + '/issues/' + str(number),'created_at':'2026-01-01T00:00:00Z','updated_at':'2026-01-02T00:00:00Z'}))
    sys.exit(0)
print('unexpected provider operation', file=sys.stderr)
sys.exit(64)
"#;
    let gh = fixture.bin.join("gh");
    fs::write(&gh, script).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    fixture
}

fn state(fixture: &Fixture, number: u64) -> serde_json::Value {
    serde_json::from_slice(&fs::read(fixture.root.join(format!("state-{number}.json"))).unwrap())
        .unwrap()
}

// RF-WI.4: Native flags mutate parent, sub-issues, blocking direction, and type without body loss.
#[test]
fn native_relationships_and_type_change_existing_issue_state() {
    let fixture = fixture();
    let output = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--parent",
            "100",
            "--type",
            "Bug",
            "--add-sub-issue",
            "101,102",
            "--remove-sub-issue",
            "201",
            "--add-blocked-by",
            "300",
            "--remove-blocked-by",
            "202",
            "--add-blocking",
            "https://github.com/other/repo/issues/400",
            "--remove-blocking",
            "203",
        ],
        "",
    );
    assert_eq!(success_json(&output)["body"], "Original body");
    let saved = state(&fixture, 7);
    assert_eq!(saved["parent"], "100");
    assert_eq!(saved["type"], "Bug");
    assert_eq!(saved["sub-issue"], serde_json::json!(["101", "102"]));
    assert_eq!(saved["blocked-by"], serde_json::json!(["300"]));
    assert_eq!(
        saved["blocking"],
        serde_json::json!(["https://github.com/other/repo/issues/400"])
    );
    let removed = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "--remove-parent",
            "--remove-type",
            "--remove-milestone",
        ],
        "",
    );
    success_json(&removed);
    let saved = state(&fixture, 7);
    assert!(saved["parent"].is_null());
    assert!(saved["type"].is_null());
    assert!(saved["milestone"].is_null());
    assert_eq!(saved["blocked-by"], serde_json::json!(["300"]));
}

// RF-WI.4: Short flags, same-repository URLs, and CSV metadata preserve native edit semantics.
#[test]
fn native_short_flags_and_url_targets_preserve_body_and_metadata() {
    let fixture = fixture();
    let output = fixture.run(
        &[
            "-R",
            "owner/repo",
            "issue",
            "edit",
            "https://github.com/owner/repo/issues/7",
            "-t",
            "Updated",
            "-b",
            "New body",
            "-m",
            "M2",
            "--add-label",
            "bug,triaged",
            "--add-assignee",
            "@me",
        ],
        "",
    );
    let result = success_json(&output);
    assert_eq!(result["title"], "Updated");
    assert_eq!(result["body"], "New body");
    let saved = state(&fixture, 7);
    assert_eq!(saved["milestone"], "M2");
    assert_eq!(saved["label"], serde_json::json!(["old", "bug", "triaged"]));
    assert_eq!(saved["assignee"], serde_json::json!(["@me"]));
    let body = fixture.root.join("body.md");
    fs::write(&body, "File body").unwrap();
    let output = fixture.run_issue(&["issue", "edit", "7", "-F", body.to_str().unwrap()], "");
    assert_eq!(success_json(&output)["body"], "File body");
}

// RF-WI.4: Batch edits return full records and stop after a failed target without retries.
#[test]
fn batch_edits_return_records_and_report_completed_targets_on_failure() {
    let fixture = fixture();
    let output = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "https://github.com/owner/repo/issues/8",
            "-t",
            "Batch title",
        ],
        "",
    );
    let result = success_json(&output);
    assert_eq!(result[0]["number"], 7);
    assert_eq!(result[1]["number"], 8);
    assert_eq!(result[0]["title"], "Batch title");
    assert_eq!(result[1]["body"], "Original body");
    let failed = fixture.run_issue(
        &["issue", "edit", "7", "8", "9", "--parent", "100"],
        "second-target-failure",
    );
    let error = error_json(&failed);
    assert_eq!(error["code"], "partial_success");
    assert_eq!(
        error["details"]["completed"],
        serde_json::json!(["https://github.com/owner/repo/issues/7"])
    );
    assert_eq!(
        error["details"]["pending"],
        serde_json::json!([
            "https://github.com/owner/repo/issues/8",
            "https://github.com/owner/repo/issues/9"
        ])
    );
    assert_eq!(state(&fixture, 7)["parent"], "100");
    assert!(!fixture.root.join("state-9.json").exists());
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert_eq!(log.matches("issue edit 8").count(), 2); // One earlier success and one failed request.
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private diagnostic"));
}

// RF-WI.4: Invalid references, incompatible changes, and stale guards cannot mutate relationships.
#[test]
fn native_edit_validation_and_guards_fail_before_writes() {
    for flags in [
        vec!["--parent", "100", "--remove-parent"],
        vec!["--type", "Bug", "--remove-type"],
        vec!["--parent", "https://evil.example/owner/repo/issues/100"],
        vec!["--parent", "https://github.com/owner/repo/pull/100"],
        vec!["--parent", "0"],
        vec![
            "--add-blocked-by",
            "100",
            "--remove-blocked-by",
            "https://github.com/owner/repo/issues/100",
        ],
        vec!["--add-sub-issue", "100", "--remove-sub-issue", "100"],
        vec!["--parent", "100", "--expect-updated-at", "stale"],
        vec!["--clear-milestone"],
    ] {
        let fixture = fixture();
        let mut args = vec!["issue", "edit", "7"];
        args.extend(flags);
        let output = fixture.run_issue(&args, "");
        assert!(!output.status.success());
        assert!(!fixture.root.join("state-7.json").exists());
    }
    let fixture = fixture();
    let output = fixture.run_issue(
        &[
            "issue",
            "edit",
            "7",
            "https://github.com/other/repo/issues/8",
            "--parent",
            "100",
        ],
        "",
    );
    assert_eq!(error_json(&output)["code"], "invalid_input");
    assert!(!fixture.log.exists());
}

// RF-WI.4: A successful native mutation followed by failed confirmation remains partial success.
#[test]
fn native_edit_readback_failure_preserves_completed_mutation() {
    let fixture = fixture();
    let output = fixture.run_issue(
        &["issue", "edit", "7", "--parent", "100"],
        "readback-failure",
    );
    let error = error_json(&output);
    assert_eq!(error["code"], "partial_success");
    assert_eq!(
        error["details"]["completed"],
        serde_json::json!(["issue edit"])
    );
    assert_eq!(
        error["details"]["pending"],
        serde_json::json!(["issue readback"])
    );
    assert_eq!(state(&fixture, 7)["parent"], "100");
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert_eq!(log.matches("issue edit 7").count(), 1);
}
