# Commit Discard Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Pressing `d` in Commit's changes list reviews and discards all staged and working-tree changes in the highlighted file or directory.

**Architecture:** Keep cursor scope independent of commit checkboxes. Build per-repository discard snapshots, use Shell's existing confirmation dialog, and revalidate native paths, HEAD, index records, and working-tree contents before writing. Restore tracked files from HEAD and remove additions/untracked files individually; preserve unrelated staged content and never recursively delete a directory.

**Tech Stack:** Rust, Ratatui/Crossterm, system Git, existing tempfile and confirmation infrastructure.

---

### Task 1: Cover real Git discard semantics

**Files:**
- Create: `src/commit/backend/discard.rs`
- Modify: `src/commit/backend.rs`

1. Add real-repository tests for an MM file alongside an unrelated staged file; assert HEAD stays fixed, the target returns to HEAD in both index and worktree, and the other staged blob survives.
2. Cover staged additions, deletions, untracked files, unborn HEAD, native/literal paths, symlinks, stale review, existing index locks, ignored files, conflicts, submodules, and unsafe file/directory replacements.
3. Run `cargo test --locked commit::backend::discard` and confirm new tests require the implementation.
4. Implement per-repository plan and execute functions using existing Git helpers and index locking. Review must not change HEAD/index/worktree; errors must identify incomplete work.
5. Rerun the focused tests and inspect actual index records and file bytes.

### Task 2: Connect the cursor and review workflow

**Files:**
- Modify: `src/commit.rs`
- Modify: `src/commit/keymap.rs`
- Modify: `src/config/keys.rs`
- Modify: `config/default.toml`
- Create: `src/commit/discard.rs`
- Create: `src/commit/discard_tests.rs`
- Create: `src/shell/discard_tests.rs`
- Modify: `src/shell.rs` (module registration only; preserve existing changes)

1. Resolve a file by native identity; resolve directory scope across changelists and only repositories represented by the visible directory.
2. Register `commit.discard` with default `d`, active only in Changes. Exclude message input, preview, and menus.
3. Build review text listing each repository/path and explaining staged/unstaged restoration and untracked deletion. Use the existing Cancel-default dialog.
4. Execute only reviewed plans, refresh after success or partial failure, and report each repository separately.
5. Test unchecked cursor targets, directory grouping with and without Module, hidden repositories, canceled/default confirmation, help/remapping, and successful refresh.

### Task 3: Document and verify

**Files:**
- Modify: `README.md`
- Modify: `docs/design.md`
- Modify: `docs/prd.md`
- Modify: `docs/architecture.md`

1. Document `d`, its review boundary, directory scope, and unsupported destructive cases in English.
2. Run `cargo test --locked commit::`, then `cargo test --locked` because configuration and Shell routing are involved.
3. Run `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`.
4. Review the final diff against the initial dirty state. Do not commit or change existing Push work.

## Completed Verification · 2026-11-06

- Implemented cursor-based review for files, physical directories, and repository rows; changelist headings remain unavailable.
- `cargo test --locked`: 254 unit tests and 5 integration tests passed. This includes discard snapshots, both directory layouts, cancellation at three terminal sizes, remapping/help, native paths, symlinks, stale staging/HEAD/worktree, and partial completion.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `git diff --check`: passed. Existing changes in shared files were preserved.
