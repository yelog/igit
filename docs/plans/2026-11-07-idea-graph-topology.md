# IDEA Graph Topology Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Match IDEA's graph topology, row-by-row lane ordering, independent parent edges, branch colors, and long-edge arrows within terminal cell rendering.

**Architecture:** Replace fixed, shared rails with a permanent DFS layout and sorted visible row elements. Keep each real parent edge independent until its parent commit; connect its positions across adjacent rows. Derive compact and expanded layouts from the same topology, and render diagonal transitions without treating crossings as joins.

**Tech Stack:** Rust, Ratatui, existing Git readers; no new runtime dependencies.

---

### Task 1: Establish layout and edge regression fixtures

**Files:** `src/log/model/graph/tests.rs`, `src/log/model/tests.rs`, `src/log/view_tests.rs`.

1. Cover independent edges to a shared parent, DFS branch ordering, adjacent-row compaction, first-parent continuity, and repositories with identical object IDs.
2. Cover 29/30-row edges, endpoint arrow targets, expansion, filtering gaps, and wide/narrow viewports.
3. Run `cargo test --locked log::model::graph` and confirm the new acceptance cases expose the fixed-rail behavior.

### Task 2: Replace fixed rail allocation and projection

**Files:** `src/log/model/graph.rs`, `src/log/model/graph/layout.rs`, `src/log/model/graph/render.rs`.

1. Index parents by repository plus object ID; deduplicate parent edges and ignore missing/backward parents.
2. Rank branch tips using IDEA's Git reference order, then assign layout indices using depth-first ancestor traversal.
3. Sort each row's node and passing edges by their layout indices and endpoints. Never merge separate edges before a commit.
4. Compute compact and expanded projections using IDEA's 30/1000-row limits and 1/250-row visible ends; retain clickable endpoint arrows.
5. Draw transitions between adjacent row positions; preserve one viewport origin and explicit offscreen markers.

### Task 3: Match branch colors and integrate display behavior

**Files:** `src/log/model/graph/color.rs`, `src/log/model/graph/load.rs`, `src/log/model.rs`, `src/log/view.rs`, `src/log/model/tests.rs`, `src/log/view_tests.rs`.

1. Use reference-name/fragment identity for colors rather than allocating a new color per edge. Read/cache full lightweight parent/reference metadata so the 160-row page does not renumber older DFS fragments.
2. Make graph width and clipping follow the selected long-edge mode; keep hit areas synchronized with the rendered arrows.
3. Update previous tests that asserted shared rails or same-row rectangular connections; retain their underlying repository-isolation and navigation checks.

### Task 4: Verify and document

**Files:** `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`.

1. Run `cargo test --locked log::`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
2. Inspect deterministic dense-history renders and smoke-test the temporary practice workspace. No production repository writes or real models.
3. Document edge continuity, crossings, arrows, reference priority, and terminal rendering limits with primary-source links.

Reference algorithms: JetBrains `GraphLayoutBuilder`, `GraphElementComparatorByLayoutIndex`, `PrintElementGeneratorImpl`, `GitRefManager`, and `GraphColorManagerImpl` in [intellij-community](https://github.com/JetBrains/intellij-community). This task does not authorize commits or publication. Preserve existing Branch/Stash/update edits in the shared workspace.


## Verification notes

- The initial regression cases failed on shared parent rails, the 12-row threshold, and shared-rail shortening suppression.
- Read-only comparison against the user-provided workspace matched the first 37 identifiable screenshot node columns, nine rows of arrows (ten arrows), and seven sampled node colors. Full-history fragment ordering also restored the screenshot's branch color identities. The final production loader/layout passed the same comparison. No project metadata is stored in these regression fixtures.
- Terminal window inspection was unavailable because the computer-use tool refused Kitty access. Offscreen glyph inspection and deterministic Ratatui viewport/hit-area checks cover the cell renderer; they do not establish pixel-identical native terminal rendering.
- Concurrent changes in unrelated Log/Branch/Stash workflows were kept intact. An isolated copy of HEAD plus this task's changes provides independent validation when those edits are incomplete.
- Final shared-workspace checks passed: `cargo test --locked log:: --lib` (113 tests), `cargo check --locked --lib`, `cargo fmt --all --check`, and `git diff --check`. The isolated copy also passed 96 Log tests and all-target Clippy with warnings denied.
- Shared-workspace all-target Clippy was blocked by `cloned_ref_to_slice_refs` in the concurrently added `src/log/file_history/tests.rs:191`. That unrelated test was left intact.
- The isolated full-suite attempt did not pass: `commit::refresh::tests::native_events_during_a_scan_are_not_lost_and_fallback_covers_missing_notifications` failed while several other Commit tests waited on internal channels. The stalled run was interrupted. Those modules were unchanged by this graph task.

## Primary sources

- [DFS branch layout](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilder.kt)
- [Row element ordering](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphElementComparatorByLayoutIndex.java)
- [Long edges and arrows](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorImpl.kt)
- [Git reference priority](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/plugins/git4idea/backend/src/log/GitRefManager.kt)
- [Reference/fragment colors](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/platform/vcs-log/impl/src/com/intellij/vcs/log/graph/GraphColorManagerImpl.kt)
- [Default hue calculation](https://github.com/JetBrains/intellij-community/blob/f34889d85fbf6530771029c408a782fb62b597f8/platform/vcs-log/impl/src/com/intellij/vcs/log/graph/DefaultColorGenerator.kt)
