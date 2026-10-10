# Architecture: workctl

> Product scope: [docs/product/vision.md](./product/vision.md)
> Observable contract: [docs/srs.md](./srs.md)
> Accepted cutover decision: [ADR-0004](./adr/0004-workctl-rust-cli.md)
> Body-edit decision: [ADR-0005](./adr/0005-attachments-and-non-rewrite-body-edits.md)
> Pull request decision: [ADR-0006](./adr/0006-pull-request-operations.md)
> GitHub CLI metadata and attachment decision: [ADR-0008](./adr/0008-gh-native-metadata-and-attachments.md), superseding ADR-0007.
> Jev label decision: [ADR-0009](./adr/0009-jev-label-suggestions.md), superseded by [ADR-0010](./adr/0010-automatic-issue-labels.md), [ADR-0011](./adr/0011-remove-issue-label-preview.md), and [ADR-0015](./adr/0015-provider-neutral-decision-model-package.md) (replacing the `--auto-labels` flag with the `@auto` label sentinel).
> Milestone-selector decision: [ADR-0013](./adr/0013-explicit-current-milestone.md), superseding [ADR-0012](./adr/0012-default-current-milestone.md).
> Decision-model adapter decision: [ADR-0015](./adr/0015-provider-neutral-decision-model-package.md), superseding the adapter contract in ADR-0014.
> GLiNER withdrawal decision: [ADR-0020](./adr/0020-remove-gliner-python-server.md), superseding the GLiNER2.5-Decide adapter and local Python service in [ADR-0014](./adr/0014-native-decision-model-adapters.md) and [ADR-0015](./adr/0015-provider-neutral-decision-model-package.md).
> Command-verb decision: [ADR-0016](./adr/0016-gh-verb-parity.md), superseding the issue and pull request verb names in ADR-0004 and ADR-0006.
> Grammar-selection decision: [ADR-0017](./adr/0017-provider-selected-cli-grammar.md), superseding ADR-0016's single-surface premise and `--provider` as a grammar selector.
> GitHub Project fields and issue defaults: [ADR-0018](./adr/0018-github-project-dynamic-fields.md), with automatic field selection added by [ADR-0019](./adr/0019-decision-model-project-field-selection.md).
> GitHub issue relationship reads: [ADR-0023](./adr/0023-github-issue-view-relationships.md).
> Read-only PR status evidence: [ADR-0028](./adr/0028-read-only-pr-status-evidence.md).
> GitHub PR merge defaults: [ADR-0029](./adr/0029-github-pr-merge-defaults.md).
> PR branch update: [ADR-0030](./adr/0030-pr-update-branch.md).
> Bounded PR-check watch: [ADR-0031](./adr/0031-pr-check-watch.md).
> Local PR checkout: [ADR-0032](./adr/0032-github-pr-checkout.md).
> Issue and PR conversation lifecycle: [ADR-0033](./adr/0033-issue-and-pr-conversation-commands.md), superseded for GitHub command grouping by [ADR-0035](./adr/0035-github-command-consolidation.md).
> Issue and pull-request linkage decision: [ADR-0036](./adr/0036-issue-pr-branch-linking.md), superseding the create-only `--closes` decision in ADR-0006.
> Configuration defaults decision: [ADR-0038](./adr/0038-configuration-defaults-expansion.md), extending the create/format/limit surface of [ADR-0029](./adr/0029-github-pr-merge-defaults.md) and [ADR-0018](./adr/0018-github-project-dynamic-fields.md).

`workctl` is a local Rust CLI. It manages GitHub issues and pull requests, and GitLab issues, by invoking the provider's official command-line tool. GitLab project URLs retain their instance host so `glab` can select the matching authenticated host. Each provider owns a static grammar selected before arguments are parsed; no MCP server ships.

## Module map

Modules are grouped by ownership and reason to change. A module can grow while its policy stays cohesive; split it when different policies or external contracts change independently. `main.rs`, command parsing, provider execution, and domain types remain separate.

