# 0021: Edit GitHub Project fields independently of issue metadata

- Status: Accepted
- Date: 2026-10-07
- Tracker: [issue #13](https://github.com/0spura/workctl/issues/13)
- Supersedes: the creation-only Project field command surface in [ADR-0018](./0018-github-project-dynamic-fields.md). Creation defaults and [ADR-0019](./0019-decision-model-project-field-selection.md) remain unchanged.

## Context

The approved issue #13 extension requires setting and clearing existing Project fields through one issue-edit invocation. Issue metadata and Project field writes are separate remote operations without a transaction or shared revision.

## Decision

- Accept repeated `issue edit --project-field NAME=VALUE` and `--clear-project-field NAME` against the repository-allowlisted configured Project.
- Discover and validate all requested fields before mutations. Reject duplicate field requests, set/clear overlaps, incompatible Project removal, and mismatched explicit Project titles.
- Apply only explicit changes. Do not reapply creation defaults or automatic Project selection, and do not automatically add membership.
- Preserve issue body resolution and timestamp guards. Field-only edits skip `gh issue edit`; combined edits write issue metadata first, then Project fields serially.
- After any successful issue or field write, a later failure returns generic `partial_success` with the resource and completed/pending operations. Do not retry automatically.

## Consequences

A caller can resume pending field changes without recreating the issue. The issue timestamp does not protect concurrent Project field edits; no Project-revision guard is claimed.

## Alternatives

- Reapply creation defaults during edits: rejected because omitted fields must remain unchanged.
- Automatically add missing membership: rejected because editing fields must not implicitly change membership.

## Traceability

- Contract: issue #13, Project field editing section.
- Requirements: RF-WI.4 and RF-WI.6 in [SRS](../srs.md).
- Flow: [architecture](../architecture.md).
- Verification: Project edit integration scenarios in `tests/cli_issues.rs`.
