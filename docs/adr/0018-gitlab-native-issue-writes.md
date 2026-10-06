# 0018: GitLab-native issue writes

- Status: Accepted
- Date: 2026-10-04
- Tracker: [issue #12](https://github.com/0spura/workctl/issues/12)

## Context

GitLab issue create/update flags and mutation semantics differ from GitHub: labels use add/remove verbs, assignees support replacement and relative `+`/`-`/`!` operations, milestones have explicit clear values, and GitLab adds confidentiality, weight, and due-date fields. A shared GitHub-shaped patch would either lose these semantics or create a provider-union request.

A write can succeed remotely even when the CLI times out, fails after launch, or returns output that cannot be validated. Returning a generic failure could invite an unsafe retry. The existing process runner already supports stdin and bounded subprocess execution, and GitLab reads already map into the shared `Issue` result.

## Decision

1. Implement GitLab issue create/update on the GitLab-owned `GitLabIssues` adapter with typed provider-native requests and `glab issue create` / `glab issue update` flags. Do not widen `WorkItemProvider` or reuse GitHub's `NewIssue`/`IssuePatch` request types.
2. Require explicit create title and description without prompting. Send description bytes through `glab` stdin; never place description text in argv.
3. Return the shared `Issue` record. For create, validate the emitted URL against the selected project, extract its positive IID, and read the record with `glab issue view`; update also reads back the issue.
4. Treat provider write failures and failed post-write confirmation as `gitlab_write_uncertain`, with safe guidance to inspect GitLab before retrying. Never expose provider stderr.
5. Keep GitLab-native clear and assignment semantics explicit: milestone empty/`0` clears; update weight `0` is preserved; assignee prefixes retain `glab` meanings. Because `glab issue create` omits weight `0`, follow a successful create with `glab issue update --weight=0` when explicitly requested. Reject empty update descriptions, dash-only descriptions, and mutationless updates before a write.

## Consequences

- The public issue grammar remains provider-specific, consistent with ADR-0017; callers receive normalized shared output while input semantics remain native.
- No generic issue-write interface is introduced solely for GitLab. The GitHub provider and existing workflows remain unchanged.
- Create/update require a `glab` version supporting the documented native flags. Unsupported versions fail through the safe uncertain-write contract if rejection occurs after process launch.
- A confirmed mutation is distinguished from an uncertain outcome, and users are told not to retry until checking the issue.

## Alternatives

- Widen `WorkItemProvider` with a union of both providers' fields: rejected because its requests would encode unrelated native semantics and make future callers handle provider-specific combinations.
- Map GitLab fields into GitHub's generic issue patch: rejected because it cannot represent relative assignment, milestone clearing, or GitLab-only metadata faithfully.
- Return the `glab` URL or update success without a read-back: rejected because create/update should preserve the shared issue result contract and a successful process status alone does not confirm the normalized record.

## Traceability

- Requirements: [docs/srs.md](../srs.md) (RF-GL.1, RF-GL.4, RF-GL.5, RNF-SEC.1)
- Architecture: [docs/architecture.md](../architecture.md)
- Earlier provider-owned grammar: [ADR-0017](./0017-provider-selected-cli-grammar.md)
