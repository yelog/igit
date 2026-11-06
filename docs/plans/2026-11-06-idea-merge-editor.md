# IDEA Merge Editor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Recreate the reference merge window in the terminal, including the side gutters, aligned context folds, and resolution controls.

**Architecture:** Keep the Git stages, content checks, and write review in `src/merge/engine.rs`. Track decisions for each side and build a shared row layout from the three texts in `src/merge/view.rs`. Shell gives an open merge document the terminal surface and continues to own Help and write reviews.

**Tech Stack:** Rust, Ratatui, Crossterm, similar, system Git.

---

### Task 1: Track each source decision

- Extend `src/merge/engine.rs` so accepting a side leaves the other conflict side pending, ignoring a side keeps the current result, and matching edits resolve together. Keep whole-file and per-block resolution distinct.
- In `src/merge.rs`, include side decisions in undo and session drafts. Route Apply to the existing scope, index, and file guards. Test both decision orders, safe changes from one side, and undo across a mixed batch.

### Task 2: Render and operate the merge window

- Add `src/merge/view.rs` for the title, toolbar, source headers, footer, gutters, and aligned rows. Preserve real line numbers across folds and gaps. Render line and character differences with the reference colors.
- Place edits directly in Result. Route gutter actions, context folds, scrolling, details, and settings through their rendered cells. Keep single-pane navigation at narrow widths.
- In `src/shell/workspace.rs`, show the document without workspace chrome and draw Help, key hints, and write reviews above it.

### Task 3: Verify with real conflict stages

- Use Ratatui's test backend to check hit areas, layout, folding, and narrow widths. Use a temporary Git merge to verify that previewing, ignoring, editing, and saving a draft preserve the working tree and index until Apply is confirmed. Check that Continue remains separate.
- Update the Conflicts documentation and run `cargo test --locked merge::`, `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
