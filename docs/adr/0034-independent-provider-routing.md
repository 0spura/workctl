# 0034: Independent code-host and work-item provider routing

- Status: Accepted
- Date: 2026-10-09
- Tracker: No new item created per user instruction; see durable decision in ai-memory.
- Supersedes: [ADR-0017](./0017-provider-selected-cli-grammar.md) only for the premise that one provider selects the entire command grammar.

## Context

The current CLI resolves one provider before parsing and uses that selection for the complete grammar. This prevents a GitLab code host from being used alongside a different work-item tracker. It also couples GitLab host capabilities, such as merge requests and issue boards, to the provider used by generic `issue` commands.

## Decision

Resolve code-host and work-item providers independently, and route every command to the provider that owns its capability:

- `pr`/`mr` and GitLab-only host operations use the code-host provider.
- Generic `issue` operations use the work-item provider.
- GitLab-only issue-board operations use an explicit GitLab namespace and the code-host project context, so they remain available when the work-item provider is different.
- Provider grammars stay native and separate; never union provider-specific flags or emulate a missing capability with another provider.
- Repository/workspace scope is resolved for the command's owning domain. Missing provider, scope, or capability fails before authentication or network access.
- Preserve existing single-provider configuration through documented fallback precedence while introducing explicit independent selectors.

This decision separates routing and ownership. It does not claim that a Linear adapter or GitLab board commands exist until their own implementation and acceptance tests pass.

## Consequences

Configuration parsing, pre-parse provider selection, CLI tree construction, command dispatch, repository resolution, help, SRS, and architecture need distinct code-host and work-item provider contexts. A future Linear adapter can own generic work items without removing GitLab merge-request or board commands. Existing commands must retain their provider-native arguments and output semantics.

## Alternatives

- Keep one provider for all commands: rejected because choosing a tracker would hide code-host operations and vice versa.
- Route all operations through the work-item provider: rejected because merge requests and GitLab boards are host capabilities.
- Expose the union of every provider's verbs and flags: rejected because native command semantics differ and help would misrepresent which flags apply.
- Add raw provider-argument passthrough: rejected because it bypasses validation and safe error handling.

## Traceability

- Active requirements: `docs/srs.md` RF-CLI.1, RF-CFG.1, RF-CFG.2, and RF-GL.1.
- Architecture: `docs/architecture.md`, command flow and provider module map.
- Durable decision: ai-memory `decisions/independent-code-and-work-item-provider-routing.md`.
