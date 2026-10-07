# 0018: Resolve GitHub Project fields dynamically

- Status: Superseded by [ADR-0019](./0019-decision-model-project-field-selection.md)
- Date: 2026-10-07
- Tracker: [issue #13](https://github.com/0spura/workctl/issues/13)
- Supersedes: ADR-0008's decision against configured issue defaults and project-field inference, only for the explicit GitHub issue-create defaults and Project-field behavior defined here. Its native CLI boundary, optional metadata, and no-inference rule otherwise remain in force.

## Context

GitHub Projects define their own fields and values. A repository issue can be added to a Project, but fields such as Priority, Status, dates, and Effort belong to that Project rather than to a universal issue schema. `gh issue create --project` adds membership but does not set custom field values. `gh project field-list --format json` does not expose sufficient data-type information for reliable validation. GitHub CLI can edit existing project-item fields through `gh project item-edit`, one field per invocation for a non-draft item.

Project-field values are project-scoped, and a Project can contain issues from more than one repository. Configuring a Project therefore must not silently associate issues from arbitrary `--repo` targets.

## Decision

1. Add repeated `--project-field NAME=VALUE` to GitHub `issue create`. Field assignments bind to exactly one Project profile; when an explicit `--project` is supplied, its discovered title must match that profile. Requests with no applicable profile or multiple effective Projects fail before issue creation.
2. Discover the Project, field types, single-select options, and iteration values through GitHub GraphQL invoked by authenticated `gh api graphql`. Use the GraphQL IDs with `gh project item-add` and `gh project item-edit`; do not parse human-readable CLI tables. Keep the canonical Project URL and exact repository allowlist in the profile.
3. Extend strict configuration with GitHub issue defaults for assignees, labels, label candidates, and an optional Project profile containing a canonical URL, exact allowed repository paths, and string-valued field defaults. The Project profile is used only when the resolved target repository is listed. Explicit assignees replace defaults; labels are the configured-first ordered de-duplicated union with explicit labels; explicit field values override a configured value of the same name. `labelCandidates` limits the repository labels sent to the existing DecisionModel path for `@auto`; the model does not choose Project fields. Omitted milestone remains unset, per ADR-0013.
4. Validate every requested field, type, option, and value before creating the issue. Initially support Project field types that map to documented `gh project item-edit` inputs: text, number, date, single select, and iteration. Reject unknown or unsupported fields without making a remote write.
5. For field/profile operations, create the issue once, add it to the Project, then apply field assignments serially. The remote sequence is not transactional. On any post-create failure, return generic `partial_success` details with the created resource and completed/pending operations; never retry automatically. Do not expose raw provider diagnostics.
6. Keep `partial_success` provider-neutral so other adapters can reuse the same error type and details contract. Model-driven selection is limited to labels through the existing classifier; no classifier integration for Project fields is added.

## Consequences

- Project schemas and option names remain dynamic; `Priority`, `Status`, and field options are never hardcoded.
- Project profile defaults are safe across repositories because the allowlist scopes the board association. Issues outside that scope do not inherit the Project or its field defaults.
- The `gh` authentication boundary remains authoritative. Project read/write scope failures are safe errors; a write-scope failure after issue creation is represented as partial success.
- The JSON error contract gains an optional `details` object only for partial success; the reusable error type identifies a generic resource and completed/pending operations. Existing errors retain their current shape.
- A failed Project update may leave the issue associated with only some fields. Callers inspect the reported resource and do not retry issue creation.

## Alternatives

- Treat Project fields as universal issue metadata: rejected because fields and value sets vary per Project.
- Hardcode known fields such as Priority and Status: rejected because Projects are user-configurable.
- Parse human-readable `gh project field-list` output: rejected because it is not a stable typed interface and omits data needed for pre-write validation.
- Apply a default Project to every target repository: rejected because a Project association is an explicit board-membership decision and Projects can aggregate unrelated repositories.
- Retry the create-and-field sequence after a later failure: rejected because the issue may already exist.

## Traceability

- Requirements: [RF-WI.6](../srs.md), [RF-CFG.3](../srs.md), [RNF-OUT.1](../srs.md)
- Architecture: [docs/architecture.md](../architecture.md)
- Tracker: [issue #13](https://github.com/0spura/workctl/issues/13)
- GitHub CLI: [project field-list](https://cli.github.com/manual/gh_project_field-list), [project item-add](https://cli.github.com/manual/gh_project_item-add), [project item-edit](https://cli.github.com/manual/gh_project_item-edit)
- GitHub API: [Managing Projects](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects)
- Existing metadata decision: [ADR-0008](./0008-gh-native-metadata-and-attachments.md)
- Explicit milestone decision: [ADR-0013](./0013-explicit-current-milestone.md)
