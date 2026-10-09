# SRS: workctl v0

> Vision: [docs/product/vision.md](./product/vision.md)
> Architecture: [docs/architecture.md](./architecture.md)

Actors: a developer or coding agent running `workctl` locally. Observable behavior is defined at the CLI boundary: arguments, stdout, stderr, exit code, and remote GitHub issue state.

## 1. Functional requirements

### RF-CLI.1: Command surface
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- GitHub issue commands are `create|list|blockers|view|edit|close|reopen|comment|lock|unlock`; GitHub pull-request commands are `create|checkout|list|view|status|diff|checks|review|merge|edit|ready|close|reopen|comment|lock|unlock|revert|update-branch`.
- Provider selection is independent by command domain and happens before command grammar parsing. `issue` uses the work-item provider; `pr` and `mr` use the code-host provider.
- `--code-provider` overrides the code-host provider and `--work-item-provider` overrides the work-item provider. Legacy `--provider` applies to both domains unless the corresponding domain-specific flag is present.
- For each domain, precedence is domain-specific CLI flag > legacy `--provider` > domain-specific configuration > legacy `provider` configuration > known Git origin host. Unsupported providers fail closed.
- Each provider owns a static grammar. Native operations mirror that provider's CLI, with GitHub-only read diagnostics `blockers`; another provider's verb or flag is a usage error rather than a runtime rejection.
- Top-level `--help` is provider-neutral and states the resolution order; a subcommand's help reflects its resolved domain provider's grammar.
- `--repo OWNER/REPO`, `--provider`, `--code-provider`, `--work-item-provider`, and `--format json|text` are global overrides accepted before or after the group. `--repo` supplies scope to the selected command only.
- Help and version work without `git`, `gh`, or `glab` installed.
- Every flag carries a help description, and each subcommand prints usage examples for behavior a caller cannot infer from flag names — for `issue edit` and `pr edit`, the body-change set and exact-match patch contract; for `pr list`, filter composition; for `pr merge`, when an explicit method is required; for `pr status`, the review/mergeability/required-check evidence it reports; for `pr update-branch`, default merge-commit versus explicit rebase behavior; for `pr checks --watch`, bounded waiting; and for `pr checkout`, its current-worktree effect and absence of forced checkout.
- There is no delete command for issues or pull requests in v0.

**Acceptance:** With GitHub selected for both domains, `workctl issue --help` lists exactly ten supported issue subcommands and `workctl pr --help` lists exactly eighteen supported pull-request subcommands. With GitLab selected for code and GitHub for work items, root help exposes GitHub `issue` and GitLab `mr`, and each command executes through its owning provider. An unknown command produces a structured JSON error on stderr and a nonzero exit; unsupported provider verbs/flags are usage errors. Unsupported provider configuration fails before provider access. No flag in subcommand help is printed without a description, and `workctl issue edit --help` documents `patch_conflict` and exact-context matching.
**Verification:** CLI integration test.

### RF-WI.1: Create an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

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
- GitHub `view` adds nullable `issue_type` and `parent`, plus a `sub_issues` array. Related issues carry `number`, `title`, normalized `state`, and a URL that distinguishes cross-repository references. Text output escapes controls in every provider string. Neither JSON nor text includes blocked-by/blocking relationships, and the query does not request those connections; dependency diagnostics belong to `issue blockers`.
- Read direct hierarchy only, not transitive dependencies. Paginate sub-issues at 100 per page, up to ten pages and 1,000 related issues. GraphQL errors, unavailable/missing data, malformed references, duplicate URLs, changing counts, repeated cursors, and incomplete/oversized connections fail without successful output. Reads across pages are not an atomic graph snapshot.
- Keep GitHub create/edit/list and GitLab output contracts unchanged. Mutation guards continue to use the basic issue read, without relationship queries.

**Acceptance:** Valid details preserve issue body and hierarchy, cross-repository URLs, observed null/empty hierarchy, and the last page, without blocker fields or dependency queries. Invalid issue identifiers, pull requests, malformed or unavailable hierarchy data, and incomplete pagination produce structured errors without partial stdout or provider diagnostics.
**Verification:** `tests/cli_issues.rs` includes `view_exposes_native_relationship_direction_and_safe_text`, `view_paginates_each_relationship_connection`, and `view_rejects_incomplete_relationship_graphs_without_leaking_diagnostics`; native edit and GitLab suites preserve unaffected outputs.

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

### RF-WI.7: Print compact open-blocker chains
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-WI.3, RF-PRV.1

- GitHub `issue blockers NUMBER` returns only complete paths of currently open blocking issues leading to the target. Arrow direction is blocker-to-blocked, e.g. `#18 -> #29 -> #30`. Cross-repository references use `OWNER/REPO#NUMBER`; same-repository references use `#NUMBER`.
- Closed targets return no chains. Closed blockers stop propagation and are omitted. Parent/sub-issue relations are not traversed. An empty result means no open blocker chain was observed; it does not assert that other criteria for execution are satisfied.
- Default JSON output is an array of chain strings, with `[]` when empty. Text output prints one chain per line and emits no bytes when empty. No titles, bodies, relation objects, or successful status boilerplate are included.
- Explore only direct `blockedBy` edges. Bound the complete traversal to 20 issue reads, 500 edges, 20 returned chains, and depth 20, including pagination queries in the read budget. Connections page at 100 and each is limited to 1,000 records. Exceeding a bound returns `relationship_limit` with no partial output; provider/authentication errors and malformed/cyclic responses also fail without successful output.
- Sort complete chains deterministically. Reads span multiple requests and are not an atomic snapshot; conflicting observed issue states fail with `conflict`.

