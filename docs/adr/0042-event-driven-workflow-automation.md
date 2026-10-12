# 0042: Event-driven local workflow automation

- Status: Accepted
- Date: 2026-10-11
- Tracker: [0spura/workctl#31](https://github.com/0spura/workctl/issues/31)
- Supersedes: [ADR-0041](./0041-workctl-mutation-event-outbox.md) only for event capture eligibility and outbox consumer ownership; existing HTTP endpoint security and at-least-once delivery remain.

## Context

The retired MCP tracker mapped workctl-like operations to best-effort issue-stage changes. Workctl now has a typed mutation-event outbox for outgoing CloudEvents, but workflow automation remains an SRS non-goal. Implementing separate operation hooks would duplicate mutation detection and produce divergent status and webhook events. The user approved both a local workflow consumer and configured HTTP endpoints from the same event, all typed action categories discussed (stage/Project-field changes, label membership, assignee membership, comments), shared rules with local opt-in, and an exact-rule fingerprint approval. The user chose not to automatically retry a comment after an ambiguous write.

## Decision

- The application layer produces one typed receipt from confirmed provider results. It is the sole input to both workflow planning and CloudEvent generation; neither consumer parses command output or re-reads provider mutations to infer what happened.
- Persist the immutable event, selected HTTP delivery rows, and concrete local workflow action rows in the existing private SQLite outbox transaction before dispatch. Capturing an event is enabled when either consumer is authorized, so local workflow operation does not require a configured HTTP endpoint.
- HTTP delivery and local actions are independent. A local/provider failure does not affect HTTP delivery, and neither consumer changes the outcome or exit status of the original confirmed provider mutation. No action executes before its plan is durable.
- Shared `.workctl.json` contains bounded ordered declarative rules. Supported triggers are issue creation, linked-branch creation, pull-request creation, confirmed merge, and a review approval submitted through workctl. Supported actions are stage/Project-field changes, add/remove labels, add/remove assignees, and comments, subject to provider capability validation. No shell, arbitrary command, environment, or file interpolation is allowed.
- Local execution requires an explicit `workctl workflow approve` operation. It stores a fingerprint over the canonical workflow rules, repository scope, and GitHub Project destination profile in the untracked, Git-ignored `.workctl.local.json`. A changed rule set, scope, or Project destination pauses workflow execution pending renewed approval; ordinary mutations and HTTP delivery continue. Listing and resolving stored actions also require the current exact approval.
- PR-related issue targets must be explicit and fully qualified. Never infer targets from bodies, branch names, or titles. Workflow-generated child events retain origin and causation metadata and are not re-matched by the local workflow consumer.
- Dispatch remains opportunistic, resident-free, and within the existing aggregate one-second budget. A nonblocking process-level lock prevents competing local consumers from waiting. Pending safe actions are retried on later eligible mutating invocations.
- Desired-state actions re-read current state and reconcile before retry; completed actions are not repeated. Comments and other non-idempotent actions whose write may have reached the provider become uncertain and are not automatically repeated. Explicit inspection/resolution is required; no exactly-once claim is made.
- Provider confirmation and SQLite enqueue remain non-transactional. A crash before local enqueue can lose the event and planned workflow actions. An enqueue or consumer failure preserves the provider result and emits only a concise safe warning.

## Consequences

- Event capture must work with workflow-only local authorization, not only when endpoints are configured.
- Event data grows a schema version, operation discriminator, explicit related references, and workflow origin/causation metadata. It remains free of bodies, comment text, credentials, and provider diagnostics.
- Branch creation and review outcomes must be represented as typed operation facts; PR merge/review rules require explicit issue IDs when the command does not otherwise provide them.
- SQLite gains a bounded-query action queue and state machine; completed actions are not reissued, and uncertain non-idempotent writes require user resolution.
- Workflow approval must validate the local file as untracked and Git-ignored. Rule edits do not inherit a prior approval.
- The one-second budget may defer slow provider actions to later workctl invocations. There is no daemon, broker, or second database.
- The action catalog is extensible only through typed provider operations and reviewed schema changes; there is no generic workflow graph or script runner.

## Alternatives

- Direct best-effort hooks after each command: rejected because they duplicate mutation capture and cannot safely recover partially completed action sequences after a crash.
- Treat local actions as ordinary at-least-once webhook deliveries: rejected because provider comments and similar writes have no established idempotency contract.
- Atlas-only automation: rejected because the user requires a local workctl consumer as well as HTTP publication.
- Re-evaluate current shared rules for an old pending event: rejected because changing rules would silently change the authorization and meaning of already planned work.
- Arbitrary shell hooks or recursive workctl commands: rejected because they introduce an unbounded command/security boundary, recursion, and unpredictable deadlines.
- Daemon or general-purpose workflow engine: rejected; the accepted usage-driven outbox and typed rules do not require either.

## Traceability

- Requirements: RF-WF.1, RF-WF.2, RF-WF.3, RF-EVT.1 in [docs/srs.md](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md#mutation-outbox-and-local-workflows).
- Tracker: [workctl#31](https://github.com/0spura/workctl/issues/31).
