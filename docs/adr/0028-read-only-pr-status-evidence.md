# Read-only pull-request status evidence

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

Agents currently need separate calls to inspect pull-request review/mergeability metadata and required checks. A concise status command can reduce that coordination without adding an automated merge decision.

## Decision

Add `workctl pr status NUMBER` as a read-only GitHub command. Request only number, title, state, draft, URL, base/head refs, mergeability, and review decision from `gh pr view`; then retrieve `gh pr checks --required`. Return those observed fields and check records in a dedicated JSON object and concise text output. Do not fetch or print the pull-request body and do not emit a derived `ready` verdict.

The two provider reads are sequential and not an atomic snapshot. A failure in either read fails the whole command without partial success output. An empty required-check result and null mergeability/review values remain inconclusive.

## Consequences

- The public GitHub `pr` grammar gains `status`.
- `PullRequestProvider` owns the combined provider-specific read, using one authenticated provider instance.
- Consumers receive source evidence and remain responsible for interpreting organization-specific merge policy.
- Existing `pr view` and `pr checks` contracts remain unchanged.

## Alternatives

- Derive a boolean `ready_to_merge`: rejected because workctl does not know every repository's branch-protection and review policy, and empty/incomplete status data is not proof of readiness.
- Make agents issue `view` and `checks` separately: rejected because callers would duplicate the orchestration and may omit required checks.

## Traceability

- Active behavior: [SRS RF-PR.12](../srs.md#rf-pr12)
- Technical seam and failure behavior: [architecture](../architecture.md)