```text
src/
  main.rs                     provider selection, bootstrap, error-to-exit mapping only
  cli/
    mod.rs                    neutral root command, global flags, and grammar selection
    common.rs                 shared body-change flags, number and limit parsing, and nonblank value parser
    github/
      mod.rs                  the GitHub grammar enum
      issues/
        mod.rs                issue command grammar and dispatch
        create.rs              create arguments
        list.rs                list arguments
        blockers.rs            compact open-blocker chain arguments
        develop.rs             linked-branch arguments
        view.rs                issue body/type and hierarchy arguments
        edit.rs                issue edit arguments
      prs/
        mod.rs                pull-request grammar and dispatch
        create.rs              create arguments
        checkout.rs            checkout arguments
        list.rs                list arguments
        view.rs                view arguments
        status.rs              concise review/mergeability/check-status arguments
        diff.rs                diff arguments
        checks.rs              checks arguments
        review.rs              review arguments
        merge.rs               merge arguments
        edit.rs                edit arguments
        ready.rs               ready/draft arguments
        comment.rs             comment-body arguments
        lock.rs                lock-reason arguments
        revert.rs              revert-creation arguments
        update_branch.rs       update PR branch arguments
    gitlab/
      host.rs                 GitLab-only host capability namespace
      mod.rs                  the GitLab grammar enum
      issues/
        mod.rs                issue command grammar and dispatch
        create.rs              create arguments
        list.rs                list arguments
        view.rs                view arguments
        update.rs              update arguments
  commands/
    mod.rs                    dispatch by grammar provider
    github/
      mod.rs
      issues/
        mod.rs                GitHub issue use-case dispatch
        create.rs              create use case
        list.rs                list use case
        blockers.rs            bounded blocker-chain traversal use case
        view.rs                issue body/type and hierarchy use case
        edit.rs                edit use case
        comment.rs             single-comment use case
        lock.rs                lock use case
        develop.rs             linked-branch use case
      prs/
        mod.rs                pull-request use-case dispatch
        create.rs              create use case
        checkout.rs            current-worktree checkout use case
        list.rs                list use case
        view.rs                view use case
        status.rs              bounded status-evidence aggregation use case
        diff.rs                diff use case
        checks.rs              checks use case
        review.rs              review use case
        merge.rs               merge use case
        edit.rs                edit use case
        ready.rs               ready/draft use case
        comment.rs             single-comment use case
        lock.rs                lock use case
        revert.rs              revert-creation use case
        update_branch.rs       remote branch-update use case
    gitlab/
      mod.rs
      issues/
        mod.rs                GitLab issue use-case dispatch
        create.rs              create use case
        list.rs                list use case
        view.rs                view use case
        update.rs              update use case
    support.rs                shared context resolution, text reading, and body-change setup
  config/
    mod.rs
    discover.rs               Git-root and origin-remote discovery
    files.rs                  strict JSON parse and shared/local merge
    resolve.rs                independent domain selection and repository scope
  domain/
    mod.rs
    body.rs                   append, section-replacement, and closing-reference policy
    error.rs                  stable, safe user-facing errors
    issue.rs                  Issue and IssueSummary contracts
    pr.rs                     pull-request, status, summary, and check-run contracts
    label.rs                  candidate repository labels and normalized model scores
  decision_model/
    mod.rs                    provider-neutral DecisionModel contract and adapter selection
    jev_adapter.rs            Jev native System One protocol
    laya_adapter.rs           Laya native System One protocol
    glide_adapter.rs          Fastino GLiDE native System One protocol
    llm_decision_adapter.rs   generic loopback OpenAI-compatible Chat Completions client
    system_one.rs             shared typed-decision request and response validation
  process/
    mod.rs
    runner.rs                 argument-array subprocess execution, stdin, timeout, limits, bounded stdout/stderr
  providers/
    mod.rs                    WorkItemProvider and PullRequestProvider seams, request types, shared body-change resolver
    github/
      mod.rs                  shared `gh` execution, authentication, milestone-selector resolution, and raw exit-status access
      projects.rs             GitHub Project schema discovery, validation, membership, and field writes
      issues/
        mod.rs                GitHub issue provider composition
        read.rs               list/basic show
        relationships.rs      relationship-aware view and bounded open-blocker chains
        develop.rs            linked-branch creation and listing through `gh issue develop`
        write.rs              create/edit with state transitions, comment/lock, and unlock via --undo
        mapping.rs            response validation and state normalization
      prs/
        mod.rs                GitHub pull request provider composition
        read.rs               list/show/status/diff/checks
        write.rs              create/edit with state transitions, review, merge, branch-update, ready, comment/lock, revert
        mapping.rs            pull request and check-run validation and state normalization
    gitlab/
      mod.rs                  shared `glab` execution, instance-aware authentication, and project URL
      issues/
        mod.rs                GitLab issue composition and provider-native create/update requests
        read.rs               list/show
        write.rs              create/update and safe post-write confirmation
        mapping.rs            response validation, `iid`/`description`/`opened` normalization
  output/
    mod.rs
    json.rs                   compact success and structured error JSON
    text.rs                   optional human-readable success output

tests/
  common/mod.rs               shared isolated `gh`/`glab` fixtures and CLI helpers
  cli_issues.rs               built-binary GitHub issue provider-boundary behavior
  cli_prs.rs                  built-binary GitHub pull request provider-boundary behavior
  cli_gitlab.rs               built-binary GitLab provider-boundary behavior
  config.rs                   isolated config and Git-remote resolution
```

