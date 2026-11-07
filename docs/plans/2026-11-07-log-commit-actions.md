# Log Commit Actions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Run reviewed Git actions on one commit selected in the Log list.

**Architecture:** Log captures the row's repository and full object ID in its menu. The operation layer reads the current worktree and prepares a single-use plan. Shell's confirmation dispatches native Git, then refreshes the workspace, including Conflicts when Git stopped during a revert or rebase.

**Tech Stack:** Rust, Ratatui, Crossterm, and system Git; no new dependencies.

---

### Task 1: Verify and implement the Git operations

**Files:** `src/log/operations.rs`, `src/log/operations/tests.rs`.

1. Use disposable repositories to assert exact ref targets, detached checkout, Undo preserving the real index and working tree, and Revert adding an inverse commit.
2. Cover Drop and message editing on earlier commits; compare parents, trees, refs, and messages. Verify stale review, dirty state, published message edits, and conflicts.
3. Capture the repository path, commit, and current branch/HEAD before review. Recheck eligibility and those identities at execution. Use non-forcing branch/tag creation, safe detached checkout, soft Undo, native Revert, and an explicit linear rebase list for Drop and older message edits. Amend only the message at HEAD.
4. Run `cargo test --locked log::operations`; all checks must pass without touching any repository outside the fixture.

### Task 2: Wire the Log list and Shell review

**Files:** `src/log.rs`, `src/log/actions.rs`, `src/log/input.rs`, `src/log/paths.rs`, `src/log/view.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, `src/shell.rs`, `src/log/view_tests/commit_actions.rs`.

1. Right-click the actual commit row or press `a` to open its menu. Keep Enter and double-click for Diff. `F2` opens the selected commit's message editor.
2. Reuse Log text input for branch/tag names and multiline messages. Keep edits local until a separate confirmation showing the repository, commit, and effect. Esc cancels; the confirmation starts on Cancel.
3. Verify mouse targeting with matching SHAs in two repositories, modal key ownership, narrow layout, cancellation, and refresh after confirmation.
4. Run `cargo test --locked log::`.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, `docs/configuration.md`.

1. Document the entry points, supported history shapes, and worktree and conflict behavior.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
