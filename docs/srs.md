# SRS: workctl v0

> Vision: [docs/product/vision.md](./product/vision.md)
> Architecture: [docs/architecture.md](./architecture.md)

Actors: a developer or coding agent running `workctl` locally. Observable behavior is defined at the CLI boundary: arguments, stdout, stderr, exit code, and remote GitHub issue state.

## 1. Functional requirements

### RF-CLI.1: Command surface
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- Command groups in the GitHub grammar: `workctl issue create|list|view|edit` and `workctl pr create|list|view|diff|checks|review|merge|edit|ready|close|reopen`.
- The provider is resolved before the command grammar is parsed: `.workctl.json`/`.workctl.local.json` (`provider`, `workItemProvider`), then the Git origin host, with explicit `--provider` overriding both.
- Each provider owns a static grammar. The active grammar's verbs and flags mirror that provider's own CLI, and another provider's verb or flag is a usage error rather than a runtime rejection.
- Top-level `--help` is provider-neutral and states the resolution order; a subcommand's help reflects the resolved provider's grammar.
- `--repo OWNER/REPO`, `--provider`, and `--format json|text` are global overrides accepted before or after the group.
- Help and version work without `git` or `gh` installed.
- Every flag carries a help description, and each subcommand prints usage examples for the behavior a caller cannot infer from flag names — for `issue edit` and `pr edit`, the body-change set and the exact-match patch contract; for `pr list`, filter composition; for `pr merge`, when an explicit method is required.
- There is no delete command for issues or pull requests in v0.

**Acceptance:** In the GitHub grammar, `workctl issue --help` lists exactly four supported issue subcommands and `workctl pr --help` lists exactly eleven supported pull request subcommands; invoking an unknown command produces a structured JSON error on stderr and a nonzero exit. A verb or flag belonging to another provider's grammar is rejected as a usage error. No flag in any subcommand help is printed without a description, and `workctl issue edit --help` documents the `patch_conflict` failure and exact-context matching.
**Verification:** CLI integration test.

### RF-WI.1: Create an issue
**Priority:** Must Have | **Status:** In Progress | **Dependencies:** RF-CFG.1, RF-PRV.1

- `create` requires a nonblank title, accepts an optional body from `--body` or `--body-file FILE` (`-` reads standard input), and optionally accepts repeated `--assignee`, `--label`, `--project`, one `--milestone`, repeated `--attach FILE[#ALT]`.
- `--label @auto` opts into automatic label selection through the provider-neutral `DecisionModel` package. It may be combined with manual `--label` values. Only labels with scores >= 0.8 are added.
- `DECISION_MODEL` selects `jev-latest`, `laya`, `fastino/GLiDE`, or `local/<model-id>` for an OpenAI-compatible local Chat Completions service. Hosted adapters require `DECISION_MODEL_API_KEY`; local adapters do not.
- The model package consumes generic work-item title/description and named candidates with optional descriptions; it does not depend on a code-host response type. Current issue commands use the active code-host provider; PR labeling is outside scope.
- Native typed-decision adapters provide their own probabilities/confidence. Generic local LLM scores are generated estimates, not calibrated probabilities. All responses must cover each candidate exactly once with a finite score in [0,1].
- `DECISION_MODEL_BASE_URL` configures only local services and must target loopback. Hosted endpoints are fixed. Model/configuration/credential/output failures prevent the issue write.
- `--milestone @current` assigns the open GitHub milestone with the nearest due date today or later. Overdue and undated milestones are ignored. If no milestone qualifies, or two milestones tie for the nearest eligible due date, creation fails with `invalid_input` before any write.
- Omitting `--milestone` leaves the milestone unset and performs no milestone lookup, so no milestone is ever assigned implicitly. Any other value is a literal milestone name passed through unchanged. Issue edits never apply a default; omitted native metadata remains unchanged. Semantic declarations are a separate proposed extension (RF-WI.5), not part of the implemented create contract.
- The command returns the created issue's number, title, body, state, URL, and timestamps. Missing body creates an empty body; no interactive prompt is opened.
- Attachments use documented `gh issue create --attach` and require GitHub CLI 2.99.0 or newer. The version gate runs only when attachments are requested. If create fails during attachment upload, return `attachment_create_uncertain` because the issue may already exist; do not retry automatically.

**Acceptance:** `--label @auto` selects the configured model only when present, combines selected labels with explicit labels, never forwards the sentinel to the code-host provider, and adds only scores >= 0.8. Missing hosted credentials/configuration and invalid model responses fail before the issue write. `--remove-label @auto` is invalid. Local URLs outside loopback are rejected.
**Verification:** CLI fixture tests for marker parsing/combination, missing credentials before provider access, local Chat Completions protocol, threshold boundary, and invalid response; adapter tests for native request/auth/response contracts.


### RF-WI.2: List issues
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `list` returns summaries without issue bodies and never includes pull requests.
- State filter is `open`, `closed`, or `all`; default is `open`.
- `--limit` defaults to 30, accepts 1–1000, and bounds the number of returned issues.
- Filters pass through as fixed flags: `--label` (repeatable), `--assignee`, `--author`, `--mention`, `--milestone`, `--search`, and `--type`. Blank filter or label values are rejected; no raw provider arguments can be injected.

