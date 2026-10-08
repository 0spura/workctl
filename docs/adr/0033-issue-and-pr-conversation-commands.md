# Issue and pull-request conversation lifecycle commands

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

The GitHub grammar covered issue create/list/view/edit/blockers and pull request
create/checkout/list/view/status/diff/checks/review/merge/edit/ready/close/reopen/update-branch.
Agents still had to call `gh` directly to close or reopen an issue, to leave a comment on an issue
or pull request, to lock or unlock a conversation, and to open a revert pull request. Those direct
calls bypass the safe provider boundary: bodies were exposed in process arguments, provider
diagnostics reached the caller, and results were unstructured.

`gh` already implements every one of these operations natively (`gh issue close|reopen|comment|lock|unlock`,
`gh pr comment|lock|unlock|revert`), so workctl needs no new remote protocol or dependency.

## Decision

Add nine native `gh` verbs to the GitHub grammar:

- `issue close NUMBER [--comment TEXT] [--reason completed|not planned|duplicate] [--duplicate-of NUMBER|URL]`
- `issue reopen NUMBER [--comment TEXT]`
- `issue comment NUMBER (--body TEXT | --body-file FILE)`; `-` reads standard input
- `issue lock NUMBER [--reason off_topic|resolved|spam|too_heated]`
- `issue unlock NUMBER`
- `pr comment NUMBER (--body TEXT | --body-file FILE)`
- `pr lock NUMBER [--reason off_topic|resolved|spam|too_heated]`
- `pr unlock NUMBER`
- `pr revert NUMBER [--title TEXT] [--body TEXT | --body-file FILE] [--draft]`

Each command forwards only the explicitly requested option as its native `gh` flag and passes no
other default. Comment and revert bodies travel on stdin through `--body-file -`; they never appear
in argv. `--duplicate-of` accepts the same canonical issue reference as `issue edit`.

Close, reopen, comment, lock, and unlock are owned by `WorkItemProvider`; the pull-request
equivalents, plus revert, are owned by `PullRequestProvider`. Both operations stay behind the
existing provider seams rather than a new one, because they reuse the same authenticated `gh`
context and the same safe `github_cli` failure boundary.

Results are compact and structured: issue state changes report `{number, state}`; comments report
`{number, target}`; lock changes report `{number, target, locked}`; revert reports
`{number, pull_request}`, where `pull_request` is the number of the newly opened revert pull request
parsed from the URL `gh pr revert` prints.

## Consequences

- The GitHub issue and pull-request grammars each gain native verbs; the GitLab grammar is unchanged.
- `AppError::invalid_input` now covers a missing comment body, before provider access. `--reason`
  values are restricted by clap to the values `gh` accepts.
- Revert creates remote state. A successful revert whose output cannot be read as a pull-request URL
  returns `provider_response` with no success output.
- Comment and revert bodies remain absent from process arguments, preserving the existing
  shell-safety contract.

## Alternatives

- Shell out to `gh` from the caller: rejected; it bypasses safe error mapping and body handling.
- Extend the generic issue seam with conversation methods: rejected; the pull-request seam already
  owns its own comment, lock, and revert operations, and widening the issue seam would force each
  provider to carry capabilities it does not need.
- Implement revert through the REST API: rejected; `gh pr revert` already performs the operation.

## Traceability

- Active behavior: [SRS RF-WI.8](../srs.md#rf-wi8-close-an-issue), [RF-WI.9](../srs.md#rf-wi9-reopen-an-issue),
  [RF-WI.10](../srs.md#rf-wi10-comment-on-an-issue), [RF-WI.11](../srs.md#rf-wi11-lock-an-issue-conversation),
  [RF-WI.12](../srs.md#rf-wi12-unlock-an-issue-conversation), [RF-PR.16](../srs.md#rf-pr16-comment-on-a-pull-request),
  [RF-PR.17](../srs.md#rf-pr17-lock-a-pull-request-conversation), [RF-PR.18](../srs.md#rf-pr18-unlock-a-pull-request-conversation),
  [RF-PR.19](../srs.md#rf-pr19-revert-a-pull-request)
- Technical flow: [architecture](../architecture.md)
- Native command behavior confirmed by local `gh issue close|reopen|comment|lock|unlock --help` and
  `gh pr comment|lock|unlock|revert --help`.
