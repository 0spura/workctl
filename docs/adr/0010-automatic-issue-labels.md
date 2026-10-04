# 0010: Opt-in automatic Jev labels for issue writes

- Status: Accepted
- Date: 2026-10-02
- Tracker: [issue #8](https://github.com/0spura/workctl/issues/8)
- Supersedes: ADR-0009's decision never to apply Jev suggestions, only for issue create/edit when `--auto-labels` is supplied.

## Context

Automatic issue labels reduce manual selection but issue text must never be disclosed implicitly. The user approved opt-in classification for issue creation and edits, with edits additive and existing labels preserved. Automatic writes require a conservative probability boundary and must not silently disclose issue content.

## Decision

Add `--auto-labels` to `issue create` and `issue edit`. Both modes require `JEV_API_KEY`, classify the issue title/body with the existing label catalog, and add only labels with a Noul probability of at least 0.8. Explicit labels continue to work and combine with automatic labels. If Jev or catalog retrieval fails, fail before the issue write.

For edits, classify the final proposed title/body, including the resolved body mutation. Fetch the current issue before classification, check any caller-provided `--expect-updated-at`, and set the provider edit guard to that fetched timestamp when the caller omitted one. The provider's existing pre-write fetch then rejects a concurrent issue change during the Jev request. Automatic labels are additive; they do not remove existing or explicitly supplied labels.

Only the two explicit `--auto-labels` paths disclose issue title/body and repository label names/descriptions to Jev. Pull-request create/update remain explicit-only; including them would be a separate disclosure decision. Selected labels are passed to GitHub through its native create/add-label options.

## Consequences

- Jev remains an opt-in decision service; issue writes remain owned by `gh`.
- The 0.8 threshold favors precision over recall and is fixed rather than a per-call policy surface.
- Auto-label issue edits perform an additional issue read before the provider's guarded write, trading one extra GitHub read for protection against applying classifications to stale content.
- There is no standalone Jev label preview command; automatic application is opt-in.
- Tests use local Jev fixtures and missing-key CLI paths; no live issue or Jev request is used.

## Alternatives

- Apply every suggestion: rejected because lower-confidence labels raise false-positive risk.
- Send issue content to Jev on every create/edit: rejected because it would disclose content without explicit opt-in.
- Apply labels on pull requests too: deferred; PR title/body disclosure was not included in the approved issue scope.

## Traceability

- Requirements: [RF-WI.1](../srs.md), [RF-WI.4](../srs.md), [RNF-SEC.2](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
- Supersedes: [ADR-0009](./0009-jev-label-suggestions.md) for the stated scope.
