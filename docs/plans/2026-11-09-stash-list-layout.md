# Stash List Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make saved stashes easy to scan and identify in IGit, including at 44×12.

**Architecture:** Represent the empty repository disclosure as a list row alongside the existing repository and stash rows. Build the Diff context from the current selection and leave the Log inspector, preview worker, and reviewed Git operations in place.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, and TestBackend.

---

### Task 1: Pin down the list and Diff behavior

**Files:** `src/stash/tests.rs`. Existing Shell coverage: `src/shell/stash_tests.rs`.

1. Extend the disposable repository fixture checks: a stash message, short selector, and date share a row; the empty repository name appears only after opening the summary; mouse and keyboard can fold it. Confirm the summary and its children cannot start writes.
2. Check that Diff shows the selected repository and selector before the worker completes, after selector renumbering, and at 44×12 with a Unicode repository name. Keep the existing mouse Diff and remapped key checks.
3. Run `cargo test --locked --lib stash::tests::`; the new presentation assertions should fail against the old layout.

### Task 2: Render and navigate the compact list

**Files:** `src/stash.rs`, `src/stash/model.rs`, `src/stash/view.rs`.

1. Emit populated groups first, read errors next, and one folded summary for empty repositories. Handle the summary through the existing fold keys and row click. When the last selected stash disappears, return to its empty repository or the summary without selecting a stash in another repository.
2. Render one line per row, with the description on the left and optional aligned selector and date on the right. Clip by terminal cell width and preserve the original subject and date for the preview and reviews.
3. Reserve a context row above the existing Diff inspector. At narrow widths, give the repository and selector separate lines when needed. Leave space for the existing toolbar and file rows; attach a repository read error to its own row and show its details when selected.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document the compact layout, empty group, and Diff identity. Run `cargo test --locked stash`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
2. Check 120×36, 80×25, and 44×12, including mouse and keyboard navigation, Diff access, and the existing reviewed actions.

### Validation results

- The new layout assertions failed against the original screen. After the
  changes, `cargo test --locked stash` passed all 58 matching tests.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  and `cargo build --locked --bins` passed.
- With disposable Git repositories, `igit` at 120×36 and `ig` at 44×12 showed
  the compact list and opened the selected stash's read-only Diff. At 44×12,
  Enter opened the empty group and Tab switched from saved to original content
  in the comparison.
