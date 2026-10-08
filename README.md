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
workctl issue blockers 30
workctl issue close 123 --comment "Fixed in 1.4.3" --reason completed
workctl issue close 123 --duplicate-of 118
workctl issue reopen 123 --comment "Still reproducible"
workctl issue comment 123 --body "Reproduced on 1.4.2"
workctl issue comment 123 --body-file - < notes.md
workctl issue lock 123 --reason resolved
workctl issue unlock 123
workctl issue create --title "Crash on save" --body-file report.md --label @auto
workctl issue create --title "Plan the migration" --project-field "Priority=High" --project-field "Start date=2026-10-07"
workctl issue edit 123 --title "Updated title" --add-label @auto
workctl issue edit 123 --title "Updated title" --add-label bug --add-label @auto
workctl issue edit 123 --append-body "Reproduced on 1.4.2."
workctl issue edit 123 --replace-section "## Acceptance" --section-body "New criteria"
workctl issue edit 123 --patch-file body.patch
workctl issue edit 123 --title "Updated title" --expect-updated-at 2026-01-02T00:00:00Z
workctl issue edit 123 --project-field "Priority=High" --clear-project-field "Start date"
workctl issue edit 123 --remove-milestone --type Bug
workctl issue edit 123 --parent 100 --add-sub-issue 124,125
workctl issue edit 123 --add-blocked-by 200 --add-blocking 300
workctl issue edit 123 --remove-parent --remove-blocked-by 200
workctl issue edit 123 124 -t "Updated title" -R owner/repo
workctl issue edit https://github.com/owner/repo/issues/123 -b "Updated body"
```
`--project-field` resolves against one configured GitHub Project profile. The schema and option values are discovered dynamically; supported types are text, number, date, single-select, and iteration. Issue creation uses the profile only when the repository is explicitly allowlisted. Explicit `--project` can be paired with fields only when its title matches that profile. The agent still makes one `workctl issue create` call; workctl handles schema lookup, issue creation, Project membership, and field updates internally. If a post-create step fails, a provider-neutral partial-success error identifies the resource and completed/pending operations; workctl does not retry creation.

Issue editing accepts `--project-field NAME=VALUE` and `--clear-project-field NAME` for the same repository-allowlisted profile. It validates all requested fields before writes, never reapplies creation defaults or `autoSelectFields`, and does not automatically add Project membership. Set/clear overlaps and Project removal combined with field changes are rejected. Field-only edits skip `gh issue edit`; combined edits write the issue first, then the fields serially. A failure after any successful write returns `partial_success` with completed/pending operations and no automatic retry. `--expect-updated-at` guards the issue revision, not concurrent changes to Project fields.

Issue edits can explicitly select a Project option with `--project-field 'Priority=@auto'`. Only named single-select or iteration fields participate; creation defaults and `autoSelectFields` do not trigger selection during edit. The configured model sees the final proposed title/body and discovered options, together with any `--add-label @auto` candidates in one request per issue. A unique best score of at least 0.8 writes that option; lower scores or tied best options leave the current field untouched. Model failures stop before writes, and the issue revision is rechecked after classification. Project field values themselves have no revision guard.

GitHub editing follows native `gh` names: `--remove-milestone` replaces `--clear-milestone` without an alias; `--type`/`--remove-type`, parent/sub-issue flags, and blocked-by/blocking flags mutate the corresponding relationships. Relationship values accept issue numbers or canonical GitHub issue URLs; list flags accept repeats or comma-separated values. `--add-blocked-by 200` means issue 200 blocks the edited issue; `--add-blocking 300` means the edited issue blocks issue 300. `--parent` replaces the parent, and `--remove-parent` removes it.

Use native `-t`, `-b`, `-F`, `-m`, and `-R` shortcuts. Multiple edit targets must belong to one repository and are processed serially; success returns an array of full records instead of the single-target object. A later failure reports completed/pending target URLs as `partial_success` and stops without retrying. A successful mutation followed by failed confirmation also reports partial success; inspect GitHub before resuming. Numeric relationship references belong to the selected repository; explicit relationship URLs can name another repository.

Output is compact JSON by default. Add `--format text` for human-readable output; terminal control characters in provider data are escaped, while body newlines and tabs remain readable. Errors are JSON on stderr with a nonzero exit code. `issue list` returns summaries without bodies; `view`, `create`, and `edit` return full issue data. There is no delete command in this release.

GitHub `issue view NUMBER` additionally returns `issue_type` (string or null), `parent` (issue reference or null), and a `sub_issues` array. Each reference has `number`, `title`, `state`, and `url`. Sub-issues are paginated in pages of 100, up to 1,000 relationships and ten pages; unavailable, malformed, or incomplete hierarchy data fails the view. Blocker relationships are neither queried nor printed by `issue view`; use `issue blockers` to inspect them. Native dependency editing and create/edit/list and GitLab outputs remain unchanged.
`workctl issue blockers NUMBER` is the context-light alternative: it returns only complete chains of open blockers, such as `#18 -> #29 -> #30`; with no open blocker chain it prints nothing in text mode and `[]` in JSON. It omits titles, bodies, and closed issues. Cross-repository references include `OWNER/REPO#NUMBER`. It does not claim that the issue is ready when the output is empty. Traversal is bounded to 20 issue reads, 500 edges, 20 paths, and 20 issues per path. If a bound prevents a complete result, the command fails rather than presenting a partial chain as complete.