**Acceptance:** The output contains at most the requested limit, contains only issues, and omits body fields. Invalid state/limit values and blank filters fail before invoking `gh`; supplied filters appear in the provider invocation.
**Verification:** Integration tests for filters, bound edges, summary shape, and filter arguments.

### RF-WI.3: View an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `view NUMBER` returns the selected issue with its body and normalized `open|closed` state.
- `NUMBER` must be a positive integer; a pull request number is not accepted as an issue.

**Acceptance:** Valid issue details are returned; zero, malformed identifiers, missing issues, and pull requests produce structured errors without leaking provider stderr.
**Verification:** Integration tests with provider fixtures.

### RF-WI.4: Edit an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `edit NUMBER|URL...` accepts one or more GitHub issues in the same repository. Targets are positive numbers or canonical `https://github.com/OWNER/REPO/issues/NUMBER` URLs; a URL supplies repository/provider context when omitted. Mixed target repositories fail before provider access. Repeated targets are processed once.
- `--add-label @auto` opts into `DecisionModel`; manual additions may appear in the same argument list. `--remove-label @auto` is rejected. No model selection/configuration or request occurs without the sentinel.
- Body changes never require the caller to reproduce the whole body: `--body`/`--body-file` replaces it, `--append-body`/`--append-body-file` appends, `--replace-section HEADING` with `--section-body`/`--section-body-file` replaces one ATX section, and `--patch-file` applies a unified diff.
- `--patch-file` applies hunks by exact context match. Unmatched context is a `patch_conflict` error and no write is sent; line numbers are not trusted. Body text from a file or stdin is UTF-8 and capped at 1 MiB.
- At most one body-change flag may be supplied, and `--replace-section` requires its section body. Violations fail before invoking the code-host provider.
- `--expect-updated-at TIMESTAMP` fails with `conflict` when the fetched issue's `updated_at` differs, before any write. With `@auto`, the selected adapter classifies final proposed title/description; qualifying labels are additive and existing labels remain. The command rechecks the issue timestamp before writing to reject concurrent edits during classification.
- Metadata and attachment values are optional. Blank titles/metadata values are rejected; `--milestone` and `--remove-milestone` cannot be combined. The old `--clear-milestone` flag is removed without an alias.
- `--milestone NAME` sets a literal milestone; `--milestone @current` resolves the nearest eligible open milestone exactly as create does and fails with `invalid_input` when none qualifies or the nearest dates tie. `--remove-milestone` removes the milestone.
- Attachments use `gh issue edit --attach`; GitHub CLI 2.99.0 or newer is required only when `--attach` is supplied.
- Native GitHub edit fields are `--type`/`--remove-type`, `--parent`/`--remove-parent`, `--add-sub-issue`/`--remove-sub-issue`, `--add-blocked-by`/`--remove-blocked-by`, and `--add-blocking`/`--remove-blocking`. Relationship references accept positive numbers or canonical GitHub issue URLs, including cross-repository URLs. Set/remove conflicts and adding/removing the same normalized relationship fail before writes; GitHub enforces relationship authorization and cycle constraints.
- Native shortcuts are `-t` (title), `-b` (body), `-F` (body file), `-m` (milestone), and global `-R` (repository). Assignee/label/project changes and relationship lists accept repeated or comma-separated values.
- Each target preserves its pre-write issue guard and body resolution. Multiple targets execute serially; a later failure stops the batch and returns generic `partial_success` with completed/pending target URLs and safe failure details. A successful native mutation followed by failed readback also returns `partial_success`; no operation is retried automatically. One target returns one full issue record; multiple distinct targets return an array of full records.

**Acceptance:** Editing supports independent metadata and attachment-only updates without changing omitted fields; body changes produce the expected provider write after a pre-write fetch; a stale timestamp, non-applying patch, no-field update, blank value, conflicting or incomplete body change fail before the write. Automatic labels use final proposed text, preserve existing labels, reject concurrent changes, and perform no write if no automatic or other label is selected and no other change was requested.
**Verification:** Integration tests in `tests/cli_issues.rs` cover body changes, metadata, attachments/version gate, partial updates, concurrency, marker validation and automatic labels. Stateful `tests/cli_native_edit.rs` scenarios cover relationship direction and removal, native shortcuts, type changes, URL targets, comma-separated metadata, batch success/partial failure, invalid references/conflicts, stale guards, and post-write readback failure.
### Proposed extension RF-WI.5: Private semantic declaration on issue create
**Priority:** Proposed | **Status:** Proposed | **Dependencies:** RF-WI.1, RF-PRV.1

- An optional, user-authored semantic declaration is distinct from issue title/body and GitHub-native
  metadata. Omitting it leaves the current create path unchanged.
- If supplied, validate its version, supported fields, and finite configured size before side effects;
  persist a recoverable private intent before calling the provider. If validation or durable persistence
  fails, do not create the remote issue.
