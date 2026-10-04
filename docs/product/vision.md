# Workctl vision

## Problem

Developers and coding agents need a small local CLI for managing work items in the current repository. The existing MCP server is being replaced with a native Rust command-line tool, beginning with the GitHub issue workflow used most often.

## Vision

`workctl` provides predictable, scriptable issue operations while leaving authentication to the provider's official CLI (`gh`, `glab`). It infers the current repository and provider from Git when possible, accepts explicit overrides, and reports machine-readable errors without exposing provider diagnostics. Each provider keeps its own command grammar, mirroring that provider's CLI. Callers may explicitly opt into high-confidence automatic labels for GitHub issue create/edit; all explicit labels use the provider's native fields.

## Initial users and outcomes

- Developers can create, list, inspect, and edit GitHub issues from a terminal.
- Coding agents can invoke those same operations without an MCP host or direct access to credentials.
- Scripts can consume compact JSON output and distinguish failures using stable error codes and exit status.

## v0 boundary

The release supports GitHub issue and pull-request operations and GitLab issue reads, optional repository-native metadata, documented image/video attachments through `gh`, body edits that never require rewriting the whole body, and opt-in Jev automatic labels for GitHub issue create/edit. Those paths disclose the issue title/body and existing label catalog; create/edit apply only scores >= 0.8, and edits only add labels. Other commands and pull-request flows do not send content to Jev. Labels specified by the caller use the provider's native issue/PR fields. It excludes private attachment endpoints, project/board administration, issue deletion, Jira, comments, relationships, checklists, arbitrary HTTP integrations, generalized token management, and MCP transport. Metadata remains optional and follows repository conventions. See the [SRS](../srs.md) for observable behavior and [architecture](../architecture.md) for boundaries.

## Design principles

- Keep the CLI and provider boundary explicit; avoid an omnibus command or provider module.
- Prefer Git context, with explicit provider and repository flags for automation and non-repository use.
- Fail closed on ambiguous provider selection, invalid configuration, or malformed provider data.
- Keep user-facing output stable and safe: JSON by default, optional text, no raw subprocess stderr.
- Exercise behavior against isolated command fixtures; tests do not mutate live GitHub issues.
