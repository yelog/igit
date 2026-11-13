# Workspace Comparison Hunk Restore Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore one reviewed hunk from Log's selected commit into the current working file through a right-pointing gutter control or `L`.

**Architecture:** Reuse File History's guarded LocalCompare snapshots and the shared revision renderer's change groups. Bind each write to the comparison's captured repository, commit, and selected file; rebuild the preview and asynchronously refresh the changed-file list after applying. Keep the real index, other hunks, other files, HEAD, refs, and reflogs intact.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing tempfile and generation-safe readers; no new dependencies.

## 1. Verify the write boundary

- Add disposable-repository regressions in `src/log/workspace/tests.rs`.
- Cover a selected hunk beside another changed hunk, staged plus unstaged edits, an unrelated staged file, later committed changes, literal paths, missing/added files, stale snapshots, Roots changes, and unsupported paths.
- Start with `python3 scripts/cargo_serial.py test --locked --lib workspace_compare_applies_only_the_selected_hunk`, expecting the working-file assertion to fail before implementation.

## 2. Reuse guarded local writes

- Extend `src/log/file_history/compare.rs` with preparation and rendering for a captured workspace file.
- Preserve size, encoding, path, permission, line-ending, conversion, and snapshot guards.
- Update `src/log/workspace/backend.rs` to enable per-hunk controls only for supported ordinary files whose snapshots match the displayed sources. Preserve explained read-only previews for unsupported writes.
- Adapt `src/merge/view/revision.rs` to enable the existing gutter arrow and `L` for working-tree comparisons, without enabling apply-all.

## 3. Route application and refresh

- Handle `RevisionEvent::Apply` in `src/log/workspace.rs`, validating Roots, repository identity, selected commit, and file before writing.
- Retain revision search and view state; invalidate old hit areas and refresh the changed-file list after successful writes.
- Refresh the resident workspace on close after an applied hunk through `src/log.rs`.
- Update comparison help and footer hints to describe the available hunk action.

## 4. Document and validate

- Update `README.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md` to describe the reviewed hunk write.
- Run focused workspace comparison, File History, shared revision renderer, and Shell comparison regressions through `scripts/cargo_serial.py`.
- Run formatting, strict Clippy, and `git diff --check`; inspect all relevant remaining failures without changing unrelated code.
- Do not commit, push, or touch the old `target/` directory.

## Validation — 2026-11-12

- The initial selected-hunk regression failed with the unchanged working file before implementation, then passed after the write route was added.
- Workspace comparison: 30 tests passed, including real Shell dispatch and refresh, gutter clicks from the file pane in all three icon modes at 44×12, 80×25, and 184×36, selected older commits, insertions/deletions, missing files, literal and Unicode paths, CRLF, permissions, stale snapshots/conversion settings/scope, protected paths, and identical commit IDs across repositories. Held-key repeats and modified `L` keys cannot apply another hunk.
- File History: 16 tests passed, retaining its separate one-group and reviewed Apply All behavior.
- Shared revision renderer: 25 tests passed, including aligned change groups, selection, search, folding, and narrow panes. Total focused regression coverage: 71 passing tests.
- Review regressions reproduced and fixed two write-boundary defects: single-group application now preserves the exact working bytes of visually equivalent context; root directory identity is captured and rechecked so replacing the repository with a symlink or an identical clone cannot redirect a reviewed write.
- Real PTY smoke: the built binary rendered the ASCII `>>` gutter control and Apply toolbar. `n`, then `L`, restored only the second hunk in a temporary repository. The first local hunk, staged sibling, exact real-index bytes, and HEAD stayed unchanged, including after returning to Log.
- `python3 scripts/cargo_serial.py build --locked --bin igit`, formatting checks, `git diff --check`, and all 272 checked local documentation links passed.
- Strict all-target Clippy is blocked by existing `nonminimal_bool` in `src/commit/view.rs:109` and `needless_borrows_for_generic_args` in `src/branch/tests/smart_checkout.rs:355`. Both files are unchanged. All-target Clippy passed with only those two rules allowed on the command line; no source lint suppressions were added.
