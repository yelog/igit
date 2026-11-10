# Branch Merge Review Implementation Plan

**Goal:** Make the branch merge direction easy to scan, focus Confirm on opening, and remove y/n activation from this review.

**Architecture:** Render the immutable Branch review with a dedicated Ratatui view, using the existing shared confirmation event handler and mouse hit IDs. Give merge its own internal review token so other branch confirmations retain their behavior. Keep Git validation and execution unchanged.

**Tech Stack:** Rust, Ratatui, Crossterm, existing TextLayout wrapping and TestBackend.

## Steps

1. Add a merge review renderer in `src/branch/operations/view.rs`: emphasize From/Into branches, use muted short commit IDs and repository path, wrap long names, size to content, and keep separated Cancel/Confirm controls visible.
2. Route `branch:merge` through `src/branch.rs` and `src/shell.rs`; default to Confirm, omit y/n from buttons and help, and retain Tab/Shift-Tab/arrows, Enter, Esc, and mouse behavior.
3. Extend `src/shell/branch_tests.rs` for default focus, inert y/n, cancellation without writes, Enter execution, mouse targets, and narrow/Unicode rendering.
4. Update README and interaction/acceptance documentation. Run focused Branch merge tests with `python3 scripts/cargo_serial.py test --locked --lib branch_merge`, then relevant Branch regressions and formatting checks. Do not commit or publish.
