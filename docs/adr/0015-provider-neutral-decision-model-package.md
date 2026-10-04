# 0015: Provider-neutral DecisionModel adapters

- Status: Accepted
- Date: 2026-10-04
- Tracker: [issue #8](https://github.com/0spura/workctl/issues/8)
- Supersedes: [ADR-0010](./0010-automatic-issue-labels.md) for the `--auto-labels` opt-in flag, and [ADR-0014](./0014-native-decision-model-adapters.md) for adapter package structure and the local-model contract. The 0.8 selection policy carries forward.

## Context

Automatic label selection must use one application-facing model contract without coupling model integrations to a particular code-host provider. Native decision models return typed answers; local inference servers vary. The CLI's current GitHub provider is not a constraint on a model adapter that may also be called by future GitLab or other code-host adapters.

## Decision

Create a `DecisionModel` package with this adapter structure:

```text
DecisionModel
├── JevAdapter
├── LayaAdapter
├── GLiNERDecideAdapter
├── GLiDeRAdapter
└── LLMDecisionAdapter
```

Adapters accept provider-neutral work-item text and a candidate-label catalog, and return validated label probabilities. The package does not depend on GitHub/GitLab SDK or provider response types. GitHub issue create/edit are the current callers; PRs remain outside this labeling change.

- `JevAdapter` uses Jev's native `/v1/systemone` API and Bearer auth.
- `LayaAdapter` uses Laya's native `/v1/systemone` API and Bearer auth.
- `GLiDeRAdapter` uses Fastino GLiDE's native `/v1/systemone` API and `X-API-Key` auth.
- `GLiNERDecideAdapter` supports Fastino GLiNER2.5-Decide local inference through its dedicated local service contract.
- `LLMDecisionAdapter` supports arbitrary local models only through an OpenAI-compatible Chat Completions service. It is bounded to loopback addresses, sends structured per-label classification input, and accepts only validated JSON scores. It is not a hosted OpenAI adapter.

`DECISION_MODEL` selects one of these explicit adapters. Hosted credentials use `DECISION_MODEL_API_KEY` where needed; local inference requires no hosted key. The local endpoint is configurable and loopback-only. Remote endpoints remain fixed. The existing >=0.8 selection threshold is retained; scores from different models are not assumed to be equally calibrated.

## Consequences

- Model protocol ownership is isolated from code-host providers. Adding a new work-item provider does not require duplicating model transport.
- “Any local model” means any model served through the supported OpenAI-compatible Chat Completions contract; arbitrary model files and server-specific protocols are not auto-detected.
- GLiNER2.5-Decide retains a native local adapter in addition to the generic local LLM adapter.
- The current CLI supports GitHub only; this ADR does not claim GitLab implementation.
- `@auto` remains the existing label-list sentinel; no separate auto-label flag is added. The sentinel is rejected for label removal and may be combined with manual labels.
- Issue edits classify proposed final text and retain the timestamp concurrency guard. No automatic classification occurs without `@auto`.

## Alternatives

- One provider-specific classifier embedded in GitHub issue commands: rejected because it prevents reuse from another code-host provider.
- One generic OpenAI-compatible adapter for every model: rejected because Jev, Laya, GLiNER-Decide, and GLiDE expose native decision contracts.
- Automatically load any local model artifact: rejected because model families require distinct tokenization, inference, and score semantics; a standard local service contract is safer.

## Sources

- [Jev API](https://thejevai.com/docs)
- [Laya API docs](https://laya.studio/docs)
- [Fastino GLiDE](https://docs.fastino.ai/concepts/glide.md)
- [Fastino GLiNER2.5-Decide model card](https://huggingface.co/fastino/GLiNER2.5-Decide)

## Traceability

- Requirements: [RF-WI.1](../srs.md) and [RF-WI.4](../srs.md)
- Architecture: [docs/architecture.md](../architecture.md)
