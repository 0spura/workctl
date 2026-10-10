# 0037: Delete only the remote branch

- Status: Accepted
- Date: 2026-10-10
- Supersedes: [ADR-0029](./0029-github-pr-merge-defaults.md) only for the meaning of branch deletion.
- Tracker: [0spura/workctl#27](https://github.com/0spura/workctl/issues/27)

## Context

`pr merge --delete-branch` and `pr edit --state closed --delete-branch` forwarded `--delete-branch` to
`gh`, which deletes the local and the remote branch. The user reported that removing the local branch
is the part that breaks their workflow: the branch may still be checked out in a worktree, may hold
work that was not merged, or may be the branch a follow-up depends on. `gh`'s own deletion also
depends on its local checkout state, so the same command could fail, or succeed with a side effect
that has nothing to do with the remote change the caller asked for.

The repository already treats local branch lifecycle as `git`'s business (`issue develop` creates the
branch remotely and never touches the local repository). The deletion flag contradicted that
boundary by delegating a local deletion to `gh`.

## Decision

- `--delete-branch` deletes the pull request's **remote** head branch and never a local branch.
- `workctl` performs the deletion itself, as one `gh api --method DELETE repos/OWNER/REPO/git/refs/heads/BRANCH`
  call after the merge or close has been acknowledged. The flag is no longer forwarded to `gh pr merge`
  or `gh pr close`, so no `gh` code path can reach a local branch.
- The branch name and its ownership are read from `repos/OWNER/REPO/pulls/NUMBER`: the deletion runs
  only when the head repository of the pull request is the repository being addressed. A pull request
  opened from a fork has its head branch in another repository, and a same-named branch in this
  repository is not the branch under discussion, so it is left alone. A missing head repository is
  treated the same way.
- A ref that is already gone counts as deleted. GitHub answers such a deletion with 422
  `Reference does not exist`, which is exactly the state the caller asked for; that response is
  success, and every other failure is reported.
- The branch name is percent-encoded before it becomes a path segment, because git allows characters
  such as `#` in a ref name that would otherwise truncate the API path.
- `defaults.github.pr.deleteBranch` keeps its name, type, and default, but now means the same
  remote-only deletion. It still applies to every `pr merge` because there is no inverse flag.
- Deletion belongs to a merge that has completed here. `pr merge --auto` queues the merge instead, so
  it has no branch to delete: an explicit `--delete-branch` with `--auto` fails as `invalid_input`
  before provider access, and a deletion enabled only by configuration is not attempted.
- A deletion failure after a successful merge or close is `partial_success` with the completed
  operation and the pending `remote branch deletion`, in the existing ordered completed/pending
  shape. It is never retried or rolled back.

## Consequences

- `--delete-branch` no longer cleans up a local checkout. A caller that wants the local branch gone
  runs `git branch -d` itself, with the worktree and unmerged-work checks `git` owns.
- A merge or close with deletion costs one extra API call to read the pull request references and one
  to delete the ref. The read happens only when deletion is enabled.
- Deleting a branch that is already gone is not an error, so a repository with automatic head-branch
  deletion enabled does not turn cleanup into a failure.
- The `--auto` combination is now refused instead of forwarded. Previously `gh` received both flags
  and could not delete anything meaningful before the queued merge ran.
- Fork pull requests no longer produce a remote deletion request at all, even when the fork's branch
  name also exists here.
- `pr edit --state closed --delete-branch` keeps its order: ordinary edits, state change, remote
  deletion, then the transition comment, then the readback.

## Alternatives

- Forward `--delete-branch` and add a separate remote-only flag: rejected because the flag's current
  meaning is the one the user rejects; two flags for one intent would preserve the harmful default.
- Keep forwarding to `gh` and delete the local branch back with `git`: rejected because it needs a
  local checkout of the right repository, does not work for `--repo` targets, and would recreate the
  deletion the user does not want.
- Delete the local branch only when it is not checked out: rejected as a partial version of the same
  harmful behavior, with extra state to reason about.
- Probe the ref before deleting: rejected because the deletion response already distinguishes the only
  benign case, and the probe would double the calls while adding the same ambiguity.
- Delete with `git push origin --delete`: rejected because it resolves credentials and the remote from
  the local checkout, so `--repo other/repo` could delete in the wrong repository.

## Traceability

- Active behavior: [SRS RF-PR.7](../srs.md#rf-pr7), [SRS RF-PR.10](../srs.md#rf-pr10), [SRS RF-CFG.4](../srs.md#rf-cfg4)
- Technical flow: [architecture configuration](../architecture.md#configuration-and-context)
