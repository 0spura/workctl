# 0024: Compact GitHub issue blocker chains

- Status: Accepted
- Date: 2026-10-07
- Related tracker: none; the user explicitly set tracker work aside.
- Supersedes: none. This supplements [ADR-0023](./0023-github-issue-view-relationships.md) with a separate compact transitive diagnostic; `issue view` remains a direct-neighbor read.

## Context

The user asked for additional deterministic intelligence in workctl and a modular layout with operation-specific CLI and command files. Issue blocker relations already exist as GitHub-native edges. Agents need a small result that traces open blockers to a target without printing full issue records or implying general readiness.

## Decision

- Add `workctl issue blockers NUMBER`; leave `issue view` output unchanged.
- Traverse only GitHub `blockedBy` edges, beginning at the requested issue, and print complete open blocker-to-target chains. Do not traverse parent/sub-issue relations or infer execution readiness.
- Use same-repository `#NUMBER` and cross-repository `OWNER/REPO#NUMBER`. Omit closed blockers and return no chains for a closed target.
- Emit JSON as an array of chain strings, including `[]` when empty. Text emits one chain per line and no bytes when empty. Do not emit titles, bodies, relation objects, or boilerplate.
- Require complete validated connections; reject malformed, cyclic, changing, or incomplete data. Return no partial result on errors.
- Bound traversal to 20 issue reads including pages, 500 edges, 20 chains, depth 20, and 1,000 relations per connection. Exceeding a bound returns `relationship_limit`.
- Keep CLI grammar and command use cases in per-operation modules under GitHub issues, GitHub PRs, and GitLab issues; retain existing CLI contracts.

## Consequences

The command provides deterministic context-light transitive blocker information with bounded API work. Empty output describes only the observed blocker graph and does not mean an issue is ready. Multiple GraphQL reads are not an atomic snapshot; conflicting states fail closed. Module ownership is smaller and operation-oriented, at the cost of additional module files.

## Alternatives

- Add transitive paths to `issue view`: rejected to preserve its established direct-neighbor contract and avoid changing its output cost.
- Print issue details or a readiness verdict: rejected as noisy and unsupported by blocker edges alone.
- Return partial chains at a traversal limit: rejected because it could misrepresent an incomplete graph as complete.

## Traceability

- Observable behavior: [RF-WI.7](../srs.md).
- Ownership and module layout: [architecture](../architecture.md#module-map).
- CLI behavior: `tests/cli_issues.rs` blocker-chain tests.
