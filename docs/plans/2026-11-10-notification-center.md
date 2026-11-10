# Notification Center Implementation Plan

**Goal:** Reproduce lazydb's top-right notifications and searchable session history using IGit's palette and Git operation types.

**Architecture:** Shell owns a bounded notification center. Panels emit explicitly typed messages; Shell supplies the originating panel or service and renders overlays after normal content. Git confirmations, running progress, per-repository results, and persistent diagnostics keep their existing review boundaries.

**Tech Stack:** Existing Rust, Ratatui, Crossterm, and Unicode width utilities; no new production dependencies.

## 1. Notification model and views

- Add `src/ui/notifications.rs` and focused submodules/tests.
- Match lazydb's newest-first four-card stack, 24–56-column width, 4–8-row cards, close target, severity icon/border, timestamp, and 3/4/6/8-second success/info/warning/error expiry.
- Retain 500 complete messages for the session after dismissal or expiry.
- Provide history selection by stable ID, search and match navigation, scrolling, complete detail/copy, and confirmed clearing. Use the wide split and narrow detail layouts.
- Test expiry/capacity, Unicode, independent background clearing, mouse capture, history navigation, and complete message retention.

## 2. Shell routing and entry points

- Update `src/ui.rs`, `src/shell.rs`, `src/shell/workspace.rs`, and Shell support modules.
- Replace footer notices and the old console with the notification center.
- Route card clicks before panels and confirmations; allow closing cards over dialogs without replacing the reviewed dialog.
- Add configurable `F8` history access and an Actions entry. Preserve history across workspace rescans.
- Expire notifications during polling and keep footer search/help/progress available.

## 3. Explicit producer migration

- Convert Commit, Log, Push, Branch, Stash, Conflicts, clipboard, editor, workspace, and application-update messages to explicit severity.
- Determine batch severity from actual outcomes, including partial failure.
- Replace the AI error-only modal with a complete error notification, preserving drafts and focus.
- Preserve loading placeholders, inline validation needed to complete input, ongoing progress, actionable reviews, and per-item operation results.

## 4. Verification and documentation

- Start with `python3 scripts/cargo_serial.py test --locked --lib ui::notifications`.
- Add Shell integration coverage for background source, focus/review preservation, footer behavior, keyboard remapping, mouse capture, and workspace history retention.
- Run the cross-panel suite with `python3 scripts/cargo_serial.py test --locked`, then formatting and Clippy through the same wrapper.
- Update README, interaction design, architecture, PRD, and shortcut configuration documentation.
- Review the final diff without committing or modifying unrelated user work.