`issue close`, `issue reopen`, `issue comment`, `issue lock`, and `issue unlock` forward one native `gh issue` operation each and pass only the options you supply, so no reason, comment, or duplicate reference is inferred. `--reason` on close accepts `completed`, `not planned`, or `duplicate`; lock reasons are `off_topic`, `resolved`, `spam`, and `too_heated`. Comment text comes from exactly one of `--body` or `--body-file` (`-` reads stdin) and is sent to `gh` on stdin rather than in the process arguments. Results are compact: close and reopen return `{"number", "state"}`, comment returns `{"number", "target": "issue"}`, and lock and unlock return `{"number", "target": "issue", "locked"}`. These commands never read or edit existing comment threads.

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

`issue create --label @auto` and `issue edit NUMBER --add-label @auto` use the selected `DecisionModel` adapter and add qualifying labels alongside manual labels. `@auto` is reserved and cannot be used with `--remove-label`. Configure `defaults.github.issue.labelCandidates` to pass only an allowlisted subset of existing repository labels to the model; without it, all repository labels remain candidates. For GitHub issue creation, `defaults.github.issue.project.autoSelectFields` opts specific Project fields into automatic option selection; only single-select and iteration fields are eligible. The configured Project profile is the allowlist, and it receives only dynamically discovered options for those fields. Selection happens in the same model request as `@auto` labels when both are requested. Existing configured or explicit `--project-field` values win; scores below 0.8 leave fields unset. A tied best score or model/configuration failure stops before issue creation.

Set `DECISION_MODEL` to `jev-latest`, `laya`, `fastino/GLiDE`, or `local/<model-id>`. Jev, Laya, and GLiDE use their native hosted decision APIs and require `DECISION_MODEL_API_KEY`. `local/<model-id>` uses any local server implementing OpenAI-compatible Chat Completions at `DECISION_MODEL_BASE_URL` (default `http://127.0.0.1:11434/v1`); it is not a hosted OpenAI integration and requires no hosted key. Local endpoints must remain on loopback.

Native decision models return model-native probabilities/confidence. Generic LLM scores are generated estimates; JSON output mode does not make them calibrated probabilities. All adapters use the existing `>= 0.8` cutoff, but scores are not assumed comparable across models.


## GitLab issues

GitLab resolves to a different grammar whose verbs and flags mirror `glab`, so GitHub spellings such as `--state` and `--limit` are usage errors rather than silently accepted. The resolved provider decides which grammar exists; `--help` shows the active one.

```sh
workctl issue create --title "Unexpected crash" --description-file description.md \
  --label bug --assignee alice --milestone "1.0" --weight 2 --due-date 2026-11-30
workctl issue update 12 --title "Crash on startup" --label triage --unlabel needs-info
workctl issue update 12 --assignee=+alice --assignee=-bob --milestone "" --public
workctl issue list --all
workctl issue list --closed --label bug
workctl issue list --assignee @me --search crash --per-page 50
workctl issue view 12
```

