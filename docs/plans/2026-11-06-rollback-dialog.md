# Rollback Dialog Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Open an IDEA-style Rollback Changes dialog with `d` on a Commit changelist, repository, directory, or file.

**Architecture:** Root changelists propose their visible members only. The modal borrows the existing Changes tree projection and row renderer, while keeping its checks, grouping, and focus local. Confirm consumes the original Git snapshots for checked paths; the added-file option chooses whether to keep the working copy after unstaging.

**Tech Stack:** Rust, Ratatui/Crossterm, system Git and the existing private-index rollback backend.

---

### Task 1: Scope and backend

**Files:** `src/commit/discard.rs`, `src/commit/backend/discard.rs`, `src/commit/discard_tests.rs`, `src/shell/discard_tests.rs`.

1. Add a regression that opens `d` on Changes and expects a Rollback Changes tree without a Git write. Run `cargo test --locked discard_changes_group_opens_rollback_dialog` and verify it fails against the old flow.
2. Resolve a group into per-repository exact path sets, including collapsed members but excluding other changelists and hidden roots. Keep physical directory and file scopes intact.
3. Filter the retained snapshots by modal checks before execution. Unstage selected additions, preserving their local bytes unless the user selected deletion. Keep untracked-file deletion explicit in the footer.

### Task 2: Modal and input

**Files:** `src/commit/rollback.rs`, `src/commit.rs`, `src/commit/tree.rs`, `src/commit/rows.rs`, `src/commit/keymap.rs`, `src/shell.rs`, `src/ui.rs`, `src/shell/discard_tests.rs`.

1. Share the Changes tree and row renderer with the modal. Keep disclosure, row focus, and tri-state checkboxes separate.
2. Render the framed, scrollable tree, Diff/grouping/fold controls, selected status counts, added-file option, and Close/Rollback buttons. Disable Rollback with no selection and start on Close.
3. Route modal keys, mouse hit areas, and F1 help through Shell before background panels. Keep Commit checks, grouping, and cursor intact when closing.
4. Verify narrow and normal terminal rendering, keyboard/mouse selection, isolated group scope, safe cancellation, and real Git outcomes with `cargo test --locked discard`.

### Task 3: Documentation and regression

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document the modal, its keys, and the scope and local-copy rules in English.
2. Run `cargo fmt --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Keep existing workspace changes and do not commit.

## Observed Verification

- `cargo test --locked --quiet`: 258 unit and 5 integration tests passed. Rollback coverage includes root changelists, hidden Roots, independent checks, disabled empty selection, mouse hit areas, retained added copies, and preservation of unchecked staging and working-tree changes.
- Formatting, Clippy, and `git diff --check` passed.
- In a disposable repository, the native dialog rendered at 100×36 in Unicode mode and 44×12 in ASCII mode. Canceling preserved HEAD, index entries, status, and working-tree diff. The normal-size terminal capture was inspected visually.

## Follow-up: Full Changes Tree

**Goal:** Display the complete Changes tree in the current Roots and use the `d` target only for initial checks.

1. In `src/commit/discard_tests.rs` and `src/shell/discard_tests.rs`, reproduce the single-file case from the IDEA reference. Expect checked and unchecked siblings, partial parent checks, and a selected count of one. Run the single-file regression against the existing dialog to verify the missing sibling.
2. In `src/commit/discard.rs`, preserve the file/group/repository/directory preselection rules. In `src/commit/rollback.rs` and `src/commit/tree.rs`, render all visible changes and keep eligibility separate from the initial checked subset.
3. In `src/commit/backend/discard.rs`, capture supported files across the visible repositories. Keep unsafe or stale rows read-only with an explanation. Apply only checked snapshots and never recapture them on confirmation.
4. Verify adding and removing selections, hidden Roots, read-only paths, stale snapshots, and preservation of unrelated staging and worktree bytes. Update the README, design, PRD and architecture; run the relevant Cargo checks.

**Follow-up verification:** All 287 unit tests passed, including the single-file preselection, partial parent checkbox, selection changes, and stale sibling snapshot cases. `git diff --check` passed. The host stalled starting subsequent test executables and Clippy, so integration and Clippy results are unavailable for this follow-up.
