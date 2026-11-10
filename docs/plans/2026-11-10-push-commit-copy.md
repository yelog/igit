# Push Commit Copy Implementation Plan

**Goal:** Support Log's `yy`, `ym`, and `ya` on the focused outgoing commit in Push.

**Architecture:** Share commit-copy value retrieval and the existing prefix picker with Log. Push owns focus and prefix lifetime; Shell retains clipboard delivery and cancellation. Read immutable repository/OID data independently of asynchronous previews, fetching message or author only when requested.

**Tech Stack:** Rust, system Git, Ratatui, existing clipboard capture and serialized Cargo runner.

### Task 1: Reproduce missing copying

**Files:** `src/push/copy_tests.rs`, `src/push.rs`.

Add temporary-repository tests for full IDs, multiline Unicode messages, author identity, multiple repositories, pending previews, empty/repository rows, cancellation, and unchanged Git/checkbox state. Run one focused failing test with `python3 scripts/cargo_serial.py test --locked --lib push::copy_tests::push_commit_copy_uses_selected_repository_without_preview`.

### Task 2: Share copying and integrate Push

**Files:** `src/ui/commit_copy.rs`, `src/ui.rs`, `src/log/navigation.rs`, `src/push/commit_copy.rs`, `src/push.rs`, `src/ui/key_sequence.rs`, `src/shell.rs`, `src/config/keys.rs`, `config/default.toml`, `src/push/keymap.rs`.

Keep Log's cached author values and existing message semantics. Push reads author from the selected commit on demand. Register Push commands for remapping and expose help only on commit rows. Reuse the prefix picker and cancel on focus, mouse, overlay, and refresh changes.

### Task 3: Verify integration and document

**Files:** `src/shell/key_sequence_tests.rs`, `src/shell/configuration_tests.rs`, `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

Verify delayed hints, completion/cancellation, remapping, and existing file-copy behavior through Shell clipboard capture. Run focused Push, Log navigation, Shell sequence/configuration tests, then format and Clippy via `python3 scripts/cargo_serial.py`. Keep existing uncommitted changes and do not commit or push.
