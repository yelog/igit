# Log Filter Access Keys Implementation Plan

**Goal:** Reveal Branch, User, Date, and Paths access keys in the toolbar immediately after Space, without covering commit history.

**Architecture:** Log renders mnemonic initials using the existing pending prefix and exposes visible label rectangles. The shared sequence picker uses those rectangles for inline candidate focus and retains its existing overlay when any category initial is unavailable. Existing filter actions, mouse targets, cancellation, and configured prefix routing remain authoritative.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend.

## Implementation

1. In `src/log/view.rs`, style only B/U/D/P with accent, bold, and underline while Space is pending. Preserve label positions, active values, and available clear targets.
2. In `src/ui/key_sequence.rs`, add inline candidate rendering using existing selection and arrow handling. In `src/shell/workspace.rs`, choose inline hints when all four category initials fit and supply a concise status hint; otherwise retain the delayed overlay.
3. In `src/shell/key_sequence_tests.rs`, verify immediate styles, unchanged layout, direct suffixes, arrow/Enter navigation, cancellation, mouse clear behavior, active Unicode values, icon modes, narrow layouts, and fallback.
4. Update README, interaction design, and PRD with the implemented behavior.

## Verification

- Start with `python3 scripts/cargo_serial.py test --locked --lib log_space_filters`.
- Run `python3 scripts/cargo_serial.py test --locked --lib key_sequence`.
- Run `python3 scripts/cargo_serial.py test --locked --lib log::view_tests`.
- Check formatting with `python3 scripts/cargo_serial.py fmt --all --check`.
- Inspect the final diff without committing or changing unrelated work.
