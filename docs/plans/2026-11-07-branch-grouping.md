# Branch Grouping Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give Branch a clearer hierarchy and a menu for switching between prefix folders and full branch names.

**Architecture:** Keep Local and Remote sections in both layouts. Read the cached remote HEAD aliases in the existing ref scan to determine which path to open initially. Store grouping and fold choices in the tree; keep them independent of Git reviews.

**Tech Stack:** Rust, Ratatui, Crossterm, Git, and TestBackend.

---

### Task 1: Project grouped and flat branches

**Files:** `src/branch/model.rs`, `src/branch/loading.rs`, `src/branch/tree.rs`, `src/branch/tests.rs`.

1. Add a fixture with different nested current and default refs and an unrelated prefix. Run `cargo test --locked --lib branch_opens_current_and_default_paths`; the existing projection shows the unrelated ref.
2. Resolve the default from the local remote HEAD aliases and open only the paths to current/default refs. Keep manually changed folds through refreshes.
3. Add a flat projection with full names. Keep the selected ref when toggling layouts and honor changes made during a pending refresh.

### Task 2: Render repository icons, folders, and the menu

**Files:** `src/branch.rs`, `src/branch/display.rs`, `src/branch/keymap.rs`, `src/branch/rows.rs`, `src/branch/view.rs`, `src/config/keys.rs`, `config/default.toml`, `src/shell.rs`, `src/shell/configuration.rs`.

1. Use Push's repository tint and submodule glyph. Render disclosure arrows and folder glyphs in separate cells and give branch children more indentation.
2. Put the eye in the branch toolbar's top right. Route Space/Enter and mouse toggles to the menu before branch actions. Keep the menu and its hit area usable at 44 columns.
3. Verify folder icons and indentation, stable selection, non-writing behavior, and the configurable `o` binding in the Branch and Shell tests.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, `docs/configuration.md`.

1. Document the menu, fold defaults, and fallback when Git has no remote HEAD alias.
2. Run the Branch tests, format and Clippy checks, and a terminal smoke test using disposable repositories.