- Never send the declaration in provider fields, body, comments, process arguments, shell-visible output,
  or diagnostics. It cannot grant identity, access, approval, or a relationship.
- After observing the provider's native issue ID, associate that ID with the internal declaration
  without merging their revision histories. Do not associate by title/body similarity.
- A missing/ambiguous remote result does not imply failure or permission to retry. Preserve an uncertain
  operation state and require explicit reconciliation; never repeat create automatically.

**Acceptance proposal:** no declaration produces the existing remote request; invalid declaration or
unavailable durable storage produces no remote write. A successful create sends no declaration to the
provider and associates only the observed native ID. A lost response remains uncertain and a retry is
not automatic.

**Not yet specified for implementation:** protected input surface, private-store location/format,
reconciliation command, and safe caller-visible representation of a post-create pending association.
These require a focused architecture decision; this proposal selects no flag, storage engine, or API.


### RF-WI.6: GitHub Project defaults and dynamic issue fields
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-WI.1, RF-CFG.3

- GitHub issue creation accepts repeated `--project-field NAME=VALUE`. The fields resolve against the single effective Project profile; an explicit `--project` may select that profile only when its discovered title matches. Zero, multiple, or mismatched Projects fail before issue creation.
- An in-scope GitHub issue defaults profile can supply assignees, labels, Project membership, and Project field values. Project membership and its fields apply only when the resolved repository exactly matches an allowlisted repository path. Explicit assignees replace defaults; labels are the configured-first ordered de-duplicated union with explicit labels; explicit field values override configured values.
- `defaults.github.issue.labelCandidates` limits the repository labels supplied to `DecisionModel` for `@auto` during issue create/edit. `defaults.github.issue.project.autoSelectFields` opts in to dynamic selection for exactly the listed Project fields during issue creation. Only single-select and iteration fields are model-selectable; their discovered options are sent together with title/body and any `@auto` label candidates in one model request. Existing configured or explicit values suppress selection of their matching fields. Scores below 0.8 leave a field unset; a tied best score fails before issue creation. Model/configuration/output errors also fail before issue creation.
- Project schema and values are discovered dynamically through the authenticated GitHub GraphQL interface. Supported direct assignments: text, number, date, single-select option, and iteration. Unknown fields, unsupported types, invalid values, and unknown options fail before issue creation.
- After issue creation, Project membership and each field are applied serially. These remote writes are not transactional. A later failure returns generic `partial_success` details identifying the created issue and completed/pending operations; workctl never retries the create automatically.
- GitHub issue editing accepts repeated `--project-field NAME=VALUE` and `--clear-project-field NAME` against the in-scope configured Project. Validate all fields before writes; reject duplicate assignments, set/clear overlaps, incompatible Project removal, and mismatched explicit Project titles. Do not reapply creation defaults or automatic field selection, or automatically add membership. Field-only changes skip the issue mutation. Combined changes write the issue first and fields serially; a failure after any completed issue or field write returns `partial_success` with completed/pending operations and no retry. Preserve issue timestamp guards; Project fields have no issue-revision concurrency guarantee.

**Acceptance:** Typed field values map to discovered field IDs/options/iterations; configured defaults and CLI overrides yield one Project operation; out-of-scope repositories do not inherit Project membership or fields; Project schema/value errors cause no issue write; automatic selection is allowlisted, combines with label classification in one request, respects explicit/configured precedence, and leaves low-confidence fields unset; ties and model failures cause no issue write; post-create failures report safe partial-success details without leaking provider diagnostics or retrying.
**Verification:** Isolated CLI integration tests for field types, precedence, candidate filtering, selection threshold/tie, pre-write rejection, and partial failure; `project_edit_*` and `project_field_only_edit_reports_completed_operations_on_later_failure` exercise edit validation, scope, guards, field-only writes, clear operations, and serial partial failure.
Selection regressions `unique_best_option_wins_over_lower_ties_in_every_order` and `tied_maximum_is_rejected_in_every_order` verify that only a tie at the final maximum is ambiguous; lower-scoring ties do not suppress a unique winner, regardless of option order.


### RF-PR.1: Create a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr create` requires a nonblank `--title` and accepts an optional body from `--body` or `--body-file FILE`, where `-` reads standard input.
- `--base`, `--head`, and `--draft` select merge target, source branch, and draft state; `gh` defaults apply when omitted. `--closes NUMBER` may be repeated and adds one trailing `Closes #NUMBER` line per issue.
- Optional repeated metadata flags are `--assignee`, `--label`, `--reviewer`, `--project`, plus `--milestone`.
- `--milestone @current` assigns the open GitHub milestone with the nearest due date today or later. Overdue and undated milestones are ignored. If no milestone qualifies, or two milestones tie for the nearest eligible due date, creation fails with `invalid_input` before any write.
- Omitting `--milestone` leaves the milestone unset and performs no milestone lookup, so no milestone is ever assigned implicitly. Any other value is a literal milestone name passed through unchanged. PR updates never apply a default; omitted update metadata remains unchanged.
- `--attach FILE[#ALT]` may be repeated and uses documented `gh pr create --attach`; GitHub CLI 2.99.0 or newer is required only when attachment is requested. On an attachment-create failure, return `attachment_create_uncertain` because the PR may already exist.
- The command returns the created pull request as a full record (see RNF-DOM.1).
- Errors: `invalid_input` for blank title, body source conflict, invalid body text, or `@current` without an eligible or untied nearest milestone; `dependency_version` for attachments with older `gh`; `attachment_create_uncertain` when attachment create fails; `github_cli` for other `gh pr create` failures; `provider_response` for a created URL with no pull-request number.

