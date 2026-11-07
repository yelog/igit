# Branch Repository Panes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show one branch tree at a time, with a repository selector only when Roots includes multiple repositories.

**Architecture:** Project branch rows and upstream columns from the selected repository's cached snapshot. Keep stable row selections and fold overrides, and reuse the shell's pane focus and divider controls for the two lists. Ref loading stays in the background; review and execution still identify one repository and one source ref.

**Tech Stack:** Rust, Ratatui, Crossterm, Git, and TestBackend.

---

### Task 1: Separate repository selection from branch browsing

**Files:** `src/branch.rs`, `src/branch/tree.rs`, `src/branch/loading.rs`, `src/branch/model.rs`, `src/branch/rows.rs`, `src/branch/tests.rs`, `src/shell/branch_tests.rs`.

1. Add a shell regression with two disposable repositories and distinct branch names. Run `cargo test --locked branch_layout_tracks_roots` and verify the old tree shows branches from both repositories.
2. Build branch rows only for the selected repository. Preserve branch selection and folder state by repository and ref. Install background results for the latest selected repository, even if the selection changes while a refresh is pending.
3. Verify that changing Roots hides the old data immediately and that a confirmed action on a branch in the second repository updates only that repository.

### Task 2: Render and navigate the panes

**Files:** `src/branch/view.rs`, `src/branch/navigation.rs`, `src/branch/keymap.rs`, `src/branch/tests/`, `src/ui/panes.rs`, `src/shell/panes.rs`, `src/config/keys.rs`.

1. Use the full width for a single repository. With multiple repositories, put the repository selector on the left and the selected branch tree on the right. Give each list its own scrolling and use the existing pane commands for focus and resizing.
2. Route mouse clicks, scrolling, and keyboard motions to the appropriate list. Branch actions require focus on a branch row. Keep the current marker, group counts, and upstream details at narrow widths.
3. Run `cargo test --locked branch`. Verify both panes at 120×36, 80×25, and 44×12, and cover refresh while switching repositories.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Update scope, layout, shortcuts, and the background read boundary.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`. Inspect the terminal layout using disposable repositories.
