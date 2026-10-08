# GitHub pull-request branch update

- Status: Accepted
- Date: 2026-10-08
- Tracker: None

## Context

Agents need to bring a pull request branch up to date with its base before review or merge. `gh pr update-branch` provides this operation without checking out or modifying a local branch, and uses a merge commit by default with optional rebasing.

## Decision

Add `workctl pr update-branch NUMBER [--rebase]` to the GitHub CLI. Delegate directly to the authenticated `gh pr update-branch NUMBER --repo OWNER/REPO` operation; pass `--rebase` only when requested. On success, return the PR number and selected strategy. Do not fetch an independent PR snapshot, make local worktree changes, or infer success from output text. Map provider failure to the existing safe `github_cli` error with no success output.

## Consequences

- The GitHub PR grammar gains one native `gh` verb.
- `PullRequestProvider` owns the operation and its provider failure boundary.
- The operation changes remote PR branch state but never checks out a local branch.

## Alternatives

- Update the branch through REST or GraphQL: rejected; `gh` already provides the requested operation.
- Check out the PR and update locally: rejected; this would mutate the caller's working tree and duplicate `gh` behavior.

## Traceability

- Active behavior: [SRS RF-PR.13](../srs.md#rf-pr13)
- Technical flow: [architecture](../architecture.md)
- Native command behavior confirmed by local `gh pr update-branch --help`.