GitLab has its own grammar and adapter rather than extending the GitHub one. GitLab issue writes stay provider-owned because their native flags and mutation semantics differ from GitHub's; the adapter returns the shared `Issue` record without passing provider-specific requests through generic request types. Code-host commands (`pr`/`mr`) and work-item commands (`issue`) have independent provider ownership and can use different native grammars in the same invocation surface.

## Command flow

1. Code-host and work-item providers are selected independently before parsing. Domain-specific flags override legacy `--provider`; domain configuration overrides legacy `provider`; the Git origin is the final fallback. The root command tree combines the selected work-item provider's `issue` command and code-host provider's `pr` or `mr` command.
2. Clap parses that composite grammar plus global `--provider`, `--code-provider`, `--work-item-provider`, `--repo`, and `--format`.
3. Help/version exit without external dependencies. Grammar selection is best-effort and cannot fail; authoritative configuration validation and repository resolution run when a command executes.
4. Repository resolution is scoped to the provider domain owning the command. The origin supplies a repository only when its host matches that provider; otherwise an explicit `--repo` is required. GitLab accepts an HTTPS project URL for a self-managed instance and retains its host; unsupported/unknown provider configuration fails closed.
5. The selected provider checks its own CLI is available and authenticated — `gh auth status --hostname github.com` or `glab auth status --hostname` with the selected GitLab project host — then calls its adapter.
6. Issue creation invokes the provider-neutral `DecisionModel` when `@auto` is present, or when the applicable GitHub Project profile has an unclaimed field in `autoSelectFields`. It sends the proposed title/body and allowlisted candidate names/descriptions in one request; repository labels and eligible Project options share the request. The model boundary receives no provider credentials or unrelated metadata. Model configuration and response validation happen before issue creation. Issue edits retain the timestamp guard.
7. The GitHub adapter invokes `gh issue list` and `gh pr list` for bounded summaries, passing filters through as fixed flags. Issue reads use `gh api`; issue create/edit use `gh issue create|edit`; pull requests use `gh pr` commands. On issue/PR create/edit and `pr edit`, shared GitHub code resolves an explicit `--milestone @current` selector by querying open milestones and choosing the nearest due date today or later; a tie or no eligible milestone fails before the write instead of guessing. An omitted selector makes no request and leaves the field unset, other values pass through verbatim, list filters stay literal, and omission on an edit or update preserves the remote value. Typed optional metadata flags are forwarded directly to `gh`. Create/edit bodies are passed over stdin rather than interpolated into a shell command.
   GitHub issue creation merges configured defaults only for the exact allowlisted repository. With a configured Project profile, it queries Project fields through `gh api graphql`, validates every field/value, and resolves allowlisted auto-select options before creating the issue. Explicit/configured values take precedence; only single-select and iteration fields can be automatically selected. It then adds the issue and applies typed field edits serially. Explicit `--project` is accepted only when the single discovered Project title matches the profile. A failed post-create operation returns a generic partial-success error with the created issue and completed/pending actions; it never retries automatically.
   GitHub issue editing plans only explicitly set/cleared fields against the in-scope Project schema. An explicit `NAME=@auto` resolves discovered single-select/iteration options and shares one model request per issue with automatic labels using final proposed text. A unique best score >= 0.8 plans a field write; low confidence and tied best options preserve the current field. Choices reset between batch targets. The issue revision is rechecked after classification, field-only edits skip the issue mutation, and requested fields are written serially without automatically adding membership. A failure after a completed write reports `partial_success`. Creation defaults and configured `autoSelectFields` are never reapplied during edit; Project field revisions are outside the issue guard.
   Native issue type and parent/sub-issue/blocking changes are provider-owned `NativeIssueEdit` metadata; they share one `gh issue edit` with the generic patch without widening the provider-neutral request. The obsolete generic issue-edit trait method is removed. The GitHub command validates canonical issue references and same-repository targets before provider access, then processes distinct targets serially. It reuses one Project schema plan and one label catalog per invocation; each target retains its own body resolution/model request and timestamp guard. Work scales with the explicit target count and, for Project edits, target count times requested field count; no relationship crawl or unbounded pagination is introduced. A later target failure returns batch `partial_success`; a failed readback after a completed native mutation reports that mutation separately.
