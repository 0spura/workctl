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

`workctl` is a local Rust CLI. It manages GitHub issues and pull requests, and GitLab issues, by invoking the provider's own command-line tool. Each provider owns a static grammar that is selected before the arguments are parsed; no MCP server ships.

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
      issues.rs               GitHub issue subcommand argument types
      prs.rs                  GitHub pull request subcommand argument types
    gitlab/
      mod.rs                  the GitLab grammar enum
      issues.rs               GitLab issue subcommand argument types
  commands/
    mod.rs                    dispatch by grammar provider
    github/
      mod.rs
      issues.rs               GitHub issue use cases: resolve context, call the provider, select output
      prs.rs                  GitHub pull request use cases
    gitlab/
      mod.rs
      issues.rs               GitLab issue reads
    support.rs                shared context resolution, text reading, and body-change setup
  config/
    mod.rs
    discover.rs               Git-root and origin-remote discovery
    files.rs                  strict JSON parse and shared/local merge
    resolve.rs                grammar selection, provider and repository precedence
  domain/
    mod.rs
    body.rs                   append and section-replacement policy
    error.rs                  stable, safe user-facing errors
    issue.rs                  Issue and IssueSummary contracts
    pr.rs                     PullRequest, PullRequestSummary, PullRequestState, and CheckRun contracts
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
        read.rs               list/show
        write.rs              create/edit
        mapping.rs            response validation and state normalization
      prs/
        mod.rs                GitHub pull request provider composition
        read.rs               list/show/diff/checks
        write.rs              create/edit/review/merge/ready/close/reopen
        mapping.rs            pull request and check-run validation and state normalization
    gitlab/
      mod.rs                  shared `glab` execution, authentication, and the pinned project URL
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

GitLab has its own grammar and adapter rather than extending the GitHub one. GitLab issue writes stay provider-owned because their native flags and mutation semantics differ from GitHub's; the adapter returns the shared `Issue` record without widening `WorkItemProvider` or passing provider-specific requests through generic request types.

## Command flow

1. The provider is selected before parsing: explicit `--provider`, merged config, then the Git origin host, falling back to GitHub when nothing resolves. Only the selected provider's grammar is attached to the root command, so the other provider's verbs and flags do not exist for clap to accept.
2. Clap parses that grammar plus the global `--provider`, `--repo`, and `--format`.
3. Help/version exit without external dependencies. Grammar selection is best-effort and cannot fail, so help is always available; the authoritative resolution runs when a command executes.
4. Context resolves provider and repository using command-line overrides, merged config, then Git origin host/repository. A `--repo` host must match the resolved provider, an origin remote belonging to another provider fails closed, and `group/subgroup/project` is accepted only for GitLab.
5. The selected provider checks its own CLI is available and authenticated — `gh auth status --hostname github.com` or `glab auth status --hostname gitlab.com` — then calls its adapter.
6. Issue creation invokes the provider-neutral `DecisionModel` when `@auto` is present, or when the applicable GitHub Project profile has an unclaimed field in `autoSelectFields`. It sends the proposed title/body and allowlisted candidate names/descriptions in one request; repository labels and eligible Project options share the request. The model boundary receives no provider credentials or unrelated metadata. Model configuration and response validation happen before issue creation. Issue edits retain the timestamp guard.
7. The GitHub adapter invokes `gh issue list` and `gh pr list` for bounded summaries, passing filters through as fixed flags. Issue reads use `gh api`; issue create/edit use `gh issue create|edit`; pull requests use `gh pr` commands. On issue/PR create/edit and `pr edit`, shared GitHub code resolves an explicit `--milestone @current` selector by querying open milestones and choosing the nearest due date today or later; a tie or no eligible milestone fails before the write instead of guessing. An omitted selector makes no request and leaves the field unset, other values pass through verbatim, list filters stay literal, and omission on an edit or update preserves the remote value. Typed optional metadata flags are forwarded directly to `gh`. Create/edit bodies are passed over stdin rather than interpolated into a shell command.
   GitHub issue creation merges configured defaults only for the exact allowlisted repository. With a configured Project profile, it queries Project fields through `gh api graphql`, validates every field/value, and resolves allowlisted auto-select options before creating the issue. Explicit/configured values take precedence; only single-select and iteration fields can be automatically selected. It then adds the issue and applies typed field edits serially. Explicit `--project` is accepted only when the single discovered Project title matches the profile. A failed post-create operation returns a generic partial-success error with the created issue and completed/pending actions; it never retries automatically.
   GitHub issue editing plans only explicitly set/cleared fields against the in-scope Project schema. It fetches the issue for body resolution and the timestamp guard before mutations, skips an issue write for field-only edits, and writes requested fields serially without adding membership automatically. A failure after a completed issue or field write reports generic `partial_success`. Creation defaults and automatic Project selection are never applied during edit; Project fields are outside the issue revision.
   Native issue type and parent/sub-issue/blocking changes are provider-owned `NativeIssueEdit` metadata; they share one `gh issue edit` with the generic patch without widening the provider-neutral request. The obsolete generic issue-edit trait method is removed. The GitHub command validates canonical issue references and same-repository targets before provider access, then processes distinct targets serially. It reuses one Project schema plan and one label catalog per invocation; each target retains its own body resolution/model request and timestamp guard. Work scales with the explicit target count and, for Project edits, target count times requested field count; no relationship crawl or unbounded pagination is introduced. A later target failure returns batch `partial_success`; a failed readback after a completed native mutation reports that mutation separately.
