# Workctl mutation event outbox and dispatch

- Status: Accepted
- Date: 2026-10-10
- Tracker: #30

## Context

Workctl needs to notify configured consumers, including a future Atlas integration, about mutations performed through its CLI. Events must not require consumers to share workctl's database or run a permanent workctl process. Delivery must not make provider mutations depend on endpoint availability.

## Decision

- Emit a CloudEvents 1.0 structured JSON event after each confirmed mutation target. `source` is `urn:workctl:<provider>:<owner/repo>`; `type` is `workctl.<resource>.<action>`; `subject` is `/<resource>/<id>`. Resource is `issue` or `pull_request`; action is `created`, `updated`, `closed`, `reopened`, `commented`, or `merged`. Batch targets get distinct events; confirmed targets in partial results are represented. Read-only commands, no-ops, and pre-write failures emit none. Event data carries repository/resource identity, action, and confirmed or partial outcome, never bodies, credentials, or provider diagnostics.
- Persist events and independent per-endpoint delivery state in a private SQLite outbox under the platform user-state directory (`%LOCALAPPDATA%\workctl\state` on Windows). Event IDs are stable across retries. Delivery is at-least-once; receivers deduplicate by event ID.
- The platform workctl config directory contains a private `config.json` with ID-keyed endpoint definitions under `events.endpoints`; each endpoint has `url` and `token`. Tokens use Bearer authentication over HTTPS. On Windows, this file is `%LOCALAPPDATA%\workctl\config.json`. Repository-local `.workctl.local.json` opts in with `events.endpoints` as a list of globally defined IDs. Shared `.workctl.json` cannot contain `events`. The global config is capped at 64 KiB and 16 endpoints and is validated before provider mutation.
- Require Unix global-config mode 0600. On Windows, protect config/state with DACLs limited to the current user, SYSTEM, and Builtin Administrators. Reject unsafe existing ACLs and reparse/junction paths; use only local ACL-capable storage. This boundary protects against other ordinary accounts, not hostile same-user, SYSTEM, or administrator processes. The standard SQLite Windows VFS does not guarantee atomic no-follow on every open; validate SQLite's actual database handle and make no stronger claim.
- Windows ancestry may allow read/traverse/list rights to other accounts only where ancestors are owned by the current user, SYSTEM, Administrators, or TrustedInstaller and outsiders cannot change ACL/owner or delete/replace selected components. Sibling creation is allowed only if each created component is reopened and validated with the private DACL. Reject ACE forms not safely recognized.
- After enqueue, make a bounded synchronous delivery attempt during the mutating workctl invocation, with one aggregate one-second deadline. A 2xx response completes only that endpoint's delivery; failures remain pending for a later workctl invocation. No resident daemon is introduced. Delivery failure does not change the provider mutation result.
- The provider mutation and local SQLite enqueue cannot share a transaction. A crash after remote confirmation but before enqueue can lose an event. If enqueue itself fails after provider success, preserve the successful command result and emit a concise safe warning.
- Atlas and other consumers receive HTTP events and own their own persistence; they do not fetch or share the workctl outbox.

## Consequences

- Endpoints receive only operations made through workctl, not mutations performed directly in GitHub/GitLab or other tools.
- A delivery outage is retried only when workctl runs again; there is no background process.
- Consumers must tolerate duplicate event IDs and cannot assume exactly-once delivery.
- Events are not guaranteed for the provider-confirmed/local-enqueue crash window.
- Endpoint configuration remains private and repository opt-in is local to the user.
- Windows ACL and local-storage failures fail closed for event persistence/delivery; they do not change a confirmed provider result.

## Alternatives

- Provider-native webhooks were rejected because they report provider-side changes regardless of whether workctl caused them.
- A resident daemon or timer was rejected because delivery should follow workctl usage and should not require a continuously running service.
- Direct HTTP delivery in the mutation path without an outbox was rejected because endpoint failures would couple remote mutation success to delivery and leave no durable retry state.
- A shared database or direct Atlas polling was rejected because it couples producer and consumer storage and deployment.

## Traceability

- Requirements: `docs/srs.md` RF-CFG.10 and RF-EVT.1
- Architecture: `docs/architecture.md`, “Mutation event outbox”
- Tracker: workctl #30