**Acceptance:** A chain and multiple branches are emitted compactly in blocker-to-target order, including cross-repository identity; closed blockers/targets produce no open chain; no-chain text is empty and JSON is `[]`; failures, cycles, and limits never produce successful partial chains or provider diagnostics.
**Verification:** `tests/cli_issues.rs` covers chained/branched and cross-repository results, empty JSON/text, closed targets and blockers, bounded traversal, cycles, and provider failure.

### RF-WI.8: Close an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `issue close NUMBER` closes the issue through `gh issue close NUMBER --repo OWNER/REPO`.
- `--comment TEXT` leaves a closing comment. `--reason` accepts only `completed`, `not planned`, or `duplicate`; any other value is a usage error before provider access. `--duplicate-of` accepts a positive issue number or a canonical `https://github.com/OWNER/REPO/issues/NUMBER` URL.
- Only the explicitly supplied options are forwarded as `gh` flags; no reason or duplicate reference is inferred.
- On success, return `{ "number", "state": "closed" }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `issue close 12 --comment Fixed --reason completed` and `issue close 12 --duplicate-of 9` each forward exactly the supplied native flags and report `state: closed`; an unsupported `--reason` value fails during argument parsing before any provider call; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_issues.rs` covers flag forwarding, the result state, the invalid-reason rejection, and safe provider failure.

### RF-WI.9: Reopen an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `issue reopen NUMBER` reopens the issue through `gh issue reopen NUMBER --repo OWNER/REPO`; `--comment TEXT` adds a reopening comment.
- On success, return `{ "number", "state": "open" }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `issue reopen 12 --comment Reopened` forwards the comment and reports `state: open`; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_issues.rs` covers flag forwarding, the result state, and safe provider failure.

### RF-WI.10: Comment on an issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `issue comment NUMBER` adds one top-level comment through `gh issue comment NUMBER --repo OWNER/REPO --body-file -`.
- Comment text comes from exactly one of `--body TEXT` or `--body-file FILE`, where `-` reads standard input. Omitting both, or supplying both, is a usage error before provider access. Blank text is `invalid_input`.
- The body travels on stdin, never in process arguments, and text read from a file or stdin is capped at 1 MiB.
- On success, return `{ "number", "target": "issue" }`; text output states that the issue was commented.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics. The command reads no existing comment thread.

**Acceptance:** `issue comment 12 --body-file -` sends the supplied text to `gh` on stdin and reports the issue as the target; a missing or blank body fails before provider access; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_issues.rs` covers stdin delivery, the result shape, missing-body rejection, and safe provider failure.

### RF-WI.11: Lock an issue conversation
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `issue lock NUMBER` locks the conversation through `gh issue lock NUMBER --repo OWNER/REPO`.
- `--reason` accepts only `off_topic`, `resolved`, `spam`, or `too_heated`; any other value is a usage error before provider access. An omitted reason forwards no `--reason`.
- On success, return `{ "number", "target": "issue", "locked": true }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `issue lock 12 --reason resolved` forwards the reason and reports `locked: true`; an unsupported reason fails during argument parsing before any provider call.
**Verification:** `tests/cli_issues.rs` covers reason forwarding, the lock result, the invalid-reason rejection, and safe provider failure.

### RF-WI.12: Unlock an issue conversation
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-PRV.1

- `issue unlock NUMBER` unlocks the conversation through `gh issue unlock NUMBER --repo OWNER/REPO` with no additional flags.
- On success, return `{ "number", "target": "issue", "locked": false }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `issue unlock 12` forwards only the repository context and reports `locked: false`; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_issues.rs` covers the exact invocation, the unlock result, and safe provider failure.


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
- GitHub issue editing accepts repeated `--project-field NAME=VALUE` and `--clear-project-field NAME` against the in-scope configured Project. Validate all fields before writes; reject duplicate assignments, set/clear overlaps, incompatible Project removal, and mismatched explicit Project titles. Do not reapply creation defaults or configured `autoSelectFields`, or automatically add membership. Field-only changes skip the issue mutation. Combined changes write the issue first and fields serially; a failure after any completed issue or field write returns `partial_success` with completed/pending operations and no retry. Preserve issue timestamp guards; Project fields have no issue-revision concurrency guarantee.
- `issue edit --project-field NAME=@auto` explicitly selects only that single-select or iteration field. One model request per target uses the final proposed title/body, discovered options, and any automatic label candidates. A unique best score >= 0.8 plans a field write; a lower score or tied best score plans no write for that field and preserves its current value. Reject unsupported/unknown fields and model errors before writes. Recheck the issue revision before mutation; reset automatic field choices between batch targets. If no field or other change is selected, return the current issue without writes.

**Acceptance:** Typed field values map to discovered IDs/options/iterations; creation defaults and CLI overrides yield one Project operation only in scope. Create automatic selection is allowlisted and respects explicit/configured precedence; create ties fail. Edit automatic selection touches only explicitly named fields, uses final text, preserves current values on low confidence/ties, and isolates choices between targets. Both paths combine automatic labels and options in one request per issue, reject schema/model failures before writes, and report non-retriable partial success after completed remote writes.
**Verification:** Isolated CLI integration tests for field types, precedence, candidate filtering, selection threshold/tie, pre-write rejection, and partial failure; `project_edit_*` and `project_field_only_edit_reports_completed_operations_on_later_failure` exercise edit validation, scope, guards, field-only writes, clear operations, and serial partial failure.
Selection regressions `unique_best_option_wins_over_lower_ties_in_every_order` and `tied_maximum_is_rejected_in_every_order` verify that only a tie at the final maximum is ambiguous; lower-scoring ties do not suppress a unique winner, regardless of option order.
Edit regressions `project_auto_edit_selects_only_requested_fields_and_preserves_uncertainty`, `project_auto_edit_fails_closed_before_writes`, and `edit_choices_reset_between_targets_and_preserve_manual_assignments` verify explicit scope, final text, label/field separation, threshold/tie preservation, failure closure, and per-target selection isolation.


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

