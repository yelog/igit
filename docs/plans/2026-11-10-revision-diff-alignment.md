# Revision Diff Alignment Implementation Plan

> Implement in the current workspace and preserve the existing work in progress.

**Goal:** Keep corresponding code and comment lines together in the read-only revision preview and make additions, deletions and empty sides easy to scan.

**Architecture:** Build presentation chunks from the same ordered line pairing used by both sides of a revision comparison. Prefer stable content anchors, compare the original lines for visibility, and keep the original text for search and copy. Use the shared row renderer for the projection and leave conflict resolution in the merge engine.

**Tech Stack:** Rust, `similar`, Ratatui TestBackend, and local synthetic fixtures.

---

1. Add a revision rendering regression in `src/merge/view/revision/tests.rs` with repeated documentation blocks, equivalent tab and space indentation, and real insertions and deletions. Assert corresponding screen rows, change count, neutral gaps, and source bytes. Run it against the current renderer first.
2. Build aligned chunks for `RevisionDiff` in `src/merge/view/revision/alignment.rs`. Reuse the bounded replacement pairing in `src/merge/view/highlight.rs` and preserve contiguous source slices for each pane. Combine contiguous changed sections for navigation.
3. Use the aligned rows for revision tint, gutter markers and selection projection. Give alignment padding a neutral background and reduce it to one boundary row when only one pane is visible. Keep significant whitespace changes visible and give equivalent indentation a quiet gutter mark.
4. Extend the display lexer for XML tags, attributes, quoted values and inline comments. Check both Java and XML with synthetic fixtures, including three revisions, folds, search, copy, and narrow layouts.
5. Update the existing Diff descriptions in README, design, architecture and PRD. Run focused view and preview tests, then the full suite, formatting, Clippy and the `igit` build. Use disposable fixtures for regression testing.
