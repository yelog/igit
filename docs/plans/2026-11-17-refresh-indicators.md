# Push and Branch Refresh Indicators Implementation Plan

**Goal:** Give Push and Branch reads the same animated glyphs as Commit's Generate message while keeping cached rows and controls stable.

**Architecture:** Keep background read ownership and write guards unchanged. A small UI indicator owns a 100 ms clock and width-aware status rendering; Shell ticks it while polling existing workers.

**Tech Stack:** Rust, Ratatui, Crossterm, existing `IconMode` glyphs and palette.

---

### 1. Share the visual indicator

- Add `src/ui/loading.rs` and register it in `src/ui.rs`.
- Reuse `IconMode::spinner`; use blue for the glyph and muted text for the label.
- Advance by elapsed time, so polling or key repeat does not accelerate animation.
- Keep the glyph when labels must shorten; stop and reset on completion or cancellation.
- Verify clock boundaries, reset behavior, icon modes, and narrow widths.

### 2. Integrate Push and Branch

- Add panel-owned indicators in `src/push.rs` and `src/branch.rs`.
- Tick both from `src/shell.rs`, including when installing a snapshot is deferred.
- Put Push loading feedback in its existing toolbar; reserve space before choosing the strategy label.
- Put Branch loading feedback in the Branch column heading, leaving metadata headings and View Options available.
- Remove the transient Branch loading row so cached rows, selection, scroll, and hit areas do not move.
- Keep failures static and red, and preserve the existing read guards and retry workflow.

### 3. Verify and document

- Add deterministic rendering checks alongside existing Push and Branch tests.
- Cover first load, cached refresh, frame advancement, completion, failure, ASCII/Unicode/Nerd Font, and 120×36, 80×24, 44×12.
- Run a focused indicator test first with `python3 scripts/cargo_serial.py test --locked --lib ui::loading`.
- Run the affected panel and Shell tests with the same serial wrapper.
- Run formatting and Clippy through the wrapper.
- Update `docs/design.md`, `docs/architecture.md`, and `README.md` with the implemented behavior.
- Build both standalone binaries with `python3 scripts/cargo_serial.py build --locked --bins`.

### Verification completed

- Focused indicator and refresh geometry tests passed.
- `test --locked --lib loading`: 69 passed.
- `test --locked --lib push::view_tests`: 29 passed.
- `test --locked --lib shell::branch_tests`: 15 passed, including real polling-driven animation.
- `test --locked --lib branch::tests::presentation`: 5 passed.
- `test --locked --lib shell::push_tests::push_animation_tracks`: 1 passed.
- `fmt --all --check` and `clippy --locked --all-targets -- -D warnings` passed.
- `build --locked --bins` rebuilt both debug executables in `target-serial/debug/`.
- A disposable PTY workspace with delayed local Git reads showed multiple animation frames during initial loading and cached refresh. Branch and Push passed at 120×36, 80×24, and 44×12; completion restored ordinary controls and the narrow Push strategy stayed visible. HEAD, staged changes, and working-tree changes matched their pre-run snapshots.

All Cargo commands above used `python3 scripts/cargo_serial.py`. No full-suite
run, external model call, or operation on user repositories was required.
