# Commit Add to Git Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a configurable shortcut that stages the highlighted untracked file or the untracked members of its group, directory, or repository.

**Architecture:** The Commit controller takes the current row's exact members within Roots, independently of commit checkboxes, and opens the existing confirmation dialog. A dedicated backend reuses the commit snapshot, working-file fingerprint, index lock, and exact-path reconciliation helpers. Publication changes only the reviewed index entries; working files and HEAD remain unchanged.

**Tech Stack:** Rust, system Git, Ratatui/Crossterm, TOML keybindings.

---

### Task 1: Reproduce the missing shortcut

- Add `src/commit/add/tests.rs` using temporary repositories with untracked files, unrelated staged files, and mixed staged/unstaged content.
- Exercise `a` through panel events and require an Add to Git review. Run the focused test before implementing the handler.

### Task 2: Implement reviewed addition

- Add `src/commit/backend/add.rs`: capture exact untracked paths, branch/HEAD, raw working-file fingerprints, and a Git-filtered tree in a private index. On confirmation, revalidate, lock the real index, rebuild and compare the snapshot, and publish only those paths with `reset_index_paths`.
- Add `src/commit/add.rs`: collect current row members without expanding collapsed groups or using checkboxes, enforce Roots at review and execution, consume review plans once, report separate repository results, and request refresh.
- Register `commit.add-to-git` with `a` and `Insert`, restricted to Changes, in `config/default.toml`, `src/config/keys.rs`, `src/commit.rs`, and contextual help. Move Amend to `M` to keep the bindings distinct.
- Keep checkbox choices and the shared message unchanged. Git status moves staged additions out of Unversioned Files through the normal refresh path.

### Task 3: Verify scope and document usage

- Test single files, folded directories, repository/group scopes, hidden Roots, review cancellation, partial failure/retry, stale content/status, literal/Unicode paths, unborn HEAD, index locks, and unsafe file/directory replacements.
- Verify default/remapped/help shortcuts and text-input isolation through Shell.
- Update README, interaction design, PRD, architecture, and keybinding documentation.
- Run focused Add to Git tests, Commit/configuration tests, formatting, Clippy, and the full test suite. Preserve previous Merge changes; do not commit or push.

### Verification outcome

- The initial shortcut regression failed before implementation because no
  Add to Git review was opened. The final default is `a` with `Insert` as an alias;
  Amend uses `M` and the proposed `I` binding is inactive.
- All 11 Add to Git tests pass, including real panel/Shell events, remapping,
  help, cancellation, 44/80/120-column review rendering, partial failure, and
  preservation of unrelated staged and working content.
- Formatting, Clippy with warnings denied, and `git diff --check` pass.
- Full-suite verification is incomplete: the parallel run stalled in macOS
  FSEvents waits, and a lower-concurrency retry stalled during process launch
  before test output. Both test processes started for this task were stopped;
  no unrelated test process or source change was removed.
