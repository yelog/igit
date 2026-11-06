# Branch Background Loading Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Paint Branch immediately, even when Git takes seconds to list the refs in Roots.

**Architecture:** Tab entry queues a read, and the shell polls a bounded worker outside input and rendering. Share Push's generation-based loader so refreshes supersede earlier reads without starting competing batches. Prepare the snapshots, initial rows, and upstream column width off the terminal thread. Keep same-scope rows available for browsing while refreshing, and continue to validate all reviewed writes against Git.

**Tech Stack:** Rust standard-library threads and channels, system Git, Ratatui TestBackend.

---

### Task 1: Capture the entry regression

**Files:** `src/shell/branch_tests.rs`

1. Enter Branch with `4` and draw before polling background work. Assert that Branch is active and displays `Loading branches…` at the supported terminal sizes. Verify Help and switching to Commit still work.
2. Run `cargo test --locked --lib branch_entry_paints_before_git`. The original synchronous refresh paints the branch rows before this assertion and fails it.

### Task 2: Separate loading from browsing

**Files:** `src/core/loading.rs`, `src/core.rs`, `src/push/loading.rs`, `src/branch/loading.rs`, `src/branch.rs`, `src/branch/tree.rs`, `src/branch/rows.rs`, `src/branch/view.rs`, `src/shell.rs`

1. Move the one-batch loader, scope identity, and bounded repository reads from Push to shared code. Preserve Push's generation and retry behavior.
2. Queue Branch reads on entry, F5, terminal focus return, and known ref changes. Coalesce rapid refreshes and ignore superseded results. Invalidate reads for hidden Branch when Git writes or Roots changes make them stale.
3. Build snapshots and rows in the worker. Install current results using the latest cursor and fold choices. Keep cached same-scope rows while refreshing and gate new branch actions until the snapshot is current. Keep branch review and confirmation semantics unchanged.

### Task 3: Verify refresh behavior and document it

**Files:** Branch, Shell, and loader tests; `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`

1. Hold a read open while switching away and back, then change Roots and release the old result. Verify that only the new Roots are installed. Cover cached navigation, write gating, and ref refresh after an operation.
2. Document initial loading, cached refresh, and scope handling. Run the focused Branch and Push tests, then `cargo test --locked`, formatting, and Clippy checks.
