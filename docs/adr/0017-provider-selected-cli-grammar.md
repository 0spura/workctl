# 0017: Provider-selected CLI grammar

- Status: Accepted
- Date: 2026-10-04
- Tracker: [issue #11](https://github.com/0spura/workctl/issues/11)
- Supersedes: [ADR-0016](./0016-gh-verb-parity.md) for the single-surface premise and for `--provider` as a grammar selector. The verb parity ADR-0016 established stands inside the GitHub grammar.

## Context

Agents carry provider priors: they know `gh` and `glab`. Those two CLIs disagree on semantics, not only on spelling. Measured from the two CLIs' own help:

| concept | `gh pr merge` | `glab mr merge` |
| --- | --- | --- |
| strategy | `--merge`/`--squash`/`--rebase` | `--squash`/`--rebase`; no `--merge` |
| automatic merge | `--auto`, default **false**, after requirements | `--auto-merge`, default **true**, when the pipeline succeeds |
| undo automatic | `--disable-auto` | `--auto-merge=false` |
| delete branch | `--delete-branch` (local **and** remote) | `--remove-source-branch` (remote only) |
| commit message | `--body`/`--subject`/`--body-file` | `--message`/`--squash-message` |
| head guard | `--match-head-commit` | `--sha` |
| admin merge | `--admin` | — |
| confirmation | — | `--yes` (glab prompts by default) |

A neutral flag mapped across providers would invert the automatic-merge default and hide the local-versus-remote difference in branch deletion. Naming is not the whole problem; the semantics differ.

A provider-neutral grammar is also not selectable. `--provider` is a clap global (`src/cli/mod.rs`) resolved after parsing, in `src/config/resolve.rs`. A single surface must therefore carry the union of every provider's flags and reject the wrong ones at runtime, and its `--help` cannot say which flags apply to which provider.

The provider is nevertheless already inferable offline, before parsing, from `.workctl.json` and `.workctl.local.json` (`provider`, `workItemProvider`) and the Git origin host (`github.com`, `gitlab.com`).

Two further forcing points are in the current contract:

- The normalized domain carries GitHub vocabulary. RNF-DOM.1 mandates a pull request `mergeable` and `review_decision`, and check runs with a `bucket` and `workflow`. GitLab has `merge_status`/`detailed_merge_status`, approval rules, and pipelines; `bucket` and `workflow` do not exist there.
- The repository identifier is `OWNER/REPO`. `validate_repo` and `parse_remote` require exactly one `/`, so GitLab nested namespaces (`group/subgroup/project`) cannot be expressed.

## Decision

Resolve the provider before parsing, then build and parse with that provider's grammar.

1. **Provider resolution runs before parsing.** Read `.workctl.json`/`.workctl.local.json`, then the Git origin host, then `--provider` as an override. The information is offline and independent of the command, so it can be hoisted ahead of clap.
2. **Each provider owns a static grammar.** The provider's resource nouns and flags mirror its own CLI: the GitHub grammar uses `pr` with `gh`'s verbs and flags; a future GitLab grammar uses `mr` with `glab`'s. There is no union of flags and no runtime rejection of another provider's flag.
3. **`--provider` stops being a grammar selector.** It selects the provider when the config and remote cannot. A `--provider` that disagrees with the command's grammar is an explicit error naming the correct command.
4. **`--repo` accepts `[HOST/]OWNER[/…]/REPO`.** A host, when present, must agree with the resolved provider. Nested namespaces are supported.
5. **Output is provider-native.** Fields stay common only where they are genuinely shared. Provider-specific fields are documented as belonging to that provider.
6. **Help follows the grammar.** Top-level `--help` is provider-neutral and states the resolution order; a subcommand's help resolves the provider first and shows that provider's grammar.

## Consequences

- A caller can rely on its provider's own CLI knowledge: `workctl pr merge --auto` is `gh pr merge --auto`, and a future `workctl mr merge --auto-merge` is `glab mr merge --auto-merge`.
- Help becomes environment-dependent. In a GitLab-configured root the grammar has `mr`, not `pr`.
- The SRS must scope RF-WI.\*, RF-PR.\*, and RNF-DOM.1 to GitHub and drop the claim that provider vocabulary is normalized.
- Only GitHub is implemented, so the observable GitHub surface is unchanged; what ships now is the resolution and grammar-selection layer, the repository-identifier change, and the provider-scoped naming.
- Every added provider costs a grammar, its tests, and its documentation. There is no single provider-agnostic contract to maintain, by design.

## Alternatives

- **Explicit provider group** (`workctl github pr merge --auto`): static and exact, but discards the config/remote inference that already works and makes the provider mandatory on every invocation.
- **Inference with provider-owned resources but a shared `issue` group**: keeps zero-typing and makes `pr`/`mr` disjoint, but `issue` is shared and GitLab issues carry `--weight`, `--confidential`, and `health_status` that GitHub does not, so the shared group is a union of flags again.
- **Neutral flags mapped per provider**: rejected. It inverts the automatic-merge default and hides the branch-deletion difference; the mapping can lie.
- **Union of flags with runtime rejection**: rejected. Help cannot say what applies to whom, and the surface grows back past what ADR-0016 shrank.
- **Raw passthrough of provider arguments**: already forbidden by the architecture; it removes validation and safe errors.

## Risks

- Environment-dependent help can mislead a caller that reads help outside a repository or caches it without context.
- The pre-parse of `--provider` and `--repo` duplicates part of the grammar and must keep tracking clap's global-flag behavior, including `--provider X` and `--provider=X`.
- Scoping the SRS to GitHub narrows a contract that was previously stated as provider-neutral; consumers relying on the neutral wording must be told.

## Gaps

- Whether `--repo` also accepts a full URL or Git URL (`glab` does) is deferred; the `[HOST/]OWNER[/…]/REPO` form is what ships.
- The GitLab grammar is not designed. It lands with the GitLab adapter, and its resource nouns, flags, and output shape are decided then.

## Traceability

- Requirements: [docs/srs.md](../srs.md) (RF-CLI.1, RF-CFG.1, RF-CFG.2, RF-WI.\*, RF-PR.\*, RNF-DOM.1)
- Architecture: [docs/architecture.md](../architecture.md)
- Project stack: [docs/project.md](../project.md)
