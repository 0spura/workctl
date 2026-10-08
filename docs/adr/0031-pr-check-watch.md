# Bounded GitHub pull-request check watch

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

Agents need to wait for GitHub pull-request checks without implementing polling, interpreting provider output, or risking an unbounded `gh` process. `gh pr checks` already exposes native `--watch`, `--interval`, and `--fail-fast` controls and may return non-zero while printing valid failure or pending results.

## Decision

Extend `workctl pr checks NUMBER` with opt-in `--watch`, forwarding `--watch`, optional `--interval` (1–300 seconds), and `--fail-fast` directly to `gh`. `--interval`, `--fail-fast`, and `--watch-timeout` require `--watch`. Watch mode has a 600-second default absolute process deadline; `--watch-timeout` may override it from 1 to 3600 seconds. Keep existing 30-second subprocess behavior for every non-watch operation.

Continue parsing valid JSON output regardless of `gh` exit status, preserving failing and pending check records. Preserve the existing empty-check special case. A process deadline returns the safe `timeout` error with no successful output; do not emit raw provider diagnostics or add polling/retry logic.

## Consequences

- The checks command can wait for completion while retaining native GitHub CLI semantics.
- A finite, explicit upper bound limits unattended hangs; longer waits require a caller to rerun or choose another workflow.
- The provider remains responsible for translating process failures and check evidence; no model/JEV behavior is involved.

## Alternatives

- Add a workctl polling loop: rejected because it duplicates `gh` watch semantics and introduces extra requests and state handling.
- Reuse the normal 30-second deadline: rejected because a watch would routinely expire before checks complete.
- Permit an unlimited deadline: rejected because a stalled provider process could hang an agent indefinitely.

## Traceability

- Active behavior: [SRS RF-PR.14](../srs.md#rf-pr14)
- Existing report contract: [SRS RF-PR.5](../srs.md#rf-pr5)
- Process and provider boundaries: [architecture](../architecture.md)
- Native options confirmed by `gh pr checks --help`.
