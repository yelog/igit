# Commit Message Action Shortcuts Implementation Plan

**Goal:** Run Commit, Commit and Push, and Generate message with c/a/g only while Message has normal-mode focus, and reveal those keys inside the button labels.

**Architecture:** Keep the existing message editor and action dispatch. Route the three unmodified press events through `Hit::MessageAction`, register their Message-only command identities with configurable bindings, and derive visible mnemonics from those bindings. Render one accented, bold, underlined letter per available action while preserving button selection, hit areas, and editing input.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, local Git fixtures, local HTTP mocks.

## Implementation

1. Add regression coverage in `src/commit/view_tests/inline_message.rs`, `src/commit/message_tests.rs`, and `src/commit/ai/tests.rs` for focus-dependent rendering, narrow labels, normal typing, repeated keys, scoped commits, and message generation.
2. In `src/commit.rs`, associate c/a/g with the message actions and consume repeats without submitting. In `src/config/keys.rs` and `config/default.toml`, register Message-only commands and restrict the existing g navigation command to Changes/Preview.
3. In `src/commit/view.rs`, accent only the matching label character when Message is active and not editing. Keep compact labels recognizable and retain their mnemonic characters. Suppress hints for disabled or remapped defaults and unavailable operations.
4. Initialize mnemonic availability from application configuration in `src/shell.rs`; update contextual help in `src/commit/keymap.rs` and cover configuration routing in `src/shell/configuration_tests.rs`. Update the existing keyboard workflow regression in `src/shell/workspace_tests/inline_message.rs`.
5. Update README, interaction design, and PRD acceptance criteria with the implemented shortcuts and focus conditions.

## Verification

- Start with `python3 scripts/cargo_serial.py test --locked --lib message_action_shortcuts`.
- Run `python3 scripts/cargo_serial.py test --locked --lib inline_message`.
- Run `python3 scripts/cargo_serial.py test --locked --lib commit::` for the affected panel.
- Run focused configuration/help checks and `python3 scripts/cargo_serial.py fmt --all --check`.
- Review only this task's edits against the already modified working tree. Do not commit or change unrelated work.
