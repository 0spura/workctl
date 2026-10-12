# 0046: Remove legacy provider selectors

- Status: Accepted
- Date: 2026-10-12
- Tracker: [workctl issue #33](https://github.com/0spura/workctl/issues/33)
- Supersedes: provider-selection CLI and generic config fallback precedence in [ADR-0017](./0017-provider-selected-cli-grammar.md). Static provider grammars and offline pre-parse selection remain.

## Context

Provider selection currently accepts three command-line selectors and a generic `provider` config fallback. These repeat or obscure repository context already available from the target project's Git origin and domain-specific local configuration. Workctl is installed globally, so selection must remain scoped to whichever local Git worktree the user invokes it from, not the workctl source checkout.

## Decision

Remove all three provider-selection CLI flags and the generic `provider` config key completely. Keep only independent `codeProvider` and `workItemProvider` settings, merged per domain, followed by Git-origin fallback. These project config values support provider-specific grammar and self-managed GitLab.

Keep `--repo` as an explicit resource scope within the provider already selected from the current worktree. It does not select a provider or fetch configuration from a remote target. Running workctl in another project means running it from that project's local worktree; workctl discovers that worktree's root, origin, and config files. No configuration is fetched from another repository.

Unsupported, malformed, or ambiguous context fails closed; do not silently default to a different provider.

## Consequences

- Normal commands no longer need provider selector flags.
- The generic `--provider` selector and `provider` config key are unknown/removed, without aliases or migration. Existing configs using `provider` must remove that field.
- `codeProvider` and `workItemProvider` remain supported and validated.
- `--repo` continues to address an explicit repository but does not switch configuration scope.
- Run workctl from the target project's Git worktree; outside a worktree, provider selection is unavailable.

## Alternatives

- Keep `--provider` as an escape hatch: rejected to remove duplicated provider selection from normal CLI use and avoid preserving compatibility behavior the user asked to remove.
- Fetch config from the repository named by `--repo`: rejected because the target may not be locally checked out and remote config fetch would add network/auth/security behavior outside the request.
- Keep the generic `provider` config key as fallback: rejected because it is a legacy selector; only domain-specific settings remain.

## Traceability

- CLI and resolution: `src/cli/mod.rs`, `src/config/resolve.rs`
- Requirements: [docs/srs.md](../srs.md)
- Architecture: [docs/architecture.md](../architecture.md)
