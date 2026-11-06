# Stash Operations Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Apply, pop, or drop the currently selected stash through keyboard shortcuts and a separate confirmation.

**Architecture:** Stash owns a single-use reviewed operation containing repository identity, the selected entry, its reflog snapshot, and the destination branch/HEAD for apply/pop. Apply uses the immutable stash object; pop applies first and drops the reviewed selector only after success and another reflog check. Native Git protects local edits and supplies conflict handling; every attempted write refreshes the Stash, Commit, and Conflicts panels through the existing Shell action.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git; existing dependencies only.

---

### Task 1: Add reviewed Git operations

**Files:** Create `src/stash/operations.rs`, `src/stash/operations/scope.rs`, and `src/stash/operations/tests.rs`; modify `src/stash/model.rs`.

1. Expose read-only stash entries and compare full entry lists, including selectors, to distinguish duplicate object IDs and reject stale reviews.
2. Prepare single-repository Apply/Pop/Drop reviews with the exact saved message, time, object ID, repository path, destination, and operation effects. Reviews do not write Git state.
3. Recheck Roots, repository path, entry list, branch/HEAD, and index before apply/pop. Refuse unresolved conflicts or an active Git operation; let native Git preserve non-overlapping local edits and refuse overwrites.
4. Check saved paths against submodules, nested repositories, and directory symlinks at review and execution. Run native apply on the object ID with submodule recursion disabled. Pop drops only after successful apply and an unchanged reflog; conflicts and partial failures retain the entry. Drop never modifies working files or the index.
5. Cover staged/unstaged/untracked content, dirty worktrees, conflicts, stale entries, duplicate OIDs, Roots/workspace/branch changes, lock failures, and linked worktrees in temporary repositories. Run `cargo test --locked stash::operations`.

### Task 2: Integrate shortcuts, confirmation, and refresh

**Files:** Modify `src/stash.rs`, `src/stash/tests.rs`, `src/shell/stash_tests.rs`, `src/shell/configuration.rs`, `src/config/keys.rs`, and `config/default.toml`.

1. Add remappable `stash.apply = a`, `stash.pop = P`, and `stash.drop = d` in the list and file tree. Open Diff continues owning its input and remains read-only.
2. Route all actions through the shared Cancel-first confirmation; repository headers and empty lists have no target. Consume each review once.
3. Refresh after execution, report success/failure without replaying a successful apply, and retain a nearby selection in the same repository after removal.
4. Cover cancellation, help, bindings, empty/group rows, Diff input guards, selection after drop/pop, and Conflicts visibility through Shell at supported terminal sizes.

### Task 3: Document and verify

**Files:** Update `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, and `docs/configuration.md`.

1. Document shortcuts, confirmation, native apply staging behavior, pop conflict retention, and destructive drop semantics. Keep unrelated Branch navigation work intact.
2. Run `cargo test --locked stash`, `cargo test --locked -- --test-threads=4`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Exercise apply/pop/drop in a pseudo-terminal using disposable repositories; inspect working files, the real index, and stash refs. No commit, push, or stash mutation against user repositories.

### Verification results

- `cargo test --locked stash -- --test-threads=4`: 54 tests passed, including real Git operations, conflicts, stale reviews, repository boundaries, cancellation, remapped shortcuts, and Shell refresh.
- `cargo test --locked --test configuration_cli`: both tests passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- Targeted `rustfmt --check` for the changed Stash/configuration files and `git diff --check`: passed. The full format check reported unrelated in-progress formatting in `src/core/loading.rs`, `src/core/loading/tests.rs`, and `src/push/loading.rs`; these files were left intact.
- A real pseudo-terminal walkthrough at 120×36 passed Cancel, Apply, Pop, and Drop against two disposable repositories. Selected content and stash refs matched expectations; the other repository's stashes and local file remained unchanged. Shell rendering tests also cover 80×24 and 44×12.
- The full library run completed with 449 passed and four failures outside Stash: the concurrent Branch background-loading test, two smart-pane configuration tests still using the old Conflicts shortcut, and the workspace pull/conflict test using that old shortcut. The full command stopped at these library failures. Their code was not changed for this task.
