# Push Default Selection and Immediate Space Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Initially check every repository with outgoing commits in the current Roots, and make Space toggle the highlighted Push repository immediately.

**Architecture:** Initialize checks from the first current outgoing snapshot without changing refresh retention or Commit and Push reseeding. Let Push apply its standalone Space action while Shell waits for the notification suffix, remembering only that repository's previous checkbox state. Completing the notification chord restores that state; unrelated keys never replay the applied Space.

**Tech Stack:** Rust, system Git, Crossterm, Ratatui TestBackend, existing configuration and notification routing. No new dependencies.

---

## Task 1: Reproduce both selection failures

**Files:**
- Modify `src/push/loading/tests.rs`.
- Create `src/shell/push_tests.rs`.
- Register shell tests in `src/shell.rs`.

1. Add a first-load test with multiple outgoing repositories, an up-to-date repository, an unavailable repository, and an outgoing repository outside Roots.
2. Add shell dispatch tests that assert checkbox changes immediately after one Space, before movement or polling. Cover successive presses, following `j/k`, and unchanged Git state.
3. Cover notification chords from both checkbox states, custom suffixes, explicit Space-Space, and a remapped Push toggle.
4. Run `python3 scripts/cargo_serial.py test --locked --lib first_load_checks_all_scoped_outgoing_repositories`, then `python3 scripts/cargo_serial.py test --locked --lib shell::push_tests::`; confirm the original behavior fails the new assertions.

## Task 2: Correct initialization and prefix routing

**Files:**
- Modify `src/push/loading.rs`, `src/push.rs`, and `src/shell.rs`.
- Adjust selection assumptions in `src/push/loading/tests.rs` and `src/push.rs`.

1. Check all entries with `n > 0` only during initial installation. Keep explicit reseeds authoritative and preserve later user choices.
2. Preserve the current expansion policy independently of the larger default checkbox set.
3. Add a small repository-checkbox checkpoint so a completed notification chord can restore exactly the eager toggle it consumed.
4. Apply Space eagerly only in Push's repository pane when the configured toggle includes standalone Space. Mark it dispatched so another key cannot toggle the checkbox a second time.
5. Retain existing Log prefix handling, editing guards, repeat/release handling, and configured notification sequences.
6. Run the focused tests, then `python3 scripts/cargo_serial.py test --locked --lib push::`, `python3 scripts/cargo_serial.py test --locked --lib shell::notification_tests::`, and `python3 scripts/cargo_serial.py test --locked --lib shell::key_sequence_tests::`.

## Task 3: Document and verify the resulting behavior

**Files:**
- Modify `README.md`, `docs/design.md`, `docs/configuration.md`, `docs/architecture.md`, and `docs/prd.md`.

1. Describe initial Roots-scoped checks, retained manual choices, authoritative Commit and Push reseeding, immediate Space feedback, and notification-chord restoration.
2. Run `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`, and `git diff --check`.
3. Review changes against pre-existing workspace edits. Use temporary repositories and local bare remotes for every regression check.

Execution stays in this authorized session. Do not commit, push, or alter unrelated workspace changes.

## Verification record (2026-11-11)

- Before the implementation, the new first-load test failed because only one outgoing repository was checked. The shell tests also reproduced Space leaving the rendered checkbox unchanged until another key.
- `python3 scripts/cargo_serial.py test --locked --lib push::`: 92 passed, including push review, partial failures/retry, submodule ordering, and write guards. After preserving the original initial preview focus, the loading tests were rerun with 11 passing, and the repository-sorting/reseed test passed.
- `python3 scripts/cargo_serial.py test --locked --lib shell::push_tests::`: 4 passed against the final implementation. Coverage includes immediate rendered feedback, successive Space presses, no movement replay, refresh/tab retention, notification restoration from both checkbox states, custom notification chords, remapped toggles, and Space before initial loading.
- `python3 scripts/cargo_serial.py test --locked --lib shell::notification_tests::`: 22 passed.
- `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --lib -- -D warnings`, and `git diff --check` passed.
- The extended key-sequence suite passed 20 tests and exposed an unrelated current-workspace failure in `log_space_filters_take_priority_over_global_pull_and_keep_editor_keys`: the existing Log `/` branch-search behavior returns `Log / Branch Search`, while the assertion expects `Pane Search`.
- The extended configuration suite passed 25 tests and exposed an unrelated current-workspace failure in `configuration_hunk_view_remaps_and_help_review_keeps_git_unchanged_on_cancel`: the Commit hunk-opening assertion expects `Commit / Stage Hunks`, but the current selection remains `Commit / Changes`.
- All-target Clippy is blocked by an existing needless borrow in `src/branch/tests/smart_checkout.rs:352`. The unrelated test files and their surrounding work were preserved.
- New shell regression checks compare repository HEAD, real index entries, working-tree diff, and status before/after interaction. All fixtures use temporary repositories and local bare remotes; the user's repositories were not committed, fetched, or pushed.