**Acceptance:** Create forwards explicitly requested metadata and attachment flags, preserves title and supplied body (including `--closes` suffix), and returns the created pull request; omitting `--milestone` makes no milestone request, `--milestone @current` without an eligible or untied nearest milestone fails with `invalid_input` before the write, and attachments with `gh` <2.99.0 fail before PR creation. A blank title or body-source conflict fails before `gh`.
**Verification:** Integration tests for body/metadata arguments, attachment routing, minimum version gate, and result retrieval.

### RF-PR.2: List pull requests
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr list` returns summaries without bodies, truncated to the requested limit.
- `--state open|closed|merged|all` defaults to `open`; `--limit` defaults to 30 and accepts 1–1000.
- Filters pass through as fixed flags: `--label` (repeatable), `--assignee`, `--author`, `--base`, `--head`, `--search`, and `--draft`. Blank filter or label values are rejected; no raw provider arguments can be injected.
- Each summary carries number, title, normalized `open|closed|merged` state, draft flag, URL, base ref, head ref, and `updated_at`.
- Errors: `invalid_input` for blank filter or label values; `github_cli` when the `gh pr list` call fails; `provider_response` for malformed summaries. Invalid `--state`/`--limit` values are usage errors.

**Acceptance:** The output contains at most the requested limit and omits body fields; supplied filters appear in the `gh pr list` invocation; blank filters and invalid state/limit values fail before invoking `gh`.
**Verification:** Integration test `list_forwards_filters_and_normalizes_summaries`.

### RF-PR.3: View a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr view NUMBER` returns the selected pull request with its body and normalized `open|closed|merged` state.
- `NUMBER` must be a positive integer; a number that is not a pull request is rejected.
- The record includes draft flag, base/head refs, author, `created_at`/`updated_at`/`merged_at`, `mergeable`, `review_decision`, labels, and assignees (see RNF-DOM.1).
- Errors: `not_pull_request` when `gh pr view` reports that the number is not a pull request; `provider_response` for a malformed payload; `github_cli` when the call fails.

**Acceptance:** Valid pull request details are returned; zero or malformed numbers fail before `gh`; a non-pull-request number produces `not_pull_request` without leaking provider stderr.
**Verification:** Integration test `view_returns_a_pull_request_or_reports_it_is_not_one`.

### RF-PR.4: Print a pull request diff
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr diff NUMBER` prints the pull request's unified diff; `--name-only` prints only the names of changed files.
- JSON output wraps the patch as `{ "number", "diff" }`; `--format text` prints the raw diff.
- Errors: `github_cli` when the `gh pr diff` call fails.

**Acceptance:** The JSON `diff` field and the text output both contain the patch returned by `gh pr diff`; `--name-only` adds the corresponding flag.
**Verification:** Integration test `diff_prints_the_patch_as_json_or_raw_text`.

### RF-PR.5: Report pull request checks
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr checks NUMBER` returns check runs as an array of `{name, state, bucket, description, link, workflow}`; `--required` limits the report to the checks the repository requires.
- A pull request with no checks returns an empty array, not an error, even though `gh pr checks` exits non-zero and writes `no checks reported` to stderr.
- Failing or pending checks are reported as data even though `gh pr checks` exits non-zero.
- Errors: `github_cli` when the `gh pr checks` call fails or its report cannot be parsed; the one exception is stderr containing `no checks reported`, which yields an empty array instead.

**Acceptance:** A failing report parses into checks with lowercased states; `no checks reported` yields an empty array; `--required` appears in the invocation.
**Verification:** Integration test `checks_parse_failing_reports_and_treat_missing_checks_as_empty`.

### RF-PR.6: Submit a review
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr review NUMBER` requires exactly one of `--approve`, `--request-changes`, or `--comment`; an optional review body comes from `--body` or `--body-file FILE`, where `-` reads standard input.
- The result is `{ "number", "event" }` with event `approve`, `request_changes`, or `comment`.
- Only a top-level review event is submitted; inline line comments are not supported.
- Errors: `invalid_input` for `--body` combined with `--body-file` or unreadable/oversized/non-UTF-8 review text; `github_cli` when the call fails. Omitting the event is a usage error before `gh`.

**Acceptance:** Each event maps to the matching `gh pr review` flag and reports its event name; a review body travels on stdin; providing no event fails before `gh`; `--body` with `--body-file` fails with `invalid_input`.
**Verification:** Integration tests `review_merge_ready_close_and_reopen_use_the_gh_commands` and `review_without_an_event_is_a_usage_error`.

### RF-PR.7: Merge a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr merge NUMBER` merges the pull request; `--method merge|squash|rebase` selects the method, `--delete-branch` deletes the local and remote branch after merging, and `--auto` queues the merge once the repository requirements are met.
- `--method` is passed to `gh` only when given: workctl applies no default merge policy, so an explicit method is required whenever `gh` cannot infer one.
- The result is `{ "number", "method": null|"merge"|"squash"|"rebase", "auto" }`.
- Errors: `github_cli` when the call fails.

