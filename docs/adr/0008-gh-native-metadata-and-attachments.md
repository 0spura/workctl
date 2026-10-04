# 0008: Use native GitHub CLI metadata and attachment support

- Status: Accepted
- Date: 2026-10-02
- Tracker: none; direct implementation authorized by the user.
- Supersedes: [ADR-0007](./0007-remove-unsupported-issue-attachments.md) and the attachment/write-path portion of [ADR-0005](./0005-attachments-and-non-rewrite-body-edits.md). ADR-0005's body-edit, patch, and concurrency decisions remain in force.

## Context

GitHub CLI 2.99.0 added documented, repeatable `--attach` support to `gh issue create|edit` and `gh pr create|edit`. The current `gh` in the development environment is older, so attachment support must be checked when (and only when) the caller requests an attachment. Automatically upgrading `gh` is outside `workctl`'s authority.

The native issue and pull-request create/edit commands already accept optional assignees, labels, milestones, projects, and (for pull requests) reviewers. Exposing these flags helps callers keep metadata in the provider without requiring another write path. GitHub labels can carry descriptions; repository conventions remain repo-specific.

Pull request titles and bodies remain explicitly authored. `--fill` is intentionally not added: the repository's pull-request workflow requires contextual body content rather than a commit-derived fill.

## Decision

Use `gh issue create|edit` for issue writes and continue using `gh pr create|edit` for pull-request writes. Forward typed optional metadata values as fixed native flags; never accept arbitrary raw `gh` arguments. All metadata remains optional. `workctl` does not infer labels/projects, install defaults, or make an additional model call. Callers should follow the repository's guidance and existing label/project taxonomy; when uncertain, omit optional metadata rather than invent a value.

Expose `--attach FILE[#ALT]` on issue create/edit and pull-request create/update. Check `gh --version` before any attachment write and require version 2.99.0 or newer. Do not upgrade or download `gh`; preserve normal metadata/body operations on older versions when attachments are not requested. Let `gh` validate/upload media and keep the authentication boundary inside the official CLI.

An attachment create operation can fail after the remote item has been created. Return a safe `attachment_create_uncertain` error for a failed create that included attachments, advise checking GitHub before retrying, and never retry automatically. Edit failures remain the generic safe provider error; no provider diagnostics or paths are exposed.

## Consequences

- Issue create/edit and PR create/update support optional, typed metadata flags, project membership, and attachment routing through documented `gh` commands.
- Attachments fail closed with `dependency_version` when `gh` is older than 2.99.0 or its version cannot be parsed; no attachment command is attempted.
- The CLI neither prescribes project-specific label taxonomies nor silently assigns one. Repository instructions and GitHub label descriptions are the selection hints; no embedded model-based classifier is introduced.
- Some create failures involving attachments may represent a partially completed remote action, so callers are told to inspect the remote state before retrying.
- Body edits still fetch/check the current item before mutation, and `--expect-updated-at` continues to guard writes.

## Alternatives

- **Keep attachments unsupported:** rejected; the official GitHub CLI now provides a documented, supported upload path.
- **Call GitHub's private upload endpoint:** rejected; it bypasses the supported CLI/authentication boundary.
- **Make metadata mandatory or auto-classify it in `workctl`:** rejected; labels and projects are repository-specific and optional, and an embedded decision model adds cost and nondeterminism.
- **Automatically upgrade `gh`:** rejected; installation/update authority belongs to the user or machine package manager.
- **Add `--fill` to PR creation:** rejected; it conflicts with the required contextual PR body workflow.

## Sources

- GitHub CLI v2.99.0 release, attachment support and fixes: <https://github.com/cli/cli/releases/tag/v2.99.0>
- GitHub CLI manual, attaching files: <https://docs.github.com/en/github-cli/github-cli/attaching-files-with-github-cli>
- GitHub CLI manual, `gh label list` and JSON name/description: <https://cli.github.com/manual/gh_label_list>
- GitHub REST API label objects include name, description, and color: <https://docs.github.com/en/rest/issues/labels>

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-WI.1, RF-WI.4, RF-PR.1, RF-PR.8.
- Architecture: [docs/architecture.md](../architecture.md).
- Superseded issue-attachment removal: [ADR-0007](./0007-remove-unsupported-issue-attachments.md).
- Body edit policy: [ADR-0005](./0005-attachments-and-non-rewrite-body-edits.md).