8. The GitLab adapter invokes `glab issue list`, `view`, `create`, and `update` with a full HTTPS project URL. The URL host is retained through resolution and passed to `glab auth status --hostname`, including for self-managed instances. Reads and post-write confirmation use `--output json`; create validates the emitted issue URL against the selected project and obtains the shared result with `issue view`. Native GitLab write fields remain typed on the GitLab adapter. Since `glab issue create/update` expose descriptions only as argument values, description bytes are sent through documented `glab api --input -` JSON-body mode, never argv. Empty update descriptions remain rejected before provider access. Since `glab issue create` omits `--weight=0`, an explicit zero is applied with a follow-up `glab issue update --weight=0`. The mapping turns `iid` into `number`, `description` into `body`, and `opened` into `open` without leaking provider-only schema.
9. Typed results are serialized by `output`; errors are mapped once to a safe JSON error on stderr and nonzero exit.

## Proposed private semantic declarations

This is a proposed extension, not a current capability or an approved storage design. GitHub-native
metadata remains the existing optional issue/PR fields described in [ADR-0008](./adr/0008-gh-native-metadata-and-attachments.md);
a semantic declaration is separate and must never be forwarded to the provider.

The issue-create use case would validate and persist a private intent before the remote write, then bind
the declaration to the native issue ID only after that ID is observed. Intent, remote-create outcome,
and internal association are distinct states: a provider timeout or lost response is uncertain, not proof
that no issue exists. Never retry an uncertain create automatically or associate by title/body similarity.
Failure to persist before create blocks the remote write; failure after a remote create leaves a pending
association rather than permission to create again.

The owning seam is a private durable-intent store invoked by the issue-create use case; it is not project
configuration and does not alter `WorkItemProvider`'s provider-native issue contract. Stored declarations
must not enter provider requests, process arguments, stdout, logs, or errors; storage and reads remain
subject to local authorization. Input surface, store location/format, reconciliation flow, and
post-create pending-state output are not selected. Resolve these choices and write the store threat model
before implementation; no storage engine or new CLI flag is implied here.

## Configuration and context

- Discover both config files at the Git worktree root on every command invocation, including when provider or repository flags override their values; never walk above the root.
- Both files are strict JSON, regular non-symlink files capped at 64 KiB. Local `provider`, `codeProvider`, and `workItemProvider` replace matching shared fields independently; local `defaults` replaces shared defaults as a whole. Unknown keys and malformed files fail closed.
- `defaults.github.issue` retains repository-specific issue settings and optional Project profile (`url`, exact `repositories`, string-valued `fields`). `labelCandidates` narrows labels passed to DecisionModel. `defaults.github.pr.mergeMethod` selects `merge|squash|rebase` when `pr merge --method` is omitted; explicit CLI method wins, and absence of both leaves method selection to `gh`.
- `defaults.github.pr.deleteBranch` is a boolean defaulting to false. True enables remote branch deletion after every completed PR merge; `--delete-branch` can enable it for one invocation, but there is no inverse CLI option. Deletion is workctl's own `gh api` call on the pull request's head ref and never touches a local branch; an `--auto` invocation queues the merge and deletes nothing. Local defaults replace shared defaults as a whole.
- `defaults.github.pr` also carries `labels`, `assignees`, `reviewers`, `base`, and boolean `draft` for `pr create`; `defaults.gitlab.issue` and `defaults.gitlab.mr` carry the equivalent GitLab create inputs, with `targetBranch` matching `glab`'s flag. Create commands merge them in the command layer before provider access, and `pr merge` never reads the create keys.
- `defaults.output.format` (`json|text`) is resolved once before command dispatch: explicit `--format`, then configuration, then `json`. Errors stay one JSON object on stderr under either success format.
- `defaults.github.listLimit` (1–1000) supplies the `issue list`/`pr list` `--limit` default; explicit `--limit` wins, then 30. GitLab listings keep their native `--per-page` semantics and are not covered by this key.
- Defaults never override an explicit flag: configured labels precede explicit ones with duplicates removed, explicit assignees/reviewers replace configured lists, and explicit branch/format/limit values win.
- Provider precedence is independent per domain: domain-specific CLI flag, legacy `--provider`, domain-specific config, legacy config `provider`, then Git origin. Grammar selection reads these sources before parsing, best-effort and without external dependencies; authoritative command execution validates configuration.
- Repository precedence: CLI `--repo [HOST/]OWNER[/...]/REPO` or, for GitLab, a full HTTPS project URL; otherwise use `origin` only when it belongs to the selected command's provider. A mismatching origin requires explicit `--repo`.
- The repository is validated before inclusion in any provider argument or endpoint path: GitHub accepts exactly `OWNER/REPO`; GitLab accepts `GROUP[/SUBGROUP...]/PROJECT` or an HTTPS project URL on a self-managed instance. HTTPS and SCP-style SSH GitHub and known GitLab remotes are recognized; unknown hosts do not fall back.
- If no Git root exists, an explicit provider and repository are required for provider operations. There is no persistent `repo` config pin.
- Legacy `.mcp-tracker.json` files are neither read nor migrated.

