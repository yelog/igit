# Workspace Tabs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore right-aligned Commit, Log, and Push tabs and highlight Conflicts when a scoped repository needs attention.

**Architecture:** Shell renders and routes input to a single active tab. Merge's existing scoped list determines whether Conflicts is available, including operations ready for Continue. Pane geometry, Git reviews, and per-panel state stay with their panels.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, and Ratatui TestBackend.

---

### Task 1: Cover the tab lifecycle

In `src/shell/workspace_tests.rs`, use a temporary Git merge to check that the header hides Conflicts before the merge, shows it after workspace refresh, and hides it after the separate Apply and Continue reviews. Run `cargo test --locked conflicts_tab` before and after implementation.

### Task 2: Restore workspace navigation

- Replace the split/dialog selection in `src/shell.rs` with the active tab and keep the existing `1`–`4` key bindings. Update Actions and contextual help to use the same availability rule.
- Render right-aligned tabs and a single full-width panel in `src/shell/workspace.rs`. Keep the separate Log filter strip and route pane navigation, resizing, and maximization within the active tab in `src/shell/panes.rs`.
- Refresh Merge's scoped work list on startup, Roots changes, and workspace refreshes. Keep Conflicts available until the last pending Git operation completes; return to the most recent Log or Commit tab when it disappears.

### Task 3: Verify workflows and update docs

Update Shell tests for tab clicks, resize, maximization, and inline Commit message persistence. Document the controls in `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md`. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
