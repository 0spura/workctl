# 0044: IID-prefixed issue branches on GitHub and GitLab

- Status: Accepted
- Date: 2026-10-12
- Tracker: [0spura/workctl#24](https://github.com/0spura/workctl/issues/24)
- Supersedes: the `issue develop` branch-provider scope and naming decision in [ADR-0036](./0036-issue-pr-branch-linking.md)

## Context

GitLab links a branch to an issue when its name starts with the issue IID followed by a hyphen. The agent should provide a concise, representative slug; deriving a name from the full title can produce overly long branches. GitHub's native `gh issue develop` supports remote branch creation and listing.

## Decision

- Expose `issue develop NUMBER` in the selected GitHub or GitLab work-item grammar.
- `--name SLUG` accepts lowercase ASCII letters, digits, and single internal hyphens. The caller omits the issue IID; workctl prefixes it, yielding e.g. `12-auth-session-expiry`.
- Create the branch remotely. Checkout remains opt-in with `--checkout`; `--list` is read-only and reports branches linked by IID.
- GitHub uses `gh issue develop`. GitLab uses the project branches API; when no name is given, it follows the project's `issue_branch_template` and default branch.
- Invalid slugs fail before provider access. Provider errors return no success output.

## Consequences

- Branch naming is stable across providers when the caller specifies a slug.
- GitLab branch creation is a provider-owned API operation; local checkout fetches and switches only when explicitly requested.
- Names are not truncated automatically; the caller controls slug length and meaning.

## Alternatives

- Derive the branch from the full issue title: rejected because names can become excessively long and the agent can produce a concise representative slug.
- Create a local branch and push it: rejected because remote provider operations own issue linking and checkout must remain opt-in.

## Traceability

- Requirement: [RF-WI.13](../srs.md#rf-wi13-create-or-list-the-branches-linked-to-an-issue).
- Architecture: [docs/architecture.md](../architecture.md).
- Previous decision: [ADR-0036](./0036-issue-pr-branch-linking.md).
- Verification: `tests/cli_issues.rs`, `tests/cli_gitlab.rs`, and manual GitLab branch-only issue crosslink smoke.