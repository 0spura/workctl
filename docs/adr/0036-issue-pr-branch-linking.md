# 0036: Link pull requests and branches to issues

- Status: Accepted
- Date: 2026-10-10
- Supersedes: [ADR-0006](./0006-pull-request-operations.md) only for the create-only, same-repository `--closes` flag.

## Context

GitHub populates an issue's Development panel from two sources: branches linked to the issue and pull requests that reference it. Only the first has a documented `gh` verb, `gh issue develop`; the second is produced by a closing keyword such as `Closes #123` in the pull request's description or in a commit message.

`workctl` could already create that reference, but only at creation time and only for its own repository: `pr create --closes NUMBER` appended one `Closes #NUMBER` line. There was no way to link or unlink a pull request after it existed, no way to reference an issue in another repository, and no way to create the linked branch the Development panel's "create a branch" flow produces. The user asked for the `--closes` flag on `pr edit` and for an `issue develop` command.

Two alternatives were considered for linking an existing pull request. A dedicated `pr link`/`pr unlink` verb pair would have to edit the same body text anyway, because GitHub derives the link from that text; and a direct GraphQL/REST link call has no documented command-equivalent behavior in `gh`, so it would duplicate provider semantics the CLI already owns. Editing the body in place keeps one mechanism, one write, and one place to explain the contract.

## Decision

- A closing reference is typed as `NUMBER` or `OWNER/REPO#NUMBER`, parsed before provider access into a shared closing-reference value. The form is written back canonically, so `pr create --closes` and `pr edit --closes` accept the same values, and `--closes other/repo#7` writes `Closes other/repo#7`.
- `pr edit` accepts repeatable `--closes` and `--remove-closes`, which manage closing lines as body text in the same single `gh pr edit --body-file -` write as any other body change. Never issue a second write for linking.
- An addition appends one `Closes <reference>` line. A removal drops only a line that holds exactly one recognized closing keyword (`close`, `closes`, `closed`, `fix`, `fixes`, `fixed`, `resolve`, `resolves`, `resolved`) and that one reference. Prose and multi-reference lines are left untouched, so a removal never rewrites authored text it cannot fully account for. Keywords and repository names compare case-insensitively.
- Additions are idempotent: a reference the body already closes is not appended again. A removal that matches nothing changes nothing. With no other change, such a request returns the pull request unchanged without calling `gh`; workctl does not invent an update to justify the invocation.
- The same reference in `--closes` and `--remove-closes` fails with `invalid_input` before provider access, matching how issue relationship edits reject an add/remove overlap.
- `issue develop NUMBER [--base BRANCH] [--name BRANCH] [--checkout] [--list]` forwards to `gh issue develop`. It is a GitHub-only verb on the GitHub issue grammar and stays off `WorkItemProvider`, because no other provider has an equivalent operation; GitLab's grammar is unchanged.
- The branch is created on the remote. `workctl` never creates, moves, or deletes a local branch itself and forwards `--checkout` only when asked, so an unrequested local side effect is impossible. `--list` reports the linked branches and conflicts with `--base`, `--name`, and `--checkout`, since `gh` rejects that combination anyway.
- Reported branch names come from the `/tree/` reference `gh` prints for a creation and from the branch column of each `BRANCH<TAB>URL` listing line. Output that carries no branch reference, or a listing that is not valid UTF-8, fails as `provider_response` rather than reporting an invented name.

## Consequences

- GitHub issue subcommands become eight; `pr edit` gains two flags. Neither adds a separate remote write: linking is body text and the branch command is a single native invocation.
- `pr create --closes` widens from a positive integer to the shared reference form. Existing `--closes 12` invocations keep producing `Closes #12`.
- Because a removal matches only a single-keyword, single-reference line, a body that says `Closes #5, #6` keeps that line when only `#6` is removed; the caller must rewrite that line with `--body`/`--patch-file`. This is deliberate: the alternative is guessing which reference to strip and rewriting prose workctl did not author.
- A reference is written with the keyword `Closes` even when removal would also have accepted `Fixes` or `Resolves`, so a canonical add/remove round trip normalizes the keyword of lines it removes and adds rather than preserving the original wording.
- The linked-branch command inherits `gh`'s authentication, host selection, and its own fetching of the new remote branch into local remote-tracking refs. Workctl does not verify the created branch afterward; the reported name comes from the command output.
- A configured Project, milestone, or model path is untouched by linking: no defaulting, classification, or field write happens for `--closes` or `issue develop`.

## Alternatives

- A `pr link`/`pr unlink` verb pair: rejected because the link *is* body text, so the flags belong on the existing body-change surface instead of a second mechanism for one outcome.
- Linking through the GraphQL `linkPullRequest`-style API or a REST call: rejected as unverified command-equivalent behavior that would bypass the CLI adapter's authentication and error mapping.
- Reuse `IssueReference` for `--closes`: rejected because it exists to accept canonical issue URLs for commands that need a provider target; writing that URL into the body would not produce the documented closing line.
- Keep `pr create --closes` numeric and give `pr edit` a different value grammar: rejected because two value grammars for one concept is a convention the caller has to learn twice.
- Preserve the removed line's keyword instead of dropping it: rejected because removal must work for a reference written by anyone, including in commits, and matching one canonical keyword set is predictable.
- Implement `issue develop` inside `WorkItemProvider`: rejected because it would force an unsupported implementation on GitLab.
- Create the branch locally with `git` and push it: rejected because `gh issue develop` also records the issue link, and a local branch would add a worktree side effect the remote-only contract avoids.
- Forward `--worktree` and `--branch-repo`: deferred; a separate worktree or a branch in another repository is outside the current need and would widen the grammar without an observable acceptance case.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-CLI.1, RF-PR.1, RF-PR.8, RF-WI.13, RNF-TST.1.
- Architecture: [docs/architecture.md](../architecture.md) — module map and provider seams.
- Verification: `tests/cli_prs.rs` (`edit_manages_closing_references_in_the_body`, `create_sends_the_body_and_flags_to_gh`), `tests/cli_issues.rs` (`develop_creates_and_lists_linked_branches`), and a built-binary smoke run against GitHub.
