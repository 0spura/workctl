# workctl

A local CLI for GitHub and GitLab work items. It wraps `gh` and `glab` in argument arrays, keeps each
provider's own grammar, and never shells out. GitHub covers issues and pull requests; GitLab covers
issues and merge requests. Remote paths never touch process arguments: bodies travel on stdin.

Label and Project-field selection can call a decision model, opt-in only, through `@auto`.

## Requirements

- Rust toolchain and Cargo
- `git` for repository discovery
- `gh` (2.99.0+ for attachments) and/or `glab`, already authenticated
- `--help` and `--version` need none of the above

## Install

```sh
cargo install --git https://github.com/0spura/workctl --locked
workctl --version
```

Cargo puts the binary in `$CARGO_HOME/bin`; keep it on `PATH`. From a clone: `cargo build --release`
then `cargo test` and `./target/release/workctl --help`.

## Issues

```sh
workctl issue create --title "Fix the parser" --body-file report.md --label bug --milestone "1.4"
workctl issue list --state open --limit 30            # open|closed|all, limit 1-1000, default 30
workctl issue view 123                               # body, type, parent, sub-issues
workctl issue blockers 30                            # "#18 -> #29 -> #30", bounded traversal
workctl issue edit 123 --append-body "Reproduced on 1.4.2."
workctl issue edit 123 --replace-section "## Acceptance" --section-body-file new.md
workctl issue edit 123 --patch-file body.patch --expect-updated-at 2026-01-02T00:00:00Z
workctl issue edit 123 --state closed --reason completed --comment "Fixed in 1.4.3"
workctl issue edit 123 --parent 100 --add-sub-issue 124,125 --add-blocked-by 200
workctl issue edit 123 --project-field "Priority=High" --clear-project-field "Start date"
workctl issue comment 123 --body-file - < notes.md
workctl issue lock 123 --reason resolved
workctl issue develop 123 --name parser-crash --checkout # creates 123-parser-crash
```

- `--label @auto` and `--project-field 'Priority=@auto'` pick from model scores; one request per
  operation, scores below the threshold keep the current value, and the revision is rechecked after.
- Project fields resolve against one configured, repository-allowlisted Project profile; validation
  runs before any write.
- Body changes are one of `--body`, `--append-body`, `--replace-section`, `--patch-file` — never the
  whole body rewrite. The same table backs `pr edit`.
- `--patch-file` matches hunks by exact context and fails with `patch_conflict` without writing.

## Pull requests

```sh
workctl pr create --title "Add PR support" --body "Implements #12" --base main --closes 12
workctl pr list --state merged --author me --label bug --search "in:title workctl"
workctl pr view 42
workctl pr status 42                                  # state, draft, branches, mergeability, checks
workctl pr diff 42 --name-only
workctl pr checks 42 --required --watch --interval 5 --fail-fast
workctl pr review 42 --approve --body "Looks good"
workctl pr update-branch 42 --rebase
workctl pr merge 42 --method squash --delete-branch   # --auto only queues
workctl pr ready 42
workctl pr edit 42 --state closed --comment "Superseded by #43" --delete-branch
workctl pr revert 42 --title "Revert parser rewrite"
```

- `--milestone @current` picks the nearest open milestone due today or later; a tie or none fails
  before the write. Omitting the flag sends nothing, so nothing is assigned implicitly.
- `pr edit` adds `--base`, `--closes`/`--remove-closes`, assignee/label/reviewer add and remove, and
  `--milestone`; closing references are managed inside the body in one write.
- `pr merge --delete-branch` deletes the remote head branch through its own `gh api` call, never a
  local branch, never a fork's same-named branch.
- `pr checks --watch` uses `gh`'s native watch; `--interval`, `--fail-fast`, `--watch-timeout` are
  watch-only. `pr checkout` delegates to `gh pr checkout` and never passes `--force`.

## GitLab

GitLab mirrors `glab`, so GitHub spellings like `--state` are usage errors. `--help` shows the active
grammar; run workctl from the target GitLab worktree so its origin and project config select it.
```sh
workctl issue create --title "Unexpected crash" --description-file description.md --weight 2
workctl issue update 12 --title "Crash on startup" --assignee=+alice --assignee=-bob
workctl issue list --closed --label bug --per-page 50
workctl issue view 12
workctl mr list && workctl mr view 7
```

`iid` maps to `number`, `description` to `body`, `opened` to `open`. Descriptions go to `glab` on
stdin. Merge requests live under `mr`, not `pr`. If a write cannot be confirmed,
`gitlab_write_uncertain` asks you to check the issue before retrying.

## Output

Compact text by default: writes report `#123 created`, `#123 updated`, `!5 closed`, `unchanged`
for a no-op, and `merge queued` for `--auto`. Lists carry identity, title, and state; views, diffs,
notes, and discussions keep the requested content. Terminal controls are escaped; authored newlines
and tabs stay readable.

`--format json` or `defaults.output.format: "json"` selects the unchanged structured schemas, with the
explicit flag winning. Errors print `code: message` on stderr, or JSON when selected, and always exit
nonzero with an empty stdout. Partial failures keep resource identity plus completed/pending
operations; no retry, no rollback.

## Repository and configuration

Run globally installed workctl from the target project's Git worktree, or select another local
checkout with `--repo-path PATH`. The target worktree supplies the provider, `origin`, project
configuration, and command context. Relative paths resolve from the caller's current directory;
command file paths resolve from the selected worktree. `--repo` changes only remote resource scope
within the selected provider. It does not select a provider or fetch configuration remotely.

```sh
workctl issue create --repo-path ../other-project --title "Fix the parser"
```


`.workctl.json` (committable) and `.workctl.local.json` (gitignored, wins) are read at the Git root,
64 KiB max, non-symlink. Provider keys: `codeProvider` and `workItemProvider`; the former generic
`provider` key and provider CLI flags are removed. Remove `provider` from older config files.
Other keys: `defaults.github.issue` (`assignees`, `labels`, `labelCandidates`, `project`),
`defaults.github.pr` (`mergeMethod`, `deleteBranch`, `labels`, `assignees`, `reviewers`, `base`,
`draft`), `defaults.gitlab.issue|mr`, `defaults.output.format`, `defaults.github.listLimit`, and
`decisionModel` (`model`, `baseUrl`, local-only `apiKey`). Model choices: `jev-latest`, `laya`,
`fastino/GLiDE`, `local/<model-id>` (loopback OpenAI-compatible endpoint; no hosted key). Explicit
command options override applicable configured defaults; unknown fields fail closed.

```json
{
  "workItemProvider": "github",
  "decisionModel": { "model": "local/qwen3-4b", "baseUrl": "http://127.0.0.1:11434/v1" },
  "defaults": {
    "github": {
      "issue": { "assignees": ["@me"], "labels": ["triaged"], "labelCandidates": ["bug"] },
      "pr": { "mergeMethod": "squash", "deleteBranch": false, "base": "main" },
      "listLimit": 50
    },
    "output": { "format": "text" }
  }
}
```

## Safety and scope

No shell, no automatic CLI downloads, no private upload endpoints: attachments use documented
`gh --attach`. The decision model only ever sees title/description plus configured candidates, and a
`decisionModel.apiKey` is accepted only in the gitignored local file, never printed. Local model URLs
must be loopback. Errors never expose credentials, raw provider diagnostics, stack traces, or
internal paths.

Details: [requirements](docs/srs.md), [architecture](docs/architecture.md), and the
[ADR index](docs/architecture.md#module-map).
