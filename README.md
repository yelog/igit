# IGit

Open the same multi-repository Git workspace with `igit` or `ig`. Log, Commit, Push, and Merge run in one terminal session. Rust + Ratatui + Crossterm render the interface, while system Git performs repository operations. Use a terminal with ANSI and mouse reporting support, such as kitty or Terminal; at least 120 columns × 36 rows is recommended.

The interface, help, and documentation use English. Repository names, paths, commit messages, and file content retain Unicode support.

## Project Documentation

- [Start a new session: AGENTS.md](AGENTS.md) — Find relevant documentation and source files, with local practice and verification commands.
- [Product requirements (PRD)](docs/prd.md) — Use cases, delivered scope, pending work, and acceptance criteria.
- [Interaction design](docs/design.md) — Layout, focus, keyboard and mouse rules for all four panels, plus IDEA-based Commit acceptance criteria.
- [Architecture](docs/architecture.md) — Event routing, review of Git writes, and failure handling across repositories.

Original design references: [Four-panel overview](docs/reference/IGit-Overview.png) · [Interactive prototype](docs/reference/IGit.html) · [Illustrated design notes](docs/reference/IGit-Design.html). Both HTML files use demo data and can be opened in a local browser.

## Getting Started

Build and launch from this source directory. To use `igit` or `ig` in any workspace, optionally install both commands into Cargo's bin directory:

```sh
cargo build --release --locked
./target/release/igit /path/to/workspace
./target/release/ig /path/to/workspace

# Optional: install both commands into Cargo's bin directory
cargo install --locked --path .
```

By default, `igit` and `ig` discover repositories from the current directory. You can pass a directory; `--list-roots [directory]` only lists discovered working trees. Discovery supports ordinary Git roots, nested repositories at multiple levels, initialized submodules, and linked worktrees. Bare repositories are not editable targets. Discovery does not recurse into symlinked directories or common dependency/build directories such as node_modules, target, .venv, and build, and does not initialize or clone submodules. To use a repository inside a directory normally skipped, start directly in that directory.

**Choose terminal icons.** The default uses Nerd Font icons for repositories, branches, file types, and actions. If icons appear as boxes, try `ig --icons unicode`; for text markers such as `[x]`, use `ig --icons ascii`. Both commands accept `--icons mode [directory]`. The selected mode applies to all four panels and any workspace reopened during the same run.

| Mode | Use case |
| --- | --- |
| `nerd-font` (default) | The local terminal uses Nerd Font 3.x or Symbols Nerd Font Mono. Icons distinguish repositories, submodules, branches, tags, and common file types. |
| `unicode` | No dedicated font; uses standard symbols and file type abbreviations, such as `☑` and `TS`. |
| `ascii` | Uses ASCII icons and commit graph markers, such as `[x]`, `[-]`, `[ ]`, and `[!]`. UI text, borders, filenames, and patches may still contain Unicode. |

Commit, Log, Push, and Merge share file icons for common languages, including Java, Kotlin, Scala, Groovy, Python, Go, Rust, C / C++ / C# / F#, Swift, Dart, PHP, Ruby, and Lua. React `.jsx` / `.tsx` files use React icons distinct from ordinary JavaScript / TypeScript. Vue, Svelte, Astro, XML, SQL, GraphQL, Shell, PowerShell, and common build files also have dedicated markers. Unicode / ASCII modes use readable abbreviations such as `Jv`, `Py`, `JSX`, and `TSX`; unknown types keep a generic file icon.

IGit does not detect or install fonts. Over SSH, glyphs depend on the font configured in your local terminal.

