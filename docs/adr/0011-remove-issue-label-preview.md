# 0011: Remove the standalone issue label preview command

- Status: Accepted
- Date: 2026-10-03
- Tracker: [issue #8](https://github.com/0spura/workctl/issues/8)
- Supersedes: ADR-0009's standalone `issue suggest-labels` command and ADR-0010's statement that the preview command remains available.

## Context

The standalone read-only Jev preview command duplicates a separate user-facing label workflow and is not needed for the approved opt-in issue auto-label path. GitHub's native metadata fields already support explicit labels on issue and pull-request create/edit, including `--label`, `--add-label`, and `--remove-label`.

## Decision

Remove `workctl issue suggest-labels`, its success output, and its CLI contract. Keep `--auto-labels` on issue create/edit as an explicit opt-in. Automatic suggestions continue to be applied through GitHub's native issue create/add-label arguments; caller-selected issue and PR labels continue to use GitHub's native flags. Do not add Jev classification to PR operations.

## Consequences

- The issue command group has four commands: `create`, `list`, `show`, and `edit`.
- Jev is contacted only when `--auto-labels` is explicitly supplied to issue create/edit.
- PR label create/update remains available through GitHub-native metadata fields without Jev or a separate label API.
- No project board was attached to issue #8: the available boards were unrelated products or a temporary workctl smoke-test board, and there is no dedicated workctl project to select safely.

## Alternatives

- Keep the preview command alongside automatic labeling: rejected because it is a redundant command surface after the approved auto-label flow.
- Send PR content to Jev: rejected; native PR label fields meet the explicit-label need without adding a new content-disclosure boundary.

## Traceability

- Requirements: [RF-CLI.1](../srs.md), [RF-WI.1](../srs.md), [RF-WI.4](../srs.md), [RNF-SEC.2](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
- Supersedes: [ADR-0009](./0009-jev-label-suggestions.md) for the standalone command; [ADR-0010](./0010-automatic-issue-labels.md) remains in force for explicit issue auto-labeling.
