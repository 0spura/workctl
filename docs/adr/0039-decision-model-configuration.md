# Decision-model configuration with a local-only API key

- Status: Accepted
- Date: 2026-10-10
- Supersedes: the environment-only decision-model selection and credentials of [ADR-0014](./0014-native-decision-model-adapters.md) and [ADR-0015](./0015-provider-neutral-decision-model-package.md). The fixed hosted endpoints and the loopback-only local endpoint rule are retained.

## Context

Selecting a decision model required the `DECISION_MODEL` process environment variable, a hosted
credential required `DECISION_MODEL_API_KEY`, and the local endpoint required
`DECISION_MODEL_BASE_URL`. The rest of workctl configures a repository through the strict
`.workctl.json`/`.workctl.local.json` pair, so a project could not commit which model it uses, and a
developer could not keep the credential beside the repository without exporting it in every shell.

## Decision

Add a top-level `decisionModel` object to the strict configuration schema.

- `decisionModel.model` selects `jev-latest`, `laya`, `fastino/GLiDE`, or `local/<model-id>` — the
  same values `DECISION_MODEL` accepts.
- `decisionModel.baseUrl` configures the OpenAI-compatible local endpoint and must still resolve to
  loopback. Hosted endpoints remain fixed and are not overridable.
- `decisionModel.apiKey` is accepted only in `.workctl.local.json`. A shared file that carries it
  fails closed as a `config` error before any provider or model access, and the credential never
  appears in diagnostics: the settings type redacts it in `Debug`, and it is never serialized.

Nonblank environment variables remain per-key overrides: `DECISION_MODEL`, `DECISION_MODEL_API_KEY`,
and `DECISION_MODEL_BASE_URL` each win over the matching configured key, and a blank variable is
treated as unset. `decisionModel` keys merge independently between the two files, so a shared model
and base URL combine with a local key. This is the deliberate departure from the whole-object
replacement used by `defaults`, and it exists precisely so the credential can live only in the
gitignored file without discarding the shared settings.

Resolution stays lazy. The model is constructed only for an opted-in automatic operation (`@auto`
labels or automatic Project-field selection), so no other command selects a model or consults a
configured credential. A missing hosted credential still fails as `decision_authentication`; an
unsupported model or a non-loopback endpoint still fails as `decision_config`. Local adapters
continue to receive no hosted credential.

## Consequences

- A repository can commit the model and loopback endpoint it expects, while each developer keeps the
  key in the untracked local file; exporting environment variables is no longer required.
- Environment overrides keep one-off model or endpoint swaps possible without editing either file,
  and every existing environment-based invocation keeps working.
- The decision-model settings object holds a credential field, so credential handling is explicit:
  redacted `Debug`, no `Serialize`, and no path that copies it into errors or output.
- Selection and credential validation remain tied to model-triggering flows, so a command without
  `@auto` is unaffected by an unsupported `decisionModel.model` value.

## Alternatives

- Keep environment-only configuration: rejected because a repository cannot express its model
  choice, and the key has to be exported per shell.
- Allow `apiKey` in the shared file: rejected because `.workctl.json` is committed; a shared
  credential would leak to every clone, while the local file already exists for machine-specific
  values and is gitignored.
- Put the settings under `defaults`: rejected because `defaults` describes per-command provider
  defaults and is replaced as a whole; the credential needs a key-local-only rule.
- Replace the whole `decisionModel` object like `defaults`: rejected because a local key alone would
  discard a shared model and base URL, defeating the split.
- Drop environment support and read only configuration: rejected because it would break every
  existing invocation and the ADR-0014/ADR-0015 contract without need.
- Per-adapter credential keys: rejected because the selected adapter already determines the single
  credential in use, so one `apiKey` suffices.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-CFG.1, RF-CFG.9, RF-WI.1, RNF-SEC.2.
- Architecture: [configuration and context](../architecture.md#configuration-and-context), [provider
  seams and models](../architecture.md#provider-seams-and-models).
- Superseded decisions: environment-only selection and credentials in ADR-0014 and ADR-0015.
