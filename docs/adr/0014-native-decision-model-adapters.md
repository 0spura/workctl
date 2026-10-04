# 0014: Normalize native decision-model adapters

- Status: Accepted
- Date: 2026-10-04
- Tracker: [issue #8](https://github.com/0spura/workctl/issues/8)
- Supersedes: [ADR-0010](./0010-automatic-issue-labels.md) for the automatic-label opt-in and backend contract; Jev's 0.8 selection policy remains.
- Superseded by: [ADR-0015](./0015-provider-neutral-decision-model-package.md) for adapter package structure and the local-model contract. The `@auto` sentinel decision carries forward.

## Context

Automatic issue labels already call Jev only on an explicit opt-in path. The separate `--auto-labels` flag adds CLI surface next to existing label arguments and ties the implementation to one model API. The user wants the existing labels option to activate automatic selection, and wants native decision models available both hosted and locally. Fastino exposes GLiDE through a typed hosted System One endpoint and publishes GLiNER2.5-Decide as an open-weight local model. Their wire formats differ, but both return bounded label decisions and confidence values.

## Decision

Use the existing label arguments as the opt-in sentinel: `issue create --label @auto` and `issue edit NUMBER --add-label @auto`. Reserve `@auto`; reject it in `--remove-label`. It can be combined with explicit labels. No separate automatic-label flag is added. Pull requests remain outside automatic classification.

Add one application-facing decision seam that returns validated `{label, probability}` suggestions. Keep provider protocols inside adapters:

- `jev-latest`: Jev's fixed `/v1/systemone` endpoint with Bearer authentication.
- `fastino/GLiDE`: Fastino's fixed `/v1/systemone` endpoint with its native API-key header.
- `fastino/GLiNER2.5-Decide`: a loopback-only local HTTP service wrapping Fastino's official `gliner2` Python library and local open weights; the service exposes the normalized workctl request/response contract.

`DECISION_MODEL` selects one of the supported model IDs. `DECISION_MODEL_API_KEY` supplies hosted credentials; the selected adapter applies the provider's native authentication format. `DECISION_MODEL_BASE_URL` configures only the local service and must resolve to loopback. Remote endpoints are fixed, not user-overridable. The model server loads `fastino/GLiNER2.5-Decide` locally and never calls a hosted inference API.

All providers return validated probabilities in [0,1]. Preserve the existing 0.8 application threshold. Issue edits classify the final proposed title/body and retain the timestamp guard. Missing configuration, credentials, service, or valid model output prevents the GitHub write. Model selection and credentials are read only when `@auto` appears.

## Consequences

- Manual label names and the automatic trigger share existing CLI fields; `@auto` is a reserved name.
- Native protocols remain isolated. Jev and GLiDE use System One questions; local GLiNER uses its classification schema and per-label confidence.
- GLiDE is hosted in current public documentation; local execution uses GLiNER2.5-Decide.
- Model confidence is provider-native and may not be calibrated identically across models. The 0.8 threshold is retained for compatibility and must be evaluated on representative issue data before changing it.
- The local service adds a Python 3.10+ runtime and the official `gliner2[local]` dependency. It binds to loopback and can load the model from a local path or the model hub; the first hub load may download weights.
- The adapter seam is scoped to issue-label classification; it does not promise arbitrary decision tasks, provider plugins, PR classification, or fallback between models.

## Alternatives

- Keep `--auto-labels`: rejected because the user wants fewer CLI arguments and an existing label value can express the opt-in.
- Use generic chat completions for every model: rejected because Jev, GLiDE, and GLiNER expose native typed-decision contracts and scores.
- Use GLiDE for local inference: not selected; current official documentation documents GLiDE as hosted, while GLiNER2.5-Decide has local open weights.
- Preserve only Jev: rejected because the user requested additional hosted and local decision models.

## Traceability

- Requirements: [RF-WI.1](../srs.md), [RF-WI.4](../srs.md), and the decision-model security requirement in [docs/srs.md](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
