# Push Dialog Design Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the native Push dialog as easy to scan as the supplied IDEA reference, removing redundant status letters and hierarchy while retaining reviewed, repository-scoped operations.

**Architecture:** Keep Ratatui, shared colors/icons, stable file-tree IDs, and existing Git execution paths. The Shell owns one contextual window title; Push owns aligned pane toolbars and a responsive footer. Directory and Repository grouping start enabled; Module remains available but starts disabled to avoid repeating the same root.

**Tech Stack:** Rust, Ratatui / TestBackend, Crossterm, system Git; no new dependencies.

---

### Task 1: Reduce redundant file information

- Modify `src/push/view.rs`: render the colored filename immediately after its independent type icon, without a trailing Git status letter. Keep extension-aware clipping and gray parent paths in flat mode.
- Modify `src/push/files.rs`: default to Directory + Repository, with Module opt-in. Retain every grouping combination, stable IDs, folding, and exact file identities.
- Update `src/push/view_tests.rs`, file-tree tests, and the real-Git grouping scenario in `src/push.rs` for the new default. Verify added, copied, deleted, renamed, modified, and type-changed file colors.
- Run `cargo test --locked push::`.

### Task 2: Clarify layout and action priority

- Modify `src/shell/workspace.rs`: use one `Push Commits` title with the repository name or repository count; reserve space for Help and Close.
- Modify `src/push/view.rs`: remove the repeated header, align both lists below their toolbars, show Target / Fetch in the outgoing toolbar and Retry only for visible failures, and right-align tree expand/collapse controls. Use quiet inline checkboxes and a separated footer with Cancel and Push. Keep full action labels reachable at minimum width by wrapping the footer.
- Modify `src/push.rs`, `src/ui.rs`, and `src/shell.rs`: route Cancel back to the original workspace pane, including from Details, using an internal close-dialog action. Disable mouse Push when no eligible visible repository is checked or while a remote operation runs. Preserve keyboard review validation.
- Update rendering/hit-area tests at 44×12, 80×24, 120×36, and 180×36 in all icon modes. Check hidden selection notices and Cancel focus restoration without Git writes.

### Task 3: Verify and document

- Update `README.md`, `docs/design.md`, and `docs/prd.md` for the changed default grouping, toolbar placement, filename status colors, and footer behavior.
- Inspect actual terminal-cell renders at normal and narrow sizes. Use only temporary practice repositories for interactive/read-only review; compare HEAD, index, and worktree snapshots.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Record observed validation and provide a rendered preview. Do not commit or push the implementation.

## Design Rationale

- The original window repeated its title and content root, while status letters repeated filename colors. Remove those repetitions and preserve independent file-type icon colors.
- Align pane headings, toolbars, and first data rows so the selected branch and its files scan together. Keep repository columns and the 50/50 split, with adjustable pane widths.
- Put Target / Fetch beside outgoing commits and reveal Retry only after relevant failures. The footer gives quiet options less weight than the single Push action.
- Keep complete counts and clickable controls at minimum size. Text labels precede icon-only fallbacks; actions wrap into another footer row. Cancel restores the original pane; while an operation runs, its label becomes Close.

## Observed Verification

- `cargo test --locked --quiet`: 230 unit tests and 5 integration tests passed. Checks cover all six filename status colors, independent type icons, duplicate names, grouping identity, scoped review, toolbar/checkbox hit areas, disabled/busy Push, and Cancel focus restoration.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked` passed.
- Inspected six native-component terminal-cell renders from the temporary Push practice workspace: single/batch views, normal/narrow widths, and font fallbacks. Updated [Push dialog preview](../reference/IGit-Push-Dialog.png); temporary export code was removed.
- Real PTY walkthroughs passed with Nerd Font at 120×36, Unicode at 80×24, and ASCII at 44×12: open Push, open/close grouping and Details, cancel push review, click Cancel, and exit normally.
- All 11 temporary repositories retained their HEADs, real index entries, staged/unstaged diffs, and worktree status after the terminal walkthroughs. No hosting service or real model was used.
- Verified all 59 local link targets in the changed documentation.
