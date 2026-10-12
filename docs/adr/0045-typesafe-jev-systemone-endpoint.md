# 0045: Route `jev-latest` through TypeSafe SystemOne

- Status: Accepted
- Date: 2026-10-12
- Tracker: [workctl issue #32](https://github.com/0spura/workctl/issues/32)
- Supersedes: the Jev endpoint selection in [ADR-0014](./0014-native-decision-model-adapters.md) and [ADR-0015](./0015-provider-neutral-decision-model-package.md). Other adapter and package decisions remain in force.

## Context

`jev-latest` uses the SystemOne protocol, but two separate services expose that protocol: TypeSafe (`api.typesafe.ai`) and the independently operated Jev AI service (`thejevai.com`). Their credentials are not interchangeable. A direct request with the user's TypeSafe key succeeded at the TypeSafe endpoint and returned a structured Jev answer; the same key was rejected with HTTP 401 by the Jev AI endpoint. The workctl adapter pointed at the latter.

## Decision

Route `jev-latest` to the fixed endpoint `https://api.typesafe.ai/v1/systemone`, using Bearer authentication and the existing SystemOne request/response validation. Do not fall back to `thejevai.com`, and do not allow a remote endpoint override. The configured key and `DECISION_MODEL_API_KEY` must be a TypeSafe API credential when this model is selected.

## Consequences

- `jev-latest` requests reach the service that accepts the configured TypeSafe key.
- Existing provider-neutral model inputs, Noul-per-candidate questions, score validation, and the `>= 0.8` label threshold remain unchanged.
- Credentials for the independent Jev AI service are not accepted by this adapter unless TypeSafe also authorizes them.
- A TypeSafe authentication failure remains fail-closed; there is no alternate provider request.

## Alternatives

- Keep `thejevai.com`: rejected because it returned 401 for the TypeSafe key and represents a separate service.
- Retry/fallback across both hosts: rejected because the services have distinct credentials and commercial boundaries; fallback would obscure which service receives issue data.
- Make the hosted endpoint configurable: rejected because it would widen credential-routing and data-disclosure choices beyond the request.

## Traceability

- Implementation: `src/decision_model/jev_adapter.rs`
- Architecture: [docs/architecture.md](../architecture.md)
- Existing contract: [RF-WI.1](../srs.md)
