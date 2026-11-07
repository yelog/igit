# Software Update Center Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show IGit's running version at the bottom left and provide LazyDB-style automatic checks and a keyboard/mouse update center.

**Architecture:** Keep software updates separate from workspace Pull/Fetch. Reuse the embedded installers for manifest validation, version comparison, installation ownership, checksum verification, and activation. A Shell-owned worker reports results without blocking the terminal. Successful checks use an installation-specific cache in the global state directory.

**Tech Stack:** Existing Rust, Ratatui, Crossterm, serde, tempfile, Python/PowerShell installer helpers; no new production dependency.

---

### Task 1: Reuse installer inspections

**Files:** `src/installation.rs`, `src/installation/inspection.rs`, `scripts/install/install.py`, `pages/install.ps1`, `scripts/install/test_install.py`, `scripts/install/test-windows.ps1`.

1. Add typed check/install reports and read-only unmanaged-version inspection; retain the public CLI's existing ownership behavior.
2. Preserve the installation channel, isolate installer environment variables, and pin installation to the reviewed version.
3. Cover malformed reports, unmanaged read-only checks, native ownership, and changed manifests with local fixtures.

### Task 2: Add automatic checks and cache

**Files:** `src/config.rs`, `config/default.toml`, `src/shell/software_update.rs`, `src/shell/software_update/cache.rs`.

1. Add `[updates]` defaults: `check_on_startup = true`, `check_interval_hours = 24`; reject zero/overflowing intervals.
2. Start a background check 1.5 seconds after the first workspace frame. Cache successful results by executable/installation identity; ignore invalid, stale, future, or mismatched caches.
3. Keep automatic failures quiet, allow manual retry, serialize checks/installs, and keep an install running when its dialog closes.

### Task 3: Wire the footer and update dialog

**Files:** `src/shell.rs`, `src/shell/workspace.rs`, `src/shell/software_update/view.rs`, `src/config/keys.rs`, `src/ui/help.rs`, `src/shell/software_update/tests.rs`.

1. Render `IGit v<VERSION>` with Checking / Update / Updating / Restart / failure status and clipped click geometry.
2. Open Update Center with F9, the version/status click target, or Actions. Keep text inputs and existing reviews intact.
3. Show running/target versions and channel, default to Close, and share button actions between keyboard and mouse. Install only on explicit activation; offer restart after activation.
4. Cover narrow layouts, all states, help, remapping, mouse boundaries, draft/Git preservation, and asynchronous completion using fixtures rather than real releases.

### Task 4: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, `docs/configuration.md`, `docs/installation.md`.

1. Document the footer, F9, cache/settings, managed versus unmanaged updates, and reviewed installation/restart behavior.
2. Run targeted tests, then `cargo test --locked`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and installer fixture tests.
3. Smoke-test the temporary practice workspace at 44×12, 80×24, and 120×36. Do not update the developer's installation, contact real models, publish releases, commit, or push.

## Verification results

- `cargo test --locked --quiet`: 514 library tests and 8 integration tests passed.
- `cargo test --locked software_update --lib --quiet`: all 9 Update Center tests passed after the final compact-hint adjustment.
- `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings` passed.
- Installer fixtures: 13 Python tests passed with Python 3.9; release/Pages fixtures: 28 tests passed with Python 3.11.
- Local HTTP fixtures and native-installation PTYs passed at 44×12, 80×24, and 120×36: delayed check, cache reuse across workspaces, version clicks, F9, default Close, clicking Install, closing during installation, restart with original arguments, literal/Unicode installation paths, and unchanged practice-repository HEAD/index/worktree.
- Windows PowerShell contract cases were added but could not run on this macOS host because PowerShell is unavailable. Windows runtime verification remains for CI.
