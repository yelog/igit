# Open Log Commits in a Browser Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Press `O` in a Log commit list to open the highlighted repository's full commit ID in the default browser.

**Architecture:** Log captures the selected repository path and immutable object ID in an action. Shell starts one background browser request, which resolves a fetch remote using local Git metadata, constructs the hosting service's commit URL, and invokes a native URL handler with separate arguments. Repository Git settings provide overrides for remote choice and custom hosting URLs; no network probes, Git writes, or new dependencies are required.

**Tech Stack:** Rust, system Git, Ratatui/Crossterm, existing configurable commands and notification center, native macOS/Linux/Windows URL handlers.

---

### Task 1: Resolve hosting URLs

**Files:**
- Create `src/core/hosting.rs` and `src/core/hosting/tests.rs`.
- Modify `src/core.rs`.

1. Add table-driven URL tests for GitHub, GitLab (including nested groups), Bitbucket Cloud/Data Center, Gitee, Gitea/Forgejo/Codeberg, and Azure DevOps. Cover HTTPS credentials, SSH/SCP clone URLs, ports, suffixes, and escaped/Unicode repository names.
2. Implement URL parsing and route mapping without executing remote commands or probing servers. Preserve HTTP(S) ports, discard SSH ports, remove credentials, and reject unsupported or malformed addresses without including the original URL in errors.
3. Add temporary-repository tests for current upstream/origin/single-remote selection, remote-tracking containment, ambiguous and absent remotes, `igit.webRemote`, `igit.webCommitUrl` with `{commit}`, and Git URL rewrites.
4. Implement remote resolution using read-only Git commands. Prefer remotes known to contain the commit, then the current upstream, origin, or the sole remaining remote. An explicit URL template takes precedence over remote discovery.
5. Run `python3 scripts/cargo_serial.py test --locked --lib core::hosting::`.

### Task 2: Add background browser dispatch

**Files:**
- Create `src/ui/browser.rs`.
- Modify `src/ui.rs` and `src/shell.rs`.

1. Add the `OpenCommit` action with a repository path and full object ID.
2. Add a background request adapter with duplicate-request protection and polled success/error results.
3. Invoke `open`, `xdg-open` / `gio open`, or `rundll32.exe` through argument arrays with detached standard streams. Never interpolate a URL into a shell command.
4. Add fake launcher tests for exact argument handling, nonzero exit status, missing commands, and asynchronous dispatch. Provide an isolated capture for shell tests so regression checks do not launch real browsers.
5. Run `python3 scripts/cargo_serial.py test --locked --lib ui::browser::`.

### Task 3: Integrate Log keys and help

**Files:**
- Modify `src/log.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, and `config/default.toml`.
- Modify `src/shell/configuration.rs` to retain one-shot behavior after remapping.
- Create `src/log/view_tests/browser.rs`; register it in `src/log/view_tests.rs`.
- Extend `src/shell/log_tests.rs`.

1. Bind configurable `keybindings.log.open-in-browser` to `O`, restricted to the commit list, including file history and branch comparison lists.
2. Capture the highlighted row independently of preview data and cherry-pick checkboxes; validate current history and Roots.
3. Ignore the shortcut in text input, menus, diffs, other panes, key releases, and repeats. Keep lowercase `o` as view options.
4. Show the command in contextual help and the normal footer.
5. Test matching object IDs in two repositories, input/focus guards, custom keybindings, help execution, asynchronous failures, and unchanged HEAD/index/worktree.
6. Run the focused browser tests, followed by `python3 scripts/cargo_serial.py test --locked --lib log::` and `python3 scripts/cargo_serial.py test --locked --lib shell::log_tests`.

### Task 4: Document and verify

**Files:**
- Modify `README.md`, `docs/configuration.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md`.

1. Document `O`, supported services, deterministic remote choice, and the limitation that local remote-tracking refs do not prove server availability.
2. Document repository overrides, for example `git config --local igit.webCommitUrl 'https://forge.example/team/repo/-/commit/{commit}'`, including SSH aliases, custom web ports, and paths.
3. Run `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
4. Review the final diff against pre-existing workspace changes. Do not commit or push.

## Execution results

Implemented on 2026-11-11 without adding dependencies, committing, or pushing.

- Passed: `core::hosting::` (7 tests), `log::` (219), `shell::log_tests` (8),
  and `ui::browser::` (4): 238 relevant tests.
- Passed: formatting checks for all Rust files changed by this task, and
  `git diff --check`.
- Passed: all-target Clippy with the existing `obfuscated-if-else` and
  `needless-borrows-for-generic-args` warnings allowed. The standard
  `-D warnings` invocation remains blocked by the notification prefix hint
  in `src/shell.rs` and `src/branch/tests/smart_checkout.rs`, respectively.
- Extended configuration tests: 25 passed, 1 failed.
  `configuration_hunk_view_remaps_and_help_review_keeps_git_unchanged_on_cancel`
  navigates with `Home Down`, which now selects the repository container
  introduced by the pre-existing Commit grouping changes.
- The workspace-wide format check also reports differences in concurrently
  changed `src/push/loading/tests.rs` and `src/shell/push_tests.rs`. These
  unrelated files were left untouched.
- Browser launches use isolated captures and fake commands in regression
  tests. No real browser, external hosting service, or user repository was
  used for those tests.
