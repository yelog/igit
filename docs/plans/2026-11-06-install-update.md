# Installation and Update Implementation Plan

**Goal:** Provide verified native installation and update commands for IGit's five release targets through `igit.yelog.org`.

**Architecture:** GitHub Pages serves standalone Unix and PowerShell installers plus stable/beta manifests assembled from published GitHub Releases. Native installations keep immutable version directories and switch an activation pointer only after checksum, archive, and executable validation. Both Rust entry points delegate `update` to the same embedded platform installer, preserving installation ownership and channel; application configuration and repositories are never modified.

**Tech Stack:** Existing Rust/stdlib/tempfile, Python 3 stdlib and curl on Unix, PowerShell 5.1/7 on Windows, GitHub Actions/Pages. No new Rust production dependencies.

---

## Scope and coordination

- Develop in `codex/install-update` at `/Users/yelog/workspace/tui/igit-install-update`, based on `20448ee`.
- Preserve concurrent Commit/Log changes in the original worktree. Integrate only this task's patch after validation.
- Public endpoints: `/install.sh`, `/install-beta.sh`, `/install.ps1`, `/install.ps1.txt`, `/channels/stable.json`, `/channels/beta.json`.
- Commands: `igit update [--check] [--channel stable|beta] [--json]`, with identical `ig` support. Explicit downgrade requires `--allow-downgrade`.
- First published beta predates the built-in update command; reinstalling through the script remains the bootstrap update path until a release includes this implementation.
- No Homebrew/tap changes, uninstall UI, background update checks, or replacement of published release assets.
- Preparing this implementation does not itself authorize a new software release or inclusion of concurrent changes in a commit.

## Task 1: Unix installation engine

**Files:** `scripts/install/install.py`, generated `install.sh` / `install-beta.sh`, `scripts/install/test_install.py`.

1. Add fixture archives/manifests and tests for channel identity, literal paths, corrupted checksums, archive traversal/links, executable version mismatch, foreign launchers, read-only checks, and rollback before activation.
2. Implement strict manifest and asset validation, bounded HTTPS download, safe regular-file extraction, and staged checks of both commands.
3. Record native ownership separately from application configuration. Keep releases immutable, serialize writes with an owned lock, and atomically switch the current release.
4. Add optional shell PATH setup and actionable diagnostics for unavailable channels and unsupported targets.
5. Run `python3 -m unittest discover -s scripts/install -p 'test_*.py'` and an isolated installation of the published beta.

## Task 2: Windows installation engine

**Files:** `pages/install.ps1`, `scripts/install/test-windows.ps1`.

1. Match the Unix manifest, channel, ownership, checksum, and version contracts.
2. Use immutable version directories and owned `.cmd` launchers so updating never overwrites a running executable. Atomically replace the active-version pointer.
3. Validate ZIP members before extraction, keep paths literal, and add the installation directory to the user PATH unless explicitly disabled.
4. Test with temporary directories and fixture executables under Windows PowerShell and PowerShell 7; never mutate the developer's PATH in tests.

## Task 3: Built-in update command

**Files:** `src/installation.rs`, `src/lib.rs`, `tests/update_cli.rs`.

1. Dispatch `update` before TUI/configuration initialization and expose concise command help.
2. Resolve the running executable's native installation; refuse mutation of Cargo, manual, unknown, or altered installations.
3. Run the embedded platform engine with structured arguments; preserve stdout for JSON, stderr for progress, and failures as nonzero exits.
4. Test both commands, invalid arguments, configuration independence, and unmanaged installation behavior.

## Task 4: Pages and CI

**Files:** `scripts/release/pages.py`, `scripts/release/test_pages.py`, `pages/CNAME`, `pages/index.html`, `.github/workflows/pages.yml`, `.github/workflows/ci.yml`, `scripts/install/smoke.py`.

1. Select the highest published version independently for each channel; validate all five asset names, URLs, and checksums. Do not invent a stable release.
2. Generate standalone piped shell scripts from one Python engine, copy the PowerShell text endpoint, and serve only the public allowlist.
3. Add pinned Pages actions after checking official revisions, with serialized deployments after Release success and installer changes.
4. Add installer/CLI tests and native package installation smoke checks to CI; online checks use isolated installation roots.

## Task 5: Documentation and integration

**Files:** `README.md`, `docs/installation.md`, `docs/releasing.md`, `docs/architecture.md`, `docs/prd.md`, `.agents/skills/release/SKILL.md`, `CHANGELOG.md`.

1. Document stable/beta commands, prerequisites, paths, channel switches, update checks, manual/Cargo ownership, and the first-beta bootstrap limitation.
2. Extend release verification to Pages manifests and online installation, without changing existing published tags.
3. Run Python suites, Rust formatting/Clippy/tests, workflow lint, native archive/install/update checks, and public manifest generation from the existing beta.
4. Apply only this task's changes to the original worktree, resolving documentation contexts without overwriting parallel edits.
5. Before publishing the new scripts, present the concrete diff and required repository Pages/DNS configuration. Commit/push or a new release needs explicit scope authorization under the user's AGENTS.md.
