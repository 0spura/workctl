# 0022: Native GitHub issue editing and explicit relationships

- Status: Accepted
- Date: 2026-10-07
- Tracker: [issue #14](https://github.com/0spura/workctl/issues/14)
- Supersedes: [ADR-0021](./0021-github-project-field-editing.md) only for its single-target edit surface and generic issue-edit entry point. Project field validation, explicit-only changes, serial writes and partial-success behavior remain in force.

## Context

The user requested correction of the identified `gh issue edit` divergences and implementation of parent/sub-issue and blocking relationships. These operations already have native flags in the installed GitHub CLI; no relationship inference or new remote API is needed.

## Decision

- Use `--remove-milestone` for GitHub issue and PR editing, with no `--clear-milestone` alias. Add native `-t`, `-b`, `-F`, `-m`, and global `-R` shortcuts where the corresponding fields exist.
- Accept `--type`/`--remove-type`, `--parent`/`--remove-parent`, and add/remove variants for sub-issues, blocked-by, and blocking relationships. Relationship lists support repeated and comma-separated numbers or canonical GitHub issue URLs.
- Keep these fields in provider-owned `NativeIssueEdit`, alongside but not inside the generic `IssuePatch`. All native fields join the same guarded `gh issue edit` invocation as ordinary issue metadata. Remove the unused generic issue-edit trait method rather than retain a second entry point.
- Validate canonical references, conflicting set/remove flags and normalized relationship overlaps before writes. GitHub remains responsible for authorization, relationship existence and cycle rules.
- Support multiple distinct edit targets in one repository. URL targets can supply GitHub/repository context; mismatched target repositories fail closed. Explicit relationship URLs may reference another repository.
- Process targets serially, reusing one Project schema plan and one label catalog. Each target retains its own body resolution, automatic-label classification and pre-write timestamp guard. Work is bounded by the explicit target/field lists; no relationship traversal is added.
- Preserve single-target object output. Multiple distinct targets return an array of full issue records. A later failure stops the batch and reports completed/pending target URLs and safe nested failure details as `partial_success`, without rollback or retries. Failed confirmation after a successful native mutation reports that completed mutation.

## Consequences

Agents can use native GitHub relationship flags rather than separate GraphQL calls. Batch writes are not transactional; completed targets remain changed. An issue revision guard does not protect Project-field revisions. GitLab keeps its provider-selected grammar and native write semantics; only harmless shared argument shortcuts change.

## Alternatives

- Invent relationship commands or infer relationships from model scores: rejected; the request is explicit native editing.
- Keep old milestone spelling as an alias: rejected; clean cutover avoids two conventions.
- Widen the provider-neutral patch with GitHub-specific fields: rejected; provider-owned metadata keeps GitLab contracts independent.

## Traceability

- Requirements: [RF-WI.4 and RF-PR.8](../srs.md).
- Flow and bounded I/O: [architecture](../architecture.md).
- Stateful verification: `tests/cli_native_edit.rs`; existing issue, PR and GitLab CLI regression suites.
