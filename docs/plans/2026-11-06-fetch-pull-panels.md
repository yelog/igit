# Fetch and Pull Panels Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give Fetch and Pull the same visual hierarchy as Push while preserving reviewed global Roots scope and existing Git safety checks.

**Architecture:** Keep the shell update dialog and its Git plans. Separate terminal presentation into `src/shell/update/view.rs`, with structured repository rows and outcomes that survive review, running, and results. Reuse existing colors, repository icons, cell-aware clipping, and button styles without adding dependencies or changing Git commands.

**Tech Stack:** Rust, Ratatui / TestBackend, Crossterm, system Git.

---

### Task 1: Build a consistent repository review

- Modify `src/shell/update.rs`: retain repository identity, submodule markers, source/target labels, and structured outcomes for presentation. Preserve plan revalidation, scope, strategy, cancellation, and error redaction.
- Create `src/shell/update/view.rs`: render a contextual title strip, a bordered repository pane, aligned repository/branch/remote/status columns, stable repository colors, and a separated footer. Fetch explains remote-ref updates; Pull presents compact Merge / Rebase controls.
- Keep the initial keyboard action on Cancel. Highlight the primary action independently of focus; distinguish keyboard focus without changing Enter behavior.
- Hide optional columns before clipping essential content. Use terminal-cell widths for all icon modes, Unicode names, controls, and hit areas.

### Task 2: Support inspection and complete states

- Add repository browsing with arrows, `j/k`, paging, Home/End, and mouse rows/wheel; browsing never changes the update scope or confirms writes.
- Show the highlighted repository's complete target or error in a wrapped details region. Permit scrolling long details without moving the selected repository.
- Reuse the repository list for running and results, with colored status labels and separate succeeded/failed/skipped counts. Keep the dialog active until the worker reports completion.
- Test empty scope, disabled actions, long labels/errors, many repositories, strategy hit areas, resizing, and all icon modes in `src/shell/update/view_tests.rs`.

### Task 3: Verify existing Git behavior and document

- Adapt presentation assertions in `src/shell/update_tests.rs` without weakening real-Git checks for scope, cancellation, fetch-only changes, merge/rebase, stale plans, and retained conflicts.
- Run `cargo test --locked shell::update`, then `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Inspect actual terminal-cell renders at 44×12, 80×24, and 120×36, including multi-repository and result states. Use disposable fixtures only.
- Update `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md` for the presentation, navigation, and module boundary. Preserve existing unrelated edits. Do not commit or push.

## Design decisions

- The current dialog presents every field with the same weight and wastes wide-terminal space. A Push-style title, blue pane heading, tinted repository icons, aligned fields, and one primary footer action establish hierarchy.
- Fetch lists remotes without an integration arrow. Pull shows the current branch receiving its reviewed upstream. No new repository checkboxes are introduced: Roots remains the authoritative scope.
- Selection is for inspection. Long repository names, target names, and Git errors remain accessible through wrapped details even when the main list clips them.

## Observed verification

- `cargo test --locked --quiet`: 304 unit tests and 6 integration tests passed. The 8 update-specific tests cover real local Git operations, stale-plan rejection, retained conflicts, all icon modes, responsive controls, read-only browsing, Unicode details, disabled actions, running state, and result colors.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, and `git diff --check` passed. All 131 local links in the changed documentation resolved.
- Inspected native Ratatui cell renders of both reviews at wide, medium, and narrow sizes. Temporary preview-export code was removed; generated previews are outside the repository.
- Real PTY walkthroughs passed at 120×36 with Nerd Font, 80×24 with Unicode, and 44×12 with ASCII: open Fetch/Pull, browse repositories, change strategy, restore Cancel focus, cancel, reopen, and quit normally.
- The 11-repository disposable practice workspace retained every HEAD, real index entry, staged/unstaged diff, worktree status, and remote-tracking ref after the walkthroughs.