8. The GitLab adapter invokes `glab issue list`, `view`, `create`, and `update` with the project as a full `https://gitlab.com/...` URL. Reads and post-write confirmation use `--output json`; create validates the emitted issue URL and obtains the shared result with `issue view`. Native GitLab write fields remain typed on the GitLab adapter. Description content travels on stdin; empty update descriptions and dash-only descriptions are rejected before provider access. Since `glab issue create` omits `--weight=0`, an explicit zero is applied with a follow-up `glab issue update --weight=0`. The mapping turns `iid` into `number`, `description` into `body`, and `opened` into `open`, and rejects malformed responses instead of emitting partial records.
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

- Discover both config files at the Git worktree root on every command invocation, including when `--provider`/`--repo` override their values; never walk above the root.
- Both files are strict JSON, regular non-symlink files capped at 64 KiB. Local `provider`, `workItemProvider`, and `defaults` replace their shared top-level counterparts; `defaults` is replaced as a whole, not deep-merged. Unknown keys and malformed files fail closed.
- `defaults.github.issue` keeps repository-specific assignees/labels and optional Project profile data (`url`, exact `repositories`, string-valued `fields`). `labelCandidates` narrows the labels passed to DecisionModel. The application resolves this configuration at issue creation and applies Project behavior only to allowlisted repository paths.
- Provider precedence: CLI `--provider`, local/shared `workItemProvider`, local/shared `provider`, then Git remote host. Grammar selection reads the same sources before parsing, best-effort and without a command tree; the authoritative resolution reports the real error.
- Repository precedence: CLI `--repo [HOST/]OWNER[/...]/REPO`, then the Git `origin` remote.
- The repository is validated before inclusion in any provider argument or endpoint path: GitHub accepts exactly `OWNER/REPO`, GitLab accepts `GROUP[/SUBGROUP...]/PROJECT`, and a leading host must belong to the resolved provider. HTTPS and SCP-style SSH GitHub and GitLab remotes are recognized; unknown hosts do not fall back, and an origin remote belonging to another provider than the selected one fails closed.
- If no Git root exists, both explicit provider and repository are required. There is no persistent `repo` config pin.
- Legacy `.mcp-tracker.json` files are neither read nor migrated.

## Provider seams and models

