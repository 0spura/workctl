# 0049: Sanitized provider error cause

- Status: Accepted
- Date: 2026-10-16
- Tracker: Not created.
- Supersedes: the "keep raw stderr withheld" clause of [0004](0004-workctl-rust-cli.md)

## Context

Coding agents drive workctl non-interactively. Failures such as `github_cli: the GitHub CLI operation failed` or `invalid_input: invalid command-line arguments` gave the agent nothing to act on: a wrong flag, a missing resource, a permission error, and a clap usage error were indistinguishable, and agents fell back to calling `gh` directly. One real defect (`pr status` failing on repositories without required checks) was invisible for the same reason.

## Decision

Failed `gh`/`glab` invocations and command-line usage errors carry an optional `cause`: the first non-empty stderr line (or clap message line), at most 300 characters, with token-like words (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`, `glpat-`, `gldt-`), the word after `Bearer`/`token`/`Basic`/`Authorization:`, and URLs with embedded credentials replaced by `[redacted]`. Text output appends `: <cause>` to the error line with control characters escaped; JSON adds a top-level `cause` string. The stable `code` and `message` are unchanged.

Authentication probes, uncertain-write errors, and `partial_success` details keep withholding provider stderr. Remaining stderr lines are still discarded.

Body-like flags (`--body`, `--append-body`, `--section-body`) accept values beginning with `-`, since Markdown bodies commonly start with a list marker.

## Consequences

- Agents can act on the provider's reason without a second tool.
- One provider line may reach logs; redaction is pattern-based, so an unrecognized secret format in a first stderr line could pass through.
- `gh pr checks` output matching `checks reported` (including `no required checks reported`) is an empty result, not a failure.
