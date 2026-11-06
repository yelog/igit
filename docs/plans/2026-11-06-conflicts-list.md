# Conflicts List Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the Conflicts tab show the current operation, grouped files, and each Git side's change, with direct access to resolution previews.

**Architecture:** `src/merge/list.rs` owns list selection, display, and click targets; `src/merge.rs` routes them to the existing three-way editor and review actions. `src/merge/engine.rs` remains the write boundary. List actions create session drafts or open previews; Apply file and Continue remain separate confirmations.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git.

---

### Task 1: Render operation context and grouped files

- In `src/merge/list.rs`, draw the repository and operation above File / Yours / Theirs columns. Show statuses from the unmerged index stages, using the actual Git sides for rebase. Group unresolved files and repositories ready to continue, retaining selection by repository and path across refresh.
- In `src/merge.rs`, load the operation context alongside the scoped conflict list. Keep the same tab availability and three-pane editor.
- Update the `open_and_merge_gutters_match_their_rendered_cells_in_each_mode` test in `src/merge/engine.rs` for the Resolve Manually button. Check 120×36, 80×24, and 44×12 with the test backend.

### Task 2: Connect the list controls

- Add fold, full-path display, and reviewed navigation via the list controls, `src/merge/keymap.rs`, `src/config/keys.rs`, and `config/default.toml`.
- Accept opens a full-file version in the editor; preparing simple changes adds only unambiguous blocks to session drafts in the highlighted repository. Keep manual drafts, stage and working-file guards, and the separate Continue review.
- Test real merge stages, preservation of external edits and unrelated staging, and the ready-to-continue state. Run `cargo test --locked merge::` and `cargo test --locked conflicts_tab`.

### Task 3: Document and verify

- Update `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md`. Run `cargo fmt --all --check`, `cargo test --locked`, and Clippy. Try the Conflicts tab in the local practice workspace.
