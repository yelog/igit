# Log History Shortcuts Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Offer reviewed Drop, Undo, Revert, and Edit Message shortcuts with explicit unpublished-history boundaries.

**Architecture:** Reuse Log's captured repository/OID target and single-use Shell confirmation. Validate eligibility against stored Git history both before review and execution. Drop removes only the current unpublished tip; Undo preserves the real index and worktree. Message editing rewrites immutable commit objects and affected parent links, preserving every tree and merge topology before a compare-and-swap branch update.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing tempfile support; no new dependencies.

---

### Task 1: Constrain and verify operations

**Files:** `src/log/operations.rs`, `src/log/operations/reword.rs`, `src/log/operations/tip.rs`, `src/log/operations/transaction.rs`, `src/log/operations/tests.rs`, `src/log/operations/tests/reword.rs`, `src/log/operations/tests/tip.rs`.

1. Add disposable-repository regressions for published tips, older commits, merge tips, root commits, dirty indexes, ignored collisions, stale reviews, and message edits through merges.
2. Check all locally known remote-tracking refs, using stored history without replacement refs or grafts. Do not silently fetch or modify remote refs.
3. Prepare a Git ref transaction with the expected HEAD and lock HEAD plus its branch before the final checks. Undo updates refs only, retaining soft-reset semantics. Drop uses a clean-index/worktree preflight and non-forcing two-tree transition while refs remain locked. Use native Revert for non-merge commits. Explain first-parent removal of a merge tip in review.
4. Replace linear interactive rebase for message edits with byte-preserving commit reconstruction. Rewrite only the selected commit and its descendants on the checked-out branch, retaining other branches and tags. Strip invalidated signatures and explain that behavior in review.
5. Recheck branch/OID and remote publication under the prepared ref lock before writing refs or files. Preserve working files and index bytes during message edits, including staged changes on unrelated paths. Test an external commit after validation, a branch switch at the same OID, and publication during transaction preparation.
6. Start verification with `python3 scripts/cargo_serial.py test --locked --lib log::operations::tests::undo_moves_only_the_tip_and_preserves_the_real_index_and_worktree`, then run the operation tests.

### Task 2: Add discoverable keyboard paths

**Files:** `src/log.rs`, `src/log/actions.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, `config/default.toml`, `src/log/view_tests/commit_actions.rs`, `src/shell/log_tests.rs`.

1. Bind graph-focused `d`, `u`, `R`, and existing `F2` to Drop, Undo, Revert, and Edit Message. Keep `Space d/u`, `Ctrl-D/U`, and editor input unchanged.
2. Route menu and shortcut actions through the same review preparation helper. Target the highlighted row independently of cherry-pick checkboxes.
3. Expose `drop-commit`, `undo-commit`, and `revert-commit` configuration commands and add labels to contextual help and the action menu.
4. Cover cancellation, modal ownership, remapping, multiple repositories, and 44×12 rendering with existing test harnesses.

### Task 3: Document and validate

**Files:** `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, `docs/configuration.md`.

1. Document shortcut scope, published-history detection, destructive Drop review, Undo staging semantics, and message-only rewriting.
2. Run `python3 scripts/cargo_serial.py test --locked --lib log::`, targeted Shell/configuration tests, `python3 scripts/cargo_serial.py fmt --all --check`, and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
3. Preserve pre-existing workspace changes. Do not commit, push, or operate on user repositories during verification.

## Validation — 2026-11-11

- `python3 scripts/cargo_serial.py test --locked --lib log::`: 183 passed, including 38 operation tests and four transaction-race regressions.
- `python3 scripts/cargo_serial.py test --locked --lib shell::log_tests`: 2 passed.
- `python3 scripts/cargo_serial.py test --locked --lib config::tests`: 12 passed.
- `python3 scripts/cargo_serial.py fmt --all --check`: passed.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`: passed.
- `git diff --check`: passed. Local documentation links resolved.
- Verification used disposable repositories and the terminal test backend. The full repository test suite and an interactive terminal walkthrough were not run.
- Publication eligibility reflects local remote-tracking refs at validation; it is not a live remote-server guarantee. Ref transactions lock HEAD and its branch, not remote servers or the entire remote-ref namespace.
