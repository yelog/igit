# Historical Merge Diff Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Match IDEA's merge-commit file groups and read-only parent/result/parent comparison in Log.

**Architecture:** Load combined paths and each parent's changes from immutable Git trees. Keep group identity on every file leaf. Reuse the conflict editor's pure three-way alignment, row renderer, folding and scrollbars through a separate historical viewer that owns no writable document or apply actions.

**Tech Stack:** Rust, system Git, Ratatui, Crossterm, existing `similar` line/character comparison.

---

### Task 1: Merge file groups and exact revision reads

Files: `src/log/model.rs`, `src/log/model/merge_diff.rs`, `src/log/inspector.rs`, `src/log/preview.rs`.

1. Add temporary Git fixtures for two-parent merges with common and parent-only changed paths.
2. Load combined files (changed against every parent), followed by `Changes to <hash> <subject>` groups.
3. Preserve byte paths and parent identity on duplicate file leaves; compare a parent-group file against that parent.
4. Load three blobs by immutable tree entries with bounded reads; distinguish absent files, binary content, mode-only changes and gitlinks.
5. Verify grouping, literal paths, and repository/index/worktree immutability.

### Task 2: Read-only three-pane viewer

Files: `src/merge/view/revision.rs`, `src/merge/view/rows.rs`, `src/merge/view.rs`, `src/merge.rs`, `src/log.rs`, `src/log/keymap.rs`.

1. Let the shared row map/renderer consume chunks without requiring a writable `Document`.
2. Render first parent / committed result / second parent with lock indicators, immutable hashes, aligned line numbers, blue block/character highlights, folded context and scrollbars.
3. Add next/previous change, pane focus, per-pane horizontal scroll, zoom, fold controls and mouse targets; retain a focused-pane view in narrow terminals.
4. Open combined files in the historical viewer; keep parent comparisons and unsupported content in explicit read-only patch views. Support file navigation without losing group identity.
5. Verify rendering and interactions at wide, ordinary and narrow terminal sizes; preserve the live conflict editor's decisions and write boundaries.

### Task 3: Validation and documentation

Files: Log/merge tests, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, `README.md`.

1. Run targeted Log and merge checks, then the full Rust tests and formatting/lint checks appropriate to the shared renderer changes.
2. Render a real temporary merge fixture for visual inspection and verify all three source bytes, line numbers, highlight boundaries and unchanged Git state.
3. Document group meanings, read-only viewer controls and terminal limitations.
4. Build both `ig` and `igit` from the current complete workspace into its normal target directory; isolated validation must use a separate target directory.

No commit or push is part of this implementation request.

## Validation completed

- `cargo test --locked`: 312 library tests and 6 integration tests passed.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` passed.
- A temporary merge repository rendered the combined/per-parent tree and the three-way viewer at 210×48, 120×36 and 44×12. Reviewed aligned line numbers, folded function sections, side-specific line/character highlights and code colors.
- Rebuilt both debug binaries from the complete current workspace. Native pseudo-terminal runs of `ig` at 180×40 and `igit` at 80×24 opened the grouped tree and historical three-way viewer; the temporary repository remained clean.
- Regression fixtures preserve unrelated staged and unstaged bytes and the exact real index while opening combined and parent-specific comparisons. The existing live conflict-editor tests passed after sharing its renderer.
