# GitHub pull-request merge defaults

- Status: Accepted
- Date: 2026-10-08
- Superseded by: [ADR-0037](./0037-remote-only-branch-deletion.md) for the meaning of `deleteBranch` and `--delete-branch`; the merge-method defaults below still hold.
- Tracker: None

## Context

The CLI should support repository/user defaults for repeated PR merge choices without changing existing behavior when defaults are absent. The user explicitly chose a boolean branch-deletion setting with no inverse command-line option.

## Decision

Add strict configuration under `defaults.github.pr`:

- `mergeMethod`: optional `merge`, `squash`, or `rebase`.
- `deleteBranch`: boolean, default `false`.

Resolve merge method in this order: explicit `pr merge --method`, configured `mergeMethod`, then GitHub CLI inference. Report the effective explicit/configured method in the result; report null when `gh` chooses.

When `deleteBranch` is true, enable branch deletion on every `pr merge`. When false, preserve current behavior unless that invocation supplies `--delete-branch`. The CLI has no per-command inverse option; a true configuration value cannot be overridden for one invocation. Existing config-file precedence remains unchanged, including local `defaults` replacing shared `defaults` as a whole.

## Consequences

- Existing users retain current merge behavior because `deleteBranch` defaults to false and `mergeMethod` is optional.
- Unknown fields and invalid method values remain configuration errors under strict parsing.
- `--delete-branch` remains the only command-line branch deletion option.

## Alternatives

- Add a tri-state per-command override: rejected because the requested contract is a boolean default with only an enabling CLI option.
- Default a merge method: rejected; absent configuration preserves `gh` inference.

## Traceability

- Active behavior: [SRS RF-PR.7](../srs.md#rf-pr7), [SRS RF-CFG.4](../srs.md#rf-cfg4)
- Technical flow: [architecture configuration](../architecture.md#configuration-and-context)
