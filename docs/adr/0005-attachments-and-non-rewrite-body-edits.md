# 0005: Attachment upload and non-rewrite body edits

- Status: Superseded in part by [ADR-0008](./0008-gh-native-metadata-and-attachments.md)
- Date: 2026-10-02
- Tracker: none; the user authorized direct implementation in this repository.
- Refines: [ADR-0004](./0004-workctl-rust-cli.md). Its body-edit, patch, and concurrency decisions remain in force; the attachment/write-path decisions were superseded by ADR-0008.

## Context

The user asked for more agent autonomy in two specific gaps:

- Attaching images to an issue was impossible; only title and body could be set.
- Editing an issue required the caller to reproduce the entire new body. For a long issue, regenerating the whole text to change one line is expensive and error-prone, and a single paraphrase silently rewrites unrelated content.

Two facts bounded the design:

- GitHub's REST API has no documented endpoint for uploading an issue attachment. Attachment upload exists only in the `gh issue create|edit --attach` command.
- GitHub's "Update an issue" endpoint reference does not document conditional-request semantics, and `gh api` would require threading an `ETag` header by hand. Whether `If-Match` is honored on this endpoint is unverified, so the concurrency guard cannot depend on it.

## Decision

- **Attachments go through `gh issue create|edit --attach`.** When an edit or create carries at least one attachment, the adapter switches from `gh api` to the `gh issue` command; the body still travels on stdin via `--body-file -`. `FILE[#ALT]` carries optional alt text, and each path must be an existing regular file. Without attachments, the `gh api` path is unchanged.
- **Body edits are client-side read-modify-write with typed mutations**, not a diff supplied by the caller against an opaque server state: `--body`/`--body-file` (replace), `--append-body`/`--append-body-file` (append), `--replace-section` + `--section-body`/`--section-body-file` (replace one ATX section), and `--patch-file` (unified diff).
- **`--patch-file` applies a unified diff with exact context matching.** Location is content-based: each hunk's pre-image must appear verbatim at or after the previous hunk's end. There is no fuzzy matching and no three-way merge; a shifted or edited context is a hard `patch_conflict` failure, never a silent misapplication. Patch text uses LF and must be UTF-8.
- **`--expect-updated-at` is the client-side concurrency guard.** The command fetches the issue once, and if the fetched `updated_at` differs from the supplied value it fails with `conflict` before sending any PATCH. The fetch also rejects a pull request and supplies the current body for the mutation.
- **At most one body-change flag per invocation**, and each requires its companion argument where applicable (`--replace-section` needs `--section-body` or `--section-body-file`). Conflicts fail as `invalid_input` before any `gh` call.
- **List filters pass through to `gh issue list`**: `--label` (repeatable), `--assignee`, `--author`, `--mention`, `--milestone`, `--search`, and `--type`. Blank filter values are rejected. These are fixed flags, never a raw argument passthrough.

## Consequences

- One `edit` performs exactly one fetch plus one write; the fetch is required by the mutation and the guard rather than by the write alone.
- A create or edit with attachments costs one extra fetch after the write, because `gh issue create|edit` prints a URL rather than the issue JSON. The URL is parsed for the issue number and the issue is then read back.
- Body text read from a file or stdin is capped at 1 MiB and must be valid UTF-8; a larger or non-UTF-8 source fails as `invalid_input`.
- Patch application depends on the caller's diff matching the body fetched immediately before it. A concurrent third-party edit between the fetch and the PATCH is detected only when `--expect-updated-at` is supplied.
- The non-goal list loses attachments and labels/assignees/milestones as *filters*; it retains them as broader capabilities (no attachment listing, no label management, no assignee mutation).

## Alternatives

- **Fuzzy patch application** (`patch -l` style, matching with whitespace or context tolerance): rejected. An approximate match in a prose body can land the edit in the wrong place, and the failure would be invisible. A hard error is recoverable; a wrong silent write is not.
- **Caller-supplied search/replace blocks** (a proprietary delimiter format): rejected. Agents emit `git diff` output natively, and a custom format would need its own documentation and parser for no gain in failure behavior.
- **Server-side optimistic concurrency via `If-Match`/ETag**: not adopted. Support is undocumented for this endpoint and unverified; relying on it would produce a guard that silently does nothing. It remains a possible refinement if conditional support is confirmed.
- **A custom attachment upload endpoint**: rejected. GitHub does not document one; the `gh issue` command is the only supported path, and reimplementing uploads over direct HTTP would break the `gh`-owned authentication boundary.
- **Requiring the caller to send the whole new body** (status quo): rejected as the user's stated pain point.
- **Line-number-addressed edits** (`--replace-lines 10-20`): rejected. Line numbers drift as the body changes, and counting lines is exactly what an agent does badly.

## Traceability

- Requirements: [docs/srs.md](../srs.md) — RF-WI.1, RF-WI.2, RF-WI.4, RF-OUT.1
- Architecture: [docs/architecture.md](../architecture.md)
- Project stack: [docs/project.md](../project.md)
