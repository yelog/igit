# Search Delete Word Implementation Plan

**Goal:** Support Ctrl-W consistently in every slash-search input.

**Architecture:** Share a whitespace-based backward-word boundary helper with the existing text navigation functions. Keep each editor's mutation path so live filtering, caret placement, cancellation, and notification undo remain intact. Existing message and help editors use the same boundary rule.

**Tech Stack:** Rust, Crossterm, Ratatui.

## 1. Cover search editing and key routing

- Add cases in `src/ui/search.rs` for empty input, leading/trailing Unicode whitespace, punctuation, Unicode words, and deletion in the middle of a query.
- Add notification coverage in `src/ui/notifications/tests.rs` for undo/redo and caret restoration.
- Exercise Log text search, branch search, and path search through their existing events.
- Check in `src/shell/key_sequence_tests.rs` that Ctrl-W edits search text without opening a window-key sequence.
- Run one new focused test through `python3 scripts/cargo_serial.py test --locked --lib`.

## 2. Implement the shared boundary

- Add `previous_word(text, caret)` beside `previous` and `next` in `src/ui/selection.rs`.
- Skip whitespace to the left, then consume the preceding whitespace-delimited word. Preserve text after the caret.
- Use it in `src/ui/search.rs`, `src/ui/notifications/history.rs`, `src/log/filters.rs`, and `src/log/branches.rs`.
- Reuse it in existing help and Commit message Ctrl-W handling without changing selection precedence.

## 3. Document and verify

- Add Ctrl-W to search-specific help in `src/ui/search.rs`, `src/ui/notifications/history.rs`, and `src/log/keymap.rs`.
- Update search behavior in README and interaction design.
- Run focused shared-search, notification, Log-filter, shell-routing, help, and message-editor tests serially.
- Check formatting and diff whitespace; report unrelated failures without changing them.
