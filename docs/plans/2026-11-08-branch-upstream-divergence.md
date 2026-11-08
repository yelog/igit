# Branch Upstream Divergence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show each local branch's outgoing and incoming commit counts beside its name, following the existing Log arrows and the supplied Branch reference.

**Architecture:** Read `%(upstream:track,nobracket)` in the existing background ref scan and cache counts as display metadata, separate from reviewed branch snapshots. Reserve terminal cells for the colored counts before clipping branch labels; include them in upstream-column measurements and selected-row details.

**Tech Stack:** Rust, Ratatui, system Git, and TestBackend with disposable repositories.

---

### Task 1: Read and retain tracking counts

**Files:** `src/branch/model.rs`, `src/branch/loading.rs`, `src/branch/tree.rs`, `src/branch/tests.rs`, `src/branch/tests/tracking.rs`.

1. Add real-Git fixtures for ahead-only, behind-only, diverged, synchronized, untracked, and missing-upstream branches. Include a differently named upstream and another repository.
2. Run `cargo test --locked --lib branch::tests::tracking`; before implementation, the rendered rows lack the expected counts.
3. Read six NUL-delimited ref fields, including optional tracking metadata. Parse `ahead N` and `behind N` into a map keyed by full local ref names. Preserve worktree paths and omit remote symbolic aliases.
4. Keep the tracking query in background display reads. Operation snapshots retain their existing fields and do not request commit counts.
5. Verify refresh replaces cached counts and that browsing preserves HEAD, refs, the real index, and working changes.

### Task 2: Render compact colored counts

**Files:** `src/branch/rows.rs`, `src/branch/view.rs`, `src/branch/tests/tracking.rs`.

1. Append green `↗N` and blue `↙N` spans to local branch names, using the existing icon-mode fallbacks. Omit zero counts and unavailable upstreams.
2. Reserve complete counter widths before clipping names and optional status text. Account for counts in both grouped and flat upstream-column measurements, including folded branches.
3. Include the same counters in selected-row details so they remain available with long names and narrow panes.
4. Verify all three icon modes at 120×36, 80×25, and 44×12, with grouped/flat Unicode names, stable upstream alignment, row colors, and clicks.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document arrow direction, colors, hidden zero counts, local remote-tracking refs, and Fetch/F5 refresh behavior.
2. Run `cargo test --locked --lib branch::` and `cargo test --locked --lib shell::branch_tests`; expect all tests to pass.
3. Run `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`; expect no formatting or lint failures.
4. Run `cargo build --locked --bins` to update both runnable debug binaries. Start the built executables in a terminal and verify tracking counters; library tests and Clippy do not refresh these binaries.
5. Record the final validation results here. Leave the implementation uncommitted for review.

### Validation results

- `cargo test --locked --lib branch`: 60 passed, including all 27 Branch tests,
  the Shell Branch integration tests, and related branch behavior checks.
- The four tracking tests cover actual Git counts, background/display snapshot
  equivalence, refresh, repository isolation, mixed staged/unstaged content,
  all icon modes and supported sizes, mouse identity, long Unicode names,
  selected-row details, and upstream alignment across folds/grouping.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --locked --bins`: passed; both `target/debug/ig` and
  `target/debug/igit` contain the tracking-counter implementation.
- Executable verification: launched both binaries with `.` in pseudo-terminals,
  switched from Commit to Branch, and confirmed the current workspace's two
  outgoing commits in Nerd Font, Unicode, and ASCII modes at 120×36. All six
  launches displayed the expected count and exited normally. HEAD, refs, the
  real index, working diff, and status were unchanged.
- `git diff --check`: passed.
