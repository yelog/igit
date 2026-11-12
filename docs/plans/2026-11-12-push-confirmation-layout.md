# Push Confirmation Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give the Push panel's confirmation the compact layout and visual hierarchy of Branch Push.

**Architecture:** Extract Branch's existing review frame into a shared internal renderer. Present Push's reviewed jobs with colored From/To refs, secondary commit and repository details, explicit upstream choices and update options. Preserve the existing confirmation event handler, scope checks, and Git execution.

**Tech Stack:** Rust, Ratatui, Crossterm, TextLayout, TestBackend, disposable repositories and local bare remotes. No new dependencies.

---

## Task 1: Verify the requested layout

**Files:**
- Modify `src/shell/push_tests.rs`.

1. Add a focused regression for compact height, colored refs, dimmed background, separated right-aligned controls, and default Confirm focus.
2. Exercise all icon modes at 44×12, 80×24, 120×36, and 184×48.
3. Cover scrolling, long Unicode names, Merge/Rebase text, tracking choices, and cancellation without writes.
4. Run `python3 scripts/cargo_serial.py test --locked --lib push_review_layout`; expect the original generic layout to fail.

## Task 2: Reuse the Branch review frame

**Files:**
- Create `src/ui/review.rs` and `src/push/review.rs`.
- Modify `src/ui.rs`, `src/branch/operations/view.rs`, `src/push.rs`, and `src/shell.rs`.

1. Move Branch's existing sizing, wrapping, dimming, border, scrollbar, close control, and fixed footer into the shared renderer.
2. Reuse that renderer from Branch and Push while retaining their existing keyboard shortcuts.
3. Render only reviewed Push jobs with green local refs, purple remote targets, tinted repository names, muted commit IDs and paths, and an amber Merge/Rebase strategy.
4. Preserve upstream changes, hooks/tags, conflict handling, separate repository outcomes, and no-force-push disclosures.
5. Route Push confirmation rendering through its panel before the generic fallback.

## Task 3: Document and validate

**Files:**
- Modify `README.md`, `docs/design.md`, and `docs/architecture.md`.

1. Document the compact shared layout, scrollable content, and fixed controls.
2. Run focused Push confirmation and existing Branch review regressions using `scripts/cargo_serial.py`.
3. Run the relevant Push tests, formatting check, library Clippy, and `git diff --check`.
4. Preserve existing workspace edits; do not commit, publish, or use everyday repositories for write tests.

## Verification record (2026-11-12)

- The initial `push_review_layout` regression reproduced the generic dialog's missing From/To hierarchy.
- `python3 scripts/cargo_serial.py test --locked --lib push_review -- --quiet`: 9 passed.
- `python3 scripts/cargo_serial.py test --locked --lib branch_merge_review -- --quiet`: 3 passed.
- `python3 scripts/cargo_serial.py test --locked --lib branch_pull_review -- --quiet`: 3 passed.
- `python3 scripts/cargo_serial.py test --locked --lib push::view_tests:: -- --quiet`: 19 passed.
- `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py build --locked --bin igit` passed.
- All-target Clippy stopped at the existing `nonminimal_bool` warning in `src/commit/view.rs:109`; that unrelated file was not changed by this task.
- Inspected a real 160×42 PTY render using `examples/demo-push.sh`. Canceling preserved HEAD, staged and working patches in all 11 disposable repositories, plus refs in all 11 local bare remotes.
- Tests cover three icon modes, four normal terminal sizes, multi-repository scope, upstream changes, Merge/Rebase, hooks/tags, long Unicode content, scrolling/resizing, button focus, mouse cancellation, and Enter publication against a local bare remote.
