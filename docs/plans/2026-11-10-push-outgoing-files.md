# Push Outgoing Files Implementation Plan

**Goal:** Make repository Changed files the union of paths changed by all outgoing commits, excluding remote-only history.

**Architecture:** Read the complete immutable target..HEAD range on the existing preview worker, with the same first-parent merge semantics as individual commit previews. Deduplicate native paths. Prefer cumulative changes from the merge base to HEAD; retain each remaining path's latest outgoing commit for historical comparisons when changes cancel out.

**Tech Stack:** Rust, system Git, existing Push tests and serialized Cargo runner.

### Task 1: Reproduce divergent history

**Files:** `src/push.rs`.

Add a temporary-repository regression with one local commit and two remote-only commits. Assert identical repository/commit file lists, correct Before/After contents, preserved checkboxes, and unchanged Git state. Run that test before implementation and confirm it fails.

### Task 2: Aggregate all outgoing changes

**Files:** `src/push/preview.rs`, `src/push/files.rs`, `src/push/view_tests.rs`.

Read all outgoing name-status records in one Git log command, preserving byte paths, rename sources, and commit provenance. Deduplicate paths and overlay cumulative statuses only for those paths. Share merge-base selection between file lists and Diff. Keep the real tracking tip for push validation. Cover repeated paths, reverted changes, renames, merges, new targets, and history beyond the 60 visible commits.

### Task 3: Verify and document

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

Document unique-path counts, cumulative comparisons, historical fallback, and full-range aggregation. Run the focused regression, then `python3 scripts/cargo_serial.py test --locked --lib push::`, format checks, and Clippy. User repositories are read-only evidence; all regression writes use temporary repositories.

### Verification results

- The divergence regression failed before implementation and passed afterward.
- All four `repository_preview_` regressions passed. The outgoing parser test covers byte paths and empty commits; the existing merge test now also checks repository aggregation.
- The final Push test run passed all 64 tests.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings` passed.
- `python3 scripts/cargo_serial.py fmt --all --check` reported only formatting in the concurrently added `src/push/copy_tests.rs` and its module declaration. Those independent changes were preserved.
