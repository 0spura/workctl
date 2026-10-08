# 0023: GitHub issue view exposes direct relationships

- Status: Accepted
- Date: 2026-10-07
- Related tracker: [issue #14](https://github.com/0spura/workctl/issues/14), the existing native-relationship write contract. This change does not update the tracker.
- Supersedes: the shared-record-only output of GitHub `issue view` in [ADR-0004](./0004-workctl-rust-cli.md). Shared issue records, mutation guards, and the explicit write contract in [ADR-0022](./0022-native-github-issue-editing.md) remain unchanged.

## Context

The user approved implementing relationship reads so agents can inspect existing hierarchy and blockers before writing relationships. The native `gh issue view` JSON query limits sub-issues to 100 and both blocking connections to 50, without cursor information. Those truncated lists cannot be presented as a complete graph.

## Decision

- Keep the provider-owned `GitHubIssues::view` separate from the basic `WorkItemProvider::show` read. The latter remains the REST read for issue identity, body, revision guards, and mutation confirmation.
- Return `GitHubIssueView`: the flattened basic issue plus nullable `issue_type` and `parent`, and `sub_issues`, `blocked_by`, and `blocking` arrays. Related issues have number, title, normalized state, and URL; URLs distinguish repositories. Text output identifies the same relation directions and uses existing terminal escaping.
- Authenticate through `gh`, reject pull requests with the existing REST read, and query GraphQL using fixed connection names and owner/name/number/cursor variables. The initial query fetches all three direct-neighbor connections. Subsequent pages are requested independently; no recursive traversal, model request, or write occurs.
- Bound each connection to ten pages of 100 and 1,000 related issues, for at most 29 API calls including REST and excluding authentication. Fail rather than return a truncated graph when a bound is exceeded.
- Reject GraphQL errors, missing or malformed data, incomplete connections, changed total counts, duplicate URLs, and repeated cursors. No partial stdout or fallback empty lists. Multiple reads are not an atomic graph snapshot.
- Preserve create/edit/list and GitLab output shapes; add no flags or compatibility aliases.

## Consequences

Agents can inspect direct hierarchy and dependency direction, including cross-repository references. GitHub view now requires relationship-read access and may fail where the basic REST record alone was readable. Consumers of GitHub view receive additional JSON fields; shared mutation outputs are intentionally unchanged. Very large or changing graphs require a new read rather than relying on incomplete output.

## Alternatives

- Return native `gh issue view` relationship lists unchanged: rejected because they silently truncate.
- Widen every shared `Issue` record and fetch relations for mutation guards: rejected because GitLab and unrelated mutations should not pay for GitHub-specific traversal.
- Traverse the transitive graph or infer relationships: outside the approved scope.

## Traceability

- Observable behavior: [RF-WI.3](../srs.md).
- Ownership and bounded I/O: [architecture](../architecture.md#provider-seams-and-models).
- CLI behavior: `tests/cli_issues.rs` view relationship tests.
- Native query source: [GitHub CLI query builder](https://github.com/cli/cli/blob/trunk/api/query_builder.go).
