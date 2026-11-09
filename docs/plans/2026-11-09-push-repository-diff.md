# Push Repository Diff Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Open changed files in Push with the same Before/After comparison as Log and label the primary footer action Push.

**Architecture:** Share the committed blob reader and the read-only revision viewer with Log. Push keeps its existing outgoing file tree and background read queue. A file comparison takes over the workspace until closed, with Details still available for summaries.

**Tech Stack:** Rust, system Git, Ratatui TestBackend, Crossterm.

---

### Task 1: Compare the reviewed revisions

**Files:** `src/core/revision.rs`, `src/log/model/merge_diff.rs`, `src/log/model.rs`, `src/push/preview.rs`, `src/push/loading.rs`.

1. Extend `push_preview_reads_real_statuses_and_opens_exact_files_without_writes` to check the Before and After text of modified, renamed, added, and deleted files while one compared file has staged and unstaged changes.
2. Move Log's literal blob reader and bounded patch reader into Core. Use `old_path = file.previous.as_deref().unwrap_or(&file.path)` for the Before side. Read the last fetched target and local tip for a repository row, or the first parent and selected SHA for a commit row. Use an empty Before side for a new target or root commit.
3. Build `RevisionDiff::two_way` on the existing Push preview worker. Return a bounded Git patch with the reason when the split view is unavailable. Keep the read generation checks on close, navigation, and refresh.

### Task 2: Open the viewer and simplify the footer

**Files:** `src/push.rs`, `src/push/diff.rs`, `src/push/view.rs`, `src/push/keymap.rs`, `src/push/view_tests.rs`.

1. Route a file click or Enter to the viewer and send its input to `RevisionDiff::event`. Apply previous/next file events to visible file rows, leaving the final file highlighted after Close or Esc. Test terminal cells for the two revision headings, file navigation, and restored tree.
2. Replace the dynamic footer text with `" Push "`. Keep eligibility checks, review, and hit areas. Update the existing narrow footer test to expect the new label.
3. Describe the viewer, shortcuts, and empty Before behavior in `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md`.

### Verification

Run `cargo test --locked push::` first. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings` before handing back. In the Push practice workspace, open a file, navigate to another, close the viewer, then open and cancel the Push review.
