# Startup Workbench Implementation Plan

**Goal:** Show the Commit workbench immediately, with local loading feedback until repository reads finish.

**Architecture:** Keep the existing background initialization and reviewed-write boundary. Share Commit pane geometry and shell chrome between startup and the loaded workspace. Render a small animated status and static skeleton rows without filesystem reads on the terminal thread.

**Tech Stack:** Rust, Ratatui, Crossterm, existing standard-thread worker and mpsc channel. No new dependencies.

## Design

- The first frame contains the real tab bar, Changes pane, message composer, preview pane and footer.
- Changes owns the only animation, updated every 120 ms. Its label reports discovery, changes and conflict checks; discovered root count and elapsed time are factual.
- Neutral skeleton strips suggest tree structure without inventing filenames, checkboxes, selected counts or progress percentages.
- Preview waits for changes. Message actions and navigation are visibly unavailable until initialization completes; input is not queued.
- Use the same 92-column breakpoint and pane geometry as the loaded Commit panel. Small terminals show the existing size guidance. Keep resize and immediate cancellation.
- Direct panel launch keeps the requested tab selected. Empty-workspace and failure handling remain truthful.

## Implementation

1. Extend `src/shell/startup_tests.rs` with pending-frame geometry, animation, disabled actions, direct-tab and resize checks.
2. Extract the existing pane arrangement in `src/commit/view.rs`; add the loading renderer in `src/commit/loading.rs`.
3. Reuse shell header/footer from `src/shell/workspace.rs` in `src/shell/startup.rs`. Add discovered-root reporting and clock-based redraws independent of worker stage messages.
4. Verify with `python3 scripts/cargo_serial.py test --locked --lib shell::startup_tests`, focused Commit layout tests, formatting and Clippy. Exercise an artificially delayed Git read in a disposable PTY workspace and check cancellation, normal completion and unchanged Git state.
5. Update README, interaction design, architecture and PRD to describe the delivered loading behavior and its interaction boundary.

Existing uncommitted changes must be preserved. No commit, publication or write to the user's moss-web repositories is part of this task.

## Verification results

- Passed 15 startup/loading tests, 31 Commit view tests and 18 workspace tests, including a targeted rerun of a workspace assertion after the concurrent notification changes settled.
- `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings` passed. Both local binaries build successfully.
- A disposable two-repository PTY check gated every Git invocation until after the entire workbench had painted. Three runs measured first complete frame times of 582.7, 39.1 and 37.7 ms; cancellation with Git blocked took 0.7–0.8 ms. These are local debug-build observations, including the first execution after rebuilding, not timing guarantees.
- Verified continuous animation, 120/80/44-column resizing, colored skeletons, `NO_COLOR` text placeholders, ignored loading-time input and successful replacement with real changes.
- HEAD, real-index bytes, staged/unstaged diffs and untracked file content remained unchanged. Validation used isolated app state and disabled automatic update checks; the user's moss-web workspace was not used for regression testing.
- The ready transition drains pending terminal input before exposing actions. Initial drawing uses a render-only view rather than constructing a Shell, keeping constructor-time Git reads off the first-frame path.
