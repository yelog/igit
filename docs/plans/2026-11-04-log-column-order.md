# Git Log Column Order Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show, hide, and move Author, Hash, Date, GPG Signature, Graph, and Message in the Log Columns submenu using either keyboard or mouse.

**Architecture:** Session display options retain the complete order, including hidden columns. Menu focus follows column identity during moves. The repository marker remains fixed at the start of each row. All six columns share one layout: calculate metadata and graph widths first, give the remaining width to Message, then draw in menu order. Graph owns graph-jump hit targets; Message owns titles and references. Moving and toggling these columns changes only display state.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, temporary Git repositories.

---

### Task 1: Define acceptance criteria through real input and rendering

- In `src/log/view_tests/column_order.rs`, move visible and hidden columns through Columns. Compare text positions in menu and commit rows, and check highlighting, checkboxes, boundaries, and the current commit after moving. Register cases in `src/log/view_tests.rs`.
- Test the different effects of clicking a row-end button and a column name. Recheck order on narrow screens, short windows, and after switching Log tabs. First run `cargo test --locked view_options_reorder` and confirm that the existing fixed layout fails.

### Task 2: Connect session preferences, menus, and log rows

- In `src/log/display.rs`, use a fixed-length ordered column configuration to hold both identity and visibility. Adjacent swaps stop at the ends. The menu handles hover, click, checkbox toggles, and moves by column identity.
- In Columns, `j/k` and arrow keys only browse; `K/J`, `Shift+Up/Down`, and the up/down buttons on each row move a column earlier/later. Mouse and keyboard operations use the same swap without calling Git or reloading history.
- In `src/log/view.rs`, calculate each column's width using the existing rules, then draw in preference order. Show the corresponding actions in `src/log/keymap.rs` and the menu footer.
- Recheck actual log rows with Graph and Message first, in the middle, last, and hidden: titles and references must not overlap later columns; the old graph location must no longer trigger jumps; hiding the graph removes arrow hit targets.

### Task 3: Documentation and validation

- Update usage, defaults, session scope, and acceptance criteria in `README.md`, `docs/design.md`, and `docs/prd.md`.
- Run `cargo test --locked view_options` for column reordering and existing menu behavior, followed by `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
