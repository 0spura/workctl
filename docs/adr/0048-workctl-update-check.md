# 0048: Workctl update checks

- Status: Accepted
- Date: 2026-10-13
- Tracker: Not created; GitHub CLI authentication is unavailable.

## Context

Workctl is installed from the Git repository with `cargo install --git`. Its package version alone does not identify which repository revision is installed, and update checks must not interrupt normal command output or install code unexpectedly.

## Decision

Embed the build's Git revision and compare it with the latest commit on `0spura/workctl`'s `main` branch. Expose an explicit `workctl update` check and a best-effort automatic check after successful ordinary commands, normally limited to one request per 24 hours by a local cache. If cache reads or writes fail, the next successful command may retry. Use the fixed GitHub HTTPS Git-reference API, a five-second timeout, no redirects, and no GitHub credentials. Send update notices only to stderr. Never download or install an update automatically; print the Cargo install command when newer code is available.

## Consequences

- Git-based Cargo installations can be checked even when the package version has not changed.
- The explicit command reports current and latest revisions and returns safe errors; automatic network/cache failures do not affect the requested command.
- Builds without an embedded Git revision cannot perform checks; automatic checks remain silent and the explicit command returns a safe error.
- API availability and unauthenticated GitHub rate limits can make checks unavailable; cache and timeout bound normal impact.
- The automatic check may add up to five seconds to the first successful command after the daily interval.

## Alternatives

- Check a GitHub release tag: rejected because the install workflow tracks repository Git revisions, not a release artifact.
- Check every invocation: rejected because it would add unnecessary network traffic and latency.
- Install updates automatically: rejected because it changes installed code without an explicit command.

## Traceability

- Requirement: [RF-CLI.2](../srs.md#rf-cli2-check-for-workctl-updates)
- Implementation: `build.rs`, `src/update.rs`, `src/main.rs`, `src/cli/mod.rs`
- Architecture: [docs/architecture.md](../architecture.md#update-checks)
