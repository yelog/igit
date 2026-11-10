# IDEA Diff Change Highlights Implementation Plan

> **For Codex:** Implement the following steps in this session. Preserve existing workspace changes; do not commit or push.

**Goal:** Give the shared two- and three-pane diff renderer distinct added-line, modified-line, and inserted-fragment highlights using the existing palette.

**Architecture:** Keep Git comparison sources, merge decisions, chunk layout, selection, and scrolling intact. Derive and cache display-only line comparisons for each visible revision, then use directional token operations to distinguish replacements from insertions. Preserve red pending-conflict highlights and neutral completed source panes.

**Tech Stack:** Rust, Ratatui TestBackend, the existing `similar` dependency, system Git regression fixtures.

---

1. Add regression cases for added lines, replacements, inline additions, mixed replacement/insertion blocks, three-way parent/result changes, pending conflicts, and completed decisions. Assert rendered line-number, code, trailing-space, and fragment backgrounds.
2. Extract the existing tint definitions into `src/merge/view/highlight.rs`. Add bounded line pairing and directional fragment classification. Pure additions use the green line and gutter palette; modified lines use the blue line and gutter palette, with green reserved for extra inserted fragments.
3. Use those highlights from `src/merge/view/rows.rs`, preserving Unicode graphemes, tabs, horizontal clipping, syntax colors, gutters, and the inner-diff setting. Keep presentation logic separate from `src/merge/engine.rs`.
4. Verify read-only two- and three-pane rendering, narrow/zoomed views, and live merge state transitions. Run the focused renderer checks, full Rust suite, formatting, Clippy, and whitespace checks.
5. Update the existing README, interaction design, architecture, and PRD descriptions without replacing unrelated in-progress edits.

Line and fragment analysis must stay bounded for large/generated input. Text selection and search overlays retain their existing priority over diff backgrounds. No new production dependency or Git write path is needed.

## Verification status

- The original focused regression run reproduced six failures. The first fix
  passed all eight rendering cases, and the expanded suite passed 41 view tests.
- The Chinese comment regression passed in the 42-test run. That run also exposed
  a short comparison that lost its fragments when descheduled. Small comparisons
  now use a work limit so their results are independent of scheduling delays.
- The final `cargo test --locked --lib merge::view:: -- --test-threads=2`
  passed all 42 tests. `cargo clippy --locked --all-targets -- -D warnings`
  also passed.
- The full `cargo test --locked -- --test-threads=4` run passed 813 tests,
  with one ignored. `cargo build --locked --bin igit` succeeded.
- `cargo fmt --all --check`, `git diff --check`, and relative documentation links
  passed on the final changes.
