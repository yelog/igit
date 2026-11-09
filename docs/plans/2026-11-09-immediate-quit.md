# Immediate Quit Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Quit IGit without a confirmation dialog from normal-mode shortcuts and Actions.

**Architecture:** Keep one guarded Shell exit method for built-in keys, configured bindings, help, and Actions. Preserve software-install and running-Git guards, selection-copy priority, editor input, and overlay cancellation.

**Tech Stack:** Rust, Crossterm, Ratatui, existing disposable-repository tests.

---

### Task 1: Change the shared exit path

**Files:** `src/shell.rs`, `src/shell/workspace.rs`, `src/ui/help.rs`.

1. Rename `quit_box` to `request_quit`; keep both busy checks and replace popup construction with `self.quit = true`.
2. Remove the obsolete `__quit` confirmation token branch.
3. Use `Quit IGit` in contextual help and remove `y Confirm` from the terminal-too-small hint.

### Task 2: Update behavioral coverage

**Files:** `src/shell/test_support.rs`, `src/shell/help_tests.rs`, `src/shell/configuration_tests.rs`, `src/shell/maximize_tests.rs`, `src/shell/software_update/tests.rs`, `src/shell/startup_tests.rs`, `src/shell/workspace_tests.rs`, `src/shell/workspace_tests/refresh.rs`, `src/ui/help.rs`.

1. Replace exit-confirmation fixtures in overlay tests with a generic reviewed action.
2. Assert immediate exit for built-in keys, Actions, and a configured sequence; preserve copy-before-exit coverage.
3. Keep loading, automatic-refresh, running-Git, and software-install checks meaningful without continuing an exited session.
4. Run `cargo test --locked shell::` and `cargo test --locked ui::help::`.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/configuration.md`, `docs/architecture.md`.

1. Document immediate normal-mode exit and retain context-owned input and busy guards.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Build both binaries and smoke-test actual PTY exit in a temporary repository; compare HEAD, real index, and working state.
4. Inspect the final diff. Do not commit or publish.

## Verification

- `cargo fmt --all --check` passed.
- `cargo test --locked` passed: 665 unit tests and 10 integration tests; one existing manual performance test remained ignored.
- `cargo clippy --locked --all-targets -- -D warnings` and `cargo build --locked --bins` passed.
- Real PTY checks passed for `igit` with `q` at 120×36, `ig` with Ctrl-C at 80×24, Actions → Quit by mouse at 44×12, a configured `Q Q` sequence at 120×36, and `ig` with `q` below the minimum size at 32×10.
- Every PTY case exited with status zero after one exit action, showed no confirmation, restored terminal modes, and preserved HEAD, real index bytes, staged/unstaged diffs, and working files.