### RF-PR.14: Watch pull-request checks with a bounded wait
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-PR.5

- `pr checks NUMBER --watch` delegates waiting to native `gh pr checks --watch`. Optional `--interval` accepts 1–300 seconds and `--fail-fast` stops on the first failed check; both require `--watch`. `--watch-timeout` also requires watch and accepts 1–3600 seconds.
- Watch mode defaults to a 600-second absolute process deadline; the timeout option overrides it. Non-watch calls retain the existing process deadline. The process runner kills the direct child on deadline and returns a safe `timeout` error with no partial stdout.
- Parse valid check JSON regardless of provider exit status, retaining failing and pending reports as data. Preserve the no-checks empty-array behavior. Do not add an independent polling loop or automatically retry.
- Errors: existing `github_cli` for provider failure or invalid report, `timeout` when the watch deadline expires.

**Acceptance:** Watch forwards only requested native flags; clap rejects watch-only controls outside watch mode and values outside their bounds before provider access. Failing check JSON remains successful evidence; an expired wait emits no success JSON and exposes no provider diagnostics. Without watch, command behavior remains unchanged.
**Verification:** `tests/cli_prs.rs` covers native watch option forwarding, failing and pending report preservation, argument bounds, and safe timeout.

### RF-PR.6: Submit a review
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr review NUMBER` requires exactly one of `--approve`, `--request-changes`, or `--comment`; an optional review body comes from `--body` or `--body-file FILE`, where `-` reads standard input.
- The result is `{ "number", "event" }` with event `approve`, `request_changes`, or `comment`.
- Only a top-level review event is submitted; inline line comments are not supported.
- Errors: `invalid_input` for `--body` combined with `--body-file` or unreadable/oversized/non-UTF-8 review text; `github_cli` when the call fails. Omitting the event is a usage error before `gh`.

**Acceptance:** Each event maps to the matching `gh pr review` flag and reports its event name; a review body travels on stdin; providing no event fails before `gh`; `--body` with `--body-file` fails with `invalid_input`.
**Verification:** Integration tests `review_merge_ready_close_and_reopen_use_the_gh_commands` and `review_without_an_event_is_a_usage_error`.

### RF-PR.7: Merge a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2, RF-CFG.4

- `pr merge NUMBER` merges the pull request; `--method merge|squash|rebase` selects the method, `--delete-branch` deletes the local and remote branch after merging, and `--auto` queues the merge once the repository requirements are met.
- Method precedence: explicit `--method`, then `defaults.github.pr.mergeMethod`, then GitHub CLI inference. With no config, behavior remains unchanged.
- `defaults.github.pr.deleteBranch` is a boolean defaulting to `false`. `true` adds `--delete-branch` to every merge; the CLI's `--delete-branch` can enable deletion for one merge. There is no per-command inverse flag.
- The result is `{ "number", "method": null|"merge"|"squash"|"rebase", "auto" }`; `method` reports the effective configured/explicit method, or null when GitHub CLI chooses.
- Errors: `config` for malformed/unknown config values; `github_cli` when the merge call fails.

**Acceptance:** An explicit method overrides the configured default; an omitted method passes the configured method or otherwise leaves choice to `gh`. `deleteBranch: false` preserves existing behavior, `true` adds `--delete-branch`, and the explicit CLI flag enables deletion when the setting is false. Configuration errors fail before provider access.
**Verification:** `tests/cli_prs.rs` covers configured defaults and CLI method precedence; config unit tests cover valid/invalid values and false defaults.

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

### RF-PR.12: Summarize pull-request review and merge evidence
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-PR.3, RF-PR.5

- `pr status NUMBER` returns a compact object with pull-request number, title, state, draft flag, URL, base/head refs, nullable mergeability and review decision, and `required_checks`.
- Read status metadata with one `gh pr view --json` call without requesting the body; read checks with one `gh pr checks --required` call. Each invocation uses the existing subprocess timeout and output caps; no pagination is performed. Preserve the provider's check state/bucket/link data.
- Missing mergeability or review decision is `null`. An empty `required_checks` array means no checks were reported; it does not establish that repository policy requires none or that the PR is ready to merge. The result has no inferred readiness boolean.
- The two remote reads are sequential and not an atomic snapshot. A failure of either read returns an error without successful partial status output. A non-PR target returns `not_pull_request`; valid pending or failing check reports are evidence, not command failures.

**Acceptance:** The status contains only concise review/mergeability context and required checks, excludes the PR body, preserves failing/pending check records, and never declares readiness. Empty checks remain inconclusive; a non-PR target stops before the checks call and no partial stdout is emitted.
**Verification:** `tests/cli_prs.rs` covers JSON and text status, required-check invocation, missing checks, and non-PR failure.

### RF-PR.13: Update a pull-request branch
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr update-branch NUMBER` updates the remote pull-request branch with the latest base branch changes through `gh pr update-branch NUMBER --repo OWNER/REPO`.
- Default behavior merges the latest base branch into the pull-request branch with a merge commit. `--rebase` instead rebases the pull-request branch onto the latest base branch.
- The operation does not check out or modify a local branch and does not fetch a separate PR snapshot.
- On success, return `{ "number", "rebase" }`, where `rebase` reflects the selected strategy. Text output states the PR number and whether merge or rebase was requested.
- Errors: `github_cli` when the provider operation fails. Provider diagnostics are withheld and stdout remains empty.

