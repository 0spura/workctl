# 0007: Remove unsupported issue attachment upload

- Status: Superseded by [ADR-0008](./0008-gh-native-metadata-and-attachments.md)
- Date: 2026-10-02
- Tracker: none; direct implementation authorized by the user.
- Supersedes: the issue-attachment portion of [ADR-0005](./0005-attachments-and-non-rewrite-body-edits.md). ADR-0005's body-edit, patch, and concurrency decisions remain in force. The attachment removal was superseded after `gh` 2.99.0 added documented `--attach` support.

## Context

ADR-0005 selected `gh issue create|edit --attach` for issue uploads. A real invocation against GitHub CLI 2.87.3 rejected the option with `unknown flag: --attach`; neither issue subcommand accepts it. `workctl` therefore advertised and tested a capability that always failed against the real provider. The fixture accepted arbitrary arguments, so its attachment-routing test did not detect the mismatch.

GitHub has no documented issue-attachment upload endpoint. Calling an undocumented web endpoint would leave the supported `gh` integration and authentication boundary and would depend on an unstable private contract.

## Decision

Remove issue attachment upload from the product surface. Delete `--attach` from issue create/edit, remove local file parsing and attachment fields from provider request types, and route all issue writes through the existing `gh api` JSON requests. Do not call undocumented upload endpoints. Keep the existing non-rewrite body-edit and concurrency behavior.

The CLI rejects `--attach` during argument parsing; it does not invoke `gh`. Issue and pull-request attachments are out of scope until a documented, supported upload interface exists and is approved.

## Consequences

- Callers can no longer pass `--attach` to `workctl issue create` or `workctl issue edit`; they receive the CLI's unknown-argument error before provider access.
- No issue attachment path is inspected or sent to a provider.
- The issue provider has fewer request fields and no special `gh issue` command fallback; create/edit use `gh api` with JSON on stdin.
- README, vision, architecture, and SRS describe attachments as unsupported. The SRS verifies that the unsupported flag is rejected before `gh` runs.

## Alternatives

- **Call GitHub's undocumented web upload endpoint via `gh api`:** rejected; private endpoint and payload contracts are unstable and unverified.
- **Keep the existing option and document the defect:** rejected; it guarantees failure for callers and misstates the supported CLI contract.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-WI.1, RF-WI.4.
- Architecture: [docs/architecture.md](../architecture.md).
- Superseded capability decision: [ADR-0005](./0005-attachments-and-non-rewrite-body-edits.md).
