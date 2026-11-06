# Workspace Defaults and Tiled Log Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Open Commit by default, map `1`–`4` in workspace tab order, and keep the Log panes tiled.

**Architecture:** Shell uses the header tab order for numeric navigation and applies local pane maximization only to tabs that support it. Log lays out its enabled panes with the existing split model at every supported size; filter tabs retain their own state.

**Tech Stack:** Rust, Crossterm, Ratatui, and system Git.

---

### Task 1: Align startup, navigation, and help

- In `src/shell/workspace_tests.rs`, exercise startup on Commit and navigation with the numeric keys.
- In `src/shell.rs`, initialize both active and return tabs to Commit and use the display order for numeric navigation. Align `src/config/keys.rs`, `config/default.toml`, and `src/ui/help.rs` with that order.

### Task 2: Keep Log tiled

- In `src/log/view_tests/panes.rs` and `src/shell/maximize_tests.rs`, verify that all enabled Log panes are visible on narrow screens and after switching from a maximized tab.
- In `src/log/view.rs`, use the existing split layout at all sizes, with smaller sidebar minimums on narrow screens. Remove obsolete pane-tab hit targets from `src/log.rs`. In `src/shell/panes.rs` and `src/shell/workspace.rs`, keep Log resizing available when other tabs are maximized.

### Task 3: Verify and document

- Update `README.md`, `AGENTS.md`, `docs/design.md`, `docs/configuration.md`, and `docs/prd.md` with the new defaults and Log behavior.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
