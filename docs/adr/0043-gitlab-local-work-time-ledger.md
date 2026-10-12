# 0043: GitLab local work-time ledger

- Status: Accepted
- Date: 2026-10-12
- Tracker: [0spura/workctl#31](https://github.com/0spura/workctl/issues/31)
- Extends: [ADR-0042](./0042-event-driven-workflow-automation.md)

## Context

GitLab time tracking stores accumulated spent duration, not a native start/end timestamp pair. The user wants automatic timing from ordinary workctl use, without a daemon or a separate timer command. Attribution must avoid charging another project, branch, or issue and must cap idle time.

## Decision

- Keep a private SQLite activity ledger beside the existing workflow/event outbox. No daemon, Git hook, or external database.
- Enable via the already-approved shared workflow profile's `workTracking` option. Default idle cap: 10 minutes; allowed range: 1–60 minutes.
- Count invocation runtime plus the interval from the previous workctl invocation only when repository scope, worktree, provider/project, branch, issue ID, and approval fingerprint all match. Begin issue association only from an explicit issue-targeted workctl command. A workctl command in another scope replaces the global cursor, so later activity cannot bill the old task.
- Raw Git commands and work outside workctl cannot be observed. Idle capping limits but does not eliminate attribution error after unobserved branch switches.
- On confirmed GitLab issue close, persist one non-idempotent action through the existing typed workflow queue. It calls `POST /projects/:id/issues/:issue_iid/add_spent_time` using authenticated `glab api` with `duration=<whole minutes>`. Round down; preserve exact local milliseconds and timestamps. An ambiguous write becomes `uncertain`; never retry automatically because GitLab time entries are additive.
- GitHub start/finish timing remains unchanged.

## Consequences

- GitLab records accumulated duration; start/end timestamps remain local only.
- Tracking does not start merely because an issue exists; at least one explicit issue-targeted workctl command is required to safely associate the current branch.
- The current first implementation finishes on confirmed issue close; merge-request-ready completion is not inferred without explicit issue linkage.
- Work outside workctl is intentionally not measured.

## Alternatives

- Git hooks or a daemon: rejected because they add lifecycle, repository mutation, and cross-project boundary risks.
- Charge wall-clock duration from branch creation to close: rejected because idle and off-task periods would be overcounted.
- Infer issue identity from branch names or descriptions: rejected as ambiguous and unsafe.
- Send start/end values to GitLab: unavailable in its native time-tracking contract; use additive spent duration instead.

## Traceability

- Requirement: RF-WF.4 in [docs/srs.md](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md#mutation-outbox-and-local-workflows).
- Tracker: [workctl#31](https://github.com/0spura/workctl/issues/31).
