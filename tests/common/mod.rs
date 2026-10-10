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
    pub glab_input: PathBuf,
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
        let glab_input = root.join("glab-input.txt");
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
    if [ "$WORKCTL_GH_MODE" = "project-scope-outside" ]; then
        printf '%s\n' 'https://github.com/other/repo/issues/7'
    else
        printf '%s\n' 'https://github.com/owner/repo/issues/7'
    fi
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
if [ "$1" = "project" ] && [ "$2" = "item-add" ]; then
    if [ "$WORKCTL_GH_MODE" = "project-add-failure" ]; then
        printf '%s\n' 'private project diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "item-edit" ]; then
    if [ "$WORKCTL_GH_MODE" = "project-second-edit-failure" ] && [ "$8" = "NOTES_ID" ]; then
        printf '%s\n' 'private project diagnostic' >&2
        exit 1
    fi
    if [ "$WORKCTL_GH_MODE" = "project-edit-failure" ]; then
        printf '%s\n' 'private project diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "api" ] && [ "$2" = "graphql" ]; then
    case "$WORKCTL_GH_MODE" in
        blocker-failure)
            printf '%s\n' 'private blocker query diagnostic' >&2
            exit 1
            ;;
        blocker-graph|blocker-cycle|blocker-depth)
            issue_number=""
            for argument in "$@"; do
                case "$argument" in
                    number=*) issue_number="${argument#number=}" ;;
                esac
            done
            response="$(dirname "$WORKCTL_GH_LOG")/blocker-${issue_number}.json"
            if [ -f "$response" ]; then
                cat "$response"
            elif [ "$WORKCTL_GH_MODE" = "blocker-cycle" ] && [ "$issue_number" = "30" ]; then
                printf '%s\n' '{"data":{"repository":{"issue":{"relations":{"nodes":[{"number":18,"title":"Related 18","state":"OPEN","url":"https://github.com/owner/repo/issues/18"}],"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            elif [ "$WORKCTL_GH_MODE" = "blocker-cycle" ] && [ "$issue_number" = "18" ]; then
                printf '%s\n' '{"data":{"repository":{"issue":{"relations":{"nodes":[{"number":30,"title":"Related 30","state":"OPEN","url":"https://github.com/owner/repo/issues/30"}],"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            elif [ "$WORKCTL_GH_MODE" = "blocker-depth" ] && [ "$issue_number" -gt 100 ]; then
                next_number=$((issue_number - 1))
                printf '%s\n' "{\"data\":{\"repository\":{\"issue\":{\"relations\":{\"nodes\":[{\"number\":$next_number,\"title\":\"Related $next_number\",\"state\":\"OPEN\",\"url\":\"https://github.com/owner/repo/issues/$next_number\"}],\"totalCount\":1,\"pageInfo\":{\"hasNextPage\":false,\"endCursor\":null}}}}}}"
            else
                printf '%s\n' '{"data":{"repository":{"issue":{"relations":{"nodes":[],"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            fi
            exit 0
            ;;
    esac
    if [ "$WORKCTL_GH_MODE" = "blocker-empty" ]; then
        printf '%s\n' '{"data":{"repository":{"issue":{"relations":{"nodes":[],"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
        exit 0
    fi
    case "$*" in
        *issueType*|*relations:*)
            if [ "$WORKCTL_GH_MODE" = "relationship-failure" ]; then
                printf '%s\n' 'private relationship query diagnostic' >&2
                exit 1
            fi
            case "$*" in
                *relations:*) response="$(dirname "$WORKCTL_GH_LOG")/relationship-page.json" ;;
                *) response="$(dirname "$WORKCTL_GH_LOG")/relationship-initial.json" ;;
            esac
            if [ -f "$response" ]; then
                cat "$response"
            else
                printf '%s\n' '{"data":{"repository":{"issue":{"issueType":null,"parent":null,"subIssues":{"nodes":[],"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null}},"blockedBy":{"nodes":[],"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null}},"blocking":{"nodes":[],"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            fi
            exit 0
            ;;
    esac
    printf '%s\n' '{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_owner_project","title":"Roadmap","fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","id":"PRIORITY_ID","name":"Priority","dataType":"SINGLE_SELECT","options":[{"id":"OPTION_HIGH","name":"High"},{"id":"OPTION_LOW","name":"Low"}]},{"__typename":"ProjectV2Field","id":"EFFORT_ID","name":"Effort","dataType":"NUMBER"},{"__typename":"ProjectV2Field","id":"START_ID","name":"Start date","dataType":"DATE"},{"__typename":"ProjectV2Field","id":"NOTES_ID","name":"Notes","dataType":"TEXT"},{"__typename":"ProjectV2IterationField","id":"ITERATION_ID","name":"Iteration","dataType":"ITERATION","configuration":{"iterations":[{"id":"ITERATION_A","title":"Sprint A"}]}}],"pageInfo":{"hasNextPage":false}}}}}}'
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
    if [ "$2" = "repos/owner/repo/pulls/42" ]; then
        case "$WORKCTL_GH_MODE" in
            pr-fork-head)
                printf '%s\n' '{"head":{"ref":"fork-branch","repo":{"full_name":"someone/workctl"}},"base":{"ref":"main","repo":{"full_name":"owner/repo"}}}'
                ;;
            pr-head-repo-gone)
                printf '%s\n' '{"head":{"ref":"feature-x","repo":null},"base":{"ref":"main","repo":{"full_name":"owner/repo"}}}'
                ;;
            *)
                printf '%s\n' '{"head":{"ref":"feature-x","repo":{"full_name":"owner/repo"}},"base":{"ref":"main","repo":{"full_name":"owner/repo"}}}'
                ;;
        esac
        exit 0
    fi
    if [ "$2" = "--method" ]; then
        cat > "$WORKCTL_GH_INPUT"
        case "$*" in
            *git/refs/heads/*)
                if [ "$WORKCTL_GH_MODE" = "branch-delete-missing" ]; then
                    printf '%s\n' 'gh: Reference does not exist (HTTP 422)' >&2
                    exit 1
                fi
                if [ "$WORKCTL_GH_MODE" = "branch-delete-failure" ]; then
                    printf '%s\n' 'private provider diagnostic' >&2
                    exit 1
                fi
                exit 0
                ;;
            *pulls/42/reviews*)
                if [ "$WORKCTL_GH_MODE" = "review-failure" ]; then
                    printf '%s\n' 'private review diagnostic' >&2
                    exit 1
                fi
                if [ "$WORKCTL_GH_MODE" = "review-unconfirmed" ]; then
                    printf '%s\n' '{}'
                    exit 0
                fi
                if [ "$WORKCTL_GH_MODE" = "review-pending" ]; then
                    printf '%s\n' '{"id":4242,"state":"PENDING"}'
                    exit 0
                fi
                printf '%s\n' '{"id":4242,"state":"APPROVED","commit_id":"9f8e7d6c5b4a3210cafebabe00112233445566"}'
                exit 0
                ;;
        esac
    fi
    if [ "$WORKCTL_GH_MODE" = "pull-request" ]; then
        printf '%s\n' '{"number":7,"title":"Not an issue","body":null,"state":"OPEN","html_url":"https://github.com/owner/repo/pull/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z","pull_request":{}}'
    elif [ "$WORKCTL_GH_MODE" = "blocker-closed-target" ]; then
        printf '%s\n' '{"number":7,"title":"Provider title","body":"Provider body","state":"CLOSED","html_url":"https://github.com/owner/repo/issues/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}'
    else
        printf '%s\n' '{"number":7,"title":"Provider title","body":"Provider body\n\n## Notes\n\noriginal notes","state":"OPEN","html_url":"https://github.com/owner/repo/issues/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}'
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "develop" ]; then
    if [ "$WORKCTL_GH_MODE" = "develop-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    if [ "$WORKCTL_GH_MODE" = "develop-unreadable-create" ]; then
        printf '%s\n' 'created branch feature-x'
        exit 0
    fi
    case "$*" in
        *"--list"*)
            if [ "$WORKCTL_GH_MODE" = "develop-unreadable-list" ]; then
                printf 'feat\377x\turl\n'
                exit 0
            fi
            printf '%s\n' 'feature-x	https://github.com/owner/repo/tree/feature-x'
            printf '%s\n' 'feature-y	https://github.com/owner/repo/tree/feature-y'
            exit 0
            ;;
    esac
    printf '%s\n' 'github.com/owner/repo/tree/feature-x'
    exit 0
fi
if [ "$1" = "issue" ]; then
    case "$2" in
        close|reopen|comment|lock|unlock)
            if [ "$2" = "comment" ]; then
                cat > "$WORKCTL_GH_INPUT"
            fi
            exit 0
            ;;
    esac
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
    case "$WORKCTL_GH_MODE" in
        review-inline|review-failure|review-unconfirmed|review-pending)
            printf '%s\n' '{"headRefOid":"9f8e7d6c5b4a3210cafebabe00112233445566"}'
            exit 0
            ;;
        review-head-missing)
            printf '%s\n' '{}'
            exit 0
            ;;
    esac
    if [ "$WORKCTL_GH_MODE" = "pr-view-failure" ]; then
        printf '%s\n' 'no pull requests found for branch' >&2
        exit 1
    fi
    if [ "$WORKCTL_GH_MODE" = "pr-merged" ]; then
        printf '%s\n' '{"number":42,"title":"Merged change","body":"PR body","state":"MERGED","isDraft":false,"url":"https://github.com/owner/repo/pull/42","baseRefName":"main","headRefName":"feature-x","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"2026-01-02T00:00:00Z","mergeable":"UNKNOWN","reviewDecision":"","labels":[],"assignees":[]}'
        exit 0
    fi
    if [ "$WORKCTL_GH_MODE" = "pr-body-closes" ]; then
        printf '%s\n' '{"number":42,"title":"Add pull requests","body":"PR body\n\nCloses #7\nFixes other/repo#6\nSee #9 for context","state":"OPEN","isDraft":false,"url":"https://github.com/owner/repo/pull/42","baseRefName":"main","headRefName":"feature-x","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"","mergeable":"MERGEABLE","reviewDecision":"","labels":[],"assignees":[]}'
        exit 0
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
    if [ "$WORKCTL_GH_MODE" = "checks-watch-timeout" ]; then
        exec sleep 2
    fi
    if [ "$WORKCTL_GH_MODE" = "checks-watch-pending" ]; then
        printf '%s\n' '[{"name":"integration","state":"PENDING","bucket":"pending","description":"queued","link":null,"workflow":"CI"}]'
        exit 8
    fi
    if [ "$WORKCTL_GH_MODE" = "checks-none" ]; then
        printf '%s\n' 'no checks reported' >&2
        exit 1
    fi
    printf '%s\n' '[{"name":"build","state":"SUCCESS","bucket":"pass","description":null,"link":"https://example.test/build","workflow":"CI"},{"name":"lint","state":"FAILURE","bucket":"fail","description":"style violations","link":null,"workflow":"CI"}]'
    if [ "$WORKCTL_GH_MODE" = "checks-failing" ] || [ "$WORKCTL_GH_MODE" = "checks-watch-fail-fast" ]; then
        exit 1
    fi
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "review" ]; then
    cat > "$WORKCTL_GH_INPUT"
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "checkout" ]; then
    if [ "$WORKCTL_GH_MODE" = "pr-checkout-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    if [ "$WORKCTL_GH_MODE" = "pr-checkout-git" ]; then
        git -C "$WORKCTL_GH_CWD_ROOT" checkout -b "pr-$3" >/dev/null 2>&1
        exit $?
    fi
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "update-branch" ]; then
    if [ "$WORKCTL_GH_MODE" = "pr-update-branch-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "pr" ]; then
    case "$2" in
        merge|ready|close|reopen|lock|unlock)
            exit 0
            ;;
        comment)
            case "$*" in
                *"--body-file -"*) cat > "$WORKCTL_GH_INPUT" ;;
            esac
            exit 0
            ;;
        revert)
            case "$*" in
                *"--body-file -"*) cat > "$WORKCTL_GH_INPUT" ;;
            esac
            printf '%s\n' 'https://github.com/owner/repo/pull/90'
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
if [ "$1" = "label" ] && [ "$2" = "list" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "label-malformed" ]; then
        printf '%s\n' '[{"name":""}]'
    elif [ "$WORKCTL_GLAB_MODE" = "label-overflow" ]; then
        printf '['
        index=0
        while [ "$index" -lt 1001 ]; do
            if [ "$index" -gt 0 ]; then
                printf ','
            fi
            printf '{"name":"generated-%s"}' "$index"
            index=$((index + 1))
        done
        printf ']\n'
    else
        case " $* " in
            *" --page 1 "*)
                printf '%s\n' '[{"name":"bug","description":"Broken behavior"},{"name":"docs","description":"Documentation"}]'
                ;;
            *)
                printf '%s\n' '[]'
                ;;
        esac
    fi
    exit 0
fi
if [ "$1" = "api" ] && [ "$2" = "--method" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "description-write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    cat > "$WORKCTL_GLAB_INPUT"
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "diff" ]; then
    printf '%s\n' 'diff --git a/file b/file' '--- a/file' '+++ b/file' '@@ -1 +1 @@' '-old' '+new'
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "create" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    printf '%s\n' 'https://gitlab.com/group/sub/project/-/merge_requests/6'
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ] && [ "$3" = "create" ]; then
    cat > "$WORKCTL_GLAB_INPUT"
    if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ] && [ "$3" = "list" ]; then
    printf '%s\n' '[{"id":"abcdef123456","notes":[{"body":"Review note"}]}]'
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ] && [ "$3" = "update" ]; then
    cat > "$WORKCTL_GLAB_INPUT"
    if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ]; then
    case "$3" in
        resolve|reopen)
            if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
                printf '%s\n' 'private provider diagnostic' >&2
                exit 1
            fi
            exit 0
            ;;
    esac
fi
if [ "$1" = "mr" ]; then
    case "$2" in
        close|reopen|approve|revoke|rebase|subscribe|unsubscribe|todo|merge|update)
            if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
                printf '%s\n' 'private provider diagnostic' >&2
                exit 1
            fi
            exit 0
            ;;
        checkout)
            if [ "$WORKCTL_GLAB_MODE" = "mr-checkout-git" ]; then
                git -C "$WORKCTL_GH_CWD_ROOT" checkout -b "mr-$3" >/dev/null 2>&1
                exit $?
            fi
            exit 0
            ;;
    esac
fi
if [ "$1" = "mr" ] && { [ "$2" = "approvers" ] || [ "$2" = "issues" ]; }; then
    printf '%s\n' '[{"username":"reviewer"}]'
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "list" ]; then
    printf '%s\n' '[{"iid":5,"title":"Native merge request","description":"must not appear in list","state":"opened","draft":false,"web_url":"https://gitlab.com/group/sub/project/-/merge_requests/5","target_branch":"main","source_branch":"feature","updated_at":"2026-01-04T00:00:00Z"}]'
    exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "view" ]; then
    if [ "$7" = "6" ]; then
        printf '%s\n' '{"iid":6,"title":"Created merge request","description":"Created MR description","state":"opened","draft":true,"web_url":"https://gitlab.com/group/sub/project/-/merge_requests/6","target_branch":"main","source_branch":"feature","author":{"username":"reviewer"},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-04T00:00:00Z","labels":["backend"],"assignees":[{"username":"reviewer"}]}'
    else
        printf '%s\n' '{"iid":5,"title":"Native merge request","description":"Review details","state":"merged","draft":true,"web_url":"https://gitlab.com/group/sub/project/-/merge_requests/5","target_branch":"main","source_branch":"feature","author":{"username":"reviewer"},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-04T00:00:00Z","merged_at":"2026-01-04T00:00:00Z","labels":["backend"],"assignees":[{"username":"reviewer"}]}'
    fi
    exit 0
fi
if [ "$1" = "issue" ]; then
    case "$2" in
        close|reopen|subscribe|unsubscribe) exit 0 ;;
    esac
fi
if [ "$1" = "issue" ] && [ "$2" = "list" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "list-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    printf '%s\n' '[{"iid":8,"title":"First","description":"first body","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/8","created_at":"2026-01-01T00:00:00.000Z","updated_at":"2026-01-02T00:00:00.000Z"},{"iid":9,"title":"Second","description":null,"state":"closed","web_url":"https://gitlab.com/group/sub/project/-/issues/9","created_at":"2026-01-03T00:00:00.000Z","updated_at":"2026-01-04T00:00:00.000Z"}]'
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "create" ]; then
    cat > "$WORKCTL_GLAB_INPUT"
    if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    if [ "$WORKCTL_GLAB_MODE" = "bad-create-url" ]; then
        printf '%s\n' 'https://gitlab.com/other/project/-/issues/21'
    else
        printf '%s\n' 'https://gitlab.com/group/sub/project/-/issues/21'
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "update" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "write-failure" ]; then
        printf '%s\n' 'private provider diagnostic' >&2
        exit 1
    fi
    exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "view" ]; then
    if [ "$WORKCTL_GLAB_MODE" = "update-stale" ]; then
        count_file="${WORKCTL_GLAB_LOG}.views"
        count=0
        if [ -f "$count_file" ]; then
            count="$(cat "$count_file")"
        fi
        printf '%s\n' "$((count + 1))" > "$count_file"
        if [ "$count" -eq 0 ]; then
            printf '%s\n' '{"iid":12,"title":"Crash on startup","description":"Steps to reproduce","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/12","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-03T04:05:06.000Z"}'
        else
            printf '%s\n' '{"iid":12,"title":"Crash on startup","description":"Steps to reproduce","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/12","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-04T04:05:06.000Z"}'
        fi
        exit 0
    fi
    if [ "$WORKCTL_GLAB_MODE" = "description-updated" ]; then
        printf '%s\n' '{"iid":12,"title":"Updated title","description":"Replacement\nwith exact bytes\n","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/12","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-04T04:05:06.000Z"}'
    elif [ "$7" = "21" ]; then
        printf '%s\n' '{"iid":21,"title":"Created issue","description":"Created description","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/21","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-03T04:05:06.000Z"}'
    else
        printf '%s\n' '{"iid":12,"title":"Crash on startup","description":"Steps to reproduce","state":"opened","web_url":"https://gitlab.com/group/sub/project/-/issues/12","created_at":"2026-01-02T03:04:05.000Z","updated_at":"2026-01-03T04:05:06.000Z"}'
    fi
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
        fs::set_permissions(&glab_path, permissions).expect("make glab fixture executable");
        Self {
            root,
            bin,
            log,
            input,
            glab_log,
            glab_input,
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
            .env("WORKCTL_GH_CWD_ROOT", &self.root)
            .env("WORKCTL_GH_MODE", mode)
            .env("WORKCTL_GLAB_LOG", &self.glab_log)
            .env("WORKCTL_GLAB_INPUT", &self.glab_input)
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

    /// Exact UTF-8 request body captured from the GitLab CLI's stdin.
    pub fn glab_input(&self) -> Vec<u8> {
        fs::read(&self.glab_input).unwrap_or_default()
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
