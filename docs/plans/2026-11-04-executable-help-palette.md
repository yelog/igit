# Executable Help Palette Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Match lazydb's help style and interaction precisely: typing by default, up/down selection, and Enter to execute.

**Architecture:** Replace help with a fixed Search input and compact candidate list, using lazydb's 74-column width, rounded border, dark-blue background, cyan title, blue keys, and `>` selection marker. Split multi-action summaries into separate static key sequences. List only region, shared panel, and global actions available in the current context. Shell restores the original state and executes through existing event paths, preserving Git review and validation.

**Tech Stack:** Existing Rust / Ratatui / Crossterm / unicode-width; no new dependencies.

---

### Task 1: Input and candidate interaction

**Files:** `src/ui/help.rs`, `src/shell/help_tests.rs`.

1. First test the default input cursor, Chinese text editing, arrow selection, reset after filtering, Enter with no results, the visible viewport, and mouse selection.
2. Replace grouped body text and browse mode. Match lazydb's rendering dimensions, colors, borders, column widths, and footer.
3. Run `cargo test --locked ui::help::`.

### Task 2: Executable shortcuts

**Files:** `src/{log,commit,push,merge}/keymap.rs`, `src/log/branches.rs`, `src/shell.rs`.

1. Give each row one explicit action and verifiable execution data for multi-key sequences and modifier keys. As in lazydb, show only actions available in the current context.
2. Enter restores the previous dialog/editing state, validates context, and dispatches the original shortcut.
3. Test real navigation, search, panel switching, multi-key sequences, editing actions, and commit review. Verify that cancellation and search alone leave HEAD, index, and worktree unchanged.

### Task 3: Documentation and validation

**Files:** `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`.

1. Update Search, selection/execution, and style documentation, replacing the previous read-only browsing description.
2. Inspect actual rendering at 120×36, 80×24, and 44×12.
3. Run `cargo fmt --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`.

Execute directly; do not commit, push, or change lazydb.

## Implementation and acceptance record

- Completed lazydb-style default Search focus, compact single-line candidates, up/down selection, Enter execution, input editing, mouse selection, and scrolling. Deduplicate current-region, shared, and global actions by priority.
- All 19 help tests passed, including real key-sequence execution, original input/dialog restoration, stale-context validation, and commit review boundaries. Search text never becomes execution data; Git writes still follow the original review path.
- Full `cargo test --locked`: 146 unit tests and 3 integration tests passed. `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and documentation relative-link checks passed.
- Inspected actual TestBackend rendering at 120×36, 80×24, and 44×12, including the default input cursor and filtered candidate markers. Temporary export tests were removed.
- Integrated the concurrently added Commit View Options help context and adjusted its mouse match branch to pass Clippy. Preserved interception of other mouse buttons within the menu; the corresponding targeted cases passed.
