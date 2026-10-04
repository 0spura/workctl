# Project: workctl

## Stack

- **Runtime:** native Rust binary, built with the stable toolchain.
- **CLI:** `clap` derive API; 4.6.7 was the latest release checked on 2026-10-01.
- **Serialization:** `serde` and `serde_json` for strict config and provider payloads.
- **External services:** `gh` currently owns GitHub operations; `reqwest` backs the provider-neutral `DecisionModel` package and its explicit native/local model adapters.
- **Model credentials:** hosted adapters read `DECISION_MODEL_API_KEY` only when `@auto` selects one; local endpoints are loopback-only and require no hosted key.
- **Subprocesses:** `std::process::Command` behind one process runner; `wait-timeout` enforces the child deadline.
- **Tests:** Rust unit and CLI integration tests use isolated HTTP/code-host fixtures; no live code-host mutation or hosted inference is part of tests.

## Global constraints

- Model adapters accept provider-neutral work-item text and candidate labels; the package does not depend on GitHub or GitLab SDK types.
- No hosted generic LLM adapter; generic local LLMs require an OpenAI-compatible Chat Completions service. Native decision adapters retain their own protocols.
- No shell invocation; untrusted values remain argument/stdin/JSON data.
- User errors never include raw provider stderr, credentials, stack traces, or internal paths.
- The current CLI implements GitHub issue and pull-request workflows. Model package reuse by future GitLab/other provider integrations does not claim those providers are implemented.
- Modules are split by ownership and reason to change; no omnibus `main.rs`, command, provider, or output module.

## Repository layout

```text
src/              Rust binary modules
tests/            CLI/provider integration tests
docs/             vision, SRS, architecture, ADRs
Cargo.toml        application manifest
Cargo.lock        resolved application dependency versions
rust-toolchain.toml stable Rust channel
target/           local build output; not committed
```

## Environments and verification

- Runtime: developer machine with `git`, `gh`, and GitHub authentication configured.
- `cargo test`: unit and isolated CLI tests.
- `cargo build --release`: optimized local-target binary at `target/release/workctl`.