**Acceptance:** `--method squash --delete-branch --auto` invokes `gh pr merge` with those flags and reports the method and auto state; omitting `--method` passes no method flag to `gh`.
**Verification:** Integration test `review_merge_ready_close_and_reopen_use_the_gh_commands`.

### RF-PR.8: Edit a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr edit NUMBER` accepts a title change, one body change, a base-branch change, label/reviewer/assignee/project additions and removals, a milestone set/removal, attachments, or any combination; at least one change is required.
- Body changes reuse the same set and shared resolver as `issue edit`: `--body`/`--body-file` replaces, `--append-body`/`--append-body-file` appends, `--replace-section` replaces one ATX section, and `--patch-file` applies a unified diff. At most one body change may be supplied; `--replace-section` requires its section body.
- `--add-label`/`--remove-label`, `--add-reviewer`/`--remove-reviewer`, `--add-assignee`/`--remove-assignee`, and `--add-project`/`--remove-project` may be repeated. `--milestone NAME` sets a literal milestone and `--milestone @current` resolves the nearest eligible open milestone exactly as create does, failing with `invalid_input` when none qualifies or the nearest dates tie; `--remove-milestone` removes it. `--attach FILE[#ALT]` may be repeated. GitHub PR edit accepts native `-t`, `-b`, `-F`, and `-m` shortcuts; `--clear-milestone` is removed without an alias.
- Metadata and attachments are optional; `workctl` does not infer values or apply defaults. Attachments require GitHub CLI 2.99.0 or newer, checked only when requested.
- The pull request is fetched and validated before writing; `--expect-updated-at TIMESTAMP` fails with `conflict` when `updated_at` differs.
- The updated pull request is returned as a full record; a target that is not a pull request fails with `not_pull_request` before any write.
- Errors: `invalid_input` for no change, blank values, conflicting/incomplete body changes; `dependency_version` for attachments with older `gh`; `not_pull_request`; `conflict`; `patch_conflict`; `section_not_found`; `github_cli`.

**Acceptance:** Body changes and metadata reach `gh pr edit` with no implicit fields; attachment-only edits work with supported `gh`; stale timestamps, non-applying patches, non-PR targets, and no-field updates fail without writes.
**Verification:** Integration tests for body mutation, metadata argument forwarding, attachment-only updates, version gating, and concurrency guards.

### RF-PR.9: Mark a pull request ready or draft
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr ready NUMBER` marks the pull request ready for review; `--undo` converts it back to a draft.
- The result is `{ "number", "draft" }`, where `draft` is true only with `--undo`.
- Errors: `github_cli` when the call fails.

**Acceptance:** `pr ready 42` and `pr ready 42 --undo` invoke `gh pr ready` with and without `--undo` and report the resulting draft state.
**Verification:** Integration test `review_merge_ready_close_and_reopen_use_the_gh_commands`.

### RF-PR.10: Close a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr close NUMBER` closes the pull request; `--comment TEXT` leaves a closing comment and `--delete-branch` deletes the local and remote branch after closing.
- The result is `{ "number", "state": "closed" }`.
- Errors: `github_cli` when the call fails.

**Acceptance:** `pr close 42 --comment bye --delete-branch` invokes `gh pr close` with the comment and branch deletion and reports `state: closed`.
**Verification:** Integration test `review_merge_ready_close_and_reopen_use_the_gh_commands`.

### RF-PR.11: Reopen a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr reopen NUMBER` reopens the pull request; `--comment TEXT` adds a reopening comment.
- The result is `{ "number", "state": "open" }`.
- Errors: `github_cli` when the call fails.

**Acceptance:** `pr reopen 42 --comment back` invokes `gh pr reopen` with the comment and reports `state: open`.
**Verification:** Integration test `review_merge_ready_close_and_reopen_use_the_gh_commands`.

### RF-GL.1: GitLab provider and grammar
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.2

- Selecting GitLab (explicit `--provider gitlab`, `provider`/`workItemProvider`, or a `gitlab.com` origin) activates the GitLab grammar in place of the GitHub grammar.
- The GitLab grammar mirrors the `glab` CLI. Its `issue` group provides `create`, `list`, `view`, and `update`, using GitLab flag names; the `mr` group remains a later slice.
- GitHub-only verbs and flags (`edit`, `--body`, `--state`, `--limit`) are usage errors raised before any provider call. `pr` is also a usage error whose message names `mr`.
- Each valid GitLab operation authenticates with `glab auth status --hostname gitlab.com` before invoking the operation; local syntax/field validation completes first. The project reaches `glab` as a full `https://gitlab.com/GROUP[/SUBGROUP]/PROJECT` URL so the host never depends on the working directory.
- A missing or unauthenticated `glab` maps to `dependency` and `authentication`; no provider stderr reaches the user.