**Acceptance:** The default invocation omits `--rebase`; the explicit flag reaches `gh` and selects rebase. Success reports the selected strategy; provider failure emits a structured error and no partial success output.
**Verification:** `tests/cli_prs.rs` covers default and rebase argument forwarding, text output, and safe provider failure.

### RF-PR.15: Check out a pull request branch
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr checkout NUMBER` checks out the pull request's branch in the caller's current worktree by delegating to `gh pr checkout NUMBER --repo OWNER/REPO`.
- Use GitHub CLI's default checkout behavior. Do not pass `--force` or expose branch-name, detached-HEAD, or separate-worktree options; `gh` remains responsible for branch lookup and its normal local-change protections.
- On success, JSON output is `{ "number" }`; text output states that the PR was checked out. The command changes the current local branch.
- Errors: `github_cli` when checkout fails. Provider diagnostics are withheld and stdout remains empty.

**Acceptance:** A successful checkout changes the current worktree to the PR branch; provider failure returns no partial success and no diagnostic leakage; `--force` is rejected before provider access. The exact `gh pr checkout` invocation includes repository context and never includes `--force`.
**Verification:** `tests/cli_prs.rs` runs against a temporary Git worktree, verifies its current branch after checkout, and covers safe failure and rejection of force.

### RF-PR.16: Comment on a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr comment NUMBER` adds one top-level comment through `gh pr comment NUMBER --repo OWNER/REPO --body-file -`.
- Comment text comes from exactly one of `--body TEXT` or `--body-file FILE`, where `-` reads standard input. Omitting both, or supplying both, is a usage error before provider access. Blank text is `invalid_input`.
- The body travels on stdin, never in process arguments, and text read from a file or stdin is capped at 1 MiB.
- On success, return `{ "number", "target": "pr" }`; text output states that the pull request was commented.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics. The command never edits or lists existing comments and never opens an editor or browser.

**Acceptance:** `pr comment 42 --body "Looks good"` sends the text to `gh` on stdin and reports the pull request as the target; a missing or blank body fails before provider access; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_prs.rs` covers stdin delivery, the result shape, missing-body rejection, and safe provider failure.

### RF-PR.17: Lock a pull-request conversation
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr lock NUMBER` locks the conversation through `gh pr lock NUMBER --repo OWNER/REPO`.
- `--reason` accepts only `off_topic`, `resolved`, `spam`, or `too_heated`; any other value is a usage error before provider access. An omitted reason forwards no `--reason`.
- On success, return `{ "number", "target": "pr", "locked": true }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `pr lock 42 --reason too_heated` forwards the reason and reports `locked: true`; an unsupported reason fails during argument parsing before any provider call.
**Verification:** `tests/cli_prs.rs` covers reason forwarding, the lock result, the invalid-reason rejection, and safe provider failure.

### RF-PR.18: Unlock a pull-request conversation
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr unlock NUMBER` unlocks the conversation through `gh pr unlock NUMBER --repo OWNER/REPO` with no additional flags.
- On success, return `{ "number", "target": "pr", "locked": false }`.
- Errors: `github_cli` when the provider call fails, with no success output and no provider diagnostics.

