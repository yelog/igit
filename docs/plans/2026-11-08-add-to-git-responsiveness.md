# Add to Git Responsiveness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Add to Git acknowledge input immediately, reduce Git subprocess overhead, keep browsing responsive, and show truthful progress and completion without a confirmation dialog.

**Architecture:** Capture exact untracked row members on the UI thread, then prepare, stage, and read affected repository statuses on one background worker. Publish results through the existing Shell polling loop, update only affected Commit rows, and discard stale refresh reads. Reuse existing Git snapshot/index safeguards while batching untracked paths into each private-index add. Inline progress uses the current palette and reserved footer space; repeated Add requests and conflicting writes are blocked while browsing, Help, resizing, and draft editing remain usable.

**Tech Stack:** Rust, standard threads and mpsc, Ratatui/Crossterm, system Git, disposable repositories.

---

### Task 1: Measure the existing path

**Files:** `src/commit/backend/add/tests.rs`.

1. Add an ignored manual benchmark for 6 and 60 untracked files with assertions for exact staged paths and preserved worktree contents.
2. Run `cargo test --locked add_to_git_profile_batch -- --ignored --nocapture --test-threads=1` and record preparation, index publication, and status-read timings with Git trace command counts.
3. Inspect `src/shell.rs`, `src/commit/refresh.rs`, and `src/commit/backend.rs` to distinguish write cost from unnecessary full-workspace refresh cost.

### Task 2: Reduce batch staging overhead

**Files:** `src/commit/backend/add.rs`, `src/commit/backend/working.rs`, `src/commit/backend/add/tests.rs`.

1. Batch exact untracked paths into each private-index Git add using NUL-delimited literal paths.
2. Retain working fingerprints, repeated content/status checks, index locking, Git conversion checks, and rejection of implicit changes to unselected paths.
3. Run `cargo test --locked commit::backend::add` and repeat the manual benchmark to compare timings and Git process counts.

### Task 3: Keep the terminal responsive

**Files:** `src/commit/add.rs`, `src/commit.rs`, `src/commit/refresh.rs`, `src/shell.rs`.

1. Add one pending Add job with concrete repository/path identity, progress events, and per-repository results. Enqueue work before any Git read and return a status notice immediately.
2. Poll the job even when another tab or Help is active. Make completion update affected Commit statuses without triggering the synchronous global refresh.
3. Preserve selection/drafts, reject duplicate Add and overlapping write operations, and reject stale pre-add refresh results. Keep the operation running safely across tab changes; do not claim Git writes were canceled.
4. Update panel and Shell tests to wait for asynchronous completion and verify unchanged HEAD, unrelated staging, working files, hidden Roots, and retry scope.

### Task 4: Present clear operation feedback

**Files:** `src/commit/view.rs`, `src/commit/keymap.rs`, `src/shell/workspace.rs`, `src/commit/add/tests.rs`, `src/shell/configuration_tests.rs`.

1. Show an immediate inline `Adding N files…` state with an animated indicator, current repository/phase, and actual completed repository counts where space permits.
2. Keep progress anchored in existing footer space with no layout jump. Use readable compact text at 44 columns and the existing ASCII/icon fallbacks.
3. Show a concise persistent result with success/failure counts and actionable errors. Use no modal, artificial delay, optimistic staging state, or estimated percentage.
4. Use a deliberately delayed Git filter to verify that the first frame, browsing, duplicate suppression, Help, narrow layouts, completion, and failure recovery work while Git is still running.

### Task 5: Document and validate

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document the latency diagnosis, final interaction, affected-repository refresh, and safe busy-state behavior.
2. Run targeted Add, Commit refresh/execution, and Shell tests, formatting, and Clippy. Broaden to the full suite when integration requires it.
3. Record measured timings and actual responsiveness evidence. No commit, push, or publication is part of this task.

## Measurements and interaction decisions

The initial path ran preparation, index writes, and a full Shell refresh on the
terminal thread. The 6/60-file manual benchmark isolates preparation, index
publication, and one status read; it excludes the former full-workspace refresh.
Measurements use disposable local repositories on this Mac, without a model or
network endpoint, and are examples rather than latency guarantees for arbitrary
repositories or configured filters.

| Untracked files | Before | After | Reduction |
| --- | --- | --- | --- |
| 6 | 1,355 ms | 939 ms | 31% |
| 60 | 6,310 ms | 1,837 ms | 71% |

Across both benchmark cases, Git trace recorded `hash-object` calls decreasing
from 330 to 10 and `add` calls from 134 to 6, including fixed fixture setup.
The remaining raw-content reads are batched with bounded native argv sizes;
private-index additions use NUL-delimited literal paths. Existing repeated
validation, conversion checks, exact index reconciliation, and locking remain.

The operation acknowledges input before Git work and uses the existing Changes
footer plus the workspace status line. The rotating indicator, `Adding N files`,
Checking/Staging/Updating phase, current repository, and completed repository
count describe real state. Compact terminals retain the complete file count.
There is no confirmation modal, estimated percentage, optimistic file color,
or reserved delay. Successful additions move only after actual Git results are
read, while failures stay available for retry and full details remain in Console.

A deliberately blocked clean filter verified the initial Shell frame in
6,363 microseconds. While the filter was still blocked, browsing, Help, tab
switching, and draft editing worked; repeated Add requests and overlapping
writes were refused. The worker completes independently of the visible tab.
Panel layout checks cover 44×12, 80×24, and 120×36 in all icon modes, with write
buttons disabled and no confirmation window.

## Final verification

- `cargo test --locked -- --test-threads=4`: 647 unit tests and 10 integration tests passed; the one ignored manual performance test was run separately and passed.
- After hardening Help so a former busy action cannot become a new Add when the worker finishes, `cargo test --locked add_to_git -- --test-threads=2`: 15 tests passed, with the manual benchmark excluded as intended.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`: passed.
- A native binary in a 120×36 ASCII pseudo-terminal emitted the first `Adding 1 file` status in 2.4 ms. The blocked Git filter left the live index unchanged while the UI continued animating, browsing, handling repeated Add input, and opening Help. Completion added only the captured file and preserved HEAD, unrelated index entries, untracked siblings, and working-tree edits. This measures terminal output, not physical display refresh latency.
- Relative links in the updated product/design/architecture/configuration documentation were checked successfully.
