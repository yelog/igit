# Key Sequence Prefix Hints Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** After an existing key sequence prefix is entered, show available continuation keys and descriptions at the bottom for the current context, with selection, execution, and cancellation.

**Architecture:** Log and Merge supply candidates based on their pending prefixes. Shell owns the shared bottom overlay and event priority. Candidate selection stores only the cursor and rendered geometry; execution sends the continuation key back through the original panel event path. No new shortcuts, Git write changes, or dependencies.

**Tech Stack:** Rust, Ratatui / TestBackend, Crossterm, temporary Git demo repositories.

---

### Task 1: Event and rendering regressions

**Files:** Create `src/shell/key_sequence_tests.rs`; modify `src/shell.rs`.

1. Use temporary repositories to verify that `g` does not jump early, the bottom overlay shows the continuation for `gg`, `y` lists `yy/ym/ya`, and completing a sequence closes the overlay.
2. Verify that arrow keys only move between candidates and Enter uses the same action as direct completion. Esc/Ctrl-C cancel only the prefix.
3. Verify that switching panels, changing focus, menus, input, paste, mouse events, and resizing cancel the prefix. The overlay intercepts mouse hits within its bounds.
4. At 44×12, 80×24, and 120×36, verify complete candidates, cell-aware clipping of Chinese text, and an unobscured mode bar. Merge shows `[c` and `]c` hints only after a conflict file is opened.
5. Run `cargo test --locked key_sequence` and confirm that the current implementation lacks the prefix panel and its interactions.

### Task 2: Shared overlay and panel integration

**Files:** Create `src/ui/key_sequence.rs`; modify `src/ui.rs`, `src/shell.rs`, `src/log/navigation.rs`, `src/merge.rs`.

1. Declare the four existing prefixes and continuation descriptions. Derive candidates only from the panel's existing prefix state.
2. Reuse the existing palette for a bottom border, blue continuation keys, and selected backgrounds. Use one to three columns based on cell width.
3. Handle candidate navigation, Enter, Esc/Ctrl-C, and overlay mouse events before ordinary global Shell keys. Preserve the existing meaning of other keys.
4. Hand candidate execution back to the current panel. Clear Log/Merge prefixes consistently when switching panels, opening global overlays, and similar paths; fix stale Merge prefixes.
5. Run `cargo test --locked key_sequence` and the related Log navigation tests.

### Task 3: Documentation and complete validation

**Files:** Modify `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document trigger scope, candidate navigation, cancellation, mouse interaction, and narrow-screen behavior, with reproducible acceptance cases.
2. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Run terminal smoke tests in temporary demo repositories and confirm that browsing and cancellation do not change HEAD, the real index, or the worktree. Preserve existing workspace changes; do not commit or push.

### Follow-up: 200ms display delay

**Files:** `src/ui/key_sequence.rs`, `src/shell.rs`, `src/shell/key_sequence_tests.rs`, and the corresponding usage documentation.

1. Store a display deadline in the prefix candidate state. Include it in Shell's poll timeout and redraw automatically at the deadline, without delaying key-sequence execution.
2. Clear the timer on completion, cancellation, or leaving the context. Restart it for each new prefix. Hidden candidates do not intercept arrow keys or similar actions.
3. Use a controllable `Instant` to validate the 199/200ms boundary, wakeup without input, fast completion, cancellation, new prefixes, and repeated Merge prefixes without sleeping test threads.
4. Run targeted and full tests, fmt, and Clippy. Check the real delay and the absence of a flash during fast completion in a pseudoterminal.
