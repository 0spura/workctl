# 0019: Select GitHub Project fields through DecisionModel

- Status: Accepted
- Date: 2026-10-07
- Tracker: [issue #13](https://github.com/0spura/workctl/issues/13)
- Supersedes: [ADR-0018](./0018-github-project-dynamic-fields.md), only its exclusion of model-selected Project fields.

## Context

GitHub Project fields are dynamic and must be discovered for the configured Project. The agent should classify allowed values in one issue-create command rather than issue multiple field-edit commands. Project-specific options can be numerous or irrelevant; passing every option to the model would waste tokens and weaken the decision boundary.

## Decision

1. Add `defaults.github.issue.project.autoSelectFields`, an explicit list of Project field names. This list is the allowlist; no CLI opt-in flag is required.
2. During issue creation, discover the configured Project schema. Pass only the configured single-select and iteration fields' valid options to the generic `DecisionModel`, alongside title/body and optional `@auto` label candidates. Use one model request for both classification tasks.
3. An existing configured field value or explicit `--project-field` value suppresses model selection for that field and remains authoritative. This also means a model is not required when all configured auto-select fields already have explicit/default values and `@auto` was not requested.
4. Use the existing score threshold of 0.8. For each field, select its highest-scoring option at or above threshold; leave the field unset below threshold. An exact tie at the best qualifying score fails before issue creation rather than guessing.
5. Missing/invalid model configuration, malformed output, missing configured fields, unsupported field kinds, or unavailable options fail before issue creation. Post-create Project membership/field writes retain ADR-0018's serial partial-success behavior.

## Consequences

- Agents need not issue separate option-query and field-write commands; the allowlisted options are resolved internally and written as part of issue creation.
- Project option catalogs are runtime-derived, not hardcoded. Only explicitly configured field names and their valid values reach the model.
- Repository labels and Project option selection share candidate scoring but remain distinct at application: label scores only add labels; Project scores only write the corresponding Project field.
- Model errors fail closed before creating the issue; low-confidence fields remain unset.

## Alternatives

- Pass all Project fields and values to the model: rejected because it needlessly increases prompt size and includes fields the user does not use.
- Require a per-call flag for auto-selection: rejected because the profile already expresses the user's allowlist and makes the intended workflow simpler.
- Guess on a score tie or below threshold: rejected because fields should not be set on ambiguous or weak evidence.

## Traceability

- Requirements: [RF-WI.6](../srs.md), [RF-CFG.3](../srs.md)
- Architecture: [docs/architecture.md](../architecture.md)
- Prior Project field write design: [ADR-0018](./0018-github-project-dynamic-fields.md)
- Tracker: [issue #13](https://github.com/0spura/workctl/issues/13)