`WorkItemProvider` owns shared GitHub issue create, list, and show operations. Editing is provider-owned: `GitHubIssues::edit_native` combines the generic body/metadata patch with GitHub type/relationship fields and an optional Project plan. The application/CLI layer receives normalized `Issue` or `IssueSummary`; provider response casing stays inside adapters. `Issue.state` is `open|closed`; an issue summary excludes the body. GitLab writes likewise use provider-owned native request types, rather than forcing distinct update semantics into the shared issue seam.

`PullRequestProvider` is a second, narrow seam rather than an extension of `WorkItemProvider`. The two domains differ in nearly every operation — issues use `gh issue create|edit`, while pull requests add diff, checks, review, merge, ready, close, and reopen and normalize state to `open|closed|merged` — so widening the issue seam would force every provider and caller to carry capabilities neither needs. Each seam owns its request types: `NewPr`, `PrQuery`, `PrPatch`, plus `ReviewEvent` and `MergeMethod`. `PrPatch::is_empty` rejects a mutationless update; its guard timestamp alone is not a write. The GitHub composition mirrors the issue adapter, splitting reads, writes, and validation.

Requests cross the issue and pull-request seams as typed values, not loose strings. The `DecisionModel` package is provider-neutral: it receives work-item title/description and generic named candidates with optional descriptions, and returns validated `{candidate, probability}` scores (serialized as `label` for existing adapter wire compatibility). Its adapters are Jev, Laya, GLiDE, and generic local LLM; each keeps its own native HTTP contract behind the same interface. GitHub issue create/edit currently call it for repository-label selection; issue create also passes configured, dynamically discovered single-select/iteration Project options. The model package itself does not depend on GitHub response types.

The adapter IDs are `jev-latest`, `laya`, `fastino/GLiDE`, or `local/<model-id>`. Jev/Laya use native System One requests with Bearer authentication; GLiDE uses System One with `X-API-Key`. `LLMDecisionAdapter` uses OpenAI-compatible `/chat/completions` on loopback. Its generated numeric scores are model estimates, not calibrated probabilities; JSON mode constrains output shape only. Native model probabilities/confidence remain provider-native and need not be calibrated identically.

The generic local endpoint accepts any model served with OpenAI-compatible Chat Completions and JSON-object output. It is not a hosted OpenAI integration. `DECISION_MODEL_BASE_URL` is restricted to loopback; hosted endpoints are fixed. Requests are bounded to 2 MiB, responses to 1 MiB, redirects are disabled, and failed/malformed scores fail closed. The existing threshold is >= 0.8; it is preserved for compatibility, not as a claim of cross-model calibration.


Pull request `view` and `checks` depend on the `gh` exit status instead of collapsing it into a generic failure: `gh pr view` exits non-zero when the number is not a pull request, and `gh pr checks` exits non-zero for failing (1) or pending (8) checks while still printing a valid report, and writes `no checks reported` to stderr when there are none. The adapter therefore calls `run_gh_raw`, which returns the raw `ProcessOutput` rather than mapping a non-zero exit to `github_cli`; `view` maps that failure to `not_pull_request`, and `checks` parses stdout while treating the `no checks reported` stderr as an empty report. To make the status interpretable, `ProcessOutput` also carries a bounded `stderr`, so the adapter can read provider diagnostics that never reach a user-facing error.

Provider asymmetry is expected. Keep only genuinely shared behavior on each seam; expose provider-specific capabilities through typed, provider-scoped arguments and reject unsupported choices explicitly. Never pass arbitrary raw command arguments through. Issue attachments use GitHub CLI 2.99.0 or newer; the version is checked only when an attachment is requested. GitHub CLI validates attachment types and performs uploads. Body text read from a file or stdin is capped at 1 MiB and must be valid UTF-8.

Issue and pull-request create/edit map to documented code-host commands, including optional native metadata. Explicit labels remain caller-provided. Issue create/edit may opt into model classification by including `@auto` in `--label`/`--add-label`; the package appends labels scoring >= 0.8 while preserving existing and manual additions. `@auto` is reserved and rejected for label removal. No model call occurs without the sentinel; PRs remain outside this path. The issue adapter fetches the current item before classification, resolves final proposed text, and sets a timestamp guard to reject concurrent changes.

