# Changelog

Release notes follow Keep a Changelog categories. Versions use the project's
decimal rollover policy described in [Releasing](docs/releasing.md).

## [0.1.0] - 2026-11-07

### Added

- Open a multi-repository terminal workspace with `igit` or `ig`, with Commit, Log, Push, Branch, Stash, and Conflicts panels and support for submodules and linked worktrees.
- Review scoped commits, rollback, Push, Pull, and Fetch with separate results per repository, preserved unchecked staging, shared message drafts, and optional reviewed AI message generation.
- Browse filtered history, file histories that follow renames, ordinary and merge-parent revision comparisons, and reviewed actions for creating branches or tags, checking out, undoing, reverting, dropping, or editing commits.
- Manage local and remote branches in separate repository and branch panes, with prefix grouping, background loading, and reviewed branch operations. Review stash Apply, Pop, and Drop against the selected entry.
- Resolve text conflicts in a three-way editor with automatic drafts, explicit side choices, undo, and separate reviews for applying a file and continuing a Git operation.
- Use configurable shortcuts, fuzzy contextual help, key-sequence hints, drag-to-copy, pane navigation and resizing, and optional Kitty integration. Open panels or a file history directly from the command line.
- Install native binaries on macOS, Linux, and Windows through `igit.yelog.org`; choose stable or beta and use `igit update` or `ig update`, including read-only checks and JSON output.

### Changed

- Use workspace shortcuts `1` Commit, `2` Log, `3` Push, `4` Branch, `5` Stash, and `6` Conflicts. Keep input fields and reviewed operations separate from navigation.
- Refresh Commit from debounced file notifications, load history and repository snapshots in the background, and render graph topology and colors using adaptations of IntelliJ IDEA algorithms.
- Standardize operation review layouts and keep interface text and documentation in English while retaining Unicode paths, names, and messages.

### Fixed

- Apply Paths filters consistently to Log's branch and tag tree.
- Load ZIP support under Windows PowerShell 5.1 and make Git and AI test fixtures portable across supported platforms.

### Internal

- Test and package the exact tagged commit for five native targets, verify release assets with SHA-256, and validate the installer site and native updater through GitHub Actions.
- Include third-party graph algorithm attribution and the Apache 2.0 license in every release archive.

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
- [`20448ee`](https://github.com/yelog/igit/commit/20448eebb5a228fa9e8392a2783a4e7380c3f24b) chore(release): prepare v0.1.0-beta.1
- [`6cd7bf4`](https://github.com/yelog/igit/commit/6cd7bf4dec957bb92a06c12a44135b60af4c41e9) fix(log): scope branch and tag tree to Paths filters
- [`5e6aeb2`](https://github.com/yelog/igit/commit/5e6aeb278a5830c587d2f68a3637680548995dfe) feat: add native updates and expand Git workspace workflows
- [`1b93f18`](https://github.com/yelog/igit/commit/1b93f18677b9eb9313d5fa832cb58531ace3dccd) fix(install): load ZIP types in Windows PowerShell
- [`569d198`](https://github.com/yelog/igit/commit/569d19830ca3f4b22f7adba33c222a950b28ebb7) test: make AI mock sockets portable on Windows
- [`c186730`](https://github.com/yelog/igit/commit/c1867308a94d15c384ce7503aad51071725dcd0f) feat(workspace): add stash actions and improve branch navigation
- [`f331685`](https://github.com/yelog/igit/commit/f3316854a5c3f3a0b73cc653d54955c90192cc66) feat: add log actions and file history with richer branch views
- [`87a59f1`](https://github.com/yelog/igit/commit/87a59f1e5b837417fd40707edd3e53da87b1e4d1) fix(release): include third-party notices in release archives
- [`553d499`](https://github.com/yelog/igit/commit/553d499479410a85fcf1557fe0adb3ffe69a9f80) chore(release): prepare v0.1.0
- [`f356f4b`](https://github.com/yelog/igit/commit/f356f4bf141a1c22741c46de1569557617ca3a78) test: normalize submodule fixture URLs on Windows

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