- `issue create` requires `--title` and either `--description TEXT` or `--description-file FILE`; `--description-file -` reads stdin. Description bytes are sent to `glab` on stdin, not exposed in process arguments; a description consisting only of `-` is rejected because `glab` reserves it for editor behavior. Supported metadata: repeatable `--label` and plain `--assignee`, `--milestone`, `--confidential`, `--weight` (including zero), and `--due-date YYYY-MM-DD`. `glab issue create` omits weight zero, so workctl sets it with a follow-up `glab issue update`.
- `issue update NUMBER` accepts any nonempty combination of `--title`, `--description`/`--description-file`, `--label`, `--unlabel`, `--assignee`, `--unassign`, `--milestone`, `--confidential`/`--public`, `--weight`, and `--due-date`. Empty description replacement is rejected. Assignee `+`/`-` prefixes add/remove usernames; milestone empty string or `0` clears it.
- `issue list` filters: `--closed` (closed only), `--all` (every state; mutually exclusive with `--closed`), `--label NAME` (repeatable), `--assignee USERNAME|@me`, `--author USERNAME|@me`, `--milestone VALUE`, `--search TEXT`, and `--per-page N` (1 through 100, default 30). The default state is open.
- `issue view NUMBER` takes the GitLab IID.
- Output retains the shared issue shapes: `issue list` returns summaries without bodies, and `issue view`, `issue create`, and `issue update` return `{"number", "title", "body", "state", "url", "created_at", "updated_at"}` with the GitLab `iid` mapped to `number`, `description` to `body`, and `opened` state to `open`. GitHub's view-only relationship fields and GitLab-only fields such as `confidential`, `weight`, and `due_date` are not part of the shared record.
- If the write succeeds but its result cannot be confirmed, `gitlab_write_uncertain` asks you to check the issue before retrying. GitLab `mr` is not implemented yet. Typing `pr` fails with an error naming `mr`, GitLab's merge-request group.

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
workctl pr checkout 42
workctl pr status 42
workctl pr update-branch 42
workctl pr update-branch 42 --rebase
workctl pr diff 42
workctl pr diff 42 --name-only
workctl pr checks 42
workctl pr checks 42 --required
workctl pr checks 42 --required --watch --interval 5 --fail-fast
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
workctl pr comment 42 --body "Looks good to me"
workctl pr lock 42 --reason resolved
workctl pr unlock 42
workctl pr revert 42 --title "Revert parser rewrite" --body "Caused the 1.4.2 regression" --draft
```

Creation accepts optional `--assignee`, `--label`, `--reviewer` (pull requests), `--milestone`, and `--project` flags; `--milestone @current` selects the open milestone with the nearest upcoming due date, ignoring overdue and undated ones, and fails before the write when none qualifies or the nearest dates tie. Omitting `--milestone` sends no milestone flag and performs no lookup. Issue edits and `pr edit` accept optional add/remove assignee, label, and project flags; both can set a milestone by name or with `@current`, or clear it, without applying a creation default. Attachments use `--attach FILE[#ALT]`; if create fails during attachment upload, check GitHub before retrying because the item may already exist.

`pr list` returns summaries without bodies; `view`, `create`, and `edit` return full pull request data. List states are `open`, `closed`, `merged`, and `all`, and the default is `open`. `pr diff` prints the patch and `--name-only` limits it to changed file names. `pr checks` returns check runs and reports `[]` when none are reported. `pr status` combines concise PR state, draft, branch, mergeability, review decision, and required checks without fetching the body. An empty required-check report or unknown review/mergeability value is inconclusive; status does not declare the PR ready to merge. Its view/check reads are sequential, not an atomic snapshot. `pr update-branch` merges the latest base branch into the PR branch by default; `--rebase` rebases it instead. It updates GitHub only and does not check out or change a local branch. `pr review` requires exactly one of `--approve`, `--request-changes`, or `--comment`, and only a top-level review is submitted.
`pr checks --watch` waits using GitHub CLI's native watch mode rather than polling in workctl. `--interval` (1–300 seconds) and `--fail-fast` are watch-only; `--watch-timeout` is also watch-only and sets an absolute process deadline from 1 to 3600 seconds (default 600). Check failures are returned as check data; a timeout returns a safe error with no partial report. Normal `pr checks` keeps its existing behavior and timeout.
`pr checkout NUMBER` delegates to `gh pr checkout NUMBER` in the current worktree. GitHub CLI retains its normal protection for local changes; workctl never passes `--force` and does not expose branch override, detached-HEAD, or separate-worktree options. Success reports `{"number": NUMBER}` in JSON or `pr #NUMBER checked out` in text; this changes the current local branch.

`pr merge` accepts `--method merge|squash|rebase`, `--delete-branch`, and `--auto`. Method precedence is explicit `--method`, then `defaults.github.pr.mergeMethod`, then GitHub CLI inference. `defaults.github.pr.deleteBranch` is a boolean (default `false`); when true, it enables branch deletion for every merge, while `--delete-branch` enables it for one merge when the setting is false. There is no per-command inverse flag. Without a configured or explicit method, `gh` chooses as before. `pr edit` reuses the body-change table above and adds `--base`, `--add-label`/`--remove-label`, `--add-reviewer`/`--remove-reviewer`, `--add-assignee`/`--remove-assignee`, and `--milestone`. `pr ready` marks the pull request ready for review, and `--undo` converts it back to a draft.

Output shape per command: `create`, `view`, and `edit` return a pull request object; `status` returns `{number, title, state, draft, url, base_ref, head_ref, mergeable, review_decision, required_checks}`; `list` returns an array of summaries; `checkout` returns `{"number"}`; `diff` returns `{"number", "diff"}`; `checks` returns an array of `{name, state, bucket, description, link, workflow}`; `review` returns `{"number", "event"}`; `merge` returns `{"number", "method", "auto"}`; `update-branch` returns `{"number", "rebase"}` on success; `ready` returns `{"number", "draft"}`; `close` and `reopen` return `{"number", "state"}`; `comment` returns `{"number", "target": "pr"}`; `lock` and `unlock` return `{"number", "target": "pr", "locked"}`; `revert` returns `{"number", "pull_request"}`, naming the reverted pull request and the newly created revert.

`pr comment`, `pr lock`, and `pr unlock` forward one native `gh pr` operation each. Comment text comes from exactly one of `--body` or `--body-file` (`-` reads stdin) and travels on stdin, never in the process arguments. `pr revert NUMBER` opens a new pull request that reverts the merge; `--title`, `--body`/`--body-file`, and `--draft` are forwarded only when supplied. Creating the revert is a remote write and is never retried automatically; if the command succeeds but prints no readable pull-request URL, it fails with `provider_response` so you can check GitHub before retrying.

## Repository context

Inside a Git worktree, `workctl` resolves the repository from the `origin` remote and infers the provider from its host: `github.com` selects GitHub and `gitlab.com` selects GitLab. Outside a Git worktree, pass both `--provider` and `--repo`. An unknown host does not fall back, and an `origin` remote belonging to a provider other than the selected one fails closed.

Provider precedence is `--provider`, project configuration, then the Git remote host, defaulting to GitHub when nothing resolves. Repository precedence is `--repo`, then the `origin` remote. The provider is selected before argument parsing, so only the resolved provider's grammar exists for a given invocation.

`--repo` accepts `OWNER/REPO` for GitHub, `GROUP[/SUBGROUP...]/PROJECT` for GitLab, and an optional leading `HOST/` that must match the resolved provider's host.

Optional strict JSON configuration files are discovered at the Git root and validated on every command, even when CLI flags override provider/repository values:

- `.workctl.json` can be committed for project-wide defaults.
- `.workctl.local.json` overrides fields locally and is gitignored.

Both files must be regular, non-symlink files no larger than 64 KiB. Supported fields include `provider` and `workItemProvider` (`github` or `gitlab`), GitHub issue defaults under `defaults.github.issue` (`assignees`, `labels`, `labelCandidates`, and an optional Project profile with `url`, exact `repositories`, `fields`, and `autoSelectFields`), and PR defaults under `defaults.github.pr` (`mergeMethod`: `merge|squash|rebase`; `deleteBranch`: boolean, default `false`). PR merge method precedence is explicit CLI option, configured default, then `gh` inference. The branch-deletion setting applies to every PR merge when true; the per-command `--delete-branch` option can enable deletion but cannot override a true config value. Local top-level values override shared values; a local `defaults` object replaces the shared defaults object as a whole. Unknown fields and malformed JSON fail closed. Legacy `.mcp-tracker*.json` configuration is not read or migrated.


Example:

```json
{
  "workItemProvider": "github",
  "defaults": {
    "github": {
      "issue": {
        "assignees": ["@me"],
        "labels": ["triaged"],
        "labelCandidates": ["bug", "documentation"],
        "project": {
          "url": "https://github.com/orgs/acme/projects/7",
          "repositories": ["acme/service"],
          "autoSelectFields": ["Priority"],
          "fields": {
            "Status": "Todo"
          }
        }
      },
      "pr": {
        "mergeMethod": "squash",
        "deleteBranch": false
      }
    }
  }
}
```

## Safety and scope

GitHub operations use `gh` argument arrays and GitLab operations use `glab` argument arrays; create/edit bodies travel on stdin. Attachments use documented `gh --attach` support; no private upload endpoints are called. DecisionModel runs for `@auto` labels or when an allowlisted Project field needs automatic selection. It receives only work-item title/description and configured candidates (repository labels and dynamically discovered Project options), never credentials or unrelated metadata. Hosted adapters use their native authentication headers and fixed HTTPS endpoints. Local model URLs are restricted to loopback; errors do not expose credentials, raw provider diagnostics, stack traces, or internal paths. No shell or automatic `gh`/`glab` download is used.

The decision-model package is independent of code-host provider implementation. Its native adapters and generic local LLM adapter share one validated input/output contract; any code-host provider can call the same package. The current CLI implements GitHub issues and pull requests plus GitLab issue reads. See [requirements](docs/srs.md), [architecture](docs/architecture.md), [ADR-0015](docs/adr/0015-provider-neutral-decision-model-package.md), [ADR-0020](docs/adr/0020-remove-gliner-python-server.md), and prior [ADRs](docs/architecture.md#module-map).