![The same practice repository's Commit list in Nerd Font, Unicode, and ASCII modes](docs/reference/IGit-Icon-Modes.png)

**Try a real practice workspace first.** The script creates repositories only in a new temporary directory. All remotes are local; nothing is pushed to a hosting service:

```sh
demo_workspace="$(sh examples/demo-workspace.sh)"
igit "$demo_workspace"
```

The workspace contains two ordinary repositories, one initialized submodule, and three bare remotes. Each ordinary repository has changes to commit and commits to push. The submodule has one real conflict and two independent changes. The script prints the workspace path and leaves your practice results in place.

## Configuration

Customize shortcuts and presentation in `~/.config/igit/settings.toml`
(macOS/Linux), or `%APPDATA%\igit\settings.toml` (Windows). `XDG_CONFIG_HOME`,
`IGIT_CONFIG_HOME`, and `--config PATH` can select another location. Only include
settings you want to change:

```toml
[ui]
icons = "unicode"

[keybindings.commit]
review = ["Ctrl-r"]

[keybindings.panes]
focus-left = ["Ctrl-w h", "Alt-h"]
```

Run `igit --check-config` to validate, or `igit --print-default-config` to inspect
every setting and command. Arrays replace defaults; `[]` disables a binding.
Changes take effect after restarting. Help and sequence hints show the effective
keys. Text editing and confirmation controls remain context-owned; `F1` and
`Esc` stay available. See the [configuration reference](docs/configuration.md)
for key syntax, conflict handling, mouse settings, paths, and precedence.
Shortcuts documented below are the built-in defaults.

To share `Cmd+Ctrl+h/j/k/l` with Kitty, Neovim, and LazyDB, configure
`keybindings.panes.smart-focus-pane-left/down/up/right` and Kitty's `IS_IGIT`
pass-through mappings. These commands move within IGit first and cross into
Kitty at an edge. See [Kitty smart focus](docs/configuration.md#shared-focus-shortcuts-with-kitty-and-neovim)
for the complete setup.

`smart-resize-pane-left/down/up/right` can similarly bind
`Cmd+Ctrl+Shift+h/j/k/l`: move an internal divider by three cells, then hand
off to Kitty when the divider is unavailable or at its limit. Install the
IGit kitten helper and pass through these chords as described in
[shared resize shortcuts](docs/configuration.md#shared-resize-shortcuts-with-kitty-and-neovim).

## Global Controls

[Workspace screenshot](docs/reference/IGit-Split-Workspace.png): Commit is above Git Log, with equal heights by default. At startup, a blue border marks the Log commit list as the keyboard target. Push and Merge open as needed; closing them restores the previous focus.

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Focus a split | `1` Git Log, `2` Commit | Click the split; the wheel scrolls the panel under the pointer. Tabs beside the Log title only switch log filter views. |
| Open or close an operation window | `3` Push, `4` Merge; `Esc` goes back one level | Click ⋯ → Open Push / Open Merge; click × in the window's top-right corner to close. |
| Update repositories | `p` reviews Pull; `f` reviews Fetch | Click ⋯ → Pull repositories or Fetch repositories. |
| Filter repositories | `r` → `j/k` → `Space` to toggle; `a` selects all; `Enter` applies | Click Roots, repository rows, and Apply. |
| Switch panes | `Tab` / `Shift-Tab` cycles forward/backward (also `F6` / `Shift-F6`); press `Ctrl-W`, then `h/j/k/l` for left/down/up/right | Click the target pane. |
| Resize panes | After `Ctrl-W`, use `>` / `<` to increase/decrease width, `+` / `-` for height, or `=` to restore defaults | Drag the divider between adjacent panes; drag the horizontal Commit / Git Log divider to change split heights. |
| Maximize or restore a pane | `Ctrl-W f`; optional Kitty synchronization uses `smart-toggle-pane-maximized` | In maximized mode, only the focused pane is shown; Tab still switches panes. |
| Actions and help | `:` action list; `?` / `F1` help for the current area | Click ⋯ (Actions) or ? (Help). |
| Refresh and quit | `F5` refreshes; `q` or normal-mode `Ctrl-C` opens quit confirmation | Click ⋯ → Refresh workspace or Quit. |
| Review writes | `y` confirms; Cancel has initial focus, `n/Esc` cancels; scroll long content with `j/k` or arrow keys | Click Confirm / Cancel; use the wheel to scroll. |

In normal mode, Tab moves between Commit and Git Log. With Push or Merge open, it cycles only within that dialog. Hidden sidebars are skipped; narrow terminals still allow Tab to reach panes collapsed into tabs. Resizing defaults to one cell at a time; specify a count after the prefix, such as `Ctrl-W 8 >` to widen by eight columns. Sizes last only for the current run.

`Ctrl-W f` maximizes the focused pane across the workspace and restores its saved split sizes on the next press. The footer shows `MAX`; Tab and panel shortcuts switch the maximized view. Local resizing and divider dragging are suspended while maximized. To make `Cmd+Ctrl+F` toggle both Kitty's stack layout and IGit's focused pane, see [pane maximize configuration](docs/configuration.md#pane-maximize-and-kitty-synchronization).

The current row determines the preview; checkboxes determine commit and push candidates. Repositories hidden in global Roots do not participate. While entering text, letters and digits are treated as text and do not trigger global shortcuts. Actions → Open / rescan workspace reruns discovery. Opening a workspace restarts panel sessions, so finish any current input first.

**Pull and Fetch:** Both reviews list every repository in global Roots, regardless of the current Log tab or Push checkboxes. Pull uses each current branch's upstream. Choose **Merge incoming changes** (the default, allowing fast-forward) or **Rebase current branches** with `m/r` or a click. Fetch updates refs from the repository's remotes without integrating them into the checked-out branch. Press `y` or click Pull / Fetch to proceed; `Esc` cancels before Git runs. The results list shows which repositories succeeded, failed, or were skipped. If Pull stops with conflicts, close results and open Merge with `4`.

**Select and copy text:** Log's lower-right commit details and file Diff, Commit's inline message editor and Diff preview, and Push's Details / Diff support left-button drag selection with a blue highlight. **Releasing the left button immediately copies the text and clears the selection; no shortcut is needed.** A normal click or empty selection does not copy. Clicking a parent commit link still navigates; dragging does not. Copy preserves original indentation, blank lines, and line breaks, without soft wraps, borders, or terminal padding. You can scroll while dragging; changing the commit/file or hiding the area clears stale selections.

Keyboard selection still supports `Ctrl-A` to select all and `Ctrl-C` / `Ctrl-Insert` to copy; read-only areas also accept `y`. Keyboard selections survive scrolling; `Esc` clears a selection first. In the commit editor, use `Shift+Arrow` or `Shift+Home/End` to select text for replacement. Typing, pasting, Backspace, and Delete replace or delete the keyboard selection. After a mouse selection is copied on release, the caret stays at the drag endpoint. Scroll long messages with the wheel; keyboard movement keeps the caret visible. `F2/Ctrl-S` or inline Save saves. Inline Cancel discards the current edit; `Esc` clears a selection before canceling. With a selection, `Ctrl-C` copies first; in normal mode without a selection, it still opens quit confirmation. Clipboard handling uses the system commands listed below and reports failures explicitly.

**Context help:** `?`, `F1`, or top-right Help opens a lazydb-style `KEYMAP // current area` panel with Search focused. Type a shortcut, description, or area name to filter. Matches in shortcuts and descriptions are highlighted. Actions for the current area come first, followed by panel-wide and global actions; each row represents one action. `↑/↓` selects, and `Enter` closes help and executes the selected shortcut. `PgUp/PgDn` changes selection by a page; click to select, or use the wheel/scrollbar to browse. Search supports left/right, Home/End, deletion, and `Ctrl-W/U/A/E` editing. `Esc/F1` cancels help and restores the original focus, draft, or dialog. `F1` also works in input fields, Diff, Roots, and review dialogs. Choosing commit, push, or Apply file still follows the existing review workflow.

**Key sequence hints:** In normal mode, press `Ctrl-W`; in the Log commit list, press `g` / `y`; or in a Merge conflict document, press `[` / `]`. If no other key arrives for 200ms, a bottom overlay lists valid continuation keys and descriptions. Type the next key to execute; quickly completed sequences do not flash a hint. Once visible, use arrow keys and `Enter`, or click a candidate. `Esc/Ctrl-C` cancels only the pending prefix. Switching panels/focus, opening a menu, pasting, or resizing also dismisses it; another prefix restarts the timer. The overlay uses one to three columns according to width. Letters in text fields and single-key actions in Commit/Push keep their existing meanings.

## The Four Panels

**Log display options:** Click the eye at the top right of Log, or press `o`, to open Show / Highlight. **Show → Branch or Tag** controls the left branch tree and is enabled by default; `B` also toggles it. Hiding it moves the commit list to the left edge and hides Refs in narrow mode. Select with `j/k` or arrow keys, and toggle with `Space/Enter`. Open Columns by clicking, hovering, or pressing `l/Right`; use `h/Left` or `Esc` to return, then `Esc` in the main menu to close. Short windows scroll the menu; narrow windows place the submenu on the left or replace the main menu.

**Reorder log columns:** The top-to-bottom order in Columns is the left-to-right order in commit rows. Select a column and press `K` or `Shift+↑` to move left, or `J` / `Shift+↓` to move right. The up/down arrows beside each column name do the same. Clicking the name or pressing Space still only toggles visibility. Hidden columns can be positioned before showing them.

Options include root names, compact references, tag names, long edges, author/commit timestamps, reference position, and six columns: Author, Hash, Date, GPG Signature, Graph, and Message. My Commits (bold), Merge Commits (dimmed), and Current Branch (blue background) are independent highlights. After selecting another Branch, Not Cherry-Picked Commits uses patch equivalence to dim already-applied commits and leave unapplied ones at normal brightness. With long edges disabled, connections longer than 12 rows with no intervening join use clickable arrows in the middle; click to jump to the corresponding endpoint.

Both sidebars, reference labels, and Author, Date, Graph, and Message are shown by default, with the first three highlights enabled. Graph and Message can also be hidden independently or moved between any other columns. Dates default to author time. Disabling Tag Names retains tag icons; names remain available under Tags on the left. Narrow columns prioritize the graph and commit subject. Options are shared across Log tabs for the current run and reset on restart. Signature and cherry comparisons run only when enabled and do not modify repositories.

**Log branch filtering:** Click **Branch** or press `Space b` to choose a favorite directly, or enter groups such as **Local** and **origin/...** to open a submenu. Clicking the star (Space on the keyboard) only toggles the favorite; clicking the name or pressing Enter filters history. **Favorites** shows commits from all favorite branches. **Select...** accepts multiple branches separated by `|`, such as `master | origin/release`.

Commit-row references use IDEA-style outlined tag icons: yellow for HEAD, purple for remote branches, and green for local branches, including the currently checked-out local branch. HEAD and a local branch on the same commit show green + yellow icons; when the upstream also points there, the icons are purple + green + yellow and the label is combined as `origin & main`. Icons have one cell between them; narrow columns shorten names while retaining the full icon group.

The lower-right commit details always show all containing branches, one per line, with the same icons and colors. Only HEAD is yellow; local branches are green and remote branches purple. Scroll with the wheel, `j/k`, or paging keys in details; no Show all / Show less or Space expansion is needed.

**Log path filtering:** Click **Paths** or press `Space p`, then choose:

- **Select...**: Type or paste one relative file/directory path per line. `Enter` adds a line; `Ctrl-S`, `Ctrl-Enter`, `F2`, or **Apply** applies. For example, separate lines `src` and `README.md` show commits that changed either path in the selected repositories.
- **Select in Tree...**: The first level contains repositories discovered within global Roots. Click arrows to expand directories and checkboxes to select a repository, directory, or file. Use up/down to move, left/right to expand/collapse, and Space to check. `Enter` or **OK** applies; `Esc` discards the draft. Duplicate filenames match only the repositories selected in the tree.
- **Roots**: Click a repository row or press Space to toggle this tab's scope immediately. Click the heading or press `Ctrl-A` to toggle all listed repositories. You can also type an exact repository name in the menu and press Enter to show only that repository. The clear button beside Paths resets this tab's repositories and paths.

**Log date filtering:** Click Date or press `Space d`. The menu contains `Select...`, `Last 24 hours`, and `Last 7 days`, in that order. The first opens a custom range: enter `YYYY-MM-DD` in From / To, use `Tab` to switch fields, `Ctrl-U` to clear a field, and `Enter` to apply. Both local boundary dates are included. Navigate with arrows or `j/k`; `Esc` cancels. The clear button beside Date removes only the date condition.

**Log navigation and copying:** `j/k` or up/down moves within the viewport; scrolling begins only when movement passes the top or bottom edge. The list does not recenter automatically. `gg/G` selects the first/last commit; `H/M/L` selects the top/middle/bottom commit on screen. `Ctrl-F/B` pages down/up, `Ctrl-D/U` moves half a page, and `PageDown/PageUp` moves a full page. Page size uses the actual visible rows. `yy` copies the full commit ID, `ym` the full message including its body, and `ya` `Name <email>`. These shortcuts apply to the commit list; input fields and menus keep their own meanings. Copying uses the system clipboard: `pbcopy` on macOS, `clip.exe` on Windows, and `wl-copy`, `xclip`, then `xsel` on Linux. Missing commands or display sessions produce an explicit error.

| Panel | Available operations |
| --- | --- |
| **Log** | [Three-column layout](docs/reference/IGit-Log-Graph.png). Identically named branches are combined across repositories. Commit rows default to author, local date/time, colored graph, subject, and references. Hold `j/k` to browse; once selection settles, the right pane loads that commit's files and containing branches. The Diff file tree uses filename colors for change types, without appended status letters. Enter or double-click opens a file Diff. `/` searches text or hashes. `Space b/u/d/p` opens Branch/User/Date/Paths; press Space first, then the letter. In the branch tree, `S` searches branch and tag names and `Enter` or `s` selects a ref. With a menu open, clicking another filter switches directly; clicking a commit selects it directly. `x` clears filters; `+` loads more history. |
| **Commit** | Files start grouped by directory. In the [flat layout](docs/reference/IGit-Commit-Flat.png), gray parent paths distinguish duplicate names. Click rows to browse, boxes or `Space` to check, and arrows to expand/collapse. Double-click a file or press `Enter` for its patch. `v` toggles preview, `o` opens display options, and `t` switches directory grouping. Click the message area or press `i` to edit directly in the bottom Commit pane. Enter adds a newline; `F2/Ctrl-S` saves and `Esc` cancels. Clicking another repository/control or using `Tab/Shift-Tab/F6` saves the draft before moving focus; `[/]` changes repository in normal mode. `c` reviews commits; `P` reviews commits and then opens Push. |
| **Push** | One contextual window title, aligned pane toolbars, and a footer with Follow tags / Hooks, Cancel, and Push N. Repository name, branch, target, and submodule align in left-side columns. Target / Fetch sit above outgoing commits; Retry appears after a push failure. The right side defaults to a full-height repository → directory → file tree with directory counts and filename colors: added green, modified blue, deleted red, renamed amber, without trailing status letters. If the current repository has no outgoing commits or no row is selected, it centers `No commits selected`. Click the **Group By** eye or press `o` to toggle Directory, Repository, and Module independently; Directory / Repository start on and Module starts off. Without Directory, files are flat with parent paths. Arrows or `h/l` expand/collapse; `[/]` expands/collapses all in the file area. Click a file or press `Enter` for Diff, `d` to toggle details, and `Esc` to go back. Cancel closes the window and restores the original workspace focus. Boxes or `Space` select repositories. `t` edits `remote:branch` (`Ctrl-U` clears input), `v/x` toggles Follow tags/hooks, `P` reviews a push, `R` retries only visible failed repositories, and `F` fetches the Push targets. Push N excludes hidden/ineligible repositories and is disabled when empty or busy. Network operations run in the background; failures remain visible per repository. At most 60 recent commits per repository are expanded, but counts and review use the full branch tip. |
| **Merge** | Lists conflict paths from repositories in scope first. Enter or double-click opens Current on the left, Result in the center, and Incoming on the right. `j/k` changes block, `h/l` changes pane, and `[c` / `]c` jumps between conflicts. The top button or `a` applies only definite non-conflicting changes. Gutter arrows or `H/L` take left/right, `b` takes left then right, `i` edits the current block, `Ctrl-S` or `Ctrl-Enter` accepts, and `u` undoes. `A` reviews and applies a file; `C` separately continues the repository operation. Session drafts are not disk writes. |

**Compare Log with IDEA:** Run these commands from the source directory to view real branches and merges across two temporary repositories. Press `Space u`, enter `me`, then Enter; press `Space p`, enter `atlas-web`, then Enter. Light blue marks history reachable from the current branch; bright blue marks the selected commit. Rails within a repository have distinct colors. `Ctrl-T` opens an unfiltered Log tab; `[/]` switches tabs while retaining filters and commit position. `Ctrl-W c` or × beside the tab closes it. For a row-by-row comparison with IDEA's single-repository log, choose the same repository name in **Paths** in both apps. IGit's right pane identifies the current commit's repository; it does not restrict the whole tab's history, so global Roots need not change. `◀/▶` at the graph edge means the row's commit is off screen; selecting it pans the entire graph.

```sh
log_workspace="$(sh examples/demo-log.sh)"
cargo run --locked --bin igit -- "$log_workspace"
```

Commit nodes sit at the center of their rails. In default mode, [kitty 0.36.2 and later](https://github.com/kovidgoyal/kitty/blob/v0.36.2/kitty/fonts/box_drawing.py#L1334-L1365) and [Ghostty](https://github.com/ghostty-org/ghostty/blob/v1.0.0/src/font/sprite/Box.zig#L1452) can draw the node and connecting lines in the same cell. In a compatible terminal through tmux, use `IGIT_GRAPH_GLYPHS=connected igit` for connected glyphs; `IGIT_GRAPH_GLYPHS=unicode igit` switches only graph nodes back to standard circles. Explicit `--icons unicode` or `--icons ascii` takes precedence: Unicode uses `●` / `◉`, and ASCII uses `*` / `@`, for ordinary commits / HEAD.

**Practice workflow:** In Commit, write a message for each ordinary repository, press `P`, review the patches, and confirm. In Push, press `P`, check both targets, and push. In Merge, open the submodule's `transport.conf` and press `a`. Paths and retry counts can merge automatically; transport still needs a manual choice. After Apply file, Git's MERGE_HEAD remains until Continue is confirmed separately.

**Compare Commit with IDEA:** Run these commands from the source directory and press `2`. The practice repository contains 11 tracked changes checked by default and 8 expanded but unchecked untracked files. Find the two `index.vue` files in separate directories and open each Diff. Then inspect `iframe-proxy.mjs`: its preview shows only the staged version that will be committed.

```sh
commit_demo="$(sh examples/demo-commit.sh)"
cargo run --locked --bin ig -- "$commit_demo"
```

To test the local build from another terminal, first run `cargo build --locked --bins` in the source directory, then **`/absolute/path/to/source/target/debug/ig .`** in the target directory and press `2`. This does not require installing or replacing global commands.

Checkboxes indicate checked, partially checked, unchecked, and unavailable (Unicode: `☑ / ⊟ / ☐ / ⊠`; ASCII: `[x] / [-] / [ ] / [!]`). Repositories and directories use expanded/collapsed folder icons; submodules keep a dedicated marker. Click the toolbar eye or press `o` for **View Options**. Under Group By, **Directory** starts enabled and **Module** (repository) starts disabled. They toggle independently. Show offers **Ignored Files** and **Git Status**. Use `j/k`, arrows, the wheel, or hover to select, `Space/Enter` to toggle immediately, and `Esc` to close. Ignored items are hidden by default and read-only when shown. Ignored directories appear as a single item without reading their contents and cannot be checked, committed, regrouped, or included in AI patches. Display options last for the session and do not alter existing selections.

`s` toggles Git XY status such as `AM`; a nonempty `NO_COLOR` displays XY directly in file rows. `z/Z` collapses/expands all; `u` undoes the last checkbox change. `n/m` creates/moves groups; right-click or drag tracked files to another group also works. `a` toggles Amend for the message area's current repository. The toolbar Diff button or `v` opens preview, side by side from 92 columns and full width below that; `Esc` returns to the original row.

**Compare Push with IDEA:** Run these commands, press `3`, and compare the [file tree](docs/reference/IGit-Push-Tree.png) and [grouping menu](docs/reference/IGit-Push-Group-By.png). The practice workspace contains moss-web, 10 real submodules, and local bare remotes. moss-web has 2 outgoing commits with 20 changed files; mes-isg has 3 outgoing commits. Press `o`, use `Space` to disable Directory, and `Esc` to close the menu. Open the `system.ts` files with en_US and zh_CN parent directories to inspect their Diffs. Press `P` to review, then `Esc` to cancel.

```sh
push_demo="$(sh examples/demo-push.sh)"
cargo run --locked --bin ig -- "$push_demo"
```

## Commit and Push Semantics

- Each repository gets its own Git commit or push; batches are not atomic across repositories. Successful repositories are not automatically rolled back.
- In Commit's Changes list, `d` opens **Rollback Changes** with the full tree for the current Roots. It prechecks the highlighted file or the eligible members of the highlighted directory, repository, or changelist. Other rows remain visible; use Space or click a checkbox to add or remove files, `v` for a file's Diff, and `o` for Group By. **Close** is focused initially; `Esc` closes, and `y` or **Rollback** applies. Only checked files are affected: tracked files return to HEAD in both index and working tree, untracked files are deleted, and added files are unstaged. Check **Delete local copies of added files** to remove those copies as well. Rollback has no undo. Changed snapshots and unsupported paths require another review or separate handling.
- Selection is per file. Files with staged changes use the **index version**; otherwise, they use the **working-tree version**. If line A was staged and line B changed afterward, only A is committed. Unselected staged entries in the real index remain intact. When a tracked file is replaced by a directory of the same name, selecting only the old file's deletion does not include files inside the new directory; AI patches use the same scope. This version does not offer manual line selection or split one file across multiple changelists.
- Review shows the commit's target branch. After switching branches in another terminal, press `F5` to refresh. A branch switch after review requires another review even if both branches point at the same commit. Changes to HEAD, selected patches, or staged versions also require review again.
- Commits run real hooks. Amend is blocked for commits contained in known remote-tracking refs; local tracking records are not absolute proof of the server's publication state. Signing, hooks, or credential helpers that need interaction must use a system-configured agent / pinentry. The workspace does not collect Git passwords.
- Push sends the selected local SHA to an explicit remote branch. It checks the remote tip and fast-forward relationship, then requires the server to update only if that tip still matches. There is no force-overwrite button. If the target or remote changes, Fetch first, use Pull to integrate upstream if needed, and review again. Follow tags includes only reachable annotated tags as defined by Git, not all tags.
- When parent and child repositories are pushed together, children run first; a failed child blocks its dependent parent. Changes to submodule pointers or only their sources in `.gitmodules` require proof that the child commit is available from the source declared in that version. Intermediate commits must pass too. Failure to verify blocks the parent push without publishing unchecked children.
- Merge resolves **existing** Git conflicts and can continue the corresponding operation. This version is not a branch manager and does not initiate arbitrary merge/rebase operations. Ordinary UTF-8 text uses the three Git index stages; rebase identifies the new base and replayed commit. Files are limited to 2 MiB. Binary files, delete/modify conflicts, special file modes, nonstandard filters, and externally edited conflict files are preserved with a message to handle them externally.

## AI Commit Messages (Optional)

In Commit, press `S` to configure a full OpenAI-compatible chat completions URL, model, and prompt. HTTPS services and local HTTP services with a port are supported. The key comes from `IGIT_LLM_API_KEY` and is not written to configuration files. The system executable `curl` is required.

Press `A` to review the repositories/files and patch size to send. Only after confirmation does IGit request a candidate per repository from the configured service, using the actual selected patches. Candidates do not automatically replace messages or create commits. All Git operations work without a model service.

Local groups, per-repository message drafts, and AI settings are stored in a workspace subdirectory under `IGIT_STATE_DIR` or `XDG_STATE_HOME/igit` (default `~/.local/state/igit`). Configuration is not written into the project by default.

## Verification

```sh
cargo test --locked
cargo fmt --check
```

Tests use newly created temporary repositories, local bare remotes, and real Git. They cover branch graphs, partial staging, stale reviews, hook rejection, multi-repository push/retry, submodules, three-way merges, and preservation of external edits. Running the binary requires Git; building requires Rust/Cargo. Current release binaries target macOS Apple Silicon. Other platforms require a local build and terminal-input verification.
