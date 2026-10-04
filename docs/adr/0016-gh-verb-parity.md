# 0016: Command verbs mirror `gh`

- Status: Accepted
- Date: 2026-10-04
- Tracker: [issue #10](https://github.com/0spura/workctl/issues/10)
- Supersedes: [ADR-0004](./0004-workctl-rust-cli.md) for the issue verb names (`show` → `view`) and [ADR-0006](./0006-pull-request-operations.md) for the pull request verb names (`show` → `view`, `update` → `edit`). Everything else in those decisions stands.
- Superseded by: [ADR-0017](./0017-provider-selected-cli-grammar.md) for the single-surface premise and for `--provider` as a grammar selector. The verb parity below stands inside the GitHub grammar.

## Context

The primary callers are coding agents that already know `gh`. Twelve of the fifteen workctl subcommands already used `gh`'s verb verbatim; three did not: `issue show`, `pr show`, and `pr update`, against `gh`'s `issue view`, `pr view`, and `pr edit`.

The cost of a divergent verb is not the help text it adds. It is that a caller primed by `gh` guesses the wrong verb, pays a usage error, and retries. The full `--help` surface for all fifteen subcommands measures 15,963 bytes, so a caller that can rely on `gh` priors can skip reading it entirely.

Consolidating commands was evaluated as the alternative token saving and rejected: merging `pr close`/`pr reopen` into `pr state`, or `pr show`/`pr diff`/`pr checks` into `pr view --diff|--checks`, saves at most ~1.4 KB (9% of the surface) while breaking exactly the parity that removes the whole read. The merges that genuinely collapse a set of variants are already in place: `pr review --approve|--request-changes|--comment`, `pr ready [--undo]`, and `pr merge --method merge|squash|rebase`.

## Decision

Name each workctl subcommand with `gh`'s verb whenever `gh` has one. Apply the rule now by renaming three subcommands, with no aliases:

- `issue show` → `issue view`
- `pr show` → `pr view`
- `pr update` → `pr edit`

A workctl-specific capability stays a flag on the existing `gh`-named verb rather than a new verb: `--append-body`, `--replace-section`, `--patch-file`, `--expect-updated-at`, and the `@auto` label value are additions to `issue edit`/`pr edit`, not commands of their own.

The rule is not retroactive to names `gh` does not define. `issue`/`pr` remain the two groups, and the flags, output shapes, error codes, and exit statuses are unchanged by this decision.

## Consequences

- `issue show`, `pr show`, and `pr update` are removed and fail with a clap usage error (exit 2). No alias keeps the old spelling alive; nothing had been released, so the cutover is free.
- Requirement titles RF-WI.3, RF-PR.3, and RF-PR.8, the architecture notes, and every README example use the new verbs.
- A future subcommand that has no `gh` counterpart keeps the workctl name and must be justified in the SRS, since it cannot lean on `gh` priors.
- The `gh`-compatible subset is now the whole command surface, so a caller can use `gh` knowledge for command selection and read `--help` only for workctl-specific flags.

## Alternatives

- **Keep `show`/`update`.** Rejected: three avoidable wrong-guess retries for the caller this CLI targets.
- **Add `view`/`edit` as aliases of the current names.** Rejected: two spellings of the same behavior double the surface to document and test, and `--help` becomes ambiguous about which is canonical.
- **Merge verbs to cut the command count.** Rejected: the measured ceiling is 9% of the help surface, against the parity that saves an entire read.
- **Rename to non-`gh` names that read better in isolation.** Rejected: the caller's prior beats internal taste; `view` and `edit` are already the natural verbs.

## Traceability

- Requirements: [docs/srs.md](../srs.md) (RF-CLI.1, RF-WI.3, RF-PR.3, RF-PR.8)
- Architecture: [docs/architecture.md](../architecture.md)
- Project stack: [docs/project.md](../project.md)