**Acceptance:** In a GitLab-configured root, `workctl issue --help` lists exactly the GitLab issue verbs, `workctl issue edit 1` and `workctl issue list --limit 5` are usage errors, `workctl pr list` fails with an error naming `mr`, and a missing `glab` produces the safe dependency error rather than a provider diagnostic.
**Verification:** CLI integration tests `tests/cli_gitlab.rs`; a manual run with `glab` absent for the dependency path.

### RF-GL.2: List GitLab issues
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue list` returns bounded issue summaries for the GitLab project, mapping `--closed`, `--all`, `--label`, `--assignee`, `--author`, `--milestone`, `--search`, and `--per-page` to the `glab` filters of the same name. `--all` and `--closed` are mutually exclusive; `--per-page` accepts 1 to 100, the provider's page ceiling.
- Each summary carries the issue number (the GitLab IID), title, normalized `open|closed` state, URL, and `updated_at`, and omits the description.
- The result never includes merge requests.
- Errors: `invalid_input` for a blank filter value or a `--per-page` outside its range; `gitlab_cli` when the `glab issue list` call fails; `provider_response` for a malformed page.

**Acceptance:** Supplied filters appear in the `glab` invocation; the output contains only issues, omits the body, and honors `--per-page`; an invalid `--per-page` fails before `glab` runs.
**Verification:** Integration tests `gitlab_list_forwards_every_filter_to_glab`, `gitlab_state_flags_select_the_requested_state`, and `gitlab_grammar_rejects_github_flags_and_conflicts`.

### RF-GL.3: View a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue view NUMBER` returns the shared issue record: the GitLab `iid` as `number`, `description` as `body` (a missing description is an empty body), the state normalized to `open|closed`, and `title`, `url`, `created_at`, and `updated_at` as `glab` reports them. GitLab-only fields (`confidential`, `weight`, `due_date`, `labels`, `assignees`, `author`) are not part of the record.
- Zero or malformed numbers are usage errors before `glab` runs.
- Errors: `gitlab_cli` when the read fails, with the provider's stderr withheld.

**Acceptance:** A valid issue returns exactly the shared record shape; zero or malformed numbers are usage errors; a failed read reports `gitlab_cli` without leaking provider stderr.
**Verification:** Integration tests `gitlab_view_maps_the_description_onto_the_body`, `gitlab_rejects_an_invalid_issue_number_before_glab`, and `gitlab_failures_report_stable_codes_without_provider_output`.

### RF-GL.4: Create a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue create` requires a nonblank `--title` and an explicitly supplied `--description` or `--description-file`; it never prompts. Description data is passed as UTF-8 on `glab` stdin, never in argv; a description consisting only of `-` is rejected because `glab` reserves it for editor behavior.
- Native optional fields are `--label` (repeatable), `--assignee` (repeatable plain usernames), `--milestone`, `--confidential`, `--weight` (including zero), and `--due-date` (valid `YYYY-MM-DD`). Because `glab issue create --weight=0` omits the zero value, `workctl` explicitly follows creation with `glab issue update --weight=0` when requested.
- On success, the CLI validates the created issue URL against the selected project, reads the created IID through `glab issue view`, and returns the shared `Issue`.
- A provider write or post-write confirmation failure returns `gitlab_write_uncertain`; raw provider diagnostics are withheld.

**Acceptance:** A fixture observes the documented native arguments and exact stdin bytes; successful create returns the read-back issue; failed or unconfirmable creates do not leak provider output.
**Verification:** Integration tests `rf_gl_4_gitlab_create_uses_native_flags_and_stdin_description` and `gitlab_write_uncertainty_hides_diagnostics_and_rejects_untrusted_create_url`.

### RF-GL.5: Update a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue update NUMBER` accepts any nonempty combination of `--title`, `--description`/`--description-file`, `--label`, `--unlabel`, `--assignee`, `--unassign`, `--milestone`, `--confidential`/`--public`, `--weight`, and `--due-date`; an empty update fails before provider access.
- Description input is sent as UTF-8 stdin, not argv. Empty descriptions are rejected because `glab issue update` does not clear the description with an empty value.
- Milestone empty string or `0` clears the milestone; weight zero is preserved on update; assignee +/-/! prefixes use GitLab's relative assignment semantics.
- A successful write is followed by `glab issue view` and returns the shared `Issue`. A failed write or read-back returns `gitlab_write_uncertain` with no raw diagnostics.

**Acceptance:** Fixture assertions cover native mutation semantics, exact description bytes, read-back, empty-update rejection before `glab`, and safe errors after an uncertain write.
**Verification:** Integration tests `rf_gl_5_gitlab_update_uses_native_flags_and_reads_back_the_issue` and `gitlab_writes_reject_empty_or_invalid_changes_before_authentication`.

