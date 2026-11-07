# Stash Review Layout Implementation Plan

**Goal:** Make the consequences, selected stash, and destination easy to verify before Apply, Pop, or Drop.

**Architecture:** Render the Stash review from the same prepared snapshot used for confirmation. Shell keeps its existing modal input and dispatch; the Stash view owns the layout and action styles.

**Tech Stack:** Rust, Ratatui, Crossterm, and system Git.

---

1. Extend `src/shell/stash_tests.rs` to check that each consequence and named action is visible at 120×36, 80×24, and 44×12. Verify that the smallest view can scroll to the full review and cancel with the mouse.
2. Draw a compact review from `src/stash/operations.rs` and `src/stash/operations/view.rs`. Keep the consequence and footer fixed, align the repository and stash fields, wrap paths and hashes by terminal cells, and use blue, amber, and red for the three actions. Use the same reviewed data for the plain confirmation payload.
3. Route only Stash operation confirmations to the new view in `src/shell.rs`. Keep current keyboard focus, confirmation, and Git behavior. Describe the controls in `README.md` and `docs/design.md`.
4. Run `cargo test --locked stash`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. Inspect actual terminal renders and use a disposable repository to verify cancellation.

## Observed Verification

- `cargo test --locked stash`: 55 tests passed, including named actions and consequences at 120×36, 80×24, and 44×12 and scrolling to the full HEAD at 44×12.
- A real pseudo-terminal rendered all three reviews at 120×36 and resized Apply to 44×12. Keyboard and mouse cancellation preserved HEAD, index entries, working-tree diff, status, and the stash list in both disposable repositories.