**Acceptance:** `pr unlock 42` forwards only the repository context and reports `locked: false`; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_prs.rs` covers the exact invocation, the unlock result, and safe provider failure.

### RF-PR.19: Revert a pull request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `pr revert NUMBER` opens a new pull request that reverts the merge of NUMBER through `gh pr revert NUMBER --repo OWNER/REPO`.
- `--title TEXT` sets the new title. `--body TEXT` or `--body-file FILE`, where `-` reads standard input, sets the new body; supplying both is a usage error, and the body travels on stdin rather than in process arguments. `--draft` opens the revert as a draft. No option is inferred when omitted.
- On success, return `{ "number", "pull_request" }`, where `number` is the reverted pull request and `pull_request` is the newly created one, parsed from the URL `gh pr revert` prints. Text output names both numbers.
- Creating the revert is a remote write and is never retried automatically.
- Errors: `github_cli` when the provider call fails, with no success output. A successful call whose output is not a pull-request URL returns `provider_response`.

**Acceptance:** `pr revert 42 --title "Revert feature" --body ... --draft` forwards exactly those native flags, delivers the body on stdin, and reports the new pull-request number; provider failure emits a structured error and no partial stdout.
**Verification:** `tests/cli_prs.rs` covers flag and stdin forwarding, both reported numbers, and safe provider failure.

### RF-GL.1: GitLab provider and grammar
**Priority:** Must Have | **Status:** In Progress | **Dependencies:** RF-CFG.2

- GitLab-specific host capabilities are limited to the native issue and merge-request grammar; interactive `glab issue board view` and destructive `glab issue delete` remain excluded.
- GitLab issue grammar exposes create/list/view/update/close/reopen/note/subscribe/unsubscribe. Native issue create/update fields include confidentiality, due date, epic, linked issues/MRs, time estimate/spent, weight, and templates; only provider-supported fields are valid, and destructive/interactive operations require explicit safe contracts.
- The GitLab `mr` grammar exposes list/view/diff/checkout/close/reopen/approve/revoke/rebase/subscribe/unsubscribe/todo/merge/note create/list/update/resolve/reopen/create/update/approvers/issues. Remaining `glab mr` commands are `for` (deprecated alias for `create --related-issue`) and destructive `delete`; delete is excluded absent a safe confirmation contract.
- GitLab-only issue filters and metadata (confidentiality, weight, due date, epics, iterations, linking) stay provider-specific. Unsupported GitHub capabilities are never fabricated from missing GitLab data.
- `glab issue list --group` is excluded because the selected repository grammar is project-scoped; `--epic` requires a group context and does not paginate, so it is not mapped onto a project list. These group-level queries need an explicit group-target CLI contract rather than inference from the selected project.
- Each valid GitLab operation authenticates with `glab auth status --hostname` using the selected project host before invoking the operation; local syntax/field validation completes first. The project reaches `glab` as a full HTTPS project URL, so the host never depends on the working directory.

- A missing or unauthenticated `glab` maps to `dependency` and `authentication`; no provider stderr reaches the user.

**Acceptance:** In a GitLab-configured root, `workctl issue --help` and `workctl mr --help` list only provider-native verbs, GitHub-only flags fail before provider access, `workctl pr` fails naming `mr`, and missing `glab` produces a safe dependency error.
**Verification:** CLI integration tests `tests/cli_gitlab.rs` and `tests/cli_gitlab_mr.rs`; a manual run with `glab` absent for the dependency path.

### RF-GL.12: Native GitLab merge-request actions
**Priority:** Must Have | **Status:** In Progress | **Dependencies:** RF-GL.1

- `mr approve`, `mr revoke`, `mr rebase`, `mr subscribe`, `mr unsubscribe`, and `mr todo` invoke the matching native `glab mr` action with the merge request IID and selected project URL.
- `mr merge IID` forwards GitLab-native `--auto-merge[=true|false]`, `--message`, `--rebase`, `--remove-source-branch[=true|false]`, `--sha`, `--squash`, `--squash-message`, and `--yes` only when specified. Omitting the optional booleans leaves `glab`/project defaults intact. It returns `{ "number", "action" }`, with `action` equal to `merged` or `merge queued`.
- The action commands return `{ "number", "action" }`; text output identifies the merge request and action. They do not imitate GitHub review, branch-update, or comment contracts.
- Unsupported operation names fail closed in the provider adapter; provider errors are sanitized and never emit success output.

**Acceptance:** Each action reaches its native `glab` verb with the selected IID and repository. Merge forwards only specified options and reports queued versus immediate merge correctly. Provider failure emits no success output.
**Verification:** `tests/cli_gitlab_mr.rs` covers six native actions, merge-option forwarding (including explicit false booleans), and sanitized mutation failure.



### RF-GL.13: Create a GitLab merge-request discussion
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `mr note create IID --message TEXT` creates a native discussion through `glab mr note create IID --repo PROJECT`, with the message passed on stdin rather than in process arguments.
- `mr note resolve DISCUSSION IID` and `mr note reopen DISCUSSION IID` invoke the native note subcommands. IDs accept a positive numeric note ID or a hexadecimal discussion ID/prefix of at least eight characters.
- A blank message is rejected during CLI parsing before provider access. Successful outputs identify the GitLab merge request and report `note added`, `discussion resolved`, or `discussion reopened`.
- Native `--reply`, `--file`, `--line`, `--old-line`, `--resolvable`, and `--unique` remain provider-specific. `--line`/`--old-line` require `--file`; the two line-side flags conflict; `--resolvable=false` conflicts with `--file` and `--reply`; discussion reply/file/unique targets are mutually exclusive.
- Provider failure returns a sanitized error and no success output; the write is never retried automatically.

**Acceptance:** The native command receives the selected IID and repository, exact message bytes arrive on stdin, message text is absent from process arguments, native options pass through, and invalid option combinations fail before authentication/provider access.
**Verification:** `tests/cli_gitlab_mr.rs` covers stdin/argument separation, native diff-note flags, pre-authentication conflict rejection, and resolve/reopen argument ordering.


### RF-GL.2: List GitLab issues
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue list` returns one bounded page of issue summaries for the GitLab project, forwarding native filters `--closed`, `--all`, `--label`, `--assignee`, `--author`, `--milestone`, `--search`, `--in`, `--confidential`, `--issue-type`, `--iteration`, `--not-assignee`, `--not-author`, `--not-label`, `--order`, `--sort`, `--page`, and `--per-page`. `--all` and `--closed` are both passed when supplied; `--per-page` accepts 1 to 100.
- The result never includes merge requests.
- Errors: `invalid_input` for a blank filter value or a `--per-page` outside its range; `gitlab_cli` when the `glab issue list` call fails; `provider_response` for a malformed page.

**Acceptance:** Supplied filters appear in the `glab` invocation; the output contains only issues, omits the body, and honors `--per-page`; an invalid `--per-page` fails before `glab` runs.
**Verification:** `tests/cli_gitlab.rs` covers native filters, state combinations, output mapping, invalid page limits, and provider-specific grammar.

### RF-GL.3: View a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue view NUMBER` returns the shared issue record: the GitLab `iid` as `number`, `description` as `body` (a missing description is an empty body), the state normalized to `open|closed`, and `title`, `url`, `created_at`, and `updated_at` as `glab` reports them. GitLab-only fields (`confidential`, `weight`, `due_date`, `labels`, `assignees`, `author`) are not part of the record.
- Zero or malformed numbers are usage errors before `glab` runs.
- Errors: `gitlab_cli` when the read fails, with the provider's stderr withheld.

