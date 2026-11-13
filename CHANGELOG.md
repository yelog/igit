# Changelog

Release notes follow Keep a Changelog categories. Versions use the project's
decimal rollover policy described in [Releasing](docs/releasing.md).

## [0.1.3] - 2026-11-13

### Added

- Add recent Log path filters with compact, width-aware labels and a single-line summary in the Paths control and Log title.

### Changed

- Resume stable distribution at 0.1.3 and include the feature set prepared for the unpublished v0.1.2 tag.

### Fixed

- Render lowercase `b/u/d/p` in the Log filter prefix so displayed initials match the actual shortcuts.
- Render Log path labels with Git's `/` separators on every platform.
- Normalize Windows verbatim paths before invoking Git and keep checkout worktree paths in Git's representation.

### Internal

- Corrected color-enabled help-search assertions and Windows path-label assertions so stable release checks validate the intended behavior.
- Raise the native build job timeout to 45 minutes so slower runners can finish tests and packaging.

### Commits

- [`96a9f75`](https://github.com/yelog/igit/commit/96a9f75c700311558a1fcb832640207bfaaeede1) fix: correct help fuzzy highlight assertion
- [`d378cf5`](https://github.com/yelog/igit/commit/d378cf531ff3a4550cefdcdaa627f38e36741739) fix(log): lowercase filter shortcut initials
- [`ab967f4`](https://github.com/yelog/igit/commit/ab967f406de028f9651e412557c483b12102e1bf) fix: normalize Windows Git paths and checkout worktrees
- [`ba49b57`](https://github.com/yelog/igit/commit/ba49b57008e1c037aaad893d10f02ef016946cb8) fix: strip verbatim roots from Windows Git paths
- [`8652421`](https://github.com/yelog/igit/commit/8652421bd9353d4d99c4ee634db7550728e7178c) feat(log): add recent path filters and compact labels
- [`85bbb31`](https://github.com/yelog/igit/commit/85bbb31419b0c593fab8f8c93370236a18e17c6a) ci: allow slower native builds to finish
- [`41c6b4d`](https://github.com/yelog/igit/commit/41c6b4d4923bcb76eb395c1136c07c659e7e3d86) fix(log): render path labels with Git separators
- [`8554aa3`](https://github.com/yelog/igit/commit/8554aa3c95a585398a097df65cac1db11c4ef302) test(log): expect Git separators in path labels

## [0.1.2] - 2026-11-13

### Added

- Add a Remote workspace tab for browsing, adding, editing, and removing repository remotes across Roots, with reviewed local Git writes, hidden credentials, and preserved tracking configuration.
- Add live notifications and a searchable session Notification Center, plus Push commit copying, browser links for commits, and richer Log filters and navigation.
- Add two-way branch comparison, recent-branch sorting, Smart Checkout with stash-backed recovery, and reviewed fast-forward Pull and Push actions for the focused branch.
- Add reviewed Push updates with a persistent Merge/Rebase default, fetching of the actual target, integration before publication, and conflict routing.
- Add transactional Log actions for dropping, undoing, reverting, and editing commits; file-history and working-tree comparisons; and `igit diff --line` with guarded hunk restores.

### Changed

- Add Remote as tab `6` and move Conflicts to `7`; use `g n` for Notification History and `gg` / `G` for first and last item navigation where applicable.
- Group Commit changes by repository and expand batch navigation, path copying, message actions, and immediate checkbox toggles.
- Standardize review layouts with default primary-action focus, clearer branch and Push evidence, and asynchronous progress across repository operations.
- Share the saved Merge/Rebase update strategy with Pull and Push, and support `Ctrl-W` word deletion in editors and searches.
- Keep notification chords isolated from immediate Space actions in Commit and Push, and expose key-sequence hints without blocking completed input.

### Fixed

- Include every path changed by outgoing commits in Push, including commits beyond the visible recent list.
- Preserve remote rejection output and prioritize server policy errors over hook progress messages in Push details and notifications.

### Internal

- Retry release draft lookup after creation, serialize paired local binary builds, share bounded revision reads, and expand integration coverage and documentation.
- Align shortcut regression tests with the current workspace interactions and keep all-target Clippy clean.

### Commits

- [`85bfa7a`](https://github.com/yelog/igit/commit/85bfa7ae25a99186653215fa33ec7daba217bf5e) fix(release): retry lookup after creating draft
- [`fb358c4`](https://github.com/yelog/igit/commit/fb358c40e9fe3e134d59e2b5e6622db472d904f4) fix(push): aggregate files from outgoing commits
- [`3d1edc1`](https://github.com/yelog/igit/commit/3d1edc1b9c18f6b950c9cba2cc98618f70dbb40d) feat: enhance workspace interactions and notifications
- [`7c24ba7`](https://github.com/yelog/igit/commit/7c24ba7799ce62e416643d2945428247df0b7aba) feat: improve branch, search, and push workflows
- [`b2569d0`](https://github.com/yelog/igit/commit/b2569d08827db870bc3bf50f4461461a2f3ac7e7) feat: expand log workflows and review interactions
- [`434e0ff`](https://github.com/yelog/igit/commit/434e0ffb5337c45f5b13685d937c44c9f942edc7) feat(branch): add two-way branch comparison
- [`99aa97d`](https://github.com/yelog/igit/commit/99aa97d1a7ad559ecaa07b7266c7df363b7fc26c) feat(log): expand comparison and navigation workflows
- [`13dbd5a`](https://github.com/yelog/igit/commit/13dbd5a9f1b8c097454fb45fbe56b59299e4207e) feat: expand repository workflows and review controls
- [`dc03dcd`](https://github.com/yelog/igit/commit/dc03dcd35ee131f7be1a9aa7fd106c625fcea3d3) feat: expand reviewed diff workflows
- [`0e7d185`](https://github.com/yelog/igit/commit/0e7d185f829a604680c127dac1b388455d338488) fix: align shortcut tests and satisfy clippy

## [0.1.1] - 2026-11-10

### Added

- Open Update Center from the workspace to check for updates, review installation, and track progress.
- Stage individual hunks, add untracked files, and open working files in an external editor from Commit.
- Search workspace trees and diff previews with Vim-style navigation, and copy file paths.
- Switch and sync branches, inspect upstream divergence, set upstream on first push, and cherry-pick multiple commits from Log.
- Abort or skip supported in-progress Git operations from Conflicts.

### Changed

- Show aligned Before/After previews in Commit and Push, with exact staged content available in Commit.
- Run Add to Git, commits, and Log reloads in the background. Keep local commit and push review separate.
- Generate AI commit-message drafts with input budgeting and protection for existing drafts.
- Standardize tree expand and collapse controls and simplify the Stash list.

### Fixed

- Set or replace a branch upstream after Push on Windows.
- Preserve file bytes when applying supported stash conflict resolutions with Git line-ending attributes.

### Internal

- Serialize local Cargo commands and isolate their build artifacts.

### Commits

- [`3fec9a5`](https://github.com/yelog/igit/commit/3fec9a529679d199947b254038d3a4769c4bbd11) feat(update): add version footer and update center
- [`d6bd639`](https://github.com/yelog/igit/commit/d6bd63962f749be761b0764e76658c54cfb14555) feat: improve commit actions and stash conflict handling
- [`b53cc52`](https://github.com/yelog/igit/commit/b53cc52d8ad4569153a9192ace3a4756d1182688) feat(branch): show upstream ahead and behind counts
- [`fe6d335`](https://github.com/yelog/igit/commit/fe6d3351438936f99f85b62e16986fb48057de36) feat: extend Git workflows and improve workspace responsiveness
- [`db72fc6`](https://github.com/yelog/igit/commit/db72fc6503515529ff2ee3e45e2da6b6a18aebbb) feat: streamline commit actions and repository previews
- [`feae58d`](https://github.com/yelog/igit/commit/feae58df8b87190a60d1984b22b640c016533a6a) feat(update): show install progress in Update Center
- [`673ebe4`](https://github.com/yelog/igit/commit/673ebe44bd449fe4e152b956e33b6741407484ef) style(ui): unify tree expand and collapse controls
- [`7492b2a`](https://github.com/yelog/igit/commit/7492b2a785f27bbf27d80eddb86626c1320ae2fe) feat(search): add Vim-style find to workspace panes
- [`9e89748`](https://github.com/yelog/igit/commit/9e89748b5cd8fbf5fdab0ca98464a3443602dc02) style(update): match Update Center to the workspace palette
- [`d952ce4`](https://github.com/yelog/igit/commit/d952ce4f0eb4a9f4fee27afba046240173737488) feat: add split commit previews and streamline workspace behavior
- [`2531184`](https://github.com/yelog/igit/commit/253118442342766249ecde31846adb964c1a2ce7) feat: improve commit previews and workspace workflows
- [`7732e5b`](https://github.com/yelog/igit/commit/7732e5b780a5da8db1a1470936f72a7e01abb942) build: serialize local Cargo commands
- [`f4f7af1`](https://github.com/yelog/igit/commit/f4f7af1b291852fb1be6b3dbcd9ee72ef8463e5c) feat: extend branch and log workflows
- [`07a26de`](https://github.com/yelog/igit/commit/07a26de1f9bef0b1b8f768a1c28d27e74ce38c2f) fix(log): remove redundant width cast
- [`4e4c4de`](https://github.com/yelog/igit/commit/4e4c4deb2d8327642a3c48aed73fe469f1c413bb) test(shell): align Push and Update Center interactions
- [`bafa295`](https://github.com/yelog/igit/commit/bafa2957b6edfb6cf5f15bd389cad23c3411821f) chore(release): prepare v0.1.1
- [`c9e068e`](https://github.com/yelog/igit/commit/c9e068e3f3e59ca7d8f8572e259089df4df12837) test: isolate preview snapshots and installer channel state
- [`41350dd`](https://github.com/yelog/igit/commit/41350dd89bfe5c87fed965bdc581976ddb666a1e) chore(release): refresh v0.1.1 changelog
- [`2682913`](https://github.com/yelog/igit/commit/26829138c1e8476d7a7d94907b131203fe0acbe4) fix(push): write upstream config with a relative lock path

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
