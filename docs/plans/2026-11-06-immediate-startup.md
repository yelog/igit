# Immediate Startup Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Paint the TUI before discovering repositories or loading Git data, and keep startup responsive in large multi-repository workspaces.

**Architecture:** Render a loading workspace before starting a standard-thread worker. The worker discovers repositories and initializes Commit and Conflicts; the main thread keeps handling resize and cancellation. Load the first Log history on demand in a separate worker and invalidate pending results on workspace refresh. Request progressive keyboard flags without waiting for a terminal query response.

**Tech Stack:** Existing Rust, Ratatui, Crossterm, system Git, standard threads and mpsc. No new dependencies.

---

## Evidence and scope

- `shell::run` currently calls `Context::discover` and `Shell::with_config` before entering the alternate screen. `with_config` refreshes branches a second time, then Commit, Log and Conflicts synchronously.
- A read-only sample of moss-web found 11 repositories: discovery 422 ms, duplicate branch reads 88 ms, Commit 166 ms, Log 425 ms, Conflicts 180 ms.
- Crossterm 0.28.1's keyboard capability probe can wait 2 seconds. The existing debug executable first painted after 3,751 ms in a PTY that did not answer that probe.
- Keep the existing discovery rules, repository identity, review/write behavior, saved drafts, configuration and synchronous refresh of already loaded panels. Preserve the user's existing uncommitted changes. No commits or publication are authorized.

## Task 1: Nonblocking startup

Files: `src/shell.rs`, new `src/shell/startup.rs`, new `src/shell/startup_tests.rs`.

1. Separate Shell's configuration-only constructor from refresh.
2. Draw a workspace loading frame before starting Git work. Report discovery, changes and conflict scan stages through an mpsc channel.
3. Poll results without blocking terminal input; Esc, q and Ctrl-C cancel startup. Restore terminal modes on cancellation and errors. Keep `--list-roots` as a synchronous plain-output CLI.
4. Initialize Commit and Conflicts in the worker without re-reading freshly discovered branch labels or loading Log.
5. Replace the synchronous keyboard probe with a balanced, best-effort flag push/pop; retain ordinary input on platforms without support.
6. Cover stage rendering, pending input, errors, worker disconnect and real disposable-workspace initialization.

## Task 2: First Log load

Files: `src/shell.rs`, `src/shell/startup.rs`, `src/shell/workspace.rs`, `src/shell/startup_tests.rs`.

1. Track dormant, pending and ready Log initialization.
2. Start initial history loading when Log is first selected. Show a loading message and keep workspace navigation/quit available; withhold panel actions until its data is ready.
3. Install results on the main thread. A refresh or scope change must discard the old receiver before another load so stale repository indices cannot be installed.
4. Cover lazy initialization, switching back to Commit, pending action guards, refresh invalidation and error recovery.

## Task 3: Verification and documentation

Files: `README.md`, `docs/architecture.md`, `docs/design.md`, `docs/prd.md`.

1. Run `cargo test --locked shell::startup_tests --lib` for the new behavior.
2. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings` for shared-shell regressions.
3. Build both debug entry points. Re-measure process-to-first-frame and Commit-ready times with the same PTY and isolated app state; benchmark the user-specified workspace only with read-only operations.
4. Exercise deliberately slow Git, startup cancellation, invalid paths, empty workspaces and Log switching in disposable repositories. Check working tree/index/HEAD preservation.
5. Document startup/loading behavior and the remaining synchronous refresh boundaries. Record observed timings, not universal guarantees.

## Observed results

- Added seven startup tests covering pending discovery/rendering/cancellation,
  worker errors, empty/invalid workspaces, multi-root state preservation,
  conflict visibility, lazy Log loading/navigation, stale scope results, and retry.
- `cargo test --locked --quiet`: 323 library tests and 6 integration tests passed
  on the final workspace state. `cargo fmt --all --check`,
  `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`
  passed. Both debug entry points were rebuilt with `cargo build --locked --bins`.
- A PTY smoke check held Git behind a gate: the loading frame appeared in 10.3 ms
  before Git was released, resize and cancellation worked, and terminal modes were
  restored. Holding the first Log load likewise allowed returning to Commit and
  quitting. Normal startup → Commit → loaded Log, invalid-path cleanup, and plain
  `--list-roots` output passed. Two disposable repositories retained byte-identical
  indexes, working files, and HEADs.
- The before/after debug executables were each run three times against the same
  11-repository moss-web workspace in a 120×36 PTY, with an empty settings file,
  ASCII icons, isolated app state, `GIT_OPTIONAL_LOCKS=0`, and no keyboard-query
  response. Process-to-first-frame median fell from **3,241.6 ms to 10.2 ms**;
  Commit-ready median fell from **3,240.9 ms to 825.8 ms**. The first run of the
  rebuilt executable painted in 479.0 ms; the following runs took 9.5 and 10.2 ms.
  These are local observations, not cross-platform timing guarantees.
- Relative links in the five touched Markdown documents were checked.
