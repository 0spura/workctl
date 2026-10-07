# 0020: Withdraw the GLiNER2.5-Decide adapter and bundled Python service

- Status: Accepted
- Date: 2026-10-07
- Tracker: none (direct user request)
- Supersedes: the `fastino/GLiNER2.5-Decide` local-service decision in [ADR-0014](./0014-native-decision-model-adapters.md) and the `GLiNERDecideAdapter` / native local adapter portion of [ADR-0015](./0015-provider-neutral-decision-model-package.md). Everything else in both records, including the `@auto` sentinel, the >= 0.8 threshold, and the provider-neutral package, stays in force.

## Context

ADR-0014 and ADR-0015 added `fastino/GLiNER2.5-Decide` as a native local decision model, delivered as a bundled
loopback-only Python service (`decision-model-server/`) wrapping Fastino's official `gliner2` library and open
weights. That choice carries a Python 3.10+ runtime, a `pip` dependency set, and a separately versioned service
process alongside the Rust binary.

The user now wants that optional server and its GLiNER-dependent adapter removed. Jev, Laya, GLiDE, and the
generic local OpenAI-compatible adapter remain; the last one already covers local inference over a documented
contract without shipping a service. Nothing in this change alters `@auto` label selection or
`autoSelectFields` behavior.

## Decision

Withdraw GLiNER2.5-Decide from the supported decision models:

- Remove `fastino/GLiNER2.5-Decide` from the configurable `DECISION_MODEL` values; the supported adapters are
  `jev-latest`, `laya`, `fastino/GLiDE`, and `local/<model-id>`.
- Delete the `GLiNERDecideAdapter` and its module entry, and delete the bundled `decision-model-server/`
  (its `server.py` and `requirements.txt`). No Python runtime or service dependency remains.
- Keep `DECISION_MODEL_BASE_URL` as the loopback-only endpoint for the generic local adapter, which continues to
  target any local service implementing OpenAI-compatible Chat Completions with JSON-object output.
- Leave the `@auto`/`--add-label @auto` sentinel, the allowlisted `labelCandidates`, the Project `autoSelectFields`
  selection, the single shared model request, the >= 0.8 threshold, and the provider-neutral package contract
  unchanged.

## Consequences

- Local inference is available only through the generic OpenAI-compatible contract; model families that expose
  only a native local protocol are no longer supported by a bundled service.
- No Python runtime, `gliner2` dependency, or extra loopback service is required to build or run `workctl`.
- The adapter surface shrinks to two hosted native adapters plus one local generic adapter; the removal
  weakens the ADR-0015 claim that a native local adapter exists alongside the generic local LLM adapter.
- Historical records are preserved: ADR-0014 and ADR-0015 keep their original text and gain only a supersession
  pointer to this record, so prior GLiNER evidence is not overwritten or invalidated.

## Alternatives

- Keep the adapter and service as an optional, undocumented path: rejected because the user requested removal of
  the Python server and its GLiNER-dependent adapter, not a quieter default.
- Keep the adapter but delete the bundled service, requiring users to run their own: rejected because the adapter's
  contract was defined against the bundled service and no maintained substitute exists.
- Replace GLiNER with another native local adapter: out of scope; no replacement model was requested, and the
  generic OpenAI-compatible adapter already serves local inference.

## Traceability

- Requirements: [RF-WI.1](../srs.md) and [RNF-SEC.2](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
- Superseded records: [ADR-0014](./0014-native-decision-model-adapters.md) and [ADR-0015](./0015-provider-neutral-decision-model-package.md).
