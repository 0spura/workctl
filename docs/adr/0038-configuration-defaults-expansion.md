# Configuration defaults for create commands, output format, and list limits

- Status: Accepted
- Date: 2026-10-10
- Supersedes: none
- Tracker: [#28](https://github.com/0spura/workctl/issues/28)

## Context

Configuration could pre-set GitHub issue creation, GitHub merge behavior, and provider routing, but
nothing else. GitLab create commands had no defaults at all, `pr create` had none, the success output
format could only be chosen per invocation, and the listing limit had to be repeated on every call.
The two providers were asymmetric for the same workflow, and a repository that already knows its
labels, reviewers, or base branch still paid the cost per invocation.

## Decision

Extend the strict configuration schema with provider create defaults and two cross-cutting defaults.

- `defaults.gitlab.issue` accepts `labels` and `assignees`; `defaults.gitlab.mr` accepts `labels`,
  `assignees`, `reviewers`, `targetBranch`, and boolean `draft`. They apply to `issue create` and
  `mr create` respectively. `targetBranch` is named after `glab`'s own `--target-branch` flag.
- `defaults.github.pr` gains `labels`, `assignees`, `reviewers`, `base`, and boolean `draft` for
  `pr create`, next to the existing `mergeMethod` and `deleteBranch` merge keys. The create keys are
  read only by `pr create`.
- `defaults.output.format` accepts `json` or `text` and selects the success format for every command.
  It is resolved once before command dispatch.
- `defaults.github.listLimit` accepts 1–1000 and supplies the `--limit` default for `issue list` and
  `pr list`.

Precedence is uniform and always favors the explicit flag: configured labels precede explicit labels
with duplicates removed in first-seen order; explicit `--assignee`/`--reviewer` replace the
configured lists; explicit `--base`/`--target-branch`, `--format`, and `--limit` win; a configured
`draft: true` adds the draft flag when the caller passed no draft option. Fallbacks are 30 for the
listing limit and `json` for the format.

Create defaults are merged in the command layer before provider access, so a blank value, an
out-of-range limit, an unsupported format, `@auto` in `pr create` labels, or an unknown key fails
closed as a `config` error without reaching `gh` or `glab`. GitLab listings keep their native
`--per-page` semantics and are deliberately not covered by `listLimit`, because `--limit` (GitHub)
and `--per-page` (GitLab) are not the same contract.

## Consequences

- Repository configuration can now express the full create shape for both providers, and the output
  format and listing limit can be set once per repository.
- Every command now loads and validates project configuration before dispatch, including commands
  that previously did not consult it; a malformed file therefore fails any invocation, consistent
  with the existing "validated on every command invocation" rule.
- `--format` no longer carries a clap-level default; the effective format is resolved from the flag,
  then configuration, then `json`.
- Existing behavior is unchanged when no new key is configured: create argv, output format, and the
  30-issue limit stay identical.

## Alternatives

- Provider-neutral `defaults.listLimit`: rejected, because GitHub `--limit` and GitLab `--per-page`
  differ in meaning and one key cannot honestly cover both.
- A nested `defaults.github.pr.create` object: rejected, because `defaults.github.issue` already
  mixes create and edit keys flat, and a second shape would fragment the schema for no gain.
- Naming the GitLab base key `base`: rejected in favor of `targetBranch`, which matches the flag the
  value fills.
- Resolving the configured format inside clap as a dynamic `default_value`: rejected; it would load
  configuration before parsing and make `--help` depend on repository state, while the explicit
  resolution keeps flag-wins precedence observable in one place.

## Traceability

- Active behavior: [SRS RF-CFG.5](../srs.md#rf-cfg5), [SRS RF-CFG.6](../srs.md#rf-cfg6),
  [SRS RF-CFG.7](../srs.md#rf-cfg7), [SRS RF-CFG.8](../srs.md#rf-cfg8),
  [SRS RF-OUT.1](../srs.md#rf-out1)
- Technical flow: [architecture configuration](../architecture.md#configuration-and-context)
