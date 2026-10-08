# Defer issue relation suggestions until Atlas

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

A standalone `issue suggest-relations` command was considered for proposing GitHub issue relationships from bounded issue search results and a configured decision model. It required sending issue titles and descriptions to the selected model and produced inferred relationships without proving dependency correctness or cycle absence. Atlas is the intended future knowledge/indexing context for this capability, but is not currently available in this repository.

## Decision

Remove `issue suggest-relations` and its implementation, output contracts, tests, and active documentation. Do not retain a CLI alias or a separate relationship-suggestion implementation ahead of Atlas. Keep the native relationship editing and `issue blockers` commands unchanged. Reconsider relationship intelligence as part of the Atlas integration rather than this standalone command.

## Consequences

- The GitHub issue command surface contains `create`, `list`, `blockers`, `view`, and `edit`.
- No issue text is sent to a decision model for relationship suggestions.
- Atlas-backed relationship discovery remains future work; this decision does not select its indexing, retrieval, or model design.
- Existing native relationship operations and blocker diagnostics remain available.

## Alternatives

- Keep the bounded standalone suggestion command: rejected because it would duplicate the future Atlas knowledge/retrieval role and operate without its broader context.
- Automatically apply model-selected relationships: rejected; the present scope is removal, and model suggestions cannot establish correct dependency semantics.

## Traceability

- Supersedes the prior approved direction recorded in ai-memory: `decisions/read-only-issue-relation-suggestions.md`.
- Active command and model-trigger contracts: [SRS](../srs.md)
- The `issue blockers` behavior is retained under RF-WI.7.
