# 0026: Explicit Project option selection on issue edit

- Status: Accepted
- Date: 2026-10-07
- Tracker: none; tracker work was explicitly set aside.
- Supersedes: the prohibition on automatic selection during edit in [ADR-0021](./0021-github-project-field-editing.md). Creation selection and its tie-error policy remain unchanged.

## Context

The user approved extending existing Project field edits with `--project-field 'Priority=@auto'`, using the final issue text and the real Project options. Low confidence and ambiguous results must not overwrite current values.

## Decision

Treat `NAME=@auto` on GitHub issue edit as explicit selection for that named single-select or iteration field. Discover options using the existing in-scope Project schema, then share one configured DecisionModel request per issue with any automatic labels. Classify final proposed title/body. A unique best score >= 0.8 produces a field write; low confidence or tied best scores produce no write for that field. No defaults or configured `autoSelectFields` are reapplied. Invalid fields and model failures fail before writes. Preserve issue timestamp guards and reset generated assignments for each batch target.

## Consequences

Automatic edit selection reuses existing adapters, schema validation, and serial Project writes without new storage, endpoints, or dependencies. The existing field value is preserved by omission rather than a read/restore operation. Project fields still lack issue-revision concurrency protection. If no changes remain after selection, return the current issue without mutations. Create retains its existing fail-on-tie policy.

## Alternatives

Implicit selection of creation defaults during edit was rejected because it would modify unrequested fields. Failing the whole edit on a field tie was rejected in favor of preserving that field while allowing independent explicit edits.

## Traceability

- [RF-WI.6](../srs.md).
- [Command flow](../architecture.md#command-flow).
- `tests/cli_issues.rs` automatic-edit boundary scenarios and Project choice unit tests.