**Acceptance:** A valid issue returns exactly the shared record shape; zero or malformed numbers are usage errors; a failed read reports `gitlab_cli` without leaking provider stderr.
**Verification:** Integration tests `gitlab_view_maps_the_description_onto_the_body`, `gitlab_rejects_an_invalid_issue_number_before_glab`, and `gitlab_failures_report_stable_codes_without_provider_output`.

### RF-GL.4: Create a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1
- `issue create` requires a nonblank `--title` and an explicitly supplied `--description` or `--description-file`; it never prompts. `glab issue create` receives an empty native `--description=` to suppress editor behavior; the actual description is patched through documented `glab api --input -` JSON-body mode, never argv. A description equal to `-` is ordinary user data.
- Native optional fields are `--label` (repeatable), `--assignee` (repeatable plain usernames), `--milestone`, `--confidential`, `--weight` (including zero), `--due-date` (valid `YYYY-MM-DD`), `--epic` (positive epic ID), `--linked-issues` (repeatable/comma-separated IIDs), `--link-type` (`relates_to`), `--linked-mr` (positive IID), `--time-estimate`, `--time-spent`, and local `--template`. These remain GitLab-specific.
- On success, the CLI validates the created issue URL against the selected project, reads the created IID through `glab issue view`, and returns the shared `Issue`.
- A provider write or post-write confirmation failure returns `gitlab_write_uncertain`; raw provider diagnostics are withheld.

**Acceptance:** A fixture observes documented `glab issue` arguments—including GitLab-only epic, link, time, and template fields—the exact JSON description sent on `glab api` stdin, and the read-back issue; invalid epic/link IIDs fail before provider access. Failed or unconfirmable writes do not leak provider output.
**Verification:** Integration tests `rf_gl_4_gitlab_create_uses_native_flags_and_stdin_description` and `gitlab_write_uncertainty_hides_diagnostics_and_rejects_untrusted_create_url`.

