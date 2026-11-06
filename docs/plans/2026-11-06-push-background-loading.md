# Push Background Loading Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Switching to Push must render immediately, independently of repository count or Git latency, while preserving reviewed push safety.

**Architecture:** Separate background reads from the existing remote-operation worker. Keep the last outgoing snapshot visible during refresh, use at most four concurrent repository readers, and coalesce replacement requests. Load selected files and patches separately; only the latest request may update the UI. Preserve selection by repository and commit OID, retain checkboxes, and require a current outgoing snapshot before Push/Fetch.

**Tech Stack:** Rust, standard threads/mpsc/atomics, system Git, Ratatui TestBackend. No new dependencies.

---

## Evidence and constraints

- `Shell::go(Push)` calls `PushPanel::refresh` on the event thread.
- Refresh performs a serial `entry` read for every scoped repository, then synchronous `inspect` and possibly a patch read.
- A read-only sample of the supplied moss-web workspace found 11 repositories, 110 Git processes and 1.122 seconds in entry reads alone (warm local run; not a performance guarantee).
- Reuse existing thread/channel conventions from Log and Stash. Keep read loading distinct from `PushPanel::busy`, which guards remote writes and quitting.
- Keep existing uncommitted work. Do not commit, push, fetch, or write to the supplied workspace. Regression tests use temporary repositories and local bare remotes.

## Task 1: Prove responsive loading and request isolation

**Files:** Create `src/push/loading.rs`, `src/push/loading/tests.rs`; modify `src/shell/workspace_tests.rs`.

1. Add deterministic channel-controlled tests: delayed reads must leave navigation/quit available; Push/Fetch must not start while refreshing; stale results after Roots/target changes must be discarded.
2. Add tests for one running batch, coalesced requests, disconnected-worker recovery, retained checks/reseed, and selected commit identity after history grows.
3. Run `cargo test --locked push::loading` and verify the initial missing implementation fails.
4. Implement request generations and bounded repository reads. Poll results from Shell; use the existing short polling interval while reads are pending.
5. Rerun the targeted tests; expected result: all pass.

## Task 2: Remove Git work from preview completion and reduce subprocesses

**Files:** Modify `src/push.rs`; create `src/push/preview.rs`; modify `src/push/view.rs`.

1. Extract file-list and patch reads into background requests. Coalesce navigation and reject stale responses. Keep grouping and checked repositories independent of preview completion.
2. Render `Loading outgoing commits…` / `Refreshing outgoing commits…` and file/patch loading states, including narrow layouts. Disable remote actions while the outgoing snapshot is refreshing or failed, without disabling close/navigation.
3. Read relevant branch/push configuration with one NUL-delimited Git query. Preserve pushRemote → pushDefault → upstream → origin/first-remote precedence and skip `git log` when the outgoing count is zero.
4. Test target precedence, selected-path/commit identity, stale preview completion, and no writes to HEAD, index, working files, or remote refs.
5. Update existing Push tests to explicitly await reads; do not hide asynchronous behavior inside production code or rendering.

## Task 3: Verify integration and document behavior

**Files:** Modify `src/shell.rs`, relevant shell tests, `README.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md`.

1. Run `cargo test --locked push::`, then relevant shell tests. Confirm Commit and Push reseeds the successful repository set and keeps reviews separate; retry still excludes successful/hidden repositories.
2. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Exercise the 11-repository `examples/demo-push.sh` workspace with a pseudo-terminal. Measure first Push frame and full load, repeat tab entry, and confirm cancel/quit remain responsive. Compare repository snapshots before/after browsing.
4. Document actual measurements, checks, and remaining synchronous boundaries. Preserve review-time source/URL checks, remote leases, and submodule safety.

Execution stays in this authorized session; no commit or separate agent is required.

## Verification record (2026-11-06)

- Read-only equivalent entry queries against the supplied 11-root moss-web workspace, three alternating local runs, excluding preview/rendering: before 963/966/933ms with 110 Git processes; after 187/176/174ms with 67 processes. Median 963ms → 176ms. These are local measurements, not timing guarantees.
- Debug-binary pseudo-terminal walkthrough of `examples/demo-push.sh` (11 roots), ASCII mode: first Push frame 3.48ms; complete outgoing/file load 205.69ms. Returning to Push: first frame 3.82ms, complete refresh 186.58ms. Review was opened and canceled; 120×36, 80×24, and 44×12 retained controls.
- With a test wrapper adding 200ms to each Git invocation: first frame 3.47ms, complete load 5202.25ms; cached reentry 5.10ms and switching back to Commit during loading 5.71ms. Quit completed during read loading. The wrapper observed at most four simultaneous Git processes in this walkthrough.
- HEADs, index entries, staged/unstaged diffs, local refs and all local bare-remote refs matched before/after the terminal walkthrough.
- Regression coverage includes obsolete Roots/targets/files/patches, coalesced reads, disconnected reader recovery, target precedence, OID-based cursor retention (including hiding other Roots), reseeding, cached Diff scroll, loading mouse controls in all icon modes, and existing push/submodule write-safety tests.
- `cargo test --locked`: 416 unit tests and 8 integration tests passed. `cargo fmt --all --check`, `git diff --check`, and `cargo clippy --locked --all-targets -- -D warnings` passed.
