# GitHub pull-request checkout

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

Agents need to inspect or modify a pull request's code in a local worktree. GitHub CLI already provides `gh pr checkout NUMBER` and applies its usual protections to the current worktree. The `--force` option can reset an existing local branch, so workctl must not opt into it.

## Decision

Add `workctl pr checkout NUMBER` to the GitHub pull-request grammar. Resolve repository and authenticate through the existing provider context, then delegate to `gh pr checkout NUMBER --repo OWNER/REPO` in the caller's current worktree. Do not pass `--force` or expose branch, detach, or worktree overrides in this command. On success, return `{ "number" }`; text output states that the PR was checked out. Provider failures return the existing safe `github_cli` error without successful output.

## Consequences

- The GitHub PR grammar gains one local worktree-changing command.
- GitHub CLI owns branch lookup and its normal checkout safety behavior; workctl does not implement Git or branch switching itself.
- A successful checkout changes the caller's current branch. Existing local changes are not forcibly reset by workctl.

## Alternatives

- Implement Git fetch/branch logic in workctl: rejected because `gh pr checkout` already handles fork remotes and branch details.
- Pass `--force` to make checkout succeed over local conflicts: rejected because it can overwrite local branch state.
- Expose all native checkout flags immediately: rejected to keep the initial operation bounded to one predictable worktree change.

## Traceability

- Active behavior: [SRS RF-PR.15](../srs.md#rf-pr15)
- Command surface: [SRS RF-CLI.1](../srs.md#rf-cli1)
- Provider and process boundaries: [architecture](../architecture.md)
- Native command behavior confirmed by `gh pr checkout --help`.
