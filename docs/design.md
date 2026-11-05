# IGit Interaction Design: Four Panels and the Commit Baseline

**Baseline: the native 0.1.0 TUI.** Layout descriptions refer to the native client. The Commit section compares its changes list with IDEA; the final section describes how to repeat acceptance checks. See [README](../README.md) for startup and dependencies. The original [interactive prototype](reference/IGit.html), [design notes](reference/IGit-Design.html), and [four-panel overview](reference/IGit-Overview.png) are available offline; prototype buttons and shortcuts do not imply implemented features.

## Workspace and Focus

Shortcuts below describe the built-in defaults. Optional
[`settings.toml`](configuration.md) overrides normal-mode commands by name,
including multi-key sequences, aliases, and unbinding. Help, sequence candidates,
split titles, and shortcut-bearing controls show the effective keys. Text entry,
selection, menus, and write confirmations keep their own controls; F1 and Esc
remain available. Mouse capture and icons are startup settings, and the sequence
hint delay is configurable (200ms by default). Changing a binding never changes
the action's repository scope or review requirements.

Optional `smart-focus-pane-left/down/up/right` bindings share directional focus
with Kitty and Neovim. Internal visible panes take precedence; an application
edge hands focus to the neighboring Kitty window without wrapping. Push/Merge
navigation stays within the open tool before handing off. Editors, menus, help,
Diff overlays, and confirmation windows consume these chords without changing
focus or text. Commands are unbound by default; see the
[Kitty setup](configuration.md#shared-focus-shortcuts-with-kitty-and-neovim).

Optional `smart-resize-pane-left/down/up/right` commands move the nearest
right/bottom divider, otherwise the left/top divider, by three cells in the
requested direction. Press and repeat events resize; release events do not.
When that divider is absent or clamped, the command resizes the originating
Kitty window. Focus and checkbox selections stay in place, and the same
editor/overlay guards apply. Existing Ctrl-W size controls retain their
behavior. See [resize setup](configuration.md#shared-resize-shortcuts-with-kitty-and-neovim).

Run `igit` or `ig` from the working directory. The area between the top bar and bottom status line is split equally by default, with a movable divider. Startup focus is the lower Git Log commit list. The active split title has `>`; a blue border marks the pane receiving keyboard input, while other borders are gray. Below five rows of height, only side borders remain to preserve content space. Real practice-workspace screenshots: [Vertical split](reference/IGit-Split-Workspace.png) · [Push dialog](reference/IGit-Push-Dialog.png).

```text
 IGit  workspace                                    Roots N/M    ⋯ Actions    ?
   Commit [2]
   Changes / optional Diff preview                       Top half: 50%
   Commit messages and commit buttons
 > Git Log [1]   Log   +
   Branch tree       Commit graph + filters   Files/details
                                                        Bottom half: 50%
 NORMAL   3 Push   4 Merge   ·   : Actions   ? Help
```

- **Keep four kinds of selection distinct.** The highlighted row determines what is being viewed; checkboxes determine candidates for the operation; Roots limits which repositories are available; write review determines whether to execute. Moving the highlight must not silently check a row. Commit/Push retain selections hidden by filters, but exclude them from the current operation. When a workspace has at least one repository, global Roots cannot be applied empty.
- Click a split or press `1` (Log) / `2` (Commit) to move keyboard focus. The wheel scrolls only the split under the pointer. Each split retains its cursor, checkboxes, and filters. Log's own filter tabs and `+` sit beside the lower split's title.
- `3` (Push), `4` (Merge), or the ⋯ action list opens a centered dialog. Mouse events do not reach the background. `Esc` first closes editing, details, or the conflict document, then closes the dialog and returns to the original split. The title's ? opens help; × closes the window. Finish or cancel text editing first. Commit & Push opens the same Push window after committing; pushing requires a separate review.
- Click Roots or press `r` for repository filtering: `j/k` moves, `Space` toggles, `a` selects all, `Enter` applies, and `Esc` cancels. `F5` refreshes both splits and any open operation window. Actions → Open / rescan workspace reruns discovery.
- In normal mode, `Tab` / `Shift-Tab` cycles forward/backward (`F6` is equivalent). Order: Commit changes, preview (if open), message; then Log branches, commits, files, and details. Hidden sidebars are skipped; panes collapsed into narrow-screen tabs remain reachable. Push and Merge each have a separate cycle. `Ctrl-W` followed by `h/j/k/l` focuses the adjacent left/down/up/right pane and stops at the outside edge. Browsing focus is independent of file checkboxes.
- `Ctrl-W` followed by `>` / `<` increases/decreases the current pane's width; `+` / `-` changes height. A count can follow the prefix, such as `Ctrl-W 10 +`. Resizing moves the nearest internal divider in the requested direction first; increasing the Log commit list's height moves the Commit / Log divider. Drag a target border directly; movement stops at its bounds and ends on release. Text selection and file dragging still belong to the area where the gesture began. `Ctrl-W =` restores default proportions for the current workspace or operation dialog without affecting other windows. Single-column mode has no vertical divider.
- Editors, filters, help, and review retain their own Tab and input rules. Shell review dialogs receive events before panels, initially select **Cancel**, require `y` to confirm, and accept `n/Esc` to cancel. Long review content scrolls.
- **Text selection.** Log commit details / Diff, Commit message editing / Diff preview, and Push Details / Diff support left-button dragging with a blue highlight. Release copies once and clears the selection. Empty selections and ordinary clicks do not copy; dragging does not activate links. The editing caret stays at the drag endpoint after release. Keyboard selections support `Ctrl-C`, `Ctrl-Insert`, and `y` in read-only text; `Ctrl-A` selects the current text area. `Esc` clears a selection before its normal back action. Selection refers to original text and does not split wide Unicode characters, combining characters, or emoji. Soft wraps and decorative separators are excluded from the clipboard; original blank lines, indentation, and line breaks remain. Drag and keyboard selections survive scrolling/resizing; changing the object or hiding the area clears stale selection. Dialogs take event priority; canceling help restores keyboard selection. Parent commit links activate on release only without dragging. The Commit editor supports Shift+Arrow / Home / End; typing/pasting replaces keyboard selection, while Backspace/Delete removes it. The wheel scrolls long messages; keyboard movement scrolls only when the caret leaves the viewport. Text dragging does not change file checkboxes, groups, or Git state.
- **Context help.** `?` / `F1` / top-bar Help captures the current focus or dialog as context. It follows lazydb's help style: centered, at most 74 columns × 34 rows, rounded borders, dark blue background, cyan `KEYMAP // area` title and Search, bold blue keys, white descriptions, and a gray footer. The list has no group headings or count row. Each item uses one line, with `>` for the current candidate and descriptions clipped by terminal cell width. At normal widths, the key column is 18 cells; below 60 columns, it uses the longest shortcut plus two cells, capped at 18, to leave more room for descriptions. The footer selects a complete hint that fits and retains Esc to close. Search opens with a caret; ordinary characters including j/k/g/G enter text. Left/right, Home/End, Backspace/Delete, and Ctrl-W/U/A/E edit. ↑/↓ cycles candidates; PgUp/PgDn changes selection by a page. Clicking only selects, while the wheel/scrollbar changes the viewport; selection stays visible. Enter closes help, restores the original area or dialog, then executes the candidate's static key sequence. With no results, Enter leaves help open. Available actions appear in current-area, panel-wide, then global order; duplicate keys keep the higher-priority entry. Switch focus to see another area's actions. Input fields and dialogs list only applicable actions; F1 and top-bar Help do not cancel input. Esc/F1 cancels help and restores state unchanged. Commit, push, and merge application keep their existing review paths.
- **Key sequence hints.** Normal-mode `Ctrl-W`, Log commit-list `g` / `y`, and Merge conflict-document `[` / `]` open a bottom overlay after 200ms without another key. It lists only valid continuation keys and descriptions for the current prefix. Typing a continuation executes immediately; completing/canceling during the delay never shows the hint, and a new prefix restarts the timer. Once visible, arrows select by displayed columns; Enter or clicking executes. Candidate navigation is not intercepted before the overlay appears. Highlighting a candidate does not change the selected commit/conflict block. Esc/Ctrl-C cancels only the prefix and keeps Merge's document open. Candidates use one to three columns based on actual cell width, without covering the bottom notification/mode line. Borders, padding, and wheel events block underlying controls; clicking outside cancels and passes through that click. Other actions, focus changes, overlays, paste, loss of focus, or resizing clear the prefix. Text input never opens candidates.
- Recommended size: **120×36 cells**. Below 44 columns or 12 rows, Shell shows a terminal-too-small message. All four panels and Roots use Commit's [shared palette](../src/ui/idea.rs): dark gray background `#1C1D22`, body text `#CFD2DB`, secondary text `#8B8F9A`, blue focus `#74A4F8`, and selected-row background `#314675`. Blue marks the browsing row; checkboxes indicate inclusion. Push and Commit patches use green/red for additions/deletions. Merge uses amber warnings for unresolved conflicts and green completion markers for resolved blocks. A shared bottom line displays mode, action hints, or the latest message without moving the splits.

**Language and text width.** Product-owned labels, messages, help, and documentation use English. Use short action labels and measure terminal cell widths when positioning text or hit areas. User-provided repository names, paths, messages, and content retain Unicode support.

**Icons and fonts.** All panels and Roots share repository, submodule, file, and selection markers. Default `nerd-font` uses named glyphs; `--icons unicode` uses standard symbols; `--icons ascii` uses text markers. Icon widths are not fixed: ASCII `[x]` occupies three cells, all of which form its hit area. When a file marker cannot fit, preserve the filename first. The input caret, current row, and committable state each have independent meanings. Exact glyphs in diagrams vary by mode.

File lists and existing file-preview titles share the [file type mapping](../src/ui/icons.rs). Icon color indicates type, while filename color still indicates Git status. Java uses a coffee cup, Python its language marker, and React JSX / TSX a React marker (cyan/blue respectively); ordinary JS / TS retain their own icons. Common JVM, systems, scripting, and mobile languages, XML, and build files have dedicated markers. Detection checks filenames and compound suffixes such as `pom.xml` and `build.gradle.kts` before extensions. Matching generally ignores ASCII case, while preserving the `.C` / `.H` convention for C++. Detection uses paths only, without reading content. Unknown types use a generic file icon; Unicode / ASCII modes retain readable abbreviations.

## 1 · Log: IDEA Git Tool Window Reference

The [actual terminal render at 180×42](reference/IGit-Log-Graph.png) uses the [temporary history workspace](../examples/demo-log.sh), with `User: me` and `Paths: atlas-web`. Filter values, authors, files, and refs come from Git. The image is a reference for the existing layout. Current file markers depend on icon mode; the ASCII graph uses `*` / `@` for ordinary commits / HEAD and characters such as `|` and `-` for edges.

| Previous layout issue | Current behavior |
| --- | --- |
| Repeated Local/Remote groups per repository; long `origin/feat/…` lists were hard to scan | The left pane has fixed HEAD / Local / Remote / Tags groups. Identical full refs share one row, annotated with repository names when present in only some Roots. Remote and slash-prefix groups collapse. The current branch stays first; upstream ahead/behind counts come from Git. |
| Large title boxes, headers, and a bottom repository legend consumed rows; long refs pushed subjects off screen | Wide layout proportions are about **17 / 57 / 26%** for left/center/right, with **65 / 35%** files/details on the right. Adjacent borders are draggable; filter values stay next to the toolbar. Each commit uses one row: author, time, graph, subject, then right-aligned refs. |
| Repository and branch edges shared colors, obscuring merge origins | A left color strip identifies the repository; a gold main rail and colored branches show topology. Light blue marks history reachable from that repository's HEAD; selection uses brighter blue. Merge commit text is dimmed. Identical SHAs in different repositories never connect. |
| Commit nodes sat beside rails, or vertical edges shifted while scrolling | All rows share rail coordinates. Filled ordinary nodes and ring-shaped HEAD nodes are centered on their rails; compatible terminals draw the node and connected edges in one cell. Horizontal lines appear only at branches, joins, or rail changes. At narrow widths, `‹/›` means additional rails exist; `◀/▶` means that row's commit is off screen. Selecting an off-screen commit pans the whole graph. |
| Flat folders and details that put the hash before the subject | The upper right shows repository → module → compact directories → files, with container counts. Lower-right details begin with the full subject, then SHA, author email, local date/minute, and containing branches, followed by body and parent commits. |

The upper-right Diff file tree uses filename colors for change types: green for added/copied, blue for modified, red for deleted, and gold for renamed/type changes. No `A/M/D/R/C/T` letters or reserved status-letter width follow filenames. File type icons keep their separate type colors.

Commit references use IDEA-style outlined tag icons: HEAD yellow, remote purple, local green (including the current branch), and Git tags warm orange. HEAD plus a local branch on the same commit shows green + yellow; matching upstream adds purple first, with the name combined as `origin & main`. Icons are separated by one cell. Narrow columns shorten names first and preserve the first reference's whole icon group; compact refs and left/right placement use the same rule. Detached HEAD has only a yellow icon and does not invent a local branch. Unicode / ASCII use `◇` / `#` instead of outlined tags.

The lower-right containing-branches list is always fully expanded, one branch per line with the same icons/colors as commit rows. Only HEAD is yellow; all local branches, including the current one, are green, and remote branches are purple. The count remains, but Show all / Show less and Space expansion are removed. Overflow scrolls with details; long names wrap.

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Find a branch or history | Click Branch or tag on the left or Text or hash in the center; `.*` toggles regex and `Cc` toggles case sensitivity | `/` in the relevant pane. While editing text search, `Alt-R` / `Alt-C` also toggle options; Enter applies and Esc cancels. |
| Filter | Click Branch, User, Date, or Paths and select beneath that button. Clear removes only that filter. | `b/a/d/p` opens the corresponding menu; use arrows or type and press Enter. `x` clears filters. User `me` matches each repository's Git author email. Paths accepts lines of text or repository/directory/file checkboxes. |
| Browse and review | Click a commit to update the right pane; double-click a file for Diff. Containing branches remain expanded; click parents to navigate. | Commit list: `j/k` browses, `gg/G` selects first/last, `H/M/L` selects top/middle/bottom on screen, `Ctrl-F/B` pages, `Ctrl-D/U` half-pages, and `PageDown/PageUp` pages. `h/l` collapses/expands or changes pane; `Tab/F6` changes pane. In files, `z/Z` collapses/expands all, Enter opens a patch, and Esc returns. In details, `j/k`, paging, or the wheel browses all branches. |
| Copy a commit | — | In the commit list, `yy` copies the full ID, `ym` the complete subject/body, and `ya` `Name <email>`. A notification reports success/failure. |
| Adjust the view | Drag visible dividers to resize. The top-right eye toggles branch tree, details, or reference labels under Show, and current-branch highlighting under Highlight. | `B/I` hides/restores left/right sidebars; `+` appends history. `H/L` in the commit list navigates within the screen. Below 82 columns, top tabs select the visible area. |
| Keep a filter set | Top `+` opens a Log tab; click tabs to switch, or `×` to close. Global Roots remains shared. | `Ctrl-T` creates, `Ctrl-W c` closes, and `[/]` switches. Returning restores that tab's filters and visible selected SHA. |

The text search field's `.*` and `Cc` controls share its background, with the right border after `Cc`. Enabled options remain blue; both outer borders turn blue while editing.

Clicking outside an open filter menu cancels unapplied input and still performs the click: User switches to User, a commit row selects it, and Roots opens repository selection. Clicking the original filter button simply closes the menu. Clicking the border or input field does not select a commit underneath.

**Branch** opens below its button. Main entries are `Select...`, `Favorites` (when more than one favorite exists), gold-starred branches, a separator, `Local`, and each `remote/...` group. Existing main/master branches, remote default branches, and HEAD are starred by default. Hovering/clicking a group opens full branch names. Favorites show a filled star; others show an outline only when highlighted. Clicking the star or pressing Space changes only the favorite; clicking the name or pressing Enter applies the filter. Arrows or `j/k` navigate, `→` expands, `←` / `Esc` returns one level, and the wheel scrolls. When two columns cannot fit, the submenu includes a back row.

[Terminal screenshot: expanded remote branches](reference/IGit-Log-Branch.png). Grouping and star click behavior follow [IntelliJ's Branch filter](https://github.com/JetBrains/intellij-community/blob/master/platform/vcs-log/impl/src/com/intellij/vcs/log/ui/filter/BranchFilterPopupComponent.java).

Typing directly in Branch searches. `Select...` supports `|` or pasted lines to combine branches, `-branch` to exclude reachable history, `A..B` for the difference between branches, and expressions such as `origin/feat/.*` to match names. Tab completes; Enter or Ctrl-Enter applies; Esc discards the draft. Use `refs/heads/…` or `refs/tags/…` to distinguish a branch and tag with the same name. `Favorites` applies the currently visible favorite set at once. Clearing Branch removes the filter but preserves favorites.

Paths contains **Select...**, **Select in Tree...**, and the **Roots** repository list, in that order.

- **Select...** opens multiline input. Each line is a file or directory relative to a repository root; commits changing any listed path are included. `api/src` targets the repository named `api`; `api/` selects the whole repository. To match a path literally named `api/src` in each repository, use `./api/src`. Multiline paste and historical paths deleted from the working tree are supported. `Enter` adds a line; `F2`, `Ctrl-S`, `Ctrl-Enter`, or **Apply** confirms. Applying empty input clears paths. Absolute paths or lines containing `..` report their position and preserve the draft.
- **Select in Tree...** starts with collapsed repositories and locations within global scope. Arrows, `h/l`, or left/right expand/collapse. Checkboxes or Space toggle selection; parents show partial selection. One selection may combine repositories, directories, and files. Repositories unchecked in this tab's Roots are unavailable. `Enter` or **OK** applies. Both selectors accept `Esc` or **Cancel** to discard changes.
- **Roots** initially checks all repositories in global scope. Clicking a row or pressing Space after up/down navigation updates the tab immediately and leaves the menu open. The heading shows selected/listed counts; clicking it or `Ctrl-A` toggles the entire group. Unchecking all produces an empty history. Typing searches repositories; Enter on an exact repository name selects only it, while other text is treated as a path. Clearing Paths resets this tab's repositories and paths while retaining other filters and global Roots.

Date has exactly three entries: `Select...`, `Last 24 hours`, and `Last 7 days`, with no search field. Initial selection is `Select...`, or the currently applied preset. Arrows or `j/k` move; Enter or click applies. `Select...` opens From / To fields in `YYYY-MM-DD` format. Tab / Shift-Tab or clicking changes field; Ctrl-U clears it. Enter / Apply applies, and Esc / Cancel cancels. Custom ranges include local 00:00:00 on the start date through 23:59:59 on the end date, including same-day ranges. Invalid/missing dates or a start after the end preserve the draft and report the error. Cancel leaves the active condition unchanged; reopening restores it. Relative periods end at query time and are recalculated on refresh. Clearing dates preserves other filters.

Default history starts from local branches, remote-tracking branches, tags, and valid HEAD; the existence of a stash does not add its index snapshot. Text, hashes, and reference names are searched within loaded results; author, date, and path conditions are passed to Git. Missing/filtered parents produce a notice, never a jump across repositories.

Holding `j/k` or up/down moves the highlight, right-pane title, and hash with selection. Once movement stops, files, body, and containing branches load for that row. Previous-row files are not shown while waiting. Enter can still open the current commit's Diff; Esc can cancel the wait.

The commit list keeps its vertical viewport, scrolling down only after selection passes its last visible commit, or up after its first. `H/M/L` selects only within the current screen without scrolling; partial screens use actual commits to determine middle/bottom. Full/half pages move both viewport and selection by the usable rows after toolbar/error space, clamped to history bounds; half a page is at least one row. Resize, filtering, and parent jumps keep selection visible. A lone `g` or `y` waits for a continuation; other actions, focus changes, clicks, or overlays cancel the sequence. `g/y` in input fields remains text. Copy always targets the current commit's repository without waiting for preview; an empty list leaves the clipboard untouched.

The [actual 180×42 terminal menu](reference/IGit-Log-View-Options.png) shows the eye menu and Columns submenu. Corresponding 48×12, 80×24, and 120×36 terminal walkthroughs and mouse hit areas have also been verified.

**Eye menu (View Options).** A dedicated eye at the far right of the graph toolbar has reserved space. In narrow single-area mode, it follows Refs / Graph / Files / Details. `o` opens/closes; `j/k`, up/down, and the wheel select; `Space/Enter` toggles immediately. The main menu has Show / Highlight sections. Click, hover, or `l/Right` opens Columns; `h/Left` or `Esc` returns, then `Esc` closes. The menu prefers below the trigger. Submenus use the right or left side, or replace the main menu when neither fits. Short windows scroll around selection. Borders/padding block underlying rows; clicking outside closes and performs that click. Clicking another split changes focus directly; top-bar Roots changes repository scope.

Show, Highlight, and Columns use Commit-list checkboxes: checked is bold blue `#629DFF`, unchecked gray `#8B8F9A`. Checkbox colors are independent of labels and survive blue row highlights; unavailable labels stay dim. The Columns entry has only aligned blank space and a submenu arrow.

**Column order.** Columns initially lists Author, Hash, Date, GPG Signature, Graph, and Message. Top-to-bottom corresponds to left-to-right in history; moving Date above Author puts dates to the left. `K/J` or `Shift+↑/↓` moves the selected column one position earlier/later; right-edge arrows do the same. Highlight follows the moved column; first/last items cannot wrap. Clicking a checkbox or name only toggles visibility. Hidden columns remain in the menu for advance positioning. Repository markers stay at the start. Graph can move between any columns, with its jump arrows. Message moves/hides its subject and refs together and takes the width left by other columns.

Show's first item, **Branch or Tag**, controls the left branch tree. When visible, it starts at the left edge; hiding it gives that space to commits while retaining branch filters and selected commit. Focus returns from a hidden branch tree to commits. Narrow mode also hides Refs; the eye menu or `B` always restores it. Commit Details controls the right-side files/details; Reference Labels controls commit-row refs.

Root Names expands repository color markers into names. Compact References View keeps the first ref plus a remaining count. Tag Names controls only text; icons remain when disabled. With Long Edges off, edges longer than 12 rows with no intervening join show only endpoints and jump arrows; enabling it restores full connections. Shared rails remain continuous, without drawing filtered-out parent/child links. Commit Timestamp switches author/commit time without changing topology or sort order. References on the Left moves refs before subjects. All six columns toggle independently. Narrow views prioritize enabled subjects and graph; hiding them releases space for other columns.

Under Highlight, My Commits matches each repository's Git author email and uses bold. Merge Commits dims merge text. Current Branch uses blue for HEAD-reachable history; selection is always brighter. Not Cherry-Picked Commits becomes available only when Branch selects another local/remote branch, and dims equivalent applied patches using `git cherry`; otherwise, the menu asks for another Branch. Defaults enable Branch or Tag, Commit Details, Reference Labels, Author, Date, Graph, Message, and the first three highlights. Other Show options and cherry highlighting are off. All Log tabs share options for the session; restart restores defaults. Dates default to author time. Signatures are checked only when the column is enabled and display Good, Unsigned, Untrusted, Bad, Expired, Key exp., Revoked, Unchecked, or Unknown; the last two do not indicate successful verification. Implementation: [display.rs](../src/log/display.rs). Semantics reference: [official IDEA Log documentation](https://www.jetbrains.com/help/idea/log-tab.html).

## 2 · Commit: IDEA Changes List

The [three icon modes](reference/IGit-Icon-Modes.png) show output from the same [practice repository](../examples/demo-commit.sh): 11 tracked changes and 8 untracked files. The [110×36 flat-layout reference](reference/IGit-Commit-Flat.png) retains glyphs from before the redesign. Compared with the [old permanent Overview](reference/IGit-Commit.png), the default gives the entire width to the list.

```text
 ▾ ☑ ▤ Changes 11 files
   ▾ ☑ ▱ moss-web 11 files  ├ master
       ☑ TS app.ts       moss-web/src/store/modules
       ☑ V  index.vue    moss-web/src/layouts/default/customer-service
       ☑ V  index.vue    moss-web/src/views/system/ui-config
 ▾ ☐ ⊕ Unversioned Files 8 files
   ▾ ☐ ▱ moss-web 8 files  ├ master
       ☐ TS aihub.ts     moss-web/src/layouts/default/customer-service
```

Both groups start expanded. Within each repository, files sort case-insensitively by filename, then parent directory for duplicate names. Modified is blue, added green, and untracked red. Repositories and directories use expanded/collapsed folder icons; repositories retain their color markers, directories use warm yellow, and submodules retain a dedicated marker. Narrow layouts preserve folder markers before truncating names. Branch names use gray badges. Only newly encountered committable Changes files start checked. Blue rows indicate browsing, checkboxes indicate commit candidates; gray parent paths and markers do not change identity. Log and Commit use the same type markers/colors for a file; Commit can additionally show Git XY status.

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Browse and check | Row names browse, arrows expand/collapse, boxes check, and the wheel scrolls. Toolbar hover shows action/shortcut. Each target has its own hit area. | `j/k` browses, `PgUp/PgDn` pages, `h/l` collapses/expands or visits parent/child, `Space` checks; `u` undoes only the last checkbox change. |
| Preview a patch or scope | Double-click a file; the toolbar preview button opens the current row. In wide mode, keep clicking other left-side rows. Use close/back-to-list to return. | `Enter` on a file opens Diff; `v` toggles preview. Groups, repositories, and directories preview their visible scope. At 92+ columns, preview is side by side; below that, it fills the tool window. `Esc` returns to the original row; `Tab/F6` switches available areas. |
| Change layout or inspect raw status | The eye opens View Options; final two buttons expand/collapse all; Diff previews the current row. | `o` opens the menu, `t` toggles directories, `s` toggles Git XY status, and `z/Z` collapses/expands all. Layout does not change identity or checkboxes. |
| Manage groups | Add creates a group; Move, right-click, or dragging tracked files moves them. | `n/m` creates/moves groups. Untracked files remain separate; regrouping does not run git add. |
| Write messages and commit | Click a bottom repository name to change draft, or the message area to edit. Amend applies to that repository; Commit… and Commit & Push… both review first. | `[/]` changes repository, `i` edits, `F2/Ctrl-S` saves, and `Esc` cancels. `a` toggles Amend; `c/P` reviews commit / commit then Push. `A/S` opens AI candidates/settings. |

**The commit target is the actual patch.** If a file has staged content, preview and commit use its index version; later working-tree edits remain. Selection is per file, without line-level splitting. AI first shows the per-repository scope to send and requests only after confirmation; accepting the returned candidate separately replaces the draft. Implementation: [commit.rs](../src/commit.rs), [commit/backend.rs](../src/commit/backend.rs).

**Display menu.** Under `Group By`, Directory and Module toggle independently. Module means Git repository and defaults on; Directory defaults off. Module alone keeps the flat list. Both off produces a flat cross-repository filename list with repository names retained in parent paths. Directory alone builds a workspace-relative tree so identically named directories in different repositories stay distinct. Switching preserves the current file and checkboxes where possible. If a container disappears, selection moves to an existing parent. Only ancestors needed for the current row expand; other collapse preferences remain.

`Show` offers Ignored Files and Git Status, both off by default. Ignored Files adds gray, read-only entries with unavailable markers. Ignored directories remain single folder entries without recursive content reads; preview only explains their ignored status. Ignored items cannot move groups, be checked, or enter commit/AI plans. Toggling display preserves checkboxes and the last-checkbox undo record. Options last only for the session. The menu opens below its trigger and stays within the window; short windows scroll to selection. `j/k`, arrows, Tab, wheel, or hover moves; `Space/Enter` toggles immediately; `Esc/o` closes. Borders/padding block underlying rows. Clicking outside closes and performs that click; another split can take focus directly, and top-bar Roots can change scope.

## 3 · Push: Review Source, Destination, and Scope

```text
 Push Commits · 2 repositories
 Outgoing commits                         │ Changed files · 3
 Target  Fetch                            │ Group By  Details          [+] [-]
 ▸ [ ] api   main → origin:main ↑1        │ ▾ web 3 files
 ▾ [x] web   main → origin:main ↑2        │   ▾ src 2 files
       Commit message                     │       App.vue
                                          │       main.ts
                                          │     README.md
 ─────────────────────────────────────────────────────────────
 [ ] Follow tags  [x] Hooks                       Cancel  Push N
```

The window has one contextual title: `Push Commits to <repository>` for a single repository, or a repository count for a batch. Pane headings and toolbars align. Target / Fetch sit above outgoing commits; Retry appears there only after a push failure in visible scope. Tree grouping/details sit on the right, with compact expand/collapse icons at its far edge. Both lists meet a shared footer separator rather than separate bottom borders. The footer uses quiet inline options, a secondary Cancel button, and one highlighted Push N button. At narrow widths, actions move onto a separate row; hidden checked repositories retain a separate notice. Toolbars keep text labels where possible; pane headings shorten to preserve complete counts. Push N counts only visible, checked repositories eligible for review and is disabled with no candidates or while a remote operation runs. Cancel closes the window and restores the original workspace focus, including when Details is open; target editing must first be saved or canceled. During a remote operation it reads Close, since closing the window leaves that operation running.

Left/right proportions are about **50%** each. The left sorts by repository name, with separate left-aligned columns for name, source branch, arrow, target, outgoing count, and submodule. Success/failure markers do not shift branch columns. Pushable targets are blue; repositories with no outgoing commits are dim. Narrow layouts hide secondary columns first. Long text clips only within its own column; Target and details still expose the full destination.

The right defaults to a full-height repository → directory → file tree, avoiding a repeated content-root name. Consecutive single-child directories compress to paths such as `dev/iframe-proxy`; directories show descendant file counts. Icons share Log / Commit type colors. Filenames use change colors: added/copied green, modified blue, deleted red, renamed/type changes amber, without trailing status letters. Only file rows open a file Diff; repository/directory details summarize scope. Opening details splits the right side about 60/40%; closing restores the same tree row.

Click the eye or press `o` for the Group By menu. Directory and Repository start enabled; Module (the repository's content root) starts disabled and can be enabled independently. All eight combinations remain available. Without Directory, files sort flat by name, then repository-relative path, with gray parent paths at the end. Disabling Repository or Module only removes that level. Switching preserves the current file and open patch; if a selected container is hidden, a still-visible parent is preferred. Grouping persists across repository/commit changes, refresh, and reopening Push within the workspace session.

Repository rows preview the batch; commit rows preview that commit. Merge commit files/Diffs compare against the first parent. If the current repository has no outgoing commits or no row is selected, the file area centers `No commits selected` without empty repository nodes; read failures still show specific reasons. The push unit is always the repository's target branch; an intermediate commit cannot be pushed alone. At most 60 recent commits per repository are displayed, but counts and execution use the full branch.

| Task | Mouse | Vim / keyboard |
| --- | --- | --- |
| Select and inspect | Boxes check repositories, triangles expand/collapse, rows update the right side, and files open Diff. | `j/k` browses; `h/l` or left/right collapses/expands; `h` in files can visit the parent. `Space` checks the owning repository; `Enter` expands/collapses containers or opens files; `Tab/F6` changes area. |
| Change file grouping | Click Group By on the right, then options; the menu stays open for further changes. | `o` opens, `j/k` or ↑/↓ selects, `Space/Enter` toggles, and `Esc/o` closes. Space in the menu changes only grouping. |
| File tree and details | Toolbar Expand / Collapse changes all nodes; Details toggles summary/Diff. | `[/]` collapses/expands all in files; `d` toggles details and `Esc` closes back to the tree. |
| Review target and options | Click the blue target or outgoing toolbar's Target to edit; Follow tags / Hooks toggles options; Fetch updates remote state in scope. | `t` edits remote:branch; Enter saves and Esc cancels. `v/x` toggles Follow tags/hooks; `f` fetches. |
| Execute and handle failures | Push N → review each source SHA and target → Confirm; inspect failed rows and click Retry. | `p` reviews, `y` confirms, and `R` retries only failed repositories still in scope with outgoing work. |

On first direct entry, a repository with outgoing commits is selected. Entry from Commit & Push sets checkboxes from the repositories that just committed successfully. Success is not rolled back because another repository failed. Changed remote tips, non-fast-forwards, and unverifiable changed submodule pointers show reasons, not success. Implementation: [push.rs](../src/push.rs), [push/submodules.rs](../src/push/submodules.rs).

## 4 · Merge: Apply File and Continue Repository Are Separate

```text
 Conflict list (repository + file) → Open file
 Repository / path   Operation   Remaining conflicts
 ‹ Files   ↑ [c   ↓ ]c   Apply non-conflicting [a]   Undo [u]   Zoom [z]
 Current (read-only) » │ Result (editable) │ « Incoming (read-only)
                      │    Common base    │
   Same change block  │   Current result  │    Same change block
 Block: Take left / Take right / Take both / Edit    Draft / Apply file
```

| Task | Mouse | Vim / keyboard |
| --- | --- | --- |
| Find files and conflict blocks | Double-click a file or click Open. Top arrows jump conflicts; all three panes scroll vertically together. | List: `j/k`, `Enter`. Document: `[c` / `]c` are two-key sequences; `j/k` changes block and `h/l` changes pane. |
| Resolve changes | Apply non-conflicting takes only definite changes. Click left/right gutters or block buttons and inspect the center. | `a` applies safe blocks, `H/L` takes left/right, `b` takes left then right, and `u` undoes. |
| Edit the result | Edit the current block in the center; use editor buttons to accept/cancel. Zoom enlarges the current pane. | `i` edits, `Ctrl-S` or `Ctrl-Enter` accepts, `Esc` cancels, `z` shows one pane, and `s` saves a session draft. |
| Write and continue | Apply file → review and confirm writes/stages only that path. Return to the list and separately choose Continue for the repository operation. | `A` in the document reviews application; `C` in the list reviews continuation. `Esc` from a file returns to the list. |

The default has three equal panes. Below **72 columns**, only the focused pane is shown; Apply non-conflicting becomes Safe, and previous/next block buttons show only directions. Side labels identify actual operation sources: rebase shows the base and replayed commit, not fixed local/remote labels. External file edits or changed index state cause refusal to overwrite. Unsupported binary, delete/modify, and special-attribute cases require external handling. No conflicts produces an empty state. This panel handles existing Git operations; it has no branch buttons to create merge/rebase operations. Implementation: [merge.rs](../src/merge.rs), [merge/engine.rs](../src/merge/engine.rs).

## Commit Comparison Acceptance

1. Run the [Commit practice script](../examples/demo-commit.sh), start IGit with the printed path, and press `2`. At 120×36 and 80×24, confirm all 19 items can be reached by scrolling the upper half while history remains visible below. Distinguish checked, partial, unchecked, unavailable, and independent row highlighting. With `--icons ascii`, click both ends of a three-cell checkbox: either end must toggle only that item; clicking a filename must only browse.
2. Open both `index.vue` files and verify their paths/patches. Switch to directory mode, collapse/expand ancestors, and switch back to flat mode while retaining the same file. Container previews show only their scope. Browsing, collapsing, and layout switching must not change checkboxes or the index.
3. Check only `scripts/iframe-proxy.mjs` and open Diff. It has both staged and unstaged changes; preview and commit review must contain only the staged version. Cancel review, then compare HEAD, the real index, and the working tree.
4. Recheck repository scope with the [multi-repository practice script](../examples/demo-workspace.sh): hide a checked repository, open commit review, and confirm only visible candidates appear. See [PRD A2 / A7 / A9](prd.md).