## Provider seams and models

`WorkItemProvider` owns shared GitHub issue create, list, and show operations. Editing is provider-owned: `GitHubIssues::edit_native` combines the generic body/metadata patch with GitHub type/relationship fields and an optional Project plan. The application/CLI layer receives normalized `Issue` or `IssueSummary`; provider response casing stays inside adapters. `Issue.state` is `open|closed`; an issue summary excludes the body. GitLab writes likewise use provider-owned native request types, rather than forcing distinct update semantics into the shared issue seam. GitLab issue creation retains native epic, linked issue/MR, time tracking, template, confidentiality, weight, and due-date inputs on its provider request.

GitHub's public `issue view` uses provider-owned `GitHubIssues::view` and `GitHubIssueView`, flattening the basic issue record and adding its type and direct parent/sub-issue relations. It does not query or print blocked-by/blocking connections. `read::show` remains the basic REST read used by mutations and their guards; its pull-request rejection also runs before hierarchy lookup. Reads use `gh api graphql` with owner/name/number/cursor variables and fixed connection names. Sub-issues are paginated at 100 per page, with ten pages and 1,000 nodes, bounding one view to 11 API calls including REST and excluding authentication. The separate `issue blockers` command traverses native `blockedBy` edges under its existing bounded, fail-closed contract; native relationship writes remain unchanged.


`PullRequestProvider` is a narrow seam separate from `WorkItemProvider`: pull requests add diff, checks, review, merge, ready, state transitions, and revert. Typed requests include `NewPr`, `PrQuery`, `PrPatch`, `PrTransition`, `ReviewEvent`, and inline review comments. `PrPatch::is_empty` rejects mutationless updates; a guard timestamp alone is not a write. GitHub's provider owns read/write sequencing and validates state transitions before mutation.

GitHub `pr status` is a read-only provider operation that makes exactly one `gh pr view` call for review/mergeability fields (no body), then one `gh pr checks --required` call. Existing subprocess timeout/output bounds apply; there is no pagination. Both calls use one authenticated provider context and execute sequentially; failure yields no combined success result, and the result is not an atomic snapshot. `PullRequestStatus` exposes those observed values and check runs without deriving a ready-to-merge verdict: an empty check report or unknown review/mergeability value is inconclusive.
`pr checks --watch` delegates waiting to native `gh pr checks --watch`, optionally forwarding `--interval` and `--fail-fast`; it does not poll in workctl or involve DecisionModel/JEV. The normal subprocess deadline remains 30 seconds. Watch mode uses an absolute deadline of 600 seconds by default, configurable from 1 to 3600 seconds, while retaining the runner's output caps and child-kill behavior. The adapter parses valid check JSON even when `gh` exits nonzero, preserving failing/pending evidence and the existing no-checks empty result; deadline expiry maps to safe `timeout` with no partial success output.

`pr update-branch` is a remote write owned by `PullRequestProvider`: after the shared provider setup authenticates, the GitHub adapter makes one `gh pr update-branch NUMBER --repo OWNER/REPO` call, adding `--rebase` only when requested. The default strategy merges the latest base into the PR branch; `--rebase` changes the strategy. It does not fetch a separate snapshot, check out a branch, or touch the local worktree. A provider failure returns no success output and is mapped through the existing safe `github_cli` error path.
`pr checkout` is a local worktree-changing operation behind `PullRequestProvider`: after provider setup resolves the repository and authenticates, the GitHub adapter invokes `gh pr checkout NUMBER --repo OWNER/REPO` with inherited current working directory. Workctl never supplies `--force`, branch-name, detached-HEAD, or alternate-worktree flags; GitHub CLI handles PR branch discovery and normal checkout conflicts. Success reports the PR number; provider failure is mapped to safe `github_cli` with no success output.

GitHub lifecycle actions are grouped in the public grammar: `issue edit`/`pr edit --state` perform close or reopen after ordinary edits; `issue lock`/`pr lock --undo` unlock; standalone issue/PR comments remain on `comment`. Combined edits are sequential, nontransactional and never retried; transition comments travel over stdin, and later failure reports ordered partial progress. `pr review --inline LOCATION TEXT` submits all line comments with one head-anchored review API request; summary-only reviews retain native `gh pr review`. GitLab keeps its separate grammar and lifecycle commands.

