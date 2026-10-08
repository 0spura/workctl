# Workctl vision

## Problem

Developers and coding agents need a small local CLI for managing work items in the current repository. The existing MCP server is being replaced with a native Rust command-line tool, beginning with the GitHub issue workflow used most often.

## Vision

`workctl` provides predictable, scriptable issue operations while leaving authentication to the provider's official CLI (`gh`, `glab`). It infers the current repository and provider from Git when possible, accepts explicit overrides, and reports machine-readable errors without exposing provider diagnostics. Each provider keeps its own command grammar, mirroring that provider's CLI. Callers may explicitly opt into high-confidence automatic labels for GitHub issue create/edit; all explicit labels use the provider's native fields.

## Initial users and outcomes

- Developers can create, list, inspect, edit, close, reopen, comment on, and lock GitHub issues from a terminal.
- Coding agents can invoke those same operations without an MCP host or direct access to credentials.
- Scripts can consume compact JSON output and distinguish failures using stable error codes and exit status.

## v0 boundary

The release supports GitHub issue and pull-request operations and GitLab issue create/update, optional repository-native metadata, GitHub issue type and parent/sub-issue/blocked-by/blocking relationships, GitHub Project field assignment with configured defaults and opt-in model field selection, documented image/video attachments through `gh`, body edits that never require rewriting the whole body, and opt-in automatic labels for GitHub issue create/edit through the provider-neutral decision-model package. Issue and pull-request conversation commands (close, reopen, comment, lock, unlock, and pull-request revert) forward single native provider operations without contacting a model. Those paths disclose the issue title/body and the label catalog, plus discovered single-select/iteration Project options when field selection is enabled; create/edit apply only scores >= 0.8, and label edits only add labels. Other commands and pull-request flows do not send content to the model. Labels specified by the caller use the provider's native issue/PR fields. It excludes private attachment endpoints, project/board administration, issue deletion, Jira, comment threads, GitLab merge requests, checklists, arbitrary HTTP integrations, generalized token management, and MCP transport.

## Design principles

- Keep the CLI and provider boundary explicit; avoid an omnibus command or provider module.
- Prefer Git context, with explicit provider and repository flags for automation and non-repository use.
- Fail closed on ambiguous provider selection, invalid configuration, or malformed provider data.
- Keep user-facing output stable and safe: JSON by default, optional text, no raw subprocess stderr.
- Exercise behavior against isolated command fixtures; tests do not mutate live GitHub issues.
