# Pull and Fetch Upstream Divergence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show the current branch's ahead/behind counts in Pull and Fetch reviews, retain Tab/Enter button activation, and remove repository counts from action labels.

**Architecture:** Read the current branch and its configured upstream from local Git refs while building review rows. Keep this display metadata separate from executable plans, and share Branch's arrow/color rendering through the existing UI helpers. Reserve counter space before clipping names; retain complete comparison labels in Details. Existing modal dispatch owns Tab/Shift-Tab and Enter.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, disposable Git repositories.

---

### Task 1: Verify divergence and button behavior

**Files:**
- Modify: `src/shell/update/view_tests.rs`
- Modify: `src/shell/update_tests.rs`

1. Add disposable repositories with ahead-only, behind-only, diverged, synchronized, missing, and differently named upstreams. Verify both reviews use only locally available refs and preserve HEAD, index, working files, and refs on cancel.
2. Verify counter colors, long-name clipping, narrow layouts, and click targets in all icon modes.
3. Exercise Tab, Shift-Tab, and Enter through Shell, including cancellation, empty scope, and confirmed Fetch/Pull. Verify only the focused action runs.
4. Change action-label assertions from `Pull N` / `Fetch N` to `Pull` / `Fetch` while retaining counts in the review summary.

Run: `cargo test --locked shell::update`.

### Task 2: Render local upstream comparisons

**Files:**
- Modify: `src/shell/update.rs`
- Modify: `src/shell/update/view.rs`
- Modify: `src/ui/idea.rs`
- Modify: `src/branch/rows.rs`

1. Move the existing tracking-arrow span renderer into the shared UI helpers and reuse it from Branch.
2. Capture current branch, actual configured upstream, and available ahead/behind counts in update display rows without adding metadata to write-plan equality.
3. Show counters beside branch names in both reviews. Preserve their space when names clip, and show the complete upstream comparison in scrollable Details. Zero/unavailable counts stay hidden.
4. Use plain `Pull` / `Fetch` action labels. Focus the available primary action initially, using a blue background for the focused button and a neutral background for the other button. Keep disabled actions unreachable, with Cancel focused when no repository is ready; retain modal Tab/Shift-Tab/Enter behavior.

Run: `cargo test --locked shell::update` and `cargo test --locked branch::tests::tracking`.

### Task 3: Document and validate

**Files:**
- Modify: `README.md`
- Modify: `docs/design.md`
- Modify: `docs/prd.md`
- Modify: `docs/architecture.md`

1. Document current-upstream counters, their local-ref basis, and plain action labels.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Inspect rendered reviews at 44×12, 80×24, 120×36, and 180×36; report actual validation results. No commit or remote publication is part of this task.

## Initial Validation Results

- `cargo test --locked shell::update`: 12 tests passed.
- `cargo test --locked`: 547 unit tests and 8 integration tests passed, including Branch tracking regressions.
- `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings`: passed.
- Ratatui layout checks passed at all four listed sizes in Nerd Font, Unicode, and ASCII modes; long branch names retain both counter colors and button hit areas remain separate.
- A real 120×36 pseudo-terminal opened both reviews against a disposable diverged repository. Both displayed `^1 v1` and plain action labels, redrew focus for Tab/Shift-Tab, and canceled with Enter without changing refs, index entries, or the working diff. The harness answered terminal cursor-position requests; startup update checks were disabled for this smoke check.
- Relative links in the updated documentation were checked successfully.
