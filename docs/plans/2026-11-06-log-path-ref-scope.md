# Log Paths Reference Scope Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Limit the Log reference tree to repositories included by the applied Paths selection, matching the repository scope of history.

**Architecture:** Derive the reference scope from global Roots, tab-local Roots, and `PathFilter::for_root`. Keep all loaded refs cached, but aggregate tree rows, owners, pins, and tracking counts only within that scope. Rebuild the tree when Roots changes in memory; existing reload and tab restoration handle applied file/directory filters.

**Tech Stack:** Rust, Ratatui TestBackend, Crossterm events, temporary Git repositories.

---

### Task 1: Reproduce repository leakage

**Files:** `src/log/refs.rs`, `src/log/view_tests/path_selection.rs`.

1. Add a reference-tree regression for intersecting global Roots, tab Roots, and qualified paths. Check local branches, remote branches, tags, shared ownership, pins, tracking counts, and reference search.
2. Exercise actual Paths checkbox, tree, and text input events against temporary repositories with distinct refs. Check draft cancellation, Apply, clearing, empty selection, and independent Log tabs.
3. Run `cargo test --locked paths_ref_tree` and confirm the new assertions fail against the current implementation.

### Task 2: Apply the scope during tree reconstruction

**Files:** `src/log/refs.rs`, `src/log/paths.rs`.

1. Pass the active Filter into reference reconstruction and compute eligible repository IDs once per rebuild using the existing path resolution semantics.
2. Filter refs before aggregating identical names so repository annotations and tracking metadata match the selected scope.
3. Pass Context to the cached Roots refilter path and rebuild refs after each Roots change, including quick selection and clearing.
4. Run the new regressions, then `cargo test --locked log::`.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Describe how applied Paths affects the left reference tree, including unqualified paths and restoration after clearing.
2. Add this behavior to A8 acceptance and document cached ref reconstruction.
3. Run `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`; inspect the final diff for unrelated changes.

No commit, push, or writes to user repositories are part of this task.

### Validation record (2026-11-06)

- The initial regression run reproduced references from excluded repositories after both repository selection and scope intersection.
- `cargo test --locked log::` passed all 83 Log tests in the working tree, including three new regressions using reference metadata and rendered mouse/keyboard interactions.
- `cargo fmt --all --check` and `git diff --check` passed.
- An isolated copy of HEAD plus this task's three Rust files passed the same 83 Log tests and `cargo clippy --locked --all-targets -- -D warnings`.
- The shared working tree's Clippy run was blocked by concurrent changes outside this task: unused `Settings` / `Ai` variants in `src/ui/icons.rs` and a collapsible conditional in `src/commit/refresh.rs`. Those files were left to their ongoing task.
