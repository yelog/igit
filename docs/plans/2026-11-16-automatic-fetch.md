# Automatic Fetch Implementation Plan

**Goal:** Keep remote information current in the terminal workspace without blocking local work, changing reviewed operations, or contacting repositories outside Roots.

**Architecture:** Shell owns one background fetch scheduler and per-target freshness. A bounded, cancellable Git backend discovers eligible remotes off the terminal thread, validates safe refspecs and repository identity, and fetches either remote-tracking branches or an exact push endpoint. Existing panels retain their local refresh and review models; fetch completion invalidates remote-dependent views.

**Tech Stack:** Rust, system Git, existing process controls, standard threads and channels, Ratatui, TOML.

Implementation runs in the current workspace, retaining existing uncommitted work. No commits or remote publication are part of this task.

## 1. Configuration and scheduling policy

Files: `src/config.rs`, `config/default.toml`, `docs/configuration.md`.

- Add `[fetch]` with `enabled = true`, `interval_seconds = 300`, `freshness_seconds = 60`, `timeout_seconds = 120`, `all_remotes = false`, and `excluded_remotes = []`.
- Validate bounded positive intervals and timeout, freshness no greater than the interval, and remote exclusion names.
- Test defaults, partial overrides, invalid settings, and disabled operation.
- Startup waits two seconds after runtime activation. Focus return and remote-dependent tab entry request stale data; F5 and file events remain local.

## 2. Safe background Git backend

Files: `src/core/fetch.rs`, `src/core/fetch/tests.rs`, `src/core.rs`, and process controls only where necessary.

- Discover repository/common-directory identity, remote configuration, safe fetch refspecs, and effective endpoints on a worker.
- Default to the current upstream remote, then origin or the sole remote; optionally include all remotes. Respect exclusions and `skipFetchAll`.
- Reject refspecs that update local branches or tags. Preserve configuration, HEAD, index, working files, tags, and FETCH_HEAD.
- Disable recursive submodule fetch, tag pruning, and automatic maintenance. Capture and revalidate endpoint/configuration identity.
- Apply timeout, bounded output, cancellation, and silent credential/SSH behavior. Return redacted diagnostics and distinguish authentication failures.
- Fetch explicit push endpoints without confusing their object IDs with the ordinary fetch endpoint.
- Test with disposable local remotes, mixed index/worktree contents, unsafe refspecs, changed endpoints, linked worktrees, failures, and cancellation.

## 3. Shell scheduler and foreground coordination

Files: `src/shell/fetch.rs`, `src/shell/fetch/tests.rs`, `src/shell.rs`, `src/shell/startup.rs`, `src/shell/rescan.rs`.

- Coalesce requests by common Git directory, remote, endpoint/configuration, and optional push branch.
- Run at most two independent fetches, serializing shared storage and staggering starts.
- Retain per-target successful timestamps, last attempt/error, bounded retry backoff, and authentication suspension.
- Pause launches when unfocused or while reviews, workspace replacement, and Git writes own the workspace. Cancel running background work before foreground mutations; do not block quit on automatic work.
- Remove obsolete scoped requests and reject stale worker results.
- Keep test constructors inert until explicitly activated; tests must never fetch users' repositories.
- Route successful manual fetch observations through the same freshness state without claiming unrelated refs are current.

## 4. Push target cache and status

Files: `src/push.rs`, `src/push/loading.rs`, `src/push/target.rs`, `src/push/view.rs`, `src/shell/workspace.rs`.

- Expose scoped push target requests and apply exact endpoint results to outgoing calculations.
- Retain captured endpoint identity so cached tips cannot be reused after remote configuration changes.
- Display fetching, successful age, unknown, disabled, and failed/cached status in existing workspace/Push surfaces.
- Preserve browsing position, checked repositories, drafts, and unchanged previews; delay view replacement during editing and selection.
- Keep execution-time target fetching, source/configuration validation, fast-forward checks, and explicit OID leases.

## 5. Pull target isolation

Files: `src/shell/update.rs`, `src/branch/sync.rs`, and their existing test modules.

- Resolve and fetch an immutable upstream OID for integration rather than rereading a shared tracking ref after fetch.
- Preserve current fast-forward or reviewed Merge/Rebase behavior and existing path protection.
- Validate that background activity and endpoint changes cannot silently change an integration target.

## 6. Verification and documentation

Files: `README.md`, `docs/prd.md`, `docs/design.md`, `docs/architecture.md`.

- Document automatic network access, default timing/scope, explicit opt-out, cached counts, failure handling, shared worktrees, and manual/automatic differences.
- Run focused backend and scheduler tests first, using `python3 scripts/cargo_serial.py test --locked --lib <filter>`.
- Run relevant Push, Pull, configuration, and Shell regressions through the same wrapper.
- Run `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
- Build both executables with `python3 scripts/cargo_serial.py build --locked --bins`.
- Inspect terminal output at wide and narrow sizes using disposable repositories. Verify repository bytes and refs after automatic fetch; preserve the existing `target/` directory.

## Verification record — 2026-11-16

- Fetch, Push, Pull, configuration, startup, and refresh regression selection: **222 passed**. This includes different fetch/push endpoints, real automatic Push cache installation, shared worktrees, excluded Roots, cancellation, timeout, immutable Pull targets, and retained drafts/previews.
- The initial complete library run reported **1,413 passed, 2 failed, 2 ignored**. The failures exposed explicit Push target changes retaining an obsolete Diff and initial Log loading waiting behind Help. Both were fixed and passed in the 222-test rerun; the complete suite was not repeated after those fixes.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings` passed.
- `python3 scripts/cargo_serial.py fmt --all --check` passed.
- `python3 scripts/cargo_serial.py build --locked --bins` built both debug executables.
- PTY smoke checks launched `igit` at 80×36 and `ig` at 120×36 with disposable repositories and local bare remotes. Automatic fetch completed after the runtime startup delay, freshness appeared, and both exited successfully. HEAD, exact index bytes, staged/unstaged working content, tags, and FETCH_HEAD remained unchanged.
- Relative file links in the six changed documents and `git diff --check` passed. Real SSH/HTTPS authentication and Windows execution were not exercised.
