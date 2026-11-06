# Commit Stash Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Press `s` in Commit to stash all changes, including untracked files, in every changed repository in Roots, using one shared message.

**Architecture:** A Commit message dialog feeds the existing confirmation overlay. A dedicated backend snapshots each repository's HEAD, branch, index, changed paths, and working files before review, rechecks them before native `git stash push --include-untracked`, and reports independent repository results. Checkboxes do not restrict this explicitly requested whole-repository action; ignored files and repositories outside Roots remain untouched. The existing Stash tab reads the resulting entries on entry/refresh.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing dependencies.

---

### Task 1: Implement safe repository stash planning and execution

**Files:** Create `src/commit/backend/stash.rs`, `src/commit/backend/stash/tests.rs`, and `src/commit/backend/working.rs`; modify `src/commit/backend.rs` and `src/commit/backend/discard.rs`.

1. Add real-Git tests using disposable repositories for mixed staging, unchecked staged files, untracked and ignored files, Unicode/native paths, changed reviews, and failure recovery.
2. Extract the existing working-file fingerprint reader so discard and stash share the same symlink and nested-repository protections.
3. Read all changed paths and both index/worktree patches without modifying repository state. Reject conflicts, active Git operations, unborn HEAD, and unsupported submodule-pointer/file-layout changes before writes.
4. Revalidate before `git -c submodule.recurse=false stash push --include-untracked --message`. Detect a published stash even if cleanup fails and retain its object ID in the result; never claim it was rolled back.
5. Run `cargo test --locked commit::backend::stash`.

### Task 2: Integrate the message dialog, review, and batch results

**Files:** Create `src/commit/stash.rs` and `src/commit/stash/tests.rs`; modify `src/commit.rs`, `src/commit/keymap.rs`, `src/config/keys.rs`, and `config/default.toml`.

1. Add `commit.stash` on `s`; move `commit.toggle-status` to `T`, retaining View Options access.
2. Reuse the input dialog for a shared stash message, separate from the saved commit draft. Enter reviews; Escape cancels with no Git writes.
3. Review current changed repositories in Roots, showing every path, the shared message, and preparation errors. Exclude checkboxes and ignored files from scope decisions.
4. Execute each reviewed repository once, recheck Roots, and refresh after results. Keep preparation/execution failures separate and retain the stash message for retry. Do not repeat already published snapshots after partial cleanup failures.
5. Test multiple repositories, narrowed Roots, empty/cancel paths, literal input, partial failures/retry, remapped bindings, and narrow dialog rendering.

### Task 3: Document and verify

**Files:** Update `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, and `docs/configuration.md`.

1. Document `s`, whole-repository scope, untracked/ignored behavior, shared messages, independent outcomes, and `T` for status columns.
2. Run `cargo test --locked commit::`, full `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings` as the environment permits. Fix failures caused by this feature and report unrelated failures accurately.
3. Inspect the final diff, preserving all pre-existing work. Do not commit, push, or stash the user's working repositories.

## Verification on 2026-11-06

- `cargo test --locked stash -- --nocapture`: all 33 matching tests passed,
  including 19 new creation tests and the existing Stash browsing coverage.
- The initial default-concurrency full run passed 403 tests; the existing AI
  mock server hit its five-second socket read timeout. Its isolated rerun
  passed, without changing that test or its implementation.
- `cargo test --locked -- --test-threads=4`: all 404 library tests and all
  eight integration tests passed. No tests were filtered or ignored.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `git diff --check`, and relative documentation links passed.
- A 120×36 pseudo-terminal walkthrough on two temporary repositories used
  Commit `s`, typed a shared message, opened review, confirmed with `y`, and
  opened Stash with `6`. Both repositories became clean; their stash worktree,
  index, and untracked snapshots retained the expected distinct contents.
- Native Git requires default pathspec environment semantics during stash
  cleanup. Tests caught and fixed inherited literal-pathspec behavior that
  otherwise left untracked filenames containing spaces in the working tree.
- No commit, push, release, or stash operation ran against the user's working
  repositories. All write verification used disposable repositories.
