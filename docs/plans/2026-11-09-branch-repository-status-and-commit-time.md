# Branch Repository Status and Commit Time Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show the current branch and its upstream divergence in every Branch repository row, and show the tip commit time in the branch tree.

**Architecture:** Reuse the background repository snapshots for current branches and tracking counts. Add the tip's formatted committer time to the display-only ref metadata, and lay out both additions by terminal cell width.

**Tech Stack:** Rust, Ratatui, system Git, and TestBackend with disposable repositories.

---

### Task 1: Verify repository rows and branch tip times

**Files:** `src/branch/tests.rs`, `src/branch/tests/tracking.rs`, `src/branch/tests/presentation.rs`.

1. Extend the real-Git tracking fixture assertions to check each repository row before and after switching repositories and refreshing a changed upstream. Verify the existing arrow colors and row click targets at 120×36, 80×25, and 44×12.
2. Create different dated tips for a local branch and its upstream. Read their expected times with `git show -s --format=%cd '--date=format-local:%Y/%m/%d, %H:%M' <ref>` and check the rendered **Last Commit** column against them. Check that folder rows have no time and a refresh updates the changed tip.
3. Run `cargo test --locked --lib branch::tests::`. The new assertions should fail until the repository labels and time column are rendered.

### Task 2: Project and render cached metadata

**Files:** `src/branch/model.rs`, `src/branch/rows.rs`, `src/branch/view.rs`.

1. Append `%(committerdate:format-local:%Y/%m/%d, %H:%M)%00` to the display scan's NUL-delimited fields. Store nonempty dates by full ref alongside tracking counts. Leave reviewed operation snapshots unchanged.
2. Right-align the current branch label and shared tracking spans in the repository row. Reserve cells for whole counts and clip names by terminal width. Show detached HEAD or the unborn branch name without counters.
3. Reserve 17 cells for the timestamp, a divider, and padding when the branch pane can still give names at least 28 cells. Align the heading and dates across local and remote rows. Keep the existing Upstream column and its narrow-pane details fallback.
4. Rerun `cargo test --locked --lib branch::tests::`; expect the repository, time, and existing navigation checks to pass.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document the new columns, local upstream refs and time zone, and narrow-pane behavior.
2. Run `cargo test --locked --lib branch`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked --bins`; expect success.
3. Open Branch in a disposable workspace to check both columns with the built application. Keep the working tree changes available for review.

### Validation results

- The new presentation assertions failed as expected before implementation.
  `cargo test --locked --lib branch` passed all 67 related tests afterward.
- Formatting, Clippy, and the `igit` / `ig` build passed.
- In a disposable workspace, `igit` at 120×36 and `ig` at 80×25 showed the
  correct repository counters and local/remote commit times. At 44×12 the
  counters and branch list remained visible while the time column was hidden.