Branch deletion after a merge or close is a workctl-owned remote write, not a forwarded flag: the adapter reads the pull request's references with one `gh api repos/OWNER/REPO/pulls/NUMBER` call and, only when the head repository is the addressed repository, removes that ref with one `gh api --method DELETE repos/OWNER/REPO/git/refs/heads/BRANCH` call whose branch segment is percent-encoded. `gh` never receives `--delete-branch`, so no local branch is reachable from this path, and a 422 `Reference does not exist` counts as the deleted state rather than a failure. Deletion belongs to a merge that completed here, so an `--auto` invocation deletes nothing and rejects an explicit `--delete-branch`. A deletion failure after an acknowledged merge or close returns `partial_success` with `remote branch deletion` pending.


`issue develop` is a GitHub-only operation reached through the GitHub issue grammar, not through `WorkItemProvider`, because no other provider has an equivalent verb: `GitHubIssues::develop_branch` and `linked_branches` forward to `gh issue develop`, which creates the branch on the remote and fetches it into the local remote-tracking refs. Workctl never creates or deletes a local branch itself and forwards `--checkout` only when the caller asks for it; the created branch name is read from the `/tree/` reference `gh` prints, and each `--list` line is a `BRANCH<TAB>URL` pair reduced to its branch name. Unparsable output fails as `provider_response`.

Closing references are pull-request body text rather than a separate remote write, so `pr create --closes` and `pr edit --closes/--remove-closes` share `domain::body::ClosingReference` and the `apply_closing_references` policy. One keyword line per reference is appended when absent, and a removal drops only a line holding a single closing keyword and that one reference, leaving prose and multi-reference lines untouched. The resulting text travels in the same single `gh pr edit --body-file -` call as any other body change, so no extra invocation is made for linking.

Requests cross the issue and pull-request seams as typed values, not loose strings. The `DecisionModel` package is provider-neutral: it receives work-item title/description and generic named candidates with optional descriptions, and returns validated `{candidate, probability}` scores (serialized as `label` for existing adapter wire compatibility). Its adapters are Jev, Laya, GLiDE, and generic local LLM; each keeps its own native HTTP contract behind the same interface. GitHub issue create/edit currently call it for repository-label selection; issue create also passes configured, dynamically discovered single-select/iteration Project options. The model package itself does not depend on GitHub response types.

The adapter IDs are `jev-latest`, `laya`, `fastino/GLiDE`, or `local/<model-id>`. Jev/Laya use native System One requests with Bearer authentication; GLiDE uses System One with `X-API-Key`. `LLMDecisionAdapter` uses OpenAI-compatible `/chat/completions` on loopback. Its generated numeric scores are model estimates, not calibrated probabilities; JSON mode constrains output shape only. Native model probabilities/confidence remain provider-native and need not be calibrated identically.

The generic local endpoint accepts any model served with OpenAI-compatible Chat Completions and JSON-object output. It is not a hosted OpenAI integration. `DECISION_MODEL_BASE_URL` is restricted to loopback; hosted endpoints are fixed. Requests are bounded to 2 MiB, responses to 1 MiB, redirects are disabled, and failed/malformed scores fail closed. The existing threshold is >= 0.8; it is preserved for compatibility, not as a claim of cross-model calibration.


Pull request `view` and `checks` depend on the `gh` exit status instead of collapsing it into a generic failure: `gh pr view` exits non-zero when the number is not a pull request, and `gh pr checks` exits non-zero for failing (1) or pending (8) checks while still printing a valid report, and writes `no checks reported` to stderr when there are none. The adapter therefore calls `run_gh_raw`, which returns the raw `ProcessOutput` rather than mapping a non-zero exit to `github_cli`; `view` maps that failure to `not_pull_request`, and `checks` parses stdout while treating the `no checks reported` stderr as an empty report. To make the status interpretable, `ProcessOutput` also carries a bounded `stderr`, so the adapter can read provider diagnostics that never reach a user-facing error.

Provider asymmetry is expected. Keep only genuinely shared behavior on each seam; expose provider-specific capabilities through typed, provider-scoped arguments and reject unsupported choices explicitly. Never pass arbitrary raw command arguments through. Issue attachments use GitHub CLI 2.99.0 or newer; the version is checked only when an attachment is requested. GitHub CLI validates attachment types and performs uploads. Body text read from a file or stdin is capped at 1 MiB and must be valid UTF-8.

