# Commit Refresh and Composer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Open Commit with its preview enabled, add backward word/line deletion, simplify composer actions, and automatically show external repository changes.

**Architecture:** Keep Commit's existing tree, checked-file semantics, inline drafts, and reviewed Git plans. Poll repository status and the current file patch on one background worker; apply only current results without replacing editor state or review snapshots. Keep manual workspace refresh available.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, standard threads and mpsc; no new dependencies.

---

### Task 1: Preview and composer

- Modify `src/commit.rs`, `src/commit/view.rs`, and `src/commit/keymap.rs`.
- Enable preview in the panel default. Preserve tree-first navigation on narrow terminals.
- Implement Ctrl-W as deletion of preceding whitespace and the preceding whitespace-delimited word; Ctrl-U deletes to the current logical line's start. A selected range is deleted first; preserve Unicode boundaries and text after the caret.
- Keep only Commit, Commit and Push..., and Generate message in the composer action area, wrapping when space permits. Retain keyboard save/cancel, Amend, and AI settings.
- Update layout/input regressions under `src/commit/view_tests/` and `src/shell/workspace_tests/` for the new defaults and button labels.
- Run `cargo test --locked commit:: --lib`; expected: all Commit tests pass.

### Task 2: Automatic refresh

- Add `src/commit/refresh.rs` and focused refresh tests, and integrate polling in `src/shell.rs`.
- The original one-second scan is superseded by [Commit file watching](2026-11-06-commit-file-watching.md): native events, 300ms debounce, affected-root scans, and 30-second reconciliation.
- Fetch status without index locks and refresh the current Diff even if XY status stays unchanged. Preserve checked paths, folds, cursor identity, draft/caret, and unchanged preview selection/scroll. Retain failed repositories' last successful state and deduplicate errors.
- Prevent overlapping workers; invalidate stale results after explicit refresh or display-option changes. Pause applying results during reviews, menus, and drags.
- Verify additions, edits, removals, staging changes, late results, draft ownership, and review/index isolation with disposable repositories.
- Run `cargo test --locked --lib`; expected: all cross-panel regressions pass.

### Task 3: Documentation and validation

- Update `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, and the handoff in `AGENTS.md` to describe the delivered behavior.
- Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Inspect wide/narrow rendered layouts and verify mixed staged/unstaged content plus unrelated staged files remain unchanged until an explicit reviewed write.
- Leave changes uncommitted as required by the workspace instructions.
