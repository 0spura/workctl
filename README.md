# workctl

`workctl` is a local CLI for code-host work items and pull requests. It manages GitHub issues and pull requests, and GitLab issues, by invoking the provider's own CLI, and each provider keeps its own command grammar. The `DecisionModel` package is provider-neutral so code-host providers can reuse the same model adapters. Issue label selection is opt-in through `@auto` in the existing label options.

## Requirements

- Stable Rust toolchain and Cargo
- `git` for repository and remote discovery
- GitHub CLI (`gh`) installed and authenticated for GitHub operations; `gh` 2.99.0 or newer for attachments
- GitLab CLI (`glab`) installed and authenticated for GitLab operations
- Hosted decision adapters use `DECISION_MODEL_API_KEY`; local adapters do not require hosted credentials

Help and version do not require `git` or a provider CLI.

## Build and verify

```sh
cargo build --release
cargo test
./target/release/workctl --help
```

## Commands

```sh
workctl issue create --title "Fix the parser" --body "Details"
workctl issue create --title "Crash on save" --body-file report.md
workctl issue list --state open --limit 30
workctl issue list --label bug --label p1 --assignee me --search "in:title fix"
workctl issue view 123
workctl issue create --title "Crash on save" --body-file report.md --label @auto
workctl issue edit 123 --title "Updated title" --add-label @auto
workctl issue edit 123 --title "Updated title" --add-label bug --add-label @auto
workctl issue edit 123 --append-body "Reproduced on 1.4.2."
workctl issue edit 123 --replace-section "## Acceptance" --section-body "New criteria"
workctl issue edit 123 --patch-file body.patch
workctl issue edit 123 --title "Updated title" --expect-updated-at 2026-01-02T00:00:00Z
```

Output is compact JSON by default. Add `--format text` for human-readable output; terminal control characters in provider data are escaped, while body newlines and tabs remain readable. Errors are JSON on stderr with a nonzero exit code. `issue list` returns summaries without bodies; `view`, `create`, and `edit` return full issue data. There is no delete command in this release.

Global options apply before or after the group subcommand:

```sh
workctl --repo owner/repo issue list
workctl issue list --provider github --format text
```

The default list state is `open`; accepted states are `open`, `closed`, and `all`. The limit defaults to 30 and accepts values from 1 through 1000. Create and edit do not prompt interactively. An explicit empty edit body clears the issue body.

Issue and pull-request metadata flags are optional. Use the repository's existing labels, milestone, assignees, and project conventions. `--milestone @current` selects the open milestone with the nearest due date today or later; overdue and undated milestones are ignored, and creation fails safely before the write when no milestone qualifies or the nearest dates tie. Omitting `--milestone` sends no milestone flag and performs no lookup, so nothing is assigned implicitly. `--assignee`, `--label`, `--milestone @current`, title, and body can all be given in one create command, which covers self-assignment with `--assignee @me`.

