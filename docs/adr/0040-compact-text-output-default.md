# ADR 0040: Compact text output by default

- Status: Accepted
- Date: 2026-10-10
- Contract: 0spura/workctl#29; RF-OUT.1 and RF-CFG.7
- Supersedes: ADR 0004's implicit JSON output default and ADR 0038's output-format precedence and success-only selection.

## Context

Full resource acknowledgements repeat submitted bodies and metadata in agent context. The approved contract asks for identity and outcome without sacrificing requested read content or explicit machine-readable output.

## Decision

Use text unless an explicit `--format` or configured `defaults.output.format` selects another format. Apply the selection to successes and errors. Keep existing explicit JSON schemas.

Represent mutation outcome separately from the serialized resource. Text acknowledgements contain only the resource identity and outcome, such as `#123 created` or `!5 updated`. Existing provider decisions establish true no-ops and queued merges without additional requests. Batch writes retain per-target outcomes.

Read projections retain requested bodies, diffs, notes, discussion identifiers, and operational facts. Validate native provider data before projecting it. Do not substitute raw resource dumps or counts for requested content.

Text errors retain stable codes and safe messages. Partial success retains resource identity and completed/pending operations, including nested failed-item progress. Escape terminal controls; never expose raw provider diagnostics or credentials.

## Consequences

Consumers relying on implicit JSON must explicitly select JSON. Configuration remains an opt-in compatibility route. Text reduces acknowledgement size without changing provider writes, confirmation reads, guards, or authorization. Separate output outcomes prevent presentation changes from widening public JSON resources.

## Verification

Focused CLI integration suites cover GitHub issues/PRs, native batch edits, and GitLab issues/MRs. Output unit tests cover safe partial-success recovery. Built-binary fixture scenarios exercise default text, explicit JSON, and retained requested content without live remote writes.
