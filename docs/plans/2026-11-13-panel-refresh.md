# Panel Refresh Implementation Plan

**Goal:** Make runtime refresh responsive, retain valid browsing state, and recover from stale previews and failed background work.

**Architecture:** Preserve synchronous initialization and direct Panel callers while enabling background reads in the production Shell. Shell records invalidations until reviews, editors, and gestures allow them to be consumed. Readers retain their previous successful snapshot within the same Roots and reject results from obsolete generations.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, standard threads and mpsc.

---

### Task 1: Repair Commit preview and state recovery

**Files:** `src/commit.rs`, `src/commit/preview.rs`, `src/commit/refresh.rs`, their existing test modules.

1. Add regressions for hiding a preview, changing bytes without changing XY, scanning, and reopening the same file.
2. Invalidate the affected preview even when its hidden row identity and Git status remain unchanged.
3. Keep unapplied repositories dirty when Add supersedes an in-flight scan.
4. Recover from unreadable or malformed saved state without disabling repository loading; preserve the original state before any later save.
5. Avoid rebuilding the Changes tree when neither its data nor its scope changed.

**Validation:** `python3 scripts/cargo_serial.py test --locked --lib commit::`

### Task 2: Repair Push retry and worker recovery

**Files:** `src/push.rs`, `src/push/loading.rs`, `src/push/preview.rs`, their existing test modules.

1. Add a regression for a preview failure followed by an unchanged successful list refresh.
2. Distinguish failed previews from reusable successful snapshots and retry failed previews on explicit refresh.
3. Handle a disconnected operation worker before Done, preserving reported successes and making the remaining outcomes explicitly uncertain.
4. Clear busy state and require a fresh review before retrying unfinished work.

**Validation:** `python3 scripts/cargo_serial.py test --locked --lib push::`

### Task 3: Queue runtime list refreshes

**Files:** `src/stash.rs`, `src/stash/loading.rs`, `src/remote.rs`, `src/remote/loading.rs`, existing panel tests.

1. Add opt-in background loading using the existing Load and Scope types.
2. Preserve synchronous refresh semantics for direct callers and existing initialization.
3. Retain successful rows on same-scope read failure; clear obsolete-scope data immediately.
4. Install results using current cursor/fold choices and invalidate reviews before writes.
5. Expose polling, pending-read state, and safe refresh guards to Shell.

**Validation:** focused Stash and Remote library tests.

### Task 4: Queue conflict-list refreshes

**Files:** `src/merge.rs`, `src/merge/list.rs`, `src/merge/loading.rs`, existing Merge tests.

1. Read conflict lists on a worker in production and preserve synchronous direct callers.
2. Preserve known conflict rows on repository read failure.
3. Retain merge drafts and mark an open document stale if its conflict sources change.
4. Continue revalidating every Apply and repository operation before writing.

**Validation:** focused Merge library tests.

### Task 5: Unify Shell invalidation and refresh completion

**Files:** `src/shell.rs`, `src/shell/startup.rs`, `src/shell/refresh.rs`, existing Shell tests.

1. Record pending invalidations for Log, Branch, Push, Stash, Remote, and Conflicts.
2. Queue production Commit refresh instead of synchronously scanning and refreshing every branch label.
3. Process invalidations when the owning panel has no review, editor, key sequence, selection, or drag that must remain stable.
4. Refresh cached Log, Push, and Conflicts after terminal focus returns; retain deferred work across overlays.
5. Poll list readers without blocking navigation and retain lazy initialization of unopened Log.
6. Report refresh started while work is queued rather than claiming immediate completion.

**Validation:** Shell refresh/focus/review regressions, then all library tests.

### Task 6: Document and verify behavior

**Files:** `docs/prd.md`, `docs/design.md`, `docs/architecture.md`, `README.md` where applicable.

1. Document asynchronous runtime refresh, cached-state behavior, deferred focus refresh, and retry semantics.
2. Run `python3 scripts/cargo_serial.py fmt --all --check`.
3. Run the relevant focused tests before expanding to `python3 scripts/cargo_serial.py test --locked --lib`.
4. Run `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
5. Review the complete diff for scope, stale writes, mixed staging, hidden Roots, and preservation of drafts and selections.

No commit, push, branch creation, or modification of `target/` is part of this work.
