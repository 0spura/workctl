# 0047: Local target worktree via `--repo-path`

- Status: Accepted
- Date: 2026-10-12
- Tracker: [workctl issue #33](https://github.com/0spura/workctl/issues/33)
- Supersedes: the current-worktree-only boundary in [ADR-0046](./0046-remove-provider-selection-cli-flags.md).

## Context

Globally installed workctl must act on a second local project without provider-selection flags or fetching that project's configuration remotely. `--repo` already identifies a remote resource scope, so using it for a local path would conflate two distinct contexts.

## Decision

Add global `--repo-path PATH`. Resolve a relative path from the invocation's original current directory. Require the path to exist, be a directory inside a Git worktree, and resolve it to the canonical worktree root. Use that target's configuration and Git origin to select provider grammar before parsing. Execute the command with the target worktree as current directory so repository-scoped config, provider commands, Git operations, and local workflow state all use the selected target context. `--repo` remains an optional remote repository-scope override within the selected provider.

A supplied invalid target fails closed; never fall back to the invocation worktree. No remote configuration fetch or provider override is introduced.

## Consequences

- `workctl issue create --repo-path ../other-project --title "..."` uses the local target project's provider, origin, and configuration.
- Relative target paths are anchored to the caller's original directory; subsequent repository-context behavior runs from the canonical worktree root.
- Relative file arguments are interpreted from the target worktree when execution begins.
- `--repo-path` selects local project context; `--repo` selects the remote resource within that context's provider.
- Missing/non-directory/non-worktree targets fail before provider access.

## Alternatives

- Reuse `--repo` for local paths: rejected because remote repository identity and local configuration context are different concepts.
- Accept a remote URL as a provider selector: rejected because the requirement is to use an already configured local checkout.
- Fetch target configuration remotely: rejected because it adds network/auth and remote-config trust behavior.

## Traceability

- CLI and bootstrap: `src/cli/mod.rs`, `src/main.rs`
- Provider/context resolution: `src/config/resolve.rs`
- Requirements: [RF-CLI.1](../srs.md#rf-cli1-command-surface)
- Architecture: [docs/architecture.md](../architecture.md)
