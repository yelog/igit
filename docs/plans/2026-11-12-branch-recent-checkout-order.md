# Branch Recent Checkout Order Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show the current local branch first and the previously used branch next by default, immediately after a verified checkout without a full Branch reload.

**Architecture:** Reuse the existing worktree HEAD reflog parser and Recently Used tree ordering. Read updated usage on the checkout worker, carry it with the verified checkout result, and merge it into only the target repository cache before rebuilding visible rows. Keep explicit Name sorting, grouping, folds, cursor identity, and remote ordering.

**Tech Stack:** Rust, Ratatui, system Git, disposable repository tests.

---

### Task 1: Reproduce default and cached ordering

**Files:**
- Modify: `src/branch/tests/sorting.rs`

1. Add a default-order test with `main`, `develop`, and `client-plugin-batch-update` at the same commit. Seed switch times and expect current, previous, then older/unknown branches.
2. Add consecutive background checkout tests without calling `reload` between switches. Cover grouped and flat trees, same-second events, newly created tracking refs, explicit Name sorting, and another repository's unchanged cache.
3. Run `python3 scripts/cargo_serial.py test --locked --lib branch::tests::sorting`. Confirm the new assertions fail on the existing default and stale usage cache.

### Task 2: Update verified checkout usage

**Files:**
- Modify: `src/branch/tree.rs`
- Modify: `src/branch/recent.rs`
- Modify: `src/branch/model.rs`
- Modify: `src/branch.rs`
- Modify: `src/branch/operations.rs`
- Modify: `src/branch/loading.rs`

1. Make `Sort::RecentlyUsed` the default while keeping `Sort::Name` selectable.
2. Allow the existing reflog reader to use known reference names, including a newly created tracking ref, without reading all refs again.
3. Read usage only after successful checkout verification, on the existing worker.
4. Carry usage in `CheckoutDone` and install it in the matching repository before projecting its tree.
5. Keep failed checkout and unrelated repositories from receiving usage updates; retain the existing safe fallback.
6. Re-run the focused sorting tests and confirm no full Branch read is queued.

### Task 3: Document, regress, and rebuild

**Files:**
- Modify: `README.md`
- Modify: `docs/design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/prd.md`

1. Document the default, immediate local usage update, explicit Name choice, and unchanged Last Commit semantics.
2. Run `python3 scripts/cargo_serial.py test --locked --lib branch::`.
3. Run `python3 scripts/cargo_serial.py test --locked --lib shell::branch`.
4. Run `python3 scripts/cargo_serial.py fmt --all --check` and `git diff --check`.
5. Run `python3 scripts/cargo_serial.py build --locked --bins` and verify the rebuilt executable's version.

Implementation stays in the current session. Do not commit, publish, or switch branches in the user's everyday repositories.