The optional semantic declaration for issue creation is **proposed, not implemented**. It would be
stored privately and kept separate from GitHub-native labels, milestones, assignees and projects. The
input channel, local store and reconciliation flow are not yet selected. Existing metadata flags do not
provide this feature; see the [SRS proposal](docs/srs.md#proposed-extension-rf-wi5-private-semantic-declaration-on-issue-create)
and [architecture](docs/architecture.md#proposed-private-semantic-declarations).

`issue create --label @auto` and `issue edit NUMBER --add-label @auto` use the selected `DecisionModel` adapter and add qualifying labels alongside manual labels. `@auto` is reserved and cannot be used with `--remove-label`. Issue text and candidate labels are sent to a model only when the marker is present.

Set `DECISION_MODEL` to `jev-latest`, `laya`, `fastino/GLiNER2.5-Decide`, `fastino/GLiDE`, or `local/<model-id>`. Jev, Laya, and GLiDE use their native hosted decision APIs and require `DECISION_MODEL_API_KEY`. GLiNER-Decide uses the local Python service below. `local/<model-id>` uses any local server implementing OpenAI-compatible Chat Completions at `DECISION_MODEL_BASE_URL` (default `http://127.0.0.1:11434/v1`); it is not a hosted OpenAI integration.

Native decision models return model-native probabilities/confidence. Generic LLM scores are generated estimates; JSON output mode does not make them calibrated probabilities. All adapters use the existing `>= 0.8` cutoff, but scores are not assumed comparable across models.

To run GLiNER2.5-Decide locally, install the service dependency and start its loopback server:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install -r decision-model-server/requirements.txt
python decision-model-server/server.py --port 8765
```

The first model load may download weights. The service binds to `127.0.0.1` by default and can load another local GLiNER2-compatible model with `--model`. Local HTTP endpoints must remain on loopback.

## GitLab issues

GitLab resolves to a different grammar whose verbs and flags mirror `glab`, so GitHub spellings such as `--state` and `--limit` are usage errors rather than silently accepted. The resolved provider decides which grammar exists; `--help` shows the active one.

```sh
workctl issue list --all
workctl issue list --closed --label bug
workctl issue list --assignee @me --search crash --per-page 50
workctl issue view 12
```

- `issue list` filters: `--closed` (closed only), `--all` (every state; mutually exclusive with `--closed`), `--label NAME` (repeatable), `--assignee USERNAME|@me`, `--author USERNAME|@me`, `--milestone VALUE`, `--search TEXT`, and `--per-page N` (1 through 100, default 30). The default state is open.
- `issue view NUMBER` takes the GitLab IID.
- Output uses the same shapes as GitHub: `issue list` returns summaries without bodies, and `issue view` returns `{"number", "title", "body", "state", "url", "created_at", "updated_at"}` with the GitLab `iid` mapped to `number`, `description` to `body`, and the `opened` state to `open`. GitLab-only fields such as `confidential`, `weight`, and `due_date` are not part of the record.
- `issue create`, `issue update`, and the `mr` group are not implemented yet. Typing `pr` fails with an error naming `mr`, since that is GitLab's name for merge requests.

## Editing without rewriting the body

An `issue edit` — or a `pr edit` — never requires the whole new body. Pick at most one body change per invocation:

| Flag | Effect |
| --- | --- |
| `--body TEXT` / `--body-file FILE` | Replaces the body (`-` reads stdin) |
| `--append-body TEXT` / `--append-body-file FILE` | Appends a new block, keeping one separating newline |
| `--replace-section HEADING --section-body TEXT` / `--section-body-file FILE` | Replaces the content of one ATX section (`## Heading`), leaving other sections alone |
| `--patch-file FILE` | Applies a unified diff to the current body (`-` reads stdin) |

The same table and the same resolver back `pr edit`: it fetches the pull request once, applies the change to that text, and sends one write. `issue edit` and `pr edit` differ only in the metadata flags they accept.

`--patch-file` takes standard `git diff` output. Hunks are located by exact context match, not by line number, so a patch applies to the right place or fails with `patch_conflict`; there is no fuzzy matching and no write on failure. Generate the diff against the body `workctl issue view` or `workctl pr view` just returned, and use `--expect-updated-at` with that item's `updated_at` to refuse the write if someone edited it in between.

Attachments are optional on issue create/edit and pull-request create/edit. `--attach FILE[#ALT]` may be repeated; GitHub CLI 2.99.0 or newer is required. `gh` validates and uploads the media; `workctl` never calls private upload endpoints.

This reference is duplicated by `workctl issue --help` and `workctl issue edit --help`, which are generated from the same definitions that parse the flags. For an agent, `--help` is the cheaper and safer source: it is read only when needed and cannot drift from the binary.

## Pull requests

```sh
workctl pr create --title "Add pull request support" --body "Implements #12" --base main --closes 12
workctl pr create --title "WIP: parser" --body "Draft" --draft --head feature-x
workctl pr list --state open --limit 30
workctl pr list --state merged --author me --label bug --draft --search "in:title workctl"
workctl pr view 42
workctl pr diff 42
workctl pr diff 42 --name-only
workctl pr checks 42
workctl pr checks 42 --required
workctl pr review 42 --approve --body "Looks good"
workctl pr review 42 --request-changes --body-file review.md
workctl pr review 42 --comment
workctl pr merge 42 --method squash --delete-branch
workctl pr merge 42 --auto --method squash
workctl pr edit 42 --title "Updated title"
workctl pr edit 42 --append-body "Reproduced on 1.4.2."
workctl pr edit 42 --replace-section "## Acceptance" --section-body "New criteria"
workctl pr edit 42 --patch-file body.patch
workctl pr edit 42 --add-label bug --remove-label stale --add-reviewer hubot --remove-reviewer octocat
workctl pr edit 42 --title "Updated title" --expect-updated-at 2026-01-02T00:00:00Z
workctl pr ready 42
workctl pr ready 42 --undo
workctl pr close 42 --comment "Superseded by #43" --delete-branch
workctl pr reopen 42 --comment "Reopening"
```

Creation accepts optional `--assignee`, `--label`, `--reviewer` (pull requests), `--milestone`, and `--project` flags; `--milestone @current` selects the open milestone with the nearest upcoming due date, ignoring overdue and undated ones, and fails before the write when none qualifies or the nearest dates tie. Omitting `--milestone` sends no milestone flag and performs no lookup. Issue edits and `pr edit` accept optional add/remove assignee, label, and project flags; both can set a milestone by name or with `@current`, or clear it, without applying a creation default. Attachments use `--attach FILE[#ALT]`; if create fails during attachment upload, check GitHub before retrying because the item may already exist.

`pr list` returns summaries without bodies; `view`, `create`, and `edit` return full pull request data. List states are `open`, `closed`, `merged`, and `all`, and the default is `open`. `pr diff` prints the patch and `--name-only` limits it to changed file names. `pr checks` returns the check runs and reports `[]` when the pull request has none. `pr review` requires exactly one of `--approve`, `--request-changes`, or `--comment`, and only a top-level review is submitted.

`pr merge` accepts `--method merge|squash|rebase`, `--delete-branch`, and `--auto`. `gh pr merge` needs a merge method, and `workctl` passes none unless you give `--method`, so supply an explicit method whenever `gh` cannot infer one. `pr edit` reuses the body-change table above and adds `--base`, `--add-label`/`--remove-label`, `--add-reviewer`/`--remove-reviewer`, `--add-assignee`/`--remove-assignee`, and `--milestone`. `pr ready` marks the pull request ready for review, and `--undo` converts it back to a draft.

Output shape per command: `create`, `view`, and `edit` return a pull request object; `list` returns an array of summaries; `diff` returns `{"number", "diff"}`; `checks` returns an array of `{name, state, bucket, description, link, workflow}`; `review` returns `{"number", "event"}`; `merge` returns `{"number", "method", "auto"}`; `ready` returns `{"number", "draft"}`; `close` and `reopen` return `{"number", "state"}`.

## Repository context

Inside a Git worktree, `workctl` resolves the repository from the `origin` remote and infers the provider from its host: `github.com` selects GitHub and `gitlab.com` selects GitLab. Outside a Git worktree, pass both `--provider` and `--repo`. An unknown host does not fall back, and an `origin` remote belonging to a provider other than the selected one fails closed.

Provider precedence is `--provider`, project configuration, then the Git remote host, defaulting to GitHub when nothing resolves. Repository precedence is `--repo`, then the `origin` remote. The provider is selected before argument parsing, so only the resolved provider's grammar exists for a given invocation.

`--repo` accepts `OWNER/REPO` for GitHub, `GROUP[/SUBGROUP...]/PROJECT` for GitLab, and an optional leading `HOST/` that must match the resolved provider's host.

Optional strict JSON configuration files are discovered at the Git root and validated on every command, even when CLI flags override provider/repository values:

- `.workctl.json` can be committed for project-wide defaults.
- `.workctl.local.json` overrides fields locally and is gitignored.

Both files must be regular, non-symlink files no larger than 64 KiB. The only supported fields are `provider` and `workItemProvider`, each with value `github` or `gitlab`. Unknown fields and malformed JSON fail closed. Legacy `.mcp-tracker*.json` configuration is not read or migrated.

Example:

```json
{
  "workItemProvider": "github"
}
```

## Safety and scope

GitHub operations use `gh` argument arrays and GitLab operations use `glab` argument arrays; create/edit bodies travel on stdin. Attachments use documented `gh --attach` support; no private upload endpoints are called. DecisionModel requests occur only for `@auto`, send only work-item title/description and candidate labels, and never include credentials in prompts. Hosted adapters use their native authentication headers and fixed HTTPS endpoints. Local model URLs are restricted to loopback; errors do not expose credentials, raw provider diagnostics, stack traces, or internal paths. No shell or automatic `gh`/`glab` download is used.

The decision-model package is independent of code-host provider implementation. Its native adapters and generic local LLM adapter share one validated input/output contract; any code-host provider can call the same package. The current CLI implements GitHub issues and pull requests plus GitLab issue reads. See [requirements](docs/srs.md), [architecture](docs/architecture.md), [ADR-0015](docs/adr/0015-provider-neutral-decision-model-package.md), and prior [ADRs](docs/architecture.md#module-map).
