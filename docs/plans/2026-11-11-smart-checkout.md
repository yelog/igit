# Smart Checkout Implementation Plan

**Goal:** Offer IDEA-style Smart Checkout, Don't Checkout, and Force Checkout when local changes prevent a branch switch.

**Architecture:** Keep the ordinary protected `git switch` fast path. A refused overwrite creates a single-repository review using the existing Commit stash backend's content, index, HEAD, and branch fingerprints. Run reviewed save/switch/restore work in Branch's existing background worker; reuse native conflict stages and the Conflicts workspace. Keep a named stash whenever restoration is incomplete.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing serial Cargo wrapper and disposable repositories.

## Research and decisions

- JetBrains documents direct checkout for compatible local changes, a blocking-files dialog for overwritten changes, and shelve → checkout → unshelve for Smart Checkout. Restoration conflicts open merge resolution. IDEA also supports Git stash as the save mechanism: https://www.jetbrains.com/help/idea/manage-branches.html
- Use Git stash because IGit already implements safe whole-repository stash capture and browsing; a second shelf format adds persistence and recovery machinery without improving this workflow.
- Include staged, unstaged, and untracked files in the reviewed save scope; preserve index separation where Git can restore it. Never recurse into another repository or save ignored files.
- Apply the saved immutable OID, retain it on conflicts/failure, and drop only the exact unchanged reflog entry after complete restoration. Never use an unchecked `stash pop`.
- Force Checkout gets a second explicit destructive review, initially focused on Cancel. Revalidate files, index, HEAD, refs, Roots, and repository identity before any write.
- Switching failures restore on the original branch when its identity is unchanged. External changes stop automatic recovery and keep the named stash.

## Steps

1. Reuse the stash backend's existing review/capture validation without changing Commit behavior. Add Branch checkout orchestration, guarded recovery, exact stash cleanup, and staging restoration.
2. Add a scrollable three-button checkout problem dialog with keyboard, mouse, help, narrow-terminal support, and a separate Force confirmation.
3. Route the dialog through Shell's existing Branch confirmation ownership; run writes in its Branch worker and open Conflicts after restore conflicts.
4. Cover compatible edits, mixed staging, untouched staged siblings, untracked collisions, conflicting restoration, remote tracking, stale reviews, ignored/nested/submodule boundaries, failed checkout recovery, cancellation, and force review with disposable Git repositories.
5. Update README, PRD, interaction design, and architecture. Run one focused test first, then relevant Branch/Shell/Stash regressions, formatting, and Clippy via `python3 scripts/cargo_serial.py`.

No commits, remote writes, or tests against user repositories are part of this work.

## Verification

Verified on 2026-11-11 with the shared serial Cargo wrapper:

- `test --locked --lib smart_checkout`: 20 passed, including original-branch recovery, mixed staging, hidden dirty submodules, remote tracking, protected paths, cancellation, force confirmation, and keyboard/mouse layouts.
- `test --locked --lib stash`: 63 passed, including existing Commit save, Stash apply/pop, linked worktrees, native filenames, and conflict recovery.
- `test --locked --lib branch`: 116 passed, one failed. `shell::tests::external_branch_switch_refreshes_commit_rows_preview_and_roots` still uses the old `o`, Down, Enter sequence to enable Module; the concurrently changed Commit menu now puts Repository there. This unrelated test was left unchanged.
- `fmt --all --check`: passed.
- `build --locked --bin igit`: passed.
- Real terminal smoke checks with the built binary at 120×36: clean restore and conflicting restore both passed. The three-action review appeared before writes; staged siblings, untracked files, and original saved content were checked after the application exited. Complete restoration removed its backup; conflicts retained it.
- `clippy --locked --all-targets -- -D warnings`: blocked by the unrelated `clippy::obfuscated_if_else` diagnostic in Shell's notification-prefix hint (`space_replays_standalone_action`). This change does not modify that code.
- Local Markdown link targets and `git diff --check`: passed.