### RF-CFG.1: Project configuration
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- Both optional files are discovered at the Git worktree root and validated on every command invocation, even when CLI flags override selected values. Local top-level fields, including the entire `defaults` object, replace the corresponding shared fields.
- The strict schema contains `provider`, `workItemProvider`, and `defaults`; provider values are `github` or `gitlab`. Files must be regular, non-symlink files no larger than 64 KiB. Unknown keys and invalid JSON fail closed.
- `.workctl.local.json` is gitignored. Old `.mcp-tracker*.json` files are not read or migrated.
- No configuration file is required when the provider and repository can be resolved from flags or the Git remote.

**Acceptance:** Local provider fields override shared fields; malformed, unknown, oversized, symlink, or non-regular configuration yields a safe JSON error even when flags provide the effective provider/repository; legacy filenames have no effect.
**Verification:** Configuration unit tests using temporary Git roots.

### RF-CFG.2: Provider and repository resolution
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1

- Provider precedence: explicit `--provider` > `workItemProvider` > `provider` > known Git remote host.
- Provider resolution runs before the command grammar is parsed and needs no network: project configuration plus the `origin` remote URL.
- Repository precedence: explicit `--repo` > repository parsed from `origin` remote.
- `--repo` accepts `[HOST/]OWNER[/...]/REPO`. A host, when present, must agree with the resolved provider; nested path segments are accepted for providers that use them.
- GitHub and GitLab resolve from configuration, remote host, or `--provider`. Any other host or provider fails explicitly and never falls back to a supported provider.
- When outside a Git worktree, an explicit provider and repository are required.

**Acceptance:** HTTPS and SSH GitHub and GitLab remotes resolve to the same repository path; an unknown host, an origin remote that belongs to another provider than the selected one, a `--repo` host that disagrees with the resolved provider, and a missing scope fail closed; explicit overrides take precedence; `group/subgroup/project` is accepted for GitLab and rejected for GitHub.
**Verification:** Unit tests for resolution and remote parsing.

### RF-CFG.3: GitHub issue defaults and label candidates
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `defaults.github.issue` accepts `assignees`, `labels`, `labelCandidates`, and optional `project`.
- `project` contains a canonical GitHub Project URL, exact allowed repository paths, and string-valued custom field defaults. Project membership and fields apply only when the resolved repository is allowlisted.
- `labelCandidates` is an optional allowlist for the labels passed to DecisionModel when `@auto` is requested. Candidates must exist in the repository; an omitted or empty list uses the full repository catalog.
- Explicit `--assignee` values replace default assignees. Default labels precede explicit labels, with duplicates removed. Explicit `--project-field NAME=VALUE` overrides the configured value of the same field.
- When `.workctl.local.json` sets `defaults`, it replaces the shared defaults object as a whole; nested maps are not deep-merged.

**Acceptance:** Strict config rejects unknown keys and malformed project profiles; repository scope prevents defaults crossing repository boundaries; field and label-candidate precedence follows the rules above.
**Verification:** Config and CLI integration tests with isolated worktree roots.

### RF-OUT.1: Output contract
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CLI.1

- Success output is compact JSON by default; `--format text` selects human-readable output.
- Text output escapes terminal control characters in provider values; issue-body newlines and tabs remain layout characters.
- Errors are one JSON object on stderr with a stable `code` and safe `message`; partial success may include structured `details` describing the affected resource and completed/pending operations. All failures exit nonzero and leave stdout empty.
- User-facing errors do not include raw provider stderr, credentials, stack traces, or internal paths. Partial-success error types remain provider-neutral.

**Acceptance:** Success and failure tests assert output stream, format, and exit status; raw fixture stderr never appears in the error response.
**Verification:** CLI integration tests.

## 2. Non-functional requirements

### RNF-SEC.1: Safe external command boundary
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- GitHub operations invoke the authenticated `gh` CLI with argument arrays; no shell is used.
- GitLab operations invoke the authenticated `glab` CLI the same way: one argument array, the project as a full URL, and the issue number as its own argument.
- GitLab issue descriptions travel on stdin to `glab`; they are never embedded in process arguments. Reads through `gh api` and `glab issue` send no request body.
- `workctl` never reads, stores, or logs provider tokens. It does not download `gh`, `glab`, or update installed toolchains automatically.

**Acceptance:** Hostile shell-like title/body text is preserved as data; code review confirms no shell invocation or token access.
**Verification:** Payload-safety test and source review.

### RNF-SEC.2: DecisionModel credential and data boundary
**Priority:** Must Have | **Status:** In Progress | **Dependencies:** RF-WI.1, RF-WI.4

- Model requests occur only when issue create/edit includes `@auto` in an existing label argument. Other code-host operations do not send work-item text to a model.
- The DecisionModel package receives provider-neutral work-item title/description and candidate labels; it excludes credentials, comments, URLs, and unrelated code-host metadata.
- `DECISION_MODEL_API_KEY` is read only for the selected hosted Jev, Laya, or GLiDE adapter, sent only to its fixed HTTPS endpoint, and never stored, logged, or included in errors.
- `DECISION_MODEL_BASE_URL` configures the OpenAI-compatible local LLM service and must be loopback. Local adapters do not receive hosted credentials.
- Requests and responses are bounded at 2 MiB and 1 MiB. Redirects are disabled. Model failures and malformed/incomplete label scores fail closed with safe errors; no retries occur.
- Generic LLM scores are generated estimates and are not represented as calibrated model-native probabilities.

