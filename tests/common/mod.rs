//! Shared harness for the CLI integration tests.
//!
//! Each test binary compiles this module separately, so helpers one binary does not use are
//! allowed to sit idle rather than being duplicated per binary.

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub log: PathBuf,
    pub input: PathBuf,
    pub glab_log: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "workctl-cli-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create isolated fixture directory");
        let log = root.join("gh-args.log");
        let input = root.join("gh-input.json");
        let glab_log = root.join("glab-args.log");
        let script = r#"#!/bin/sh
printf '%s\n' "$*" >> "$WORKCTL_GH_LOG"
if [ "$1" = "--version" ]; then
    if [ "$WORKCTL_GH_MODE" = "old-version" ]; then
        printf '%s\n' 'gh version 2.98.0 (2026-08-20)'
    else
        printf '%s\n' 'gh version 2.99.0 (2026-09-01)'
    fi
    exit 0
fi
if [ "$1" = "auth" ]; then
    if [ "$WORKCTL_GH_MODE" = "auth-failure" ]; then
        printf '%s\n' 'provider diagnostic withheld' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "list" ]; then
    printf '%s\n' '[{"number":8,"title":"First","state":"OPEN","url":"https://github.com/owner/repo/issues/8","updatedAt":"2026-01-01T00:00:00Z"},{"number":9,"title":"Second","state":"CLOSED","url":"https://github.com/owner/repo/issues/9","updatedAt":"2026-01-02T00:00:00Z"}]'
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "create" ]; then
    cat > "$WORKCTL_GH_INPUT"
    printf '%s\n' 'https://github.com/owner/repo/issues/7'
    if [ "$WORKCTL_GH_MODE" = "attachment-create-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "edit" ]; then
    cat > "$WORKCTL_GH_INPUT"
    exit 0
fi
if [ "$1" = "api" ] && [ "$2" = "--paginate" ]; then
        case "$WORKCTL_GH_MODE" in
            milestone-nearest)
                printf '%s\n' '[[{"title":"Overdue","due_on":"2000-01-01T00:00:00Z"},{"title":"Later","due_on":"2099-03-01T00:00:00Z"},{"title":"Sooner","due_on":"2099-02-01T00:00:00Z"},{"title":"Undated","due_on":null}]]'
                ;;
            milestone-tie)
                printf '%s\n' '[[{"title":"First","due_on":"2099-02-01T00:00:00Z"},{"title":"Second","due_on":"2099-02-01T00:00:00Z"}]]'
                ;;
            *)
                printf '%s\n' '[[]]'
                ;;
        esac
        exit 0
    fi
if [ "$1" = "api" ]; then
    if [ "$WORKCTL_GH_MODE" = "early-exit" ]; then
        printf '%s\n' '{"number":7,"title":"Provider title","body":"Provider body\n\n## Notes\n\noriginal notes","state":"OPEN","html_url":"https://github.com/owner/repo/issues/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}'
        exit 0
    fi
    if [ "$2" = "--method" ]; then
        cat > "$WORKCTL_GH_INPUT"
    fi
    if [ "$WORKCTL_GH_MODE" = "pull-request" ]; then
        printf '%s\n' '{"number":7,"title":"Not an issue","body":null,"state":"OPEN","html_url":"https://github.com/owner/repo/pull/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z","pull_request":{}}'
    else
        printf '%s\n' '{"number":7,"title":"Provider title","body":"Provider body\n\n## Notes\n\noriginal notes","state":"OPEN","html_url":"https://github.com/owner/repo/issues/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}'
    fi
    exit 0
fi
if [ "$1" = "label" ] && [ "$2" = "list" ]; then
    printf '%s\n' '[{"name":"bug","description":"Broken behavior"},{"name":"docs","description":null}]'
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    printf '%s\n' '[{"number":42,"title":"Open change","state":"OPEN","isDraft":false,"url":"https://github.com/owner/repo/pull/42","baseRefName":"main","headRefName":"feature-x","updatedAt":"2026-01-02T00:00:00Z"},{"number":43,"title":"Draft change","state":"OPEN","isDraft":true,"url":"https://github.com/owner/repo/pull/43","baseRefName":"main","headRefName":"draft-y","updatedAt":"2026-01-03T00:00:00Z"},{"number":44,"title":"Merged change","state":"MERGED","isDraft":false,"url":"https://github.com/owner/repo/pull/44","baseRefName":"main","headRefName":"merged-z","updatedAt":"2026-01-04T00:00:00Z"}]'
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    if [ "$WORKCTL_GH_MODE" = "pr-view-failure" ]; then
        printf '%s\n' 'no pull requests found for branch' >&2
        exit 1
    fi
    printf '%s\n' '{"number":42,"title":"Add pull requests","body":"PR body","state":"OPEN","isDraft":false,"url":"https://github.com/owner/repo/pull/42","baseRefName":"main","headRefName":"feature-x","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"","mergeable":"MERGEABLE","reviewDecision":"","labels":[{"name":"bug"}],"assignees":[{"login":"hubot"}]}'
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    cat > "$WORKCTL_GH_INPUT"
    printf '%s\n' 'https://github.com/owner/repo/pull/42'
    if [ "$WORKCTL_GH_MODE" = "attachment-create-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "edit" ]; then
    cat > "$WORKCTL_GH_INPUT"
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "diff" ]; then
    printf '%s\n' 'diff --git a/src/lib.rs b/src/lib.rs'
    printf '%s\n' 'index 1111111..2222222 100644'
    printf '%s\n' '--- a/src/lib.rs'
    printf '%s\n' '+++ b/src/lib.rs'
    printf '%s\n' '@@ -1,1 +1,1 @@'
    printf '%s\n' '-old line'
    printf '%s\n' '+new line'
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "checks" ]; then
    if [ "$WORKCTL_GH_MODE" = "checks-none" ]; then
        printf '%s\n' 'no checks reported' >&2
        exit 1
    fi
    printf '%s\n' '[{"name":"build","state":"SUCCESS","bucket":"pass","description":null,"link":"https://example.test/build","workflow":"CI"},{"name":"lint","state":"FAILURE","bucket":"fail","description":"style violations","link":null,"workflow":"CI"}]'
    if [ "$WORKCTL_GH_MODE" = "checks-failing" ]; then
        exit 1
    fi
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "review" ]; then
    cat > "$WORKCTL_GH_INPUT"
    exit 0
