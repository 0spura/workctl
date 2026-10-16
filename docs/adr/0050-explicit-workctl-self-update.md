# 0050: Explicit workctl self-update

- Status: Accepted
- Date: 2026-10-16
- Tracker: Not created; bounded single-command change.

## Context

ADR 0048 made `workctl update` a check that prints the Cargo install command and forbade installing. Users then copy and run that command by hand every time.

## Decision

`workctl update` checks as before and, when a newer commit exists, runs `cargo install --git https://github.com/0spura/workctl --locked --force` itself. `workctl update --check` keeps the report-only behavior. The automatic post-command check still only prints a stderr notice, now pointing to `workctl update`, and never installs. This supersedes the "never install" clause of ADR 0048; the check mechanism, cache, timeout and no-credentials rules are unchanged.

The child process gets a null stdin and stdout; Cargo's progress stays on stderr, so JSON and text results on stdout are unaffected. A missing or failing `cargo` returns a safe `context` error without diagnostics.

## Consequences

- One command replaces copy-pasting the install line.
- Installing needs a Rust toolchain on the machine, as before.
- The install replaces the running executable; on Unix this is safe, on Windows it may fail and surfaces as a failed install.
- The reported `latest` is the checked commit; if `main` advances during the build, Cargo may install a newer one.

## Alternatives

- Download a release binary: rejected; no release artifacts exist.
- Install from the automatic check: rejected; it would change installed code without an explicit command.

## Traceability

- Requirement: [RF-CLI.2](../srs.md#rf-cli2-check-for-and-install-workctl-updates)
- Implementation: `src/update.rs`, `src/main.rs`, `src/cli/mod.rs`, `src/output/text.rs`
- Architecture: [docs/architecture.md](../architecture.md#update-checks)
