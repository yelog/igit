# Contextual Help Implementation Plan

> Initial version; see [Executable Help Palette](2026-11-04-executable-help-palette.md) for the current help style and selection/execution rules.

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Following lazydb, turn IGit's help into a searchable, scrollable shortcut list ordered by current focus.

**Architecture:** Each panel supplies structured help for its current region, shared panel actions, and other regions through an internal `keymap` method. Shell captures the context when opening help and uses separate rendering and event handling in `ui/help.rs`, without changing the public `Panel` trait. Help only browses and does not execute listed commands. When opened with F1 from an input field or Shell dialog, closing it restores the previous state.

**Tech Stack:** Rust, Ratatui 0.29, Crossterm 0.28, existing unicode-width; no new dependencies.

---

### Task 1: Help model and list interface

**Files:** Create `src/ui/help.rs`; modify `src/ui.rs`.

1. Define shortcut groups, context, and help sessions. Retain `Panel::help` for the footer summary.
2. Implement current-region priority, search by Chinese text/key/group, empty-result hints, j/k/arrows/page/start/end navigation, and mouse scrolling.
3. Place shortcuts and descriptions in separate columns and wrap on narrow screens. Show current context, match count, scroll position, and a close control.
4. Run `cargo test --locked ui::help::`, covering filtering, long content, narrow screens, resizing, and mouse hit testing.

### Task 2: Actual focus and help entry points

**Files:** Create `src/log/keymap.rs`, `src/commit/keymap.rs`, `src/push/keymap.rs`, `src/merge/keymap.rs`; modify corresponding panel module declarations and `src/shell.rs`.

1. Register region shortcuts for all four screens against their event branches, including Diff, filters, message input, and merge result editing.
2. Keep `?` as the navigation-mode entry point; allow `F1` from inputs and dialogs. Clicking header help preserves current focus.
3. Shell saves the covered dialog. Closing help restores the dialog, drafts, and focus; help input does not reach the original panel.
4. Use Shell and panel tests for context order, typed-character isolation, and confirmation restoration, ensuring help cannot trigger Git writes.

### Task 3: Documentation and complete validation

**Files:** Modify `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`.

1. Update entry points, search/scroll actions, and context rules, with reproducible acceptance steps.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Use TestBackend and local temporary demo repositories to check rendering and read-only behavior at 120×36, 80×24, and 44×12.

Execute these steps directly as requested; do not create commits, push, or change lazydb.

## Implementation and acceptance record

- Completed regional help, search, keyboard and mouse scrolling, scrollbar clicks/dragging, narrow-screen wrapping, and F1 overlay restoration. The public `Panel` trait and dependencies are unchanged.
- Added 12 help regressions covering focus in all four screens, mouse access, draft/review restoration, input cursors, real merge editing, and read-only HEAD, index, and worktree behavior.
- `cargo test --locked`: all 133 unit tests and 3 integration tests passed. `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` passed.
- Inspected actual TestBackend rendering at 120×36, 80×24, and 44×12. These renders validate the UI and do not represent manual pseudoterminal walkthroughs.
