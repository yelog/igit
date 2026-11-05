# IDEA Commit Tool Window Implementation Plan

**Goal:** Match the supplied IDEA screenshot so that the default commit list directly shows groups, repositories, filenames, and parent paths. Actual commits still use only checked, reviewed content within visible repositories.

**Architecture:** Change tree projection, rendering, and event hit testing in the existing native Ratatui panel. Continue identifying files by repository and raw path. Make the directory tree, container summaries, file Diff, and per-repository messages available on demand, reusing the existing Git backend.

1. `src/commit/tree.rs`: implement a flat list sorted by filename while preserving directory mode. Cover duplicate filenames, collapsing, view switching, and independent checkbox state.
2. `src/commit/rows.rs`, `src/commit/view.rs`: align checkboxes, file-type icons, status colors, parent paths, repository color blocks, and branch badges. Distinguish expansion, checking, and browsing using actual rendered coordinates.
3. `src/commit.rs`, `src/shell.rs`: hide the preview by default and connect toolbar and keyboard actions. Preserve mouse access to patches, message editing, Amend, and commit review.
4. Reproduce 11 tracked changes and 8 untracked files in temporary repositories. Check the list, Diff, selection, and review at 120×36 and 80×24. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`; update actual screenshots and usage documentation.