fi
if [ "$1" = "pr" ]; then
    case "$2" in
        merge|ready|close|reopen)
            exit 0
            ;;
    esac
fi
printf '%s\n' 'unexpected fixture invocation' >&2
exit 64
"#;
        let script_path = bin.join("gh");
        fs::write(&script_path, script).expect("write gh fixture");
        let mut permissions = fs::metadata(&script_path)
            .expect("read fixture permissions")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("make fixture executable");

        let glab_script = r#"#!/bin/sh
printf '%s\n' "$*" >> "$WORKCTL_GLAB_LOG"
if [ "$1" = "auth" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "auth-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "list" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "list-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    printf '%s\n' '[{"iid":8,"title":"First","description":"first body","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/8","created_at":"2026-01-01T00:00:00.000Z","updated_at":"2026-01-02T00:00:00.000Z"},{"iid":9,"title":"Second","description":null,"state":"closed","web_url":"https://gitlab.com/group/sub/project/-/issues/9","created_at":"2026-01-03T00:00:00.000Z","updated_at":"2026-01-04T00:00:00.000Z"}]'
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "view" ]; then
    printf '%s\n' '{"iid":12,"title":"Crash on startup","description":"Steps to reproduce","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/12","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-03T04:05:06.000Z"}'
    exit 0
fi
printf '%s\n' 'unexpected fixture invocation' >&2
exit 64
"#;
        let glab_path = bin.join("glab");
        fs::write(&glab_path, glab_script).expect("write glab fixture");
        let mut permissions = fs::metadata(&glab_path)
            .expect("read fixture permissions")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&glab_path, permissions).expect("make fixture executable");
        Self {
            root,
            bin,
            log,
            input,
            glab_log,
        }
    }

    pub fn run(&self, args: &[&str], mode: &str) -> Output {
        self.run_with_model(args, mode, "jev-latest", None)
    }

    pub fn run_with_model(
        &self,
        args: &[&str],
        mode: &str,
        model: &str,
        base_url: Option<&str>,
    ) -> Output {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).expect("compose isolated PATH");
        let mut command = Command::new(env!("CARGO_BIN_EXE_workctl"));
        command
            .args(args)
            .current_dir(&self.root)
            .env("PATH", path)
            .env("WORKCTL_GH_LOG", &self.log)
            .env("WORKCTL_GH_INPUT", &self.input)
            .env("WORKCTL_GH_MODE", mode)
            .env("WORKCTL_GLAB_LOG", &self.glab_log)
            .env("WORKCTL_GLAB_MODE", mode)
            .env("DECISION_MODEL", model)
            .env_remove("DECISION_MODEL_API_KEY");
        if let Some(base_url) = base_url {
            command.env("DECISION_MODEL_BASE_URL", base_url);
        } else {
            command.env_remove("DECISION_MODEL_BASE_URL");
        }
        command.output().expect("run workctl binary")
    }

    pub fn run_issue(&self, args: &[&str], mode: &str) -> Output {
        let mut full_args = vec!["--provider", "github", "--repo", "owner/repo"];
        full_args.extend_from_slice(args);
        self.run(&full_args, mode)
    }

    pub fn run_pr(&self, args: &[&str], mode: &str) -> Output {
        let mut full_args = vec!["--provider", "github", "--repo", "owner/repo"];
        full_args.extend_from_slice(args);
        self.run(&full_args, mode)
    }

    /// Runs against a GitLab project with a nested group path, so the project argument and the
    /// repository shape are both exercised.
    pub fn run_gitlab(&self, args: &[&str], mode: &str) -> Output {
        let mut full_args = vec!["--provider", "gitlab", "--repo", "group/sub/project"];
        full_args.extend_from_slice(args);
        self.run(&full_args, mode)
    }

    /// Every `glab` invocation, in order, as the fixture received it.
    pub fn glab_invocations(&self) -> Vec<String> {
        fs::read_to_string(&self.glab_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn success_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("valid JSON success output")
}

pub fn error_json(output: &Output) -> Value {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    serde_json::from_slice(&output.stderr).expect("valid JSON error output")
}
