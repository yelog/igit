# Direct AI Commit Message Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Generate and fill the shared commit message in one action, with animated progress and an error dialog on failure.

**Architecture:** Keep AI configuration loaded at application startup and retain the existing OpenAI-compatible worker and selected-patch builder. Replace the send-review and candidate-adoption dialogs with a direct request whose successful response is saved as the editable shared draft. Keep Git writes behind their existing reviews and preserve drafts on errors or concurrent edits.

**Tech Stack:** Rust, Ratatui, Crossterm, standard threads and mpsc, curl, disposable Git fixtures, local mock HTTP server.

---

### Task 1: Cover direct generation and failure behavior

**Files:**
- Modify: `src/commit/ai/tests.rs`
- Modify: `src/shell/configuration_tests.rs`
- Modify: `src/commit/view_tests.rs`

1. Replace review/adoption assertions with one-action request, loading, automatic draft persistence, and continued editing assertions.
2. Verify scoped staged patches exclude hidden repositories, unstaged changes, and unchecked paths.
3. Verify duplicate clicks, missing configuration, service errors, save errors, and messages edited during generation do not overwrite the existing draft.
4. Check animated loading and dismissible error rendering using TestBackend and the local mock server; never call a real model or send users' patches.
5. Run `cargo test --locked commit::ai::tests --lib` and confirm the changed expectations fail before implementation.

### Task 2: Implement the direct request lifecycle

**Files:**
- Modify: `src/commit/ai.rs`
- Modify: `src/commit.rs`
- Modify: `src/commit/view.rs`
- Modify: `src/commit/keymap.rs`
- Modify: `src/shell.rs`

1. Extract request preparation from review construction, preserving selection and patch limits.
2. Start the existing worker directly from Generate message / A, and keep one pending request with the original draft snapshot.
3. Animate the Generate button while pending and continue normal browsing.
4. Persist successful output directly to the shared message; preserve the previous draft if persistence fails or it changed while waiting.
5. Replace the candidate dialog with a read-only, dismissible AI error dialog. Keep S as the explicit settings entry point.

### Task 3: Document and verify the final behavior

**Files:**
- Modify: `README.md`
- Modify: `docs/configuration.md`
- Modify: `docs/design.md`
- Modify: `docs/prd.md`
- Modify: `docs/architecture.md`

1. Document direct generation, startup-only configuration loading, animated progress, and failure preservation.
2. Run `cargo test --locked commit:: --lib` and affected Shell configuration tests.
3. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
4. Build the current source and validate the existing local settings with `--check-config`. Do not modify the release installation or run commit/push.

## Validation status

- `git diff --check` passed.
- Focused AI tests, formatting, and Clippy were attempted but produced no output before startup timeouts. Both `cargo --version` and `rustc --version` also timed out; the installed IGit and existing debug binary could not start either. These checks are not verified, and the release installation was not replaced.
- Re-run the checks above after resolving the local executable-startup problem. Tests use only disposable repositories and mock endpoints.
