# Shared Commit Message and AI Implementation Plan

> Implement each task in this workspace, preserving the existing uncommitted changes. Do not commit or publish.

**Goal:** Use one editable message for all checked repositories, show Generate message as an icon button, and support user-configured LLM generation through a complete review flow.

**Architecture:** Keep Git plans and execution isolated per repository. Store a workspace message, preserving legacy drafts during migration. Load optional AI configuration from application settings, combine reviewed selected patches into one background request, and require explicit adoption of the generated candidate.

**Tech Stack:** Rust, Ratatui, Crossterm, serde/TOML, system Git and curl; local HTTP fixtures for AI tests.

### Task 1: Shared message and composer

Files: `src/commit.rs`, `src/commit/view.rs`, `src/commit/refresh.rs`, `src/commit/keymap.rs`, Commit and Shell tests.

1. Replace repository-specific message reads/writes with one saved message; preserve old drafts for recovery and seed the message on first load.
2. Remove repository tabs from the composer. Keep repository navigation in Changes and make Amend target a single highlighted repository.
3. Clone the shared message into each repository plan. Retain it after partial failure and leave successful paths unchecked on retry.
4. Render Generate message with `Icon::AutoResolve` and the existing secondary button style, including compact layouts.
5. Verify editing/persistence, multi-repository writes, failed retries, hidden scope, staged/unstaged content, and button hit areas with `cargo test --locked commit::`.

### Task 2: Configurable AI request and candidate review

Files: `src/config.rs`, `config/default.toml`, `src/shell.rs`, new `src/commit/ai.rs` and tests.

1. Add `[ai]` endpoint, model, prompt, credential environment variable and request timeout, with schema validation and documented precedence over legacy workspace settings.
2. Extract AI request handling from the panel. Snapshot the destination, prompt, model and selected patches at review; send one request only after confirmation.
3. Present one candidate for explicit adoption, then allow inline editing. Preserve the existing draft on cancellation, request failure or persistence failure.
4. Exercise the real curl transport against a local mock endpoint: payload scope, custom prompt/model, success, malformed/empty/truncated responses, HTTP errors and credential redaction. Do not contact real model services.

### Task 3: Documentation and verification

Files: `README.md`, `docs/configuration.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, `AGENTS.md`.

1. Update shared-message behavior, configuration examples, candidate adoption, migration and Amend/navigation semantics.
2. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Review the final delta for unrelated edits and report implemented behavior, test results and any unverified interactive behavior.
