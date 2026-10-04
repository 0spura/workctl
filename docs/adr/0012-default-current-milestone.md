# 0012: Default creation to the current milestone

- Status: Superseded by [ADR-0013](./0013-explicit-current-milestone.md)
- Date: 2026-10-03
- Tracker: [issue #9](https://github.com/0spura/workctl/issues/9)

## Context

Issue and pull-request creation accept GitHub's native `--milestone` field, but callers often need the repository's active milestone and GitHub exposes no universal current-milestone marker. Open milestones may be overdue, undated, or share a due date.

## Decision

On issue and pull-request creation only, if `--milestone` is omitted, query the repository's open GitHub milestones and select the one with the nearest due date today or later. Ignore overdue and undated milestones. If there is no qualifying milestone, create without a milestone. If multiple milestones share the nearest eligible due date, fail before creation and require an explicit `--milestone`. An explicit value skips lookup. Issue edits and PR updates do not apply this default and preserve omitted milestone fields.

## Consequences

- The GitHub provider has one shared milestone-selection path for issue and PR creation.
- Lookup or malformed-response failures prevent creation rather than silently dropping the default.
- GitHub's native create commands continue to set the selected milestone.
- Selection is based on due date, not milestone title, creation order, or repository-specific status conventions.

## Alternatives

- Include overdue milestones by absolute date distance: rejected because an overdue item is not the current upcoming milestone.
- Pick an arbitrary milestone when due dates tie: rejected because the selection would be ambiguous.
- Apply the default on edits/updates: rejected because omission on a patch must preserve existing remote state.

## Traceability

- Requirements: [RF-WI.1](../srs.md), [RF-PR.1](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
