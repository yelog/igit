# IGit: Project Handoff

IGit is a standalone terminal workspace for multiple Git repositories. `igit` and `ig` open the same application. Its interactions take inspiration from IntelliJ IDEA's Git workflow; the production application uses Ratatui / Crossterm and invokes system Git. A feature shown in the browser prototype is not evidence that it exists in the terminal application.

## What to Read First

| Task | Read first | Starting point |
| --- | --- | --- |
| Confirm scope, priorities, and acceptance criteria | [Product requirements](docs/prd.md) | Distinguish delivered behavior from pending acceptance; identify the repositories, files, and writes involved in a new requirement. |
| Change layouts, mouse hit areas, or Vim shortcuts | [Interaction design](docs/design.md) | Tab layout, header controls, and mouse routing live in [workspace.rs](src/shell/workspace.rs). See each panel's `draw` and `event` methods for internal behavior, and [shell.rs](src/shell.rs) for global shortcuts and Conflicts visibility. |
| Change Git commands, state, or operations across repositories | [Architecture](docs/architecture.md) | [core.rs](src/core.rs) owns repository discovery and shared Git calls; transaction details live in the individual panels. |
| Launch the app, try workflows, or configure AI | [README](README.md) | Use the local practice workspace; external repositories and model configuration are not required. |
| Prepare or publish a beta/stable release; change CI | [Release guide](docs/releasing.md) and [release skill](.agents/skills/release/SKILL.md) | `scripts/release/` owns metadata and archive checks; `.github/workflows/` tests and publishes the exact tagged commit. Editing release automation does not authorize publication. |

**Current handoff point (2026-11-06):** Commit opens a directory tree with Directory on, Module off, and preview enabled. Untracked files are expanded but unchecked. `t` toggles directory grouping; `v` toggles the preview (file → Diff, container → scope summary). Commit uses debounced file notifications to refresh affected repositories, with a 30-second Roots scan as fallback; drafts and checked paths are retained. See [refresh](src/commit/refresh.rs) and [watching](src/commit/refresh/watch.rs). Start with the [tree model](src/commit/tree.rs), [row rendering and hit areas](src/commit/rows.rs), [view](src/commit/view.rs), and [events](src/commit.rs). Use PRD A7 to check repository scope, and A9 plus the [practice repository](examples/demo-commit.sh) to check duplicate filenames, narrow terminals, and staged content.

## Behavior to Preserve

- The highlighted row is the preview target; checkboxes determine candidates; writes apply only to items within the current scope that the user has reviewed. Do not automatically select other repositories to complete a batch.
- When changing Commit, inspect the working tree, the real index, and the committed tree. In particular, test a file with both staged and unstaged changes alongside another staged file that is not selected.
- Multi-repository commits and pushes produce separate results, without cross-repository atomicity. Retrying failures must not repeat successful items; a parent push must not publish an unavailable submodule commit.
- Merge's Apply file and Continue repository operation require separate reviews. Preserve the current state when external changes or unsupported conflict types are detected.
- AI messages are editable candidates. Send selected patches to the configured endpoint only after user review; keep keys out of the repository. Routine regression testing must not use real models or users' repositories.
- Product-owned UI text, help, errors, examples, and documentation use English. Keep wording concise enough for terminal layouts; preserve Unicode support for user-provided names, paths, messages, and file content.

## Reproduction and Verification

Run from the repository root. The practice script creates a new temporary workspace with two ordinary repositories, one submodule, and local bare remotes, then prints the workspace path:

```sh
demo_workspace="$(sh examples/demo-workspace.sh)"
python3 scripts/cargo_serial.py run --locked --bin igit -- "$demo_workspace"
```

Interactive smoke check: start on Commit. Browse files and select groups with both the mouse and `j/k`, confirming that browsing does not change checkboxes. Write one shared message for two checked repositories, press `r` to hide one, then click Commit in the practice workspace. For the keyboard path, press `c` from Changes or Preview to edit, `Esc` to save, and `Enter` with Commit selected. Verify that only the visible repository received a commit and that restoring Roots restores the other repository's selection. When changing write operations, verify the practice repositories with `git status`, `git diff --cached`, `git log`, or remote refs.

Choose checks appropriate to the change: `python3 scripts/cargo_serial.py test --locked --lib commit::` targets Commit; use `python3 scripts/cargo_serial.py test --locked` for changes across panels or shared Git logic. Common pre-commit checks are `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`. For documentation-only changes, verify relative links, commands, and claims about the implementation.

Update the PRD when product scope or delivery status changes; update interaction design and README when focus, shortcuts, or panel behavior changes; update architecture when module responsibilities, persistence, or Git execution boundaries change. Historical prototypes remain design references, not runtime dependencies.

## Local Cargo execution (macOS/Linux)

Use `python3 scripts/cargo_serial.py <command> ...` for every local build, check, test, Clippy, format or run command. It queues whole commands behind a shared per-user lock, uses `target-serial/`, and forces one build job and one test thread. Do not run raw Cargo in another terminal/agent concurrently: Cargo's own build lock does not serialize running tests against another command. Interactive `run` holds the lock until the app exits; launch an already-built binary separately when needed.

For a general application rebuild, use `python3 scripts/cargo_serial.py build --locked --bins` and report completion only after both `igit` and `ig` build successfully in the requested profile and target. They are independent Cargo binary targets, even though both call the same library entry point. The wrapper automatically adds the companion target when `build` selects only one of them and prints the effective command. Focused `build --lib` remains focused; `run --bin`, tests, checks, and Clippy do not refresh both standalone executables. Report the absolute artifact paths so the user can launch the rebuilt commands; a local build does not replace installed commands.

Keep the existing `target/` intact for the macOS freeze investigation. Do not clean, delete, copy artifacts from it, override the new target directory, or raise concurrency without an explicit request. Start with one focused test; do not automatically run the full suite while investigating freezes. Native Cargo also defaults to `target-serial/`, one build job and one test thread, but bypasses the whole-command lock. CI explicitly retains `target/` for archive compatibility; the POSIX wrapper is not used on Windows.
