# LazyDB Update Center Parity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Match LazyDB's Update Center layout, controls, and motion while using IGit's native updater.

**Architecture:** Keep the update dialog's palette and geometry local to the software update overlay. The existing worker observes the archive and stage markers in its private helper directory, so the installers can keep their single JSON result and existing transaction boundaries.

**Tech Stack:** Rust, Ratatui, Crossterm, Python 3.9+, Windows PowerShell, curl.

---

### Task 1: Match the dialog and controls

**Files:** `src/shell/software_update.rs`, `src/shell/software_update/view.rs`, `src/shell/software_update/tests.rs`.

1. Use the reference's 64-cell width, rounded cyan frame, dark surface, semantic status line, and content-based height. At 80×24, an up-to-date report uses a 64×7 dialog and one centered `[ > OK ]` button.
2. Derive labels, focus, and hit targets from the same available actions. Keep the safe default for Update and Restart, show external instructions on demand, and keep long details scrollable at 44×12.
3. Verify the status, geometry, colors, and mouse targets with Ratatui TestBackend. Keep the existing tests for preserving Git and editing state.

### Task 2: Connect the installation and animations

**Files:** `src/installation/inspection.rs`, `src/installation/progress.rs`, `scripts/install/install.py`, `pages/install.ps1`, `src/shell.rs`, `src/shell/software_update.rs`, `src/shell/software_update/view.rs`.

1. Give the embedded install helper a private progress directory on demand. Observe stage markers and archive size; use the final successful HTTP response's `Content-Length` for percentage when it is trustworthy. Preserve the CLI JSON report, curl restrictions, checksum, and activation rules.
2. Consume progress separately from the final worker result. Render a 160 ms foreground fade on open and a 100 ms moving segment for work without a known total. Stop animation redraws when the overlay is hidden or the job ends.
3. Verify a real partial download, an unknown size, phase transitions, and the terminal result with a local HTTP fixture. Keep the installer regression fixtures for failure and rollback.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

Run `cargo test --locked software_update --lib`, the installer fixtures, `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. Use a temporary installation and repositories for the terminal smoke test.