Issue and pull-request create/edit map to documented code-host commands, including optional native metadata. Explicit labels remain caller-provided. Issue create/edit may opt into model classification by including `@auto` in `--label`/`--add-label`; the package appends labels scoring >= 0.8 while preserving existing and manual additions. `@auto` is reserved and rejected for label removal. No model call occurs without the sentinel; PRs remain outside this path. The issue adapter fetches the current item before classification, resolves final proposed text, and sets a timestamp guard to reject concurrent changes.

GitLab `issue create/update --label @auto` use that same provider-neutral DecisionModel path. The GitLab adapter builds candidates from native, JSON-formatted `glab label list` pages of 100, stopping at an empty page and rejecting catalogs beyond 1,000 entries or duplicate names. The marker is removed before constructing native issue mutations; without it, neither model setup nor catalog enumeration runs. Update classification uses the final proposed title/description and compares an `updated_at` snapshot after inference, before any write.
- GitLab merge requests use a separate native adapter and the `mr` grammar; the provider resolver does not expose them as GitHub `pr` aliases. Shared pull-request mapping uses only GitLab fields present in `glab mr` output; GitHub-only mergeability and review-decision values remain absent. MR list filters use named native arguments; list/view use JSON from glab, diff uses `glab mr diff --color=never`, and checkout changes the caller's worktree without force. Lifecycle, merge, approvals/revocation/rebase/subscription/todo, discussions, create/update, approver discovery, linked-issue lookup, and note list/update use native `glab mr` commands. MR descriptions and updated note bodies use stdin/API paths to keep user text out of process arguments. Destructive delete and interactive boards are intentionally excluded; group-scoped issue queries require an explicit group target instead of inferring one from the selected project.

The System One adapters ask one Noul question per candidate label because assignment is multi-valued; answers must match requested IDs/types and probabilities must be in [0,1]. Laya supports at most 32 questions and GLiDE at most 255; Jev supports at most 1,000 labels. The generic LLM adapter requests JSON-formatted estimated scores; those are not native calibrated probabilities. Every adapter validates a complete, unique response against the supplied label catalog.

Native hosted adapters use `DECISION_MODEL_API_KEY` only when selected. Local adapters require no hosted key. The generic local model endpoint must be loopback; it may target any local service implementing the supported OpenAI-compatible contract.

Text output escapes terminal control characters in provider strings (titles, authors, labels, check names, and similar) while preserving body newlines and tabs; JSON output preserves the string data through JSON encoding.

## Process and security boundary

- One process runner owns all `git`, `gh`, and `glab` process creation. It accepts an executable and argument vector; it never invokes a shell.
- Create/edit bodies are passed on stdin to `gh`; GitLab issue descriptions and issue-note bodies use documented `glab api --input -` JSON-body mode because the native `glab issue create/update/note` interfaces have no stdin body option. Merge-request note creation uses native `glab mr note create` stdin mode. Text never appears in child-process arguments. Reads use no request body. Each output stream has an 8 MiB cap; one absolute 30-second deadline covers ordinary child completion and pipe/input workers. PR-check watch opts into an absolute 600-second deadline (configurable up to 3600 seconds); a still-running direct child is killed on expiry.

- `gh` owns GitHub credentials and `glab` owns GitLab credentials. `workctl` checks `gh auth status` or `glab auth status` and discards the output.
- Hosted model credentials use `DECISION_MODEL_API_KEY` only for an opted-in automatic selection; they are sent only to the selected adapter's fixed HTTPS endpoint. Local service URLs are rejected unless loopback.
- Model requests contain only work-item title/description and named candidate context. Credentials, comments, and unrelated code-host metadata are excluded. Candidate text remains untrusted data; model output is restricted to offered names and never authorizes a write.
- Raw provider diagnostics, config contents, credentials, stack traces, and internal paths are not included in user-facing errors.
- Invalid model configuration and provider payloads fail closed; no fallback fabricates records or silently drops requested scores.

## Failure behavior

