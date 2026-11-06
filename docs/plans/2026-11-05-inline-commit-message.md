# Inline Commit Message Implementation Plan

> Execute the steps in this workspace, preserving the existing Push changes. Do not commit unless requested.

**Goal:** Edit each repository's commit message directly in the bottom Commit pane.

**Architecture:** Keep the existing editing buffer, Unicode caret, text selection, and per-repository draft persistence. Render message editing inside the composer's actual text rectangle; keep changelist and AI dialogs as overlays. Save the draft before mouse navigation or commit review, and retain explicit save/cancel controls.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing TestBackend fixtures.

---

### Task 1: Cover inline geometry and mouse editing

**Files:** `src/commit/view_tests.rs`, `src/commit/view_tests/inline_message.rs`.

1. Add regression coverage at 120×36, 80×24, and 44×12: clicking the draft enters editing, the editor stays inside `PaneId::Message`, and the Changes rows remain visible/clickable.
2. Cover initial-click caret placement and drag selection without requiring a second click.
3. Run `cargo test --locked inline_message`; expect the geometry test to fail against the modal editor.

### Task 2: Render and route the inline editor

**Files:** `src/commit.rs`, `src/commit/view.rs`, `src/commit/keymap.rs`.

1. Reuse the input renderer in the draft rectangle and show the caret only when the Commit surface owns input.
2. Keep the Message pane border accented while editing; retain mouse hit areas for repository tabs and commit actions.
3. Exclude message editing from overlay hit testing. Save before a click leaves the field, then process the same click normally. Preserve the initial mouse press for selection.
4. Keep Enter for newline, F2/Ctrl-S for save, Esc for selection clearing/cancel, and inline Save/Cancel buttons. Keep other dialogs unchanged.
5. Test independent repository drafts and both commit review buttons using current edited text; surface persistence failures without navigating or reviewing.

### Task 3: Integrate workspace navigation and documentation

**Files:** `src/shell.rs`, `src/shell/workspace.rs`, `src/shell/workspace_tests.rs`, `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Save an inline draft when clicking another workspace surface; keep F1 help and editor text shortcuts intact.
2. Test navigation and review against the mixed staged/unstaged fixture, leaving unrelated staged content unchecked. Compare HEAD, index, and working tree before/after canceling review.
3. Update editing instructions and add a reproducible inline-editor acceptance case without changing prior Push documentation.
4. Run `cargo test --locked commit::`, then `cargo test --locked` for shared-shell regression coverage.
5. Run `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`; inspect the final diff for unrelated changes.

### Verification result

- Implemented inline rendering, initial-click caret/selection, save/cancel, per-repository blur saving, workspace navigation, and save-failure handling.
- Added eight focused regression tests; layout coverage includes 120×36, 80×24, and 44×12 using Ratatui TestBackend.
- Final `cargo test --locked -q`: 253 unit tests and 5 integration tests passed, with no skipped tests.
- Final `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`: passed.
- A real pseudo-terminal walkthrough was attempted but did not complete during a host process-startup stall; no interactive acceptance claim is made.
