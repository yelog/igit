# Changelog

Release notes follow Keep a Changelog categories. Versions use the project's
decimal rollover policy described in [Releasing](docs/releasing.md).

## [0.1.0-beta.1] - 2026-11-06

### Added

- Open a multi-repository terminal workspace with either `igit` or `ig`, with Commit, Log, Push, and Conflicts tabs and support for initialized submodules and linked worktrees.
- Review scoped commits in a directory tree, organize file-level changelists, edit per-repository messages, amend commits, and review rollback operations while preserving unchecked staged content.
- Browse repository-colored history with branch, author, date, text, and literal path filters, file trees, ordinary revision diffs, and merge-parent comparisons.
- Review Push, Pull, and Fetch across the selected repositories, with separate results and checks that outgoing submodule commits are available from their declared sources.
- Resolve ordinary text conflicts in a three-way editor with safe automatic changes, explicit side choices, undo, and separate reviews for applying files and continuing Git operations.
- Use searchable contextual help, key-sequence hints, mouse selection and copying, configurable shortcuts, resizable panes, and optional Kitty pane integration.
- Choose Nerd Font, Unicode, or ASCII icons and keep Unicode support for repository names, paths, messages, and file content throughout the English interface.
- Optionally generate editable commit-message candidates from reviewed patches through a configured AI endpoint.
- Download archives containing both commands for macOS Intel and Apple Silicon, Linux x86_64 and ARM64, and Windows x86_64, with SHA-256 checksums and bundled configuration documentation.

### Internal

- Add responsive startup, isolated Git regression fixtures, and cross-platform filename and path coverage.
- Add pinned-toolchain CI and release automation that tests and packages the tagged commit before publishing verified assets.

### Commits

- [`d32b763`](https://github.com/yelog/igit/commit/d32b763d288ebf97b107ecce6c07bda1e3e6dccf) feat: add multi-repository Git terminal workbench
- [`a1b248e`](https://github.com/yelog/igit/commit/a1b248e05be4a880ae9f77b84ae56bc896ab0e47) feat(tui): add contextual help, text selection, and view controls
- [`3ca3491`](https://github.com/yelog/igit/commit/3ca3491dfa1af8555066b48d0f36a99a74933d2c) docs(log): outline multi-path selection for history filters
- [`a4c1eed`](https://github.com/yelog/igit/commit/a4c1eeda95981b2388f464d2561c650467b973da) feat(log): add multi-path filtering and simplify sidebar controls
- [`0e0c74b`](https://github.com/yelog/igit/commit/0e0c74bb11070dc264a6abd566c0f5fdba9d32bd) feat(tui): stack Commit and Log and open Push and Merge as dialogs
- [`6e99f8a`](https://github.com/yelog/igit/commit/6e99f8aa73d2284bbf68a657bbe3be677f1eb76f) feat(tui): add pane focus navigation and split resizing
- [`dd4b0ad`](https://github.com/yelog/igit/commit/dd4b0adf823089f4e0bded1736166ffb61cc08f8) feat(ui): expand common development file icons
- [`8d0d19c`](https://github.com/yelog/igit/commit/8d0d19c66fbd3a9fbe398ad034c4344ae0eff88f) feat(i18n): use English throughout IGit
- [`7a1be1f`](https://github.com/yelog/igit/commit/7a1be1ff19852bac75536390a1ba8ac207ad4189) feat(config): add custom keybindings and Kitty pane integration
- [`8173f3c`](https://github.com/yelog/igit/commit/8173f3c17b678c9e59f8da23c382ef64be4bdb7d) feat(push): streamline dialog layout and file presentation
- [`3fcbe30`](https://github.com/yelog/igit/commit/3fcbe30724e3370b84ba5a84ae8bb3dfcb515de3) feat(help): highlight search matches
- [`97becf7`](https://github.com/yelog/igit/commit/97becf7b5242311a28834f7a97a7b6e84a5eda31) feat(panes): add maximize and Kitty layout synchronization
- [`6235853`](https://github.com/yelog/igit/commit/6235853adbc2aaaf084160aa7f18888c14a0ce5c) feat(tui): add workspace updates and Log filter shortcuts
- [`4a04f2d`](https://github.com/yelog/igit/commit/4a04f2d6e8256d79674d1db813ba6d35f2e8e665) feat(commit): add rollback and inline message editing
- [`1885490`](https://github.com/yelog/igit/commit/18854909752327a6d334cb2c3f8a8b664124a95f) feat(log): use segmented repository color bars
- [`bb4c208`](https://github.com/yelog/igit/commit/bb4c2087414c01f64e0f3818375f953f6e63d460) feat: improve Git workspace and add release automation
- [`fde1f34`](https://github.com/yelog/igit/commit/fde1f345ebac509c79e83fe4051381a12e31ddba) test: make Git fixtures portable across CI platforms
- [`3ddd75e`](https://github.com/yelog/igit/commit/3ddd75eebec021b79d34a89313aea073687ea615) test: respect native separators in path filter assertions

## Unreleased

### Added

- Add native macOS, Linux, and Windows installers at `igit.yelog.org`, with separate stable and beta channels.
- Add `igit update` and `ig update` with read-only checks, JSON output, explicit channel changes, and verified installation ownership.
- Verify installation and updates in the five-platform CI matrix; deploy installer scripts and published release manifests through GitHub Pages.
