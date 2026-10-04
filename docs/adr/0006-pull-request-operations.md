# 0006: Pull request operations in the CLI

- Status: Superseded in part by [ADR-0008](./0008-gh-native-metadata-and-attachments.md)
- Date: 2026-10-02
- Tracker: none; the user authorized direct implementation in this repository.
- Refines: [ADR-0004](./0004-workctl-rust-cli.md) and [ADR-0005](./0005-attachments-and-non-rewrite-body-edits.md). Pull request operations and body-edit behavior remain; ADR-0008 adds optional metadata and documents attachment support in newer `gh`.

## Context

The retired MCP server exposed nine pull request tools (`create_branch`, `create_pr`, `update_pr`, `get_pr`, `list_prs`, `get_pr_checks`, `merge_pr`, `get_pr_diff`, `submit_pr_review`). ADR-0004 replaced that server with this CLI but shipped only the issue surface, so the agent definitions that had used those tools fell back to raw `gh pr` invocations — the context cost the CLI exists to remove. The user asked for pull request support in the CLI.

Three provider facts bounded the design:

- `gh` implements these operations as subcommands, so the CLI wraps them instead of speaking REST or GraphQL directly. The investigated local `gh` 2.87.3 lacks `--attach`; GitHub CLI 2.99.0 supports documented attachments (see ADR-0008). No subcommand submits inline review comments.
- `gh pr checks` reports failing checks with exit status 1 and pending with 8 while still printing a valid report on stdout, and prints `no checks reported` on stderr when the pull request has none. `gh pr view` on a non-pull-request number exits non-zero. Both outcomes need the exit status and stderr, which the existing `run_gh` helper collapses into a generic failure.
- `gh pr merge` requires a merge method and prompts interactively when none is given.

## Decision

- **The pull request surface mirrors the `gh pr` subcommands**: `pr create`, `list`, `show`, `diff`, `checks`, `review`, `merge`, `update`, `ready`, `close`, and `reopen`. Arguments are typed and validated in the CLI; no raw provider arguments pass through.
- **A second narrow seam.** `PullRequestProvider` sits beside `WorkItemProvider` rather than widening it: the two carry different request types and different failure signals, and no caller needs both on one value. Each is implemented by its own repo-scoped struct, and both share the same `gh` process helpers.
- **Body changes are shared, not reimplemented.** `pr update` exposes the same body-change set as `issue edit` — `--body`/`--body-file`, `--append-body`/`--append-body-file`, `--replace-section` with `--section-body`/`--section-body-file`, and `--patch-file` — through one flag group (`cli/common.rs`), one resolver (`resolve_body_change`), and one command-side validation (`commands/support.rs`). The fetch → guard → resolve → single-write shape is identical for both resources.
- **No default merge policy.** `--method` reaches `gh` only when the caller supplies it; `workctl` never picks squash, merge, or rebase on the caller's behalf, and a `gh` failure to infer one surfaces as `github_cli`.
- **Exit-status-sensitive commands read the raw result.** `run_gh_raw` returns the bounded process output (status, stdout, stderr) for `pr checks` and `pr show`, which interpret the status themselves. The runner now captures stderr under the same 8 MiB cap instead of draining and discarding it, so the semantics add no unbounded I/O.
- **Create links issues through the body keyword.** `--closes NUMBER` (repeatable) appends one `Closes #N` line per issue, because `gh` exposes no closing-issue flag.
- **Errors stay neutral where both resources use them.** `not_pull_request` is new; `section_not_found`, `conflict`, and `patch_conflict` lost their issue-specific wording because pull requests raise them too.

## Consequences

- `pr create` and `pr update` cost one fetch plus one write, like `issue create`/`edit`: `gh` prints a URL or nothing, so the adapter reads the pull request back to return typed data.
- `pr checks` returns an empty list rather than an error when a pull request has no checks, and returns the failing report rather than an error when checks fail. A caller distinguishes the two from the payload, not the exit status.
- `pr update` reuses the body-mutation policy, including the exact-match patch contract and the `--expect-updated-at` guard, so the non-rewrite guarantee covers pull request bodies without a second implementation.
- The pull request slice is verified by nine integration tests against the shared `gh` fixture, and the read/write paths were exercised against a real pull request on GitHub. `pr merge` is the one operation not exercised against a live pull request, because a real merge rewrites `main`; it is covered by the fixture test.
- Capability that the retired MCP tools had and this surface does not: branch creation/checkout, listing the pull requests linked to an issue, and inline review comments.

## Alternatives

- **Widen `WorkItemProvider` with the pull request methods**: rejected. It would force one type to implement two unrelated contracts, make every future provider satisfy both, and blur which failure signals belong to which resource.
- **Implement only the read side** (`list`, `show`, `diff`, `checks`): rejected. The raw-`gh` fallback the user wants removed is heaviest on the write side — opening, reviewing, and landing a pull request — so a read-only surface would leave most of the problem in place.
- **Default `--method` to squash**: rejected. A silent default chooses a history policy for the caller; an explicit error from `gh` is recoverable, an unexpected squash is not.
- **Keep delegating pull requests to raw `gh pr` in the agent definitions**: rejected. That is the status quo the change exists to remove.
- **Reimplement `create_branch`, including its checkout**: rejected. The CLI must not mutate the caller's working tree, and `git switch -c` is the native path; the retired tool's checkout was an MCP-era side effect, not a requirement.
- **Submit inline review comments through `gh api`**: deferred. The REST "Create a review" endpoint accepts a `comments` array, but no consumer needs line-level comments yet and it would introduce a second request shape for no observed gain.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-CLI.1, RF-PR.1–RF-PR.11, RF-OUT.1, RNF-DOM.1
- Architecture: [docs/architecture.md](../architecture.md)
- Superseded tool surface: the `tracker_*` pull request tools removed in [ADR-0004](./0004-workctl-rust-cli.md)