The System One adapters ask one Noul question per candidate label because assignment is multi-valued; answers must match requested IDs/types and probabilities must be in [0,1]. Laya supports at most 32 questions and GLiDE at most 255; Jev supports at most 1,000 labels. The generic LLM adapter requests JSON-formatted estimated scores; those are not native calibrated probabilities. Every adapter validates a complete, unique response against the supplied label catalog.

Native hosted adapters use `DECISION_MODEL_API_KEY` only when selected. Local adapters require no hosted key. The generic local model endpoint must be loopback; it may target any local service implementing the supported OpenAI-compatible contract.

Text output escapes terminal control characters in provider strings (titles, authors, labels, check names, and similar) while preserving body newlines and tabs; JSON output preserves the string data through JSON encoding.

## Process and security boundary

- One process runner owns all `git`, `gh`, and `glab` process creation. It accepts an executable and argument vector; it never invokes a shell.
- Create/edit bodies are passed on stdin as text to `gh`; GitLab create/update descriptions are passed on stdin to `glab`, never in argv. GitHub API reads and `glab issue` reads use no request body. Each output stream has an 8 MiB cap; one absolute 30-second deadline covers child completion and pipe/input workers, and a still-running direct child is killed on expiry.

- `gh` owns GitHub credentials and `glab` owns GitLab credentials. `workctl` checks `gh auth status` or `glab auth status` and discards the output.
- Hosted model credentials use `DECISION_MODEL_API_KEY` only after `@auto` explicitly selects a hosted adapter; they are sent only to that adapter's fixed HTTPS endpoint. Local service URLs are rejected unless loopback.
- Model requests contain only the proposed work-item title/description and candidate labels; credentials, comments, URLs, and unrelated code-host metadata are excluded.
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
| `glab` missing or unauthenticated | Safe `dependency`/`authentication` JSON error on stderr; nonzero exit | Install/authenticate `glab`, retry |
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
| Project membership/field update fails after issue creation | Generic `partial_success` with the created resource and completed/pending operations; no automatic retry | Inspect the resource and resume only the pending operations |
| GitLab command failure | Generic `gitlab_cli` JSON error; raw stderr withheld | Check `glab` authentication/permissions and retry |
| GitLab issue write fails, times out, or cannot be confirmed by a read-back | `gitlab_write_uncertain`; the issue may exist or have changed | Check GitLab before retrying |
| Non-write child timeout/output cap exceeded | `timeout`/`output_limit` JSON error | Retry after resolving remote/tool issue |
| Malformed provider response | `provider_response` JSON error | Report provider contract mismatch |

## Verification strategy

- Unit tests cover strict config merge, Git remote parsing, provider and grammar selection, GitHub and GitLab issue state mapping, `iid`/`description`/`opened` normalization, request serialization, body mutation, and patch application.
- Decision-model unit tests use loopback HTTP fixtures to verify native request shapes and authentication headers (Bearer for Jev/Laya, `X-API-Key` for GLiDE), `noul` answer validation, candidate-coverage and score-range checks, loopback-only URL enforcement, and size/redirect behavior. The built-binary missing-credential test proves no code-host access or model call occurs without `@auto`.
  - Integration tests launch the compiled binary with temporary `gh` and `glab` fixtures and verify observable stdout/stderr, exit status, body safety, CRUD mapping, filter and metadata arguments, attachment routing/version gating, body-change behavior, concurrency guards, bounds, and the `@auto` path against an OpenAI-compatible local fixture. The `glab` fixture also pins the exact argument vector, so the GitLab grammar's flag names stay the ones `glab` documents, and proves the GitLab grammar rejects GitHub verbs and flags before any provider call. No test creates or edits a real GitHub or GitLab issue or pull request, and no test calls a hosted model.
- Release verification builds `workctl` and smoke-runs one success and one failure path against the isolated fixture.
