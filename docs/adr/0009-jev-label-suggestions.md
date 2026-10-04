# 0009: Add explicit Jev-backed issue label suggestions

- Status: Accepted
- Date: 2026-10-02
- Tracker: [issue #8](https://github.com/0spura/workctl/issues/8)
- Supersedes: The no-direct-HTTP boundary in [ADR-0004](./0004-workctl-rust-cli.md) and the no-model-classification decision in [ADR-0008](./0008-gh-native-metadata-and-attachments.md), only for the explicit issue label suggestion command.

## Context

Repository labels are project-specific, and `workctl` previously left selection entirely to the caller. The user approved an opt-in Jev AI integration for suggesting labels. The command needs a small, explicit HTTPS boundary without replacing `gh` as the GitHub credential and issue-operation owner.

## Decision

Add `workctl issue suggest-labels NUMBER`. It is read-only: fetch the issue and repository label catalog through authenticated `gh`, send only issue title/body plus current label names/descriptions to the fixed official Jev endpoint, and return each existing label with its Noul probability. Sort descending by probability, then by label name. Never apply or mutate labels.

Read `JEV_API_KEY` from the process environment only when this command is invoked, before any GitHub access. Send it only as an HTTPS Bearer header to `https://thejevai.com/v1/systemone`; never persist, log, or include it in errors. Do not allow an endpoint override. Use `jev-latest`, a 30-second timeout, no redirects or retries, a 2 MiB serialized-request cap, a 1 MiB response cap, and a maximum catalog of 1,000 labels. An empty catalog returns no suggestions without calling Jev.

The external disclosure is explicit in command help and documentation: invoking the command sends the selected issue title/body and repository labels to Jev. No comments, URL, GitHub credentials, or unrelated metadata are sent.

## Consequences

- `gh` remains responsible for GitHub authentication and all GitHub operations; Jev is a separate decision-only service boundary.
- The process now owns one narrow direct HTTPS integration and reads one provider API key from the environment. This is not a general HTTP client or credential-management facility.
- Suggestions remain advisory; callers decide whether to apply any label using existing explicit metadata commands.
- Tests use a local Jev HTTP fixture and never send issue data or credentials to the live API.

## Alternatives

- Apply suggested labels automatically: rejected; Jev probabilities are signals, not correctness guarantees, and the user only asked for selection guidance.
- Make one Choice question: rejected; a single-choice result cannot suggest multiple labels.
- Add a model classifier inside every create/edit: rejected; it would silently disclose issue text and change the existing write contract.
- Keep all direct HTTP forbidden: rejected for this user-approved, explicit feature; the exception is limited to the fixed Jev endpoint and this command.

## Traceability

- Requirements: [RF-WI.5](../srs.md), [RNF-SEC.2](../srs.md).
- Architecture: [docs/architecture.md](../architecture.md).
- Product scope: [docs/product/vision.md](../product/vision.md).