### RF-GL.5: Update a GitLab issue
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue update NUMBER` accepts any nonempty combination of `--title`, `--description`/`--description-file`, `--label`, `--unlabel`, `--assignee`, `--unassign`, `--milestone`, `--confidential`/`--public`, `--weight`, and `--due-date`; an empty update fails before provider access.
- Description input is sent as UTF-8 stdin through `glab api --input -`, never argv. Empty descriptions remain rejected because this CLI's native update contract does not accept an empty description value.
- Milestone empty string or `0` clears the milestone; weight zero is preserved on update; assignee +/-/! prefixes use GitLab's relative assignment semantics.
- A successful write is followed by `glab issue view` and returns the shared `Issue`. A failed write or read-back returns `gitlab_write_uncertain` with no raw diagnostics.

**Acceptance:** Fixture assertions cover native metadata semantics, exact JSON description body on stdin, read-back, empty-update rejection before `glab`, and safe errors after an uncertain write.
**Verification:** Integration tests `rf_gl_5_gitlab_update_uses_native_flags_and_reads_back_the_issue` and `gitlab_writes_reject_empty_or_invalid_changes_before_authentication`.

### RF-GL.6: Automatic GitLab issue labels
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.4, RF-GL.5, RF-CFG.2

- `issue create` and `issue update NUMBER` opt into classification with `--label @auto`. Manual labels remain additive; `@auto` is consumed by `workctl` and is never sent to `glab`. `--unlabel @auto` is rejected before provider access.
- The DecisionModel configuration is validated before any `glab` command. Without `@auto`, no model configuration lookup or label-catalog read occurs.
- The candidate catalog comes from native `glab label list --output json`, paged at 100 labels and bounded to 1,000 total labels. Invalid or duplicate catalog entries fail closed. Only returned candidate labels scoring at least 0.8 are added.
- Update classification receives the final title and description (explicit replacements or the fetched current text). The issue is fetched again after model inference; a changed `updated_at` returns `conflict` before mutation. If no label qualifies and no other update was requested, no write is issued.

**Acceptance:** Isolated CLI tests cover native catalog paging, malformed/oversized catalog rejection, model threshold/manual-label combination, no sentinel forwarding, no catalog/model access without opt-in, missing credentials before `glab`, final update text, concurrent-change rejection, and `--unlabel @auto` rejection. No test writes to a live project.
**Verification:** `tests/cli_gitlab.rs` RF-GL.6 cases, GitLab label-mapping tests, and a read-only `glab label list` smoke.


### RF-GL.7: List and view GitLab merge requests
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- GitLab's provider-selected grammar exposes `mr list` and `mr view`, never a `pr` alias. List filters use native `glab mr list` flags: state, draft, label/not-label, assignee, author, reviewer, milestone, source/target branch, search, created/deployed date, environment, order, sort, page, and per-page count.
- List results use the shared pull-request summary with IID, state, draft status, URL, target/source branches, and update timestamp; descriptions are omitted.
- View maps GitLab's `description`, `target_branch`, `source_branch`, `web_url`, and usernames into the shared pull-request record. GitHub-only mergeability and review-decision values remain null.
- Invalid or zero IIDs fail before provider access. Malformed provider JSON fails closed as `provider_response`.

**Acceptance:** `mr list` forwards native filters and omits descriptions; `mr view` normalizes GitLab state and maps native fields without inventing GitHub-only data; GitHub flags and verbs fail before `glab`.
**Verification:** `tests/cli_gitlab_mr.rs`; read-only smoke against the authorized test project.

### RF-GL.8: Close and reopen GitLab merge requests
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `mr close IID` and `mr reopen IID` invoke the corresponding native `glab mr` command after host authentication.
- The commands return the IID and resulting state (`closed` or `open`) only after the provider reports success. Provider failure returns the safe `gitlab_write_uncertain` error.
- A non-positive or malformed IID fails before authentication/provider access.

**Acceptance:** Isolated CLI tests observe exact native `mr close`/`mr reopen` invocation and the resulting state; provider errors do not emit success output or diagnostics.
**Verification:** `tests/cli_gitlab_mr.rs`.

### RF-GL.9: Print a GitLab merge request diff
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `mr diff IID` delegates to native `glab mr diff IID --repo URL --color=never`; `--raw` is forwarded only when requested.
- Output is `{ "number", "diff" }`; invalid IIDs fail before authentication, and malformed UTF-8 provider output fails as `provider_response`.
- Provider stderr remains withheld under the shared GitLab error contract.

**Acceptance:** The isolated CLI test observes the native diff invocation and proves the returned diff content is preserved.
**Verification:** `tests/cli_gitlab_mr.rs`.

### RF-GL.10: GitLab issue lifecycle, notes, and subscriptions
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `issue close` and `issue reopen` delegate to their native `glab issue` operations and report the resulting state only on success.
- `issue subscribe` and `issue unsubscribe` delegate to the native notification commands and report the completed operation.
- `issue note IID --message TEXT` creates a GitLab issue note. Since the native `glab issue note --message` interface has no stdin option, workctl uses the documented `glab api --input -` mode with the native issue-notes API path; note text is JSON on stdin, never a child-process argument.
- Issue note messages must be nonblank; invalid IIDs and messages fail before authentication.
- Provider mutation failures return a safe uncertain-write error; no provider diagnostics are emitted.

**Acceptance:** Isolated tests verify native lifecycle/subscription command names, note endpoint and exact stdin JSON, safe output, and rejection of blank notes before provider access.
**Verification:** `tests/cli_gitlab.rs`.

### RF-GL.11: Check out a GitLab merge request
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-GL.1

- `mr checkout IID` delegates to `glab mr checkout IID --repo URL` in the caller's current worktree.
- Workctl never exposes or forwards `--force`, branch-name, detached-HEAD, or separate-worktree options; native `glab` applies its normal local-change protections.
- Success returns `{ "number" }`; errors return a safe `gitlab_cli` result without partial success output.

**Acceptance:** A CLI integration test proves the current local branch changes to the MR branch via native checkout, and the invocation omits force.
**Verification:** `tests/cli_gitlab_mr.rs` uses a temporary Git repository and `glab` fixture.

### RF-CFG.1: Project configuration
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** none

- Both optional files are discovered at the Git worktree root and validated on every command invocation, even when CLI flags override selected values. Local top-level fields, including the entire `defaults` object, replace the corresponding shared fields.
- The strict schema contains legacy `provider`, independent `codeProvider` and `workItemProvider`, and `defaults`; code and work-item provider values are currently `github` or `gitlab`. `provider` remains a compatibility default for both domains; domain-specific fields override it. Linear is not yet a supported provider and must be rejected, never silently routed to GitHub or GitLab.
- `.workctl.local.json` is gitignored. Old `.mcp-tracker*.json` files are not read or migrated.
- No configuration file is required when providers and the command-specific repository can be resolved from flags or the Git origin.

**Acceptance:** Shared and local provider settings merge independently by domain, while local values override matching shared values; `provider` is a fallback for both domains. Malformed, unknown, oversized, symlink, or non-regular configuration yields a safe JSON error even when flags override effective providers. Linear and unsupported provider names fail closed.
**Verification:** Configuration unit tests using temporary Git roots, including independent code/work-item overrides and legacy fallback.

### RF-CFG.2: Provider and repository resolution
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1

- Provider precedence is resolved independently for each domain: domain-specific CLI flag (`--code-provider` or `--work-item-provider`) > legacy `--provider` > matching domain-specific configuration (`codeProvider` or `workItemProvider`) > legacy configuration `provider` > known Git origin host.
- `issue` is owned by the work-item provider; `pr`/`mr` is owned by the code-host provider. Grammar selection happens before parsing and requires no network. Unknown or unsupported configured values fail closed rather than falling back.
- `--repo` scopes only the selected command; precedence is explicit `--repo` > the `origin` repository when its host matches the selected provider. If the provider and origin host differ, an explicit repository is required.
- `--repo` accepts `[HOST/]OWNER[/...]/REPO`; GitLab also accepts a full HTTPS project URL, including self-managed instances. URL credentials, query strings, fragments, non-HTTPS schemes, and invalid project paths are rejected.
- GitHub and known GitLab hosts resolve from configuration, remote host, or provider flags. Other hosts require explicit GitLab selection and a full project URL; unknown hosts do not fall back to a supported provider.
- When outside a Git worktree, an explicit provider and repository are required.

**Acceptance:** Mixed code/work-item provider selections use the correct native grammar and provider adapter independently. Domain-specific flags override the legacy flag in their own domain; legacy config and Git origin provide fallback in that order. HTTPS and SSH remotes resolve to their repository paths; self-managed GitLab URLs preserve their host for authentication/operations. Unknown remotes, invalid provider names, mismatched provider without explicit repo, unsafe URLs, and missing repository scope fail closed; `group/subgroup/project` is accepted for GitLab and rejected for GitHub.
**Verification:** Unit tests for provider selection, domain repository resolution, and remote parsing; CLI integration tests exercise mixed-provider routing and unsupported configuration.

### RF-CFG.3: GitHub issue defaults and label candidates
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `defaults.github.issue` accepts `assignees`, `labels`, `labelCandidates`, and optional `project`.
- `project` contains a canonical GitHub Project URL, exact allowed repository paths, and string-valued custom field defaults. Project membership and fields apply only when the resolved repository is allowlisted.
- `labelCandidates` is an optional allowlist for the labels passed to DecisionModel when `@auto` is requested. Candidates must exist in the repository; an omitted or empty list uses the full repository catalog.
- Explicit `--assignee` values replace default assignees. Default labels precede explicit labels, with duplicates removed. Explicit `--project-field NAME=VALUE` overrides the configured value of the same field.
- When `.workctl.local.json` sets `defaults`, it replaces the shared defaults object as a whole; nested maps are not deep-merged.

**Acceptance:** Strict config rejects unknown keys and malformed project profiles; repository scope prevents defaults crossing repository boundaries; field and label-candidate precedence follows the rules above.
**Verification:** Config and CLI integration tests with isolated worktree roots.

### RF-CFG.4: GitHub pull-request merge defaults
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-CFG.1, RF-CFG.2

- `defaults.github.pr` accepts `mergeMethod` (`merge`, `squash`, or `rebase`) and boolean `deleteBranch` (default `false`).
- Explicit `pr merge --method` overrides `mergeMethod`; otherwise the configured method is used, falling back to GitHub CLI inference. `--delete-branch` enables deletion for one invocation; a configured `deleteBranch: true` applies to every merge because no inverse CLI flag exists.
- Invalid method values and unknown keys fail closed as `config` errors before provider access. Local `defaults` replaces the shared defaults object as a whole, consistent with RF-CFG.3.

**Acceptance:** Valid typed defaults are applied with the precedence above; missing deleteBranch behaves as false; invalid method values and unknown config keys prevent provider access.
**Verification:** Config unit tests and isolated `tests/cli_prs.rs` merge invocations.

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
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-WI.1, RF-WI.4, RF-GL.6

- Model requests occur only for issue create/edit automatic labels or opted-in Project field selection. Other operations do not send work-item text to a model.
- The DecisionModel package receives provider-neutral work-item title/description and named candidates. Credentials and unrelated code-host metadata are not fetched for model context.
- `DECISION_MODEL_API_KEY` is read only for the selected hosted Jev, Laya, or GLiDE adapter, sent only to its fixed HTTPS endpoint, and never stored, logged, or included in errors.
- `DECISION_MODEL_BASE_URL` configures the OpenAI-compatible local LLM service and must be loopback. Local adapters do not receive hosted credentials.
- Requests and responses are bounded at 2 MiB and 1 MiB. Redirects are disabled. Model failures and malformed/incomplete label scores fail closed with safe errors; no retries occur.
- Generic LLM scores are generated estimates and are not represented as calibrated model-native probabilities.

**Acceptance:** No model/config/key read occurs without one of the explicit model-triggering flows; missing hosted credentials fail before code-host access; model inputs exclude unrelated metadata; local non-loopback URLs, oversized payloads, redirects, and invalid scores fail closed before writes.
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
**Priority:** Must Have | **Status:** Implemented | **Dependencies:** RF-WI.1, RF-WI.2, RF-WI.3, RF-WI.4, RF-WI.8, RF-WI.9, RF-WI.10, RF-WI.11, RF-WI.12, RF-PR.1, RF-PR.2, RF-PR.3, RF-PR.4, RF-PR.5, RF-PR.6, RF-PR.7, RF-PR.8, RF-PR.9, RF-PR.10, RF-PR.11, RF-PR.16, RF-PR.17, RF-PR.18, RF-PR.19, RF-GL.6

- Tests exercise consumer-visible CLI output and provider boundaries without requiring live network access or mutating a real repository; Jev calls use a local HTTP fixture.
- At least one built-binary smoke run exercises a successful command and a failure path.

**Acceptance:** `cargo test` passes and the smoke run observes the expected stdout/stderr and exit status.
**Verification:** Focused Cargo tests and isolated smoke fixture.

## 3. Non-goals for v0

- Jira, local Markdown tracking, provider plugin systems, and project/board administration. Add/remove membership of an existing project is supported through GitHub CLI metadata flags.
- Branch creation, checkout, or standalone deletion: `git` owns branch lifecycle. `--head`/`--base` select existing branches, and `--delete-branch` asks `gh` to remove a branch only after a merge or close.
- Workflow/status transitions and label/assignee/milestone *administration*. Explicit assignment/removal of existing issue and PR metadata is supported; `workctl` does not infer or require project-specific values.
- Inline review comments: `pr review` submits one top-level approve, request-changes, or comment; `pr comment` and `issue comment` each add one top-level comment; `pr close`/`pr reopen` and `issue close`/`issue reopen` accept a single comment.
- Reading, listing, editing, or deleting existing comments and their threads; comment attachments; and alternate comment targets such as review-comment replies.
- Listing the pull requests linked to an issue.
- Checklists. GitHub relationship reads and edits are supported by `issue view`, `issue blockers`, and `issue edit`; a decision model never proposes relationships (ADR-0027).
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
- **Conversation lock:** the provider-side state that prevents new comments on an issue or pull request; `lock` sets it with an optional reason, `unlock` clears it.
- **Revert pull request:** a new pull request that reverts the merge of an existing pull request, opened by `pr revert`; identified by the number `gh pr revert` prints.