**Acceptance:** No model/config/key read occurs without `@auto`; missing hosted credentials fail before code-host access; model inputs exclude unrelated metadata; local non-loopback URLs, oversized payloads, redirects, and invalid scores are rejected without a label write.
**Verification:** Built-binary isolated code-host fixture, local OpenAI-compatible HTTP fixture, and adapter HTTP tests.

### RNF-EXT.1: Bounded subprocess execution
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- One absolute 30-second deadline covers child completion and all pipe/input worker completion; a still-running direct child is killed at expiry.
- Captured stdout and drained stderr each have an 8 MiB upper bound; timeout and output-limit failures map to safe structured errors.

**Acceptance:** A hanging child or inherited pipe is reported as timeout; oversized stdout/stderr does not grow memory without bound.
**Verification:** Process-runner tests.

### RNF-DOM.1: Validated provider responses
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- Provider JSON is parsed into typed issue and pull request structures; required fields and allowed state values are validated.
- GitHub state casing is normalized to `open|closed` for issues and `open|closed|merged` for pull requests.
- A pull request record carries the draft flag (`isDraft`), `mergeable`, and `review_decision`; these optional provider fields are lowercased and empty strings dropped. Summaries exclude the body.
- Check runs are parsed into typed records with a lowercased state, a `bucket`, and optional `description`, `link`, and `workflow`.

**Acceptance:** Malformed JSON, missing required fields, and unknown states fail explicitly; valid provider records serialize with stable CLI field names.
**Verification:** Provider mapping tests.

### RNF-DIST.1: Native binary
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- The project builds as a Rust stable binary named `workctl`; `Cargo.lock` records resolved dependencies.
- The CLI parser uses the latest stable `clap` release selected for the implementation.

**Acceptance:** `cargo build --release` produces an executable `workctl` and `workctl --version` succeeds.
**Verification:** Release build and binary smoke run.

### RNF-TST.1: Isolated behavior verification
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-WI.1, RF-WI.2, RF-WI.3, RF-WI.4, RF-PR.1, RF-PR.2, RF-PR.3, RF-PR.4, RF-PR.5, RF-PR.6, RF-PR.7, RF-PR.8, RF-PR.9, RF-PR.10, RF-PR.11

- Tests exercise consumer-visible CLI output and provider boundaries without requiring live network access or mutating a real repository; Jev calls use a local HTTP fixture.
- At least one built-binary smoke run exercises a successful command and a failure path.

**Acceptance:** `cargo test` passes and the smoke run observes the expected stdout/stderr and exit status.
**Verification:** Focused Cargo tests and isolated smoke fixture.

## 3. Non-goals for v0

- Jira, local Markdown tracking, provider plugin systems, and project/board administration. Add/remove membership of an existing project is supported through GitHub CLI metadata flags.
- Branch creation, checkout, or standalone deletion: `git` owns branch lifecycle. `--head`/`--base` select existing branches, and `--delete-branch` asks `gh` to remove a branch only after a merge or close.
- Workflow/status transitions and label/assignee/milestone *administration*. Explicit assignment/removal of existing issue and PR metadata is supported; `workctl` does not infer or require project-specific values.
- Inline review comments: `pr review` submits one top-level approve, request-changes, or comment, and `pr close`/`pr reopen` accept a single comment.
- Listing the pull requests linked to an issue.
- Relationships, checklists, and issue-comment threads.
- Attachment listing, removal, or download; upload to issues/PRs is through `gh` 2.99.0 or newer.
- Issue deletion (there is no delete command for issues or pull requests), interactive prompts, MCP transport, arbitrary HTTP integrations, generalized token management, automatic retries, telemetry, and pull-request automatic label assignment.
- Three-way merge or fuzzy patch application: a patch either matches the fetched body exactly or fails.

## 4. Glossary

- **Issue:** GitHub issue, excluding pull requests.
- **Pull request:** GitHub pull request addressed by number, with normalized `open|closed|merged` state, a separate draft flag, and merge/review metadata.
- **Check run:** one status check reported for a pull request's head commit; it carries a lowercased state, a `bucket` (`pass`, `fail`, `pending`, `skipping`, or `cancel`), and optional description, link, and workflow.
- **Review event:** the top-level review outcome submitted by `pr review` — `approve`, `request_changes`, or `comment`.
- **Merge method:** how a pull request is merged — `merge`, `squash`, or `rebase` — passed to `gh` only when the caller supplies `--method`.
- **Draft:** a pull request explicitly marked not ready for review; `pr ready` clears the flag and `pr ready --undo` sets it.
- **Repository target:** `owner/repo`, selected explicitly or from the Git `origin` remote.
- **Worktree config:** optional project configuration found at the Git root; local config is untracked and overrides the shared file.
