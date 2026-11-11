# Log Pane Focus Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Enter in the ordinary Log commit list focus its Diff file tree, and keep normal Escape presses within the focused Log pane.

**Architecture:** Update the existing Log key dispatch without changing file-comparison overlays, File History, or Branch comparison controls. Reuse pane visibility and focus helpers, preserve pending preview reads, and remove the obsolete Escape help entry.

**Tech Stack:** Rust, Ratatui, Crossterm, existing disposable Git fixtures and terminal test backend.

---

### Task 1: Cover keyboard focus behavior

**Files:** `src/log/view_tests/panes.rs`, `src/log/view_tests.rs`, `src/log.rs`, `src/shell/log_tests.rs`, `src/shell/help_tests.rs`.

1. Add Enter coverage at 120×36, 80×24, and 44×12, including a hidden inspector.
2. Assert that Escape keeps Branches, Diff files, and Details focused and preserves the selected commit and file.
3. Exercise the same keys through Shell with mixed staged and unstaged content.
4. Run `python3 scripts/cargo_serial.py test --locked --lib log::view_tests::panes::` and confirm that the new behavior fails before implementation.

### Task 2: Update dispatch and contextual help

**Files:** `src/log.rs`, `src/log/keymap.rs`, `src/log/view_tests.rs`.

1. For Enter from the ordinary commit list, reveal the inspector, focus its file tree, and expedite its pending preview without opening a comparison.
2. Remove the normal Escape-to-commit-list fallback; preserve search cancellation, selection clearing, and comparison closing.
3. Update the ordinary Log help description and remove the obsolete general Escape entry.
4. Update existing keyboard inspection tests to enter the file tree before opening a file comparison.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`.

1. Document the commit-list-to-file-tree keyboard path and pane-local Escape behavior.
2. Run focused Log tests, Shell Log tests, help tests, formatting checks, and `git diff --check` through the required serial Cargo wrapper.
3. Preserve all existing workspace changes. Do not commit, push, or operate on user repositories.

## Validation — 2026-11-11

- `python3 scripts/cargo_serial.py test --locked --lib log::view_tests::panes::`: 5 passed.
- `python3 scripts/cargo_serial.py test --locked --lib log::`: 196 passed.
- `python3 scripts/cargo_serial.py test --locked --lib shell::log_tests`: 3 passed.
- `python3 scripts/cargo_serial.py test --locked --lib shell::help_tests`: 18 passed.
- `python3 scripts/cargo_serial.py check --locked --lib`: passed.
- `python3 scripts/cargo_serial.py fmt --all --check`: passed after correcting the new test's formatting.
- `git diff --check`: passed.
- `python3 scripts/cargo_serial.py clippy --locked --lib -- -D warnings`: blocked by `clippy::obfuscated_if_else` in the separate notification-prefix changes in `src/shell.rs`.
- Verification used disposable Git repositories and the terminal test backend at 120×36, 80×24, and 44×12. HEAD, index bytes, staged and unstaged content remained unchanged. An interactive terminal walkthrough and the full repository test suite were not run.
