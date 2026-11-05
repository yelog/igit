# Commit / Git Log Split Workspace and Operation Dialogs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Place Commit above Git Log, each taking half of the workspace. Open Push and Merge as modal windows and remove global panel tabs.

**Architecture:** Shell owns split focus, the current operation window, and the previous frame's hit regions. The two persistent panels independently retain selection and scrolling; help and write review cover the current window. Reuse the existing Git plans, confirmation, and background jobs.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, temporary Git workspaces.

---

1. In `src/shell/workspace_tests.rs`, verify that both panels contain real data in the same frame, split the height equally (an odd height may differ by one row), receive correct focus from mouse and number keys, and isolate underlying events while operation dialogs are open. Esc must return to the previous location. First run `cargo test --locked split_workspace` to confirm the old layout cannot meet these requirements.
2. Separate workspace focus from operation windows in `src/shell.rs` and `src/shell/workspace.rs`. Adjust drawing order, event routing, and refresh timing. Preserve Log's own filter tabs. Editing, help, and confirmation receive input first; Esc returns from the innermost layer outward.
3. Adapt available height and overlay hit testing in `src/commit/view.rs`, `src/commit.rs`, `src/log.rs`, and related panels. At 44×12, 80×24, and 120×36, check lists, filters, editors, and dialogs in short windows.
4. Update `src/ui/help.rs`, `README.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md`. Run targeted Shell tests, full `cargo test --locked`, fmt, and clippy. Recheck layout and dialog return with actual terminal input in a temporary multi-repository workspace.
