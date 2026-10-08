# 0025: Separate blockers from issue view

- Status: Accepted
- Date: 2026-10-07
- Tracker: none; tracker work was explicitly set aside.
- Supersedes: the blocked-by/blocking query and output portion of [ADR-0023](./0023-github-issue-view-relationships.md). The separate command in [ADR-0024](./0024-compact-github-issue-blocker-chains.md) remains unchanged.

## Context

The user clarified and approved that blocker information should not be repeated in `issue view`, while `issue blockers` must remain available. GitHub has native dependency relationships distinct from references written in an issue body.

## Decision

Remove `blocked_by` and `blocking` fields from GitHub `issue view` JSON and text, and do not request those connections in its GraphQL query. Preserve the issue record, body, type, parent, and sub-issues. Keep native dependency editing and the separate bounded blocker-chain command unchanged.

## Consequences

Viewing an issue performs no dependency lookup and does not repeat native blocker information. Consumers needing dependency chains use `issue blockers`; no compatibility fields or aliases remain. Sub-issue pagination and validation still fail closed.

## Alternatives

Keeping dependency fields in JSON only was rejected because the approved view contract excludes blocker information, rather than merely hiding text rows. Removing the blocker command was explicitly rejected by the user.

## Traceability

- [RF-WI.3 and RF-WI.7](../srs.md).
- [Provider seams](../architecture.md#provider-seams-and-models).
- `tests/cli_issues.rs` verifies issue body/hierarchy without blocker fields and preserves blocker-chain behavior.