| Failure | Observable result | Recovery |
|---|---|---|
| Decision-model credential missing/blank | `decision_authentication`; no provider or model call | Set `DECISION_MODEL_API_KEY` for a hosted adapter |
| Unsupported model ID or non-loopback local URL | `decision_config`; no model call | Select a supported adapter and keep local endpoints on loopback |
| Model network failure, timeout, non-success status, or malformed scores | Safe `decision_model`/`decision_response`; no label write | Check the selected model service and its protocol |
| More than 1,000 effective label candidates, or the selected adapter's question limit | `decision_input_limit`; model is not called | Configure a smaller `labelCandidates` allowlist |
| Generic local LLM returns invalid, duplicate, missing, or out-of-range scores | `decision_response`; no label write | Use a compatible model/server response or choose a native decision adapter |
| `gh` missing or unauthenticated | Safe `dependency`/`authentication` JSON error on stderr; nonzero exit | Install/authenticate `gh`, retry |
| `glab` missing or unauthenticated for the selected GitLab host | Safe `dependency`/`authentication` JSON error on stderr; nonzero exit | Install `glab` and authenticate to that instance |
| A verb or flag belonging to the other provider's grammar | clap usage error, exit 2, no provider call | Read the resolved provider's help (`workctl issue --help`) |
| Attachments requested with `gh` older than 2.99.0 or an unparseable version | `dependency_version` JSON error before attachment write | Update `gh` through the user's package manager, or omit `--attach` |
| Create with attachments exits nonzero | `attachment_create_uncertain`; the item may exist | Check GitHub before retrying |
| Git remote missing, unsupported, or belonging to another provider than the selected one | `context`/`provider_unsupported` JSON error; no provider call | Pass explicit `--provider` and `--repo` for the intended host |
| Invalid, oversized, linked, or non-regular config | `config` JSON error without path contents; overrides do not bypass validation | Correct/remove invalid config |
| Invalid arguments or a non-positive number | `invalid_input` or a clap usage error; no remote call | Correct arguments |
| `--milestone @current` with no eligible open milestone, or tied nearest due dates | `invalid_input` JSON error; no write | Pass an explicit `--milestone NAME` |
| Body-change flags combined or incomplete | `invalid_input` JSON error; no `gh` call | Supply one body change and its companion argument |
| Patch context absent from the current body | `patch_conflict` JSON error; no write | Regenerate the diff against the fetched body |
| `--replace-section` heading not found | `section_not_found` JSON error; no write | Correct the heading to match the fetched body |
| `--expect-updated-at` differs from the fetched issue or pull request | `conflict` JSON error; no write | Re-read with `issue view`/`pr view` and retry with the returned `updated_at` |
| A number given to a pull request command is not a pull request | `not_pull_request` JSON error; no write | Use a pull request number, or the `issue` group for issues |
| An issue command is given a pull request number | `not_issue` JSON error; no write | Use the issue number, or the `pr` group for pull requests |
| GitHub command failure | Generic `github_cli` JSON error; raw stderr withheld | Check `gh` authentication/permissions and retry |
| Conversation command with a missing/blank comment body, an unsupported `--reason`, or both body sources | `invalid_input` or clap usage error; no `gh` call | Supply one body source and a reason `gh` accepts |
| `pr revert` succeeds but prints no readable pull-request URL | `provider_response` JSON error; the revert may exist | Check GitHub for the revert pull request before retrying |
| Project membership/field update fails after issue creation | Generic `partial_success` with the created resource and completed/pending operations; no automatic retry | Inspect the resource and resume only the pending operations |
| GitLab command failure | Generic `gitlab_cli` JSON error; raw stderr withheld | Check `glab` authentication/permissions and retry |
| GitLab issue or MR write fails or cannot be confirmed | `gitlab_write_uncertain`; the remote resource may exist or have changed | Inspect GitLab before retrying |
| Non-write child timeout/output cap exceeded | `timeout`/`output_limit` JSON error | Retry after resolving remote/tool issue |
| Malformed provider response | `provider_response` JSON error | Report provider contract mismatch |

## Verification strategy

- Unit tests cover strict config merge, Git remote parsing, provider and grammar selection, GitHub and GitLab issue state mapping, `iid`/`description`/`opened` normalization, request serialization, body mutation, and patch application.
- Decision-model unit tests use loopback HTTP fixtures to verify native request shapes and authentication headers (Bearer for Jev/Laya, `X-API-Key` for GLiDE), `noul` answer validation, candidate-coverage and score-range checks, loopback-only URL enforcement, and size/redirect behavior. The built-binary missing-credential test proves no code-host access or model call occurs without `@auto`.
  - Integration tests launch the compiled binary with temporary `gh` and `glab` fixtures and verify observable stdout/stderr, exit status, body safety, CRUD mapping, filter and metadata arguments, attachment routing/version gating, body-change behavior, concurrency guards, bounds, and the `@auto` path against an OpenAI-compatible local fixture. The `glab` fixture also pins the exact argument vector, so the GitLab grammar's flag names stay the ones `glab` documents, and proves the GitLab grammar rejects GitHub verbs and flags before any provider call. No test creates or edits a real GitHub or GitLab issue or pull request, and no test calls a hosted model.
- Release verification builds `workctl` and smoke-runs one success and one failure path against the isolated fixture.
