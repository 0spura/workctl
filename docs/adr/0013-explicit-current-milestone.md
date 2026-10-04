# 0013: Explicit current-milestone selection

- Status: Accepted
- Date: 2026-10-03
- Tracker: [issue #9](https://github.com/0spura/workctl/issues/9)
- Supersedes: [ADR-0012](./0012-default-current-milestone.md)

## Context

ADR-0012 made issue and pull-request creation select the repository's current milestone
implicitly whenever `--milestone` was omitted. That made one command's remote effect depend on
repository milestone state: an unassigned item and an implicitly assigned one produced the same
command line, and the caller could not opt out of assignment except by naming a specific
milestone. The user asked for explicit control while keeping creation a single invocation with
optional metadata.

## Decision

Milestone assignment is explicit. `--milestone @current` selects the open GitHub milestone with
the nearest due date today or later; overdue and undated milestones are ignored, and the command
fails with `invalid_input` before writing when no milestone qualifies or the nearest eligible due
dates tie. Omitting `--milestone` performs no lookup and sends no milestone flag, so nothing is
assigned implicitly. Any other value is a literal milestone name passed through unchanged. The
selector applies to issue create/edit and pull-request create/update; list filters remain
literal, and omission on an edit or update preserves the remote value. `@current` is reserved and
cannot address a milestone whose literal title is `@current`.

## Consequences

- One shared resolution path serves every issue and pull-request milestone setter.
- A create or setter is fully determined by its arguments plus a successful `@current` lookup; a
  failed lookup fails the write instead of silently dropping the request.
- Self-assignment and current-milestone assignment stay in one command:
  `workctl issue create --title T --assignee @me --milestone @current`.
- A provider that does not implement the selector must reject or define it itself rather than
  inheriting GitHub's lookup.
- The milestone lookup now also runs on edits that set `@current`, so a setter can fail for a
  reason unrelated to the target item.

## Alternatives

- Keep the implicit default and add a separate opt-out flag: rejected because the surprising
  behavior would remain the default.
- Assign nothing and require a literal milestone name: rejected because callers would still need a
  second command to discover the current milestone.
- A dedicated `--current-milestone` boolean: rejected because it cannot share one typed field with
  a literal `--milestone` value and adds a second flag to every command.

## Traceability

- Requirements: [RF-WI.1](../srs.md), [RF-WI.4](../srs.md), [RF-PR.1](../srs.md),
  [RF-PR.8](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
