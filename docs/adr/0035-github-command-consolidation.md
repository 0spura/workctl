# 0035: Consolidate GitHub lifecycle commands

- Status: Accepted
- Date: 2026-10-10
- Supersedes: [ADR-0016](./0016-gh-verb-parity.md) for GitHub lifecycle command grouping; [ADR-0033](./0033-issue-and-pr-conversation-commands.md) for separate close/reopen/unlock public verbs; [ADR-0006](./0006-pull-request-operations.md) for deferring inline review comments.

## Context

The GitHub grammar mirrors `gh`, but separate close/reopen and lock/unlock verbs expand the agent-facing surface for inverse operations. The user approved consolidating those operations into resource edit/lock commands and adding inline comments to the existing PR review command, with no new subcommands. The approved design keeps resource/action/options semantics familiar while letting skills describe one canonical invocation.

The native `gh issue edit` and `gh pr edit` commands do not accept state. `gh pr review` does not accept line comments. Therefore the public grouping cannot be represented as a single native CLI invocation for every operation. Existing GitHub provider adapters already own native commands, stdin transport, bounded process execution, and partial-success reporting.

## Decision

- `issue edit` and `pr edit` accept `--state open|closed`; omission preserves the current state. Move transition-specific options to edit: issue `--comment`, `--reason`, `--duplicate-of`; PR `--comment`, `--delete-branch`. A transition-only invocation is valid.
- Validate transition modifiers before provider access. `--comment` requires `--state`; `--reason`, `--duplicate-of`, and `--delete-branch` require `--state closed`. Keep ordinary description body flags distinct from the transition comment.
- Keep native `gh issue/pr edit`, close, and reopen operations internally. For a combined edit, execute ordinary issue/PR fields first, configured Project fields next, state transition next, transition comment last, then read back once. Never call an empty native edit. Multi-issue edits execute serially and stop at first failure. A failure after an acknowledged write returns `partial_success` naming ordered completed and pending operations; no retries or rollback. A transition comment uses the existing stdin-only comment path rather than child argv.
- `issue lock` and `pr lock` accept `--undo`; without it they lock, with it they unlock. `--reason` conflicts with `--undo`. Remove public `unlock` commands and do not retain aliases. Preserve `{number,target,locked}` output.
- `pr review` accepts repeatable `--inline LOCATION TEXT` and `--inline-file LOCATION FILE`, where location is `PATH:LINE[:left|right]`; side defaults to `right`. Parse the numeric line from the end so a path may contain colons. Lines are positive source-line numbers, not diff positions. Inline review comments are single-line; thread replies, ranges, and file-wide comments remain out of scope.
- Inline bodies are nonblank UTF-8; each text source retains the existing 1 MiB limit and all inline payload text/paths together is capped at 1 MiB. `--body-file -` and `--inline-file LOCATION -` share stdin and may have only one consumer. Resolve and validate all input before provider writes.
- For inline COMMENT and REQUEST_CHANGES events require a nonblank authored summary in `--body` or `--body-file`; APPROVE may omit it. Existing summary-only reviews preserve their behavior.
- Summary-only reviews continue using `gh pr review`. An inline review reads the current PR head SHA and submits one JSON payload to GitHub's create-review REST endpoint through `gh api --input -`, with explicit event, head `commit_id`, optional summary and all line comments. Use `line`/`side`, never legacy diff `position`. Do not submit one request per comment, fall back to standalone comments, retry, or claim success when the write outcome is unconfirmed. Keep authentication with `gh`; serialize the payload on stdin, never argv.
- Keep GitLab grammar and behavior unchanged.

## Consequences

- GitHub issue subcommands become seven; GitHub PR subcommands become fifteen. Removed close/reopen/unlock verbs fail as unknown commands; no aliases preserve them.
- Combined edit plus transition is explicitly nontransactional. Native edit and transition may be separate remote writes. A later failure reports partial progress and requires inspection before retry. Ordinary edits are attempted before state transition; consequently an edit to a closed PR may fail before a requested reopen is applied.
- A failed or timed-out inline-review write may have reached GitHub. Return a safe error indicating the outcome is unconfirmed; never retry automatically.
- Inline comments are anchored to the head SHA observed for this invocation. GitHub validates that requested path/line/side belongs to the diff; the CLI does not fetch and parse the diff itself.
- Existing work-item / PR output shapes remain stable on success. The canonical SRS defines exact command/flag validation, bounds, ordering, and failure reporting.

## Alternatives

- Keep separate verbs: rejected because it preserves redundant lifecycle commands against the approved compact interface.
- Forward `--state` to `gh edit`: rejected because native help does not support it.
- Reject combinations of ordinary edits and state changes: rejected because it removes the approved combined edit behavior.
- Use a direct HTTP client or replace all metadata edit calls with REST: rejected because it duplicates provider/authentication semantics and is broader than the existing GitHub CLI adapter.
- Create a review and then submit comments separately: rejected because it allows partial reviews and multiple writes when the review API accepts the comment array together.
- Use diff positions or synthesize review text: rejected because positions are not stable source line identifiers and authored content must not be invented.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-CLI.1, RF-WI.4, RF-WI.8–RF-WI.12, RF-PR.6, RF-PR.8, RF-PR.10–RF-PR.11, RF-PR.17–RF-PR.18.
- Architecture: [docs/architecture.md](../architecture.md).
- Superseded decisions: ADR-0016, ADR-0033, and the inline-review deferral in ADR-0006.
