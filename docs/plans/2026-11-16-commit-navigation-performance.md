# Commit Navigation Performance Implementation Plan

**Goal:** Keep Changes navigation responsive while preserving exact staged/unstaged previews, checkbox scope, and repository contents.

**Architecture:** Reuse the shared bounded background reader for Commit previews. Navigation updates the existing in-memory tree immediately; a stable file selection schedules one read after 60 ms. Read sources and construct revision alignment on the worker, install only the current repository/path/status/side generation, and coordinate status refresh with the same prepared preview format.

**Tech Stack:** Rust, Crossterm, Ratatui, the existing `core::loading::Load` and Git read cancellation.

## 1. Measure the current path

- Inspect `src/commit/preview.rs`, `src/commit/view.rs`, `src/commit/refresh.rs`, and Shell polling.
- Add an opt-in, read-only navigation profiler in `src/commit/preview/tests.rs`.
- Run `IGIT_PERF_WORKSPACE=/path/to/workspace python3 scripts/cargo_serial.py test --locked --lib commit::preview::tests::profile_navigation -- --ignored --nocapture`.
- Record per-frame navigation times and repository-state invariants; do not use the real workspace for ordinary regression tests.

## 2. Move preview work off navigation and rendering

- Add a debounced target and shared background loader in `src/commit/preview.rs`.
- Prepare both source data and `RevisionDiff` on the worker.
- Schedule from rendering/input without reading Git on the terminal thread in runtime mode.
- Show current file identity with a loading message instead of another file's preview.
- Cancel obsolete reads on selection, side, scope, refresh, and preview visibility changes.
- Preserve synchronous standalone Panel callers, matching other panel loading adapters.

## 3. Integrate refresh and event polling

- Enable background previews through `src/shell/refresh.rs`.
- Include the preview deadline and pending worker in `src/shell.rs` polling.
- Reuse prepared previews for background status refresh and preserve unchanged preview selection/scroll.
- Document runtime loading behavior and acceptance criteria in architecture, interaction design, and PRD.

## 4. Verify behavior and performance

- Add deterministic tests for burst coalescing, stalled reads, stale replies, side/scope changes, hidden previews, and read-only mixed staging.
- Start with one focused test, then run the affected Commit/Shell tests and format/Clippy checks as warranted.
- Repeat the opt-in profiler against the requested workspace and compare with the baseline.
- Build both application binaries with `python3 scripts/cargo_serial.py build --locked --bins`; report their absolute paths.

## Observations

- `draw_preview` calls `ensure_preview`, which synchronously executes multiple Git reads.
- `set_preview` constructs the aligned revision viewer on the terminal thread.
- Shell already batches consecutive tree motion events; batching does not prevent the next draw from blocking on a file.
- The working tree contains substantial existing changes; preserve them and keep unrelated work outside this change's commit.

## Local measurements

The opt-in profiler used the requested workspace with 40 changed files and 70
tree rows. Each mode processed the same 138 movement-and-draw frames at 180×50
using the debug build and Ratatui's TestBackend, including event dispatch and
preview polling. These figures exclude a terminal emulator's rendering cost.

| Mode | Total | Median | P95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Synchronous preview | 5887.14 ms | 60.40 ms | 90.36 ms | 140.70 ms |
| Background preview | 748.04 ms | 5.20 ms | 6.89 ms | 8.13 ms |

The final file comparison appeared 153.51 ms after stopping, including the 60 ms
settle interval. HEAD, the real index bytes, and the changed files' working bytes
were unchanged. Normal regression tests use disposable repositories; only the
explicitly invoked ignored profiler reads the supplied real workspace.

## Verification

- Commit regression run: 299 passed, two manual profiles ignored.
- Final focused Commit preview, Shell workspace, refresh, and startup run:
  60 passed, one manual profile ignored. This includes all eight new preview
  behavior tests and the background-preview Shell integration test.
- The focused preview tests cover navigation bursts, blocked workers, stale
  replies after revisiting a file, mixed staging, hidden Roots, pending search
  input, same-status refresh, unchanged viewer retention, and worker failure.
- Both `target-serial/debug/igit` and `target-serial/debug/ig` built successfully.
- A real PTY smoke test against `examples/demo-commit.sh` processed 50 `j/k`
  events at approximately 30 Hz, displayed the final split comparison, and quit
  cleanly. HEAD, real index bytes, and working files stayed unchanged.
- A second PTY run deliberately blocked preview `git ls-files` behind a private
  executable shim. Preview `p/f` stayed in context, another 50 paced movements
  remained usable, and hiding the preview and quitting completed while reads
  were blocked. The disposable repository's HEAD, index, and files were intact.
- Formatting passed for the modified Commit modules, Shell loading adapter, and
  preview tests using the serial wrapper with explicit files and
  `skip_children=true`.
- Production library Clippy passed with `--locked --lib -- -D warnings`.
- The shared workspace continued changing during verification. After the
  successful focused tests, all-target Clippy encountered new concurrent Fetch/
  Log callers of the private `Shell::refresh_allowed` method. Full formatting
  also reported concurrent Fetch/Push/update formatting differences. These
  broader checks need another run when that work is stable; do not remove,
  stub, or rewrite those unrelated changes to make this task's checks pass.

Before committing, the performance patch was exported as a standalone snapshot
against the existing HEAD, excluding concurrent workflow and automatic-fetch
changes. The isolated snapshot passed 76 focused Commit/Shell tests with one
manual profile ignored, full formatting checks, and all-target Clippy with
`-D warnings`. Mixed-staging coverage uses file navigation so the commit does not
depend on the concurrent explicit Staged/Unstaged controls.
