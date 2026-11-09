# IGit Interaction Design: Workspace Panels and the Commit Baseline

**Pane maximize:** In Commit, Branch, Stash, Push, and Conflicts, `Ctrl-W f` shows the focused pane across the workspace.
Tab changes the maximized pane within the active tab;
workspace shortcuts still switch tabs. Hidden panes clear their mouse targets
and selections. Restoring preserves the prior split sizes, including after a
terminal resize. Divider dragging and local size changes are suspended while
maximized. Log stays tiled and resizable. Push and Conflicts retain their essential actions. The conflict editor's `z`
zoom remains a separate document preference.

An optional `smart-toggle-pane-maximized` binding synchronizes with Kitty's
stack/previous layout, following LazyDB. It uses an explicit verified target
state, falls back to local toggle where supported for a single Kitty group or outside Kitty,
and ignores repeats and editor/review contexts. See
[configuration](configuration.md#pane-maximize-and-kitty-synchronization).

**Baseline: the native 0.1.0 TUI.** Layout descriptions refer to the native client. The Commit section compares its changes list with IDEA; the final section describes how to repeat acceptance checks. See [README](../README.md) for startup and dependencies. The original [interactive prototype](reference/IGit.html), [design notes](reference/IGit-Design.html), and [four-panel overview](reference/IGit-Overview.png) are available offline; prototype buttons and shortcuts do not imply implemented features.

## Workspace and Focus

Shortcuts below describe the built-in defaults. Optional
[`settings.toml`](configuration.md) overrides normal-mode commands by name,
including multi-key sequences, aliases, and unbinding. Help, sequence candidates,
and shortcut-bearing controls show the effective keys. Text entry,
selection, menus, and write confirmations keep their own controls; F1 and Esc
remain available. Mouse capture and icons are startup settings, and the sequence
hint delay is configurable (200ms by default). Changing a binding never changes
the action's repository scope or review requirements.

Optional `smart-focus-pane-left/down/up/right` bindings share directional focus
with Kitty and Neovim. Internal visible panes take precedence; an application
edge hands focus to the neighboring Kitty window without wrapping. Navigation
stays within the active tab before handing off. Editors, menus, help,
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

Run `igit` or `ig` from the working directory. A single header row places Log filter tabs or the Push title on the left and workspace tabs on the right. The active tab fills the area between the header and status line. Startup focus is the Commit changes list. The selected tab has a blue background; a blue border marks the pane receiving keyboard input. Below five rows of height, only side borders remain to preserve content space.

The bottom-left footer reads `IGit v<VERSION>`, followed by contextual hints or
notifications. Checking, Update, Updating, Restart, and update-failure indicators
appear beside the version. The version and indicator share one clipped mouse
target; the brand and remaining hints do not open the update window. The footer
also remains visible below the full-screen conflict editor.

`F9` in normal mode, the version-label click, and Actions → Update IGit open
Update Center. Its cyan rounded frame fades in and fits the status, version,
and channel. A current installation shows one centered **OK** button. When
there is a choice, **Not now** starts focused. `Tab`/arrows switch buttons;
`Enter` or a click activates; `Esc` closes. `r` checks again after a current or
failed check; `j/k` or the wheel scrolls long details. The footer lists available
shortcuts and also accepts clicks. Input and write-review dialogs keep their
own keyboard controls. Background checks never open a window or install a
release. The progress bar fills from downloaded bytes when the total is known;
otherwise it moves while the current stage is shown. **Run in background**
closes the dialog while installation continues. Completion offers a separate
restart after text entry and remote operations have finished.

Startup first paints a Workspace loading view, before scanning directories or
reading Git state. It reports `Discovering repositories...`, `Loading changes...`,
and `Checking conflicts...`. Resize remains active; `Esc`, `q`, or `Ctrl-C` exits
without a review because no editing session has begun. Commit becomes available
after changes and conflicts are loaded. The first Log entry shows `Loading
history...` while its worker runs; workspace tabs, help, and quit remain usable,
but history-specific input waits until loading finishes. An empty-workspace
message appears only after discovery completes.

```text
 Log   +                                          Commit  Log  Push  Branch  Stash  Conflicts
   Branch tree     │  Commit graph + filters        │  Files
                   │                                │  Commit details
 IGit  v0.1.0  Tab Panes  Ctrl-W Pane keys  p Pull  f Fetch  F9 Updates  F1 Help
```

- **Keep four kinds of selection distinct.** The highlighted row determines what is being viewed; checkboxes determine candidates for the operation; Roots limits which repositories are available; an explicit action starts execution. Moving the highlight must not silently check a row. Commit/Push retain selections hidden by filters, but exclude them from the current operation. When a workspace has at least one repository, global Roots cannot be applied empty.
- Click a workspace tab or press `1` for Commit, `2` for Log, `3` for Push, `4` for Branch, or `5` for Stash. Each panel retains its cursor, checkboxes, and filters. Log's own filter tabs and `+` share the header row on the left; their hit areas stop before the workspace tabs.
- **Conflicts** appears in bold amber while any repository in Roots has unmerged files or an operation awaiting Continue. Open it by clicking or pressing `6`. When the last pending operation ends, the tab disappears and returns to the last Commit, Log, Branch or Stash tab if it was active. The same tabs are available from Actions (`:`).
- In Push or Conflicts, `Esc` first backs out of editing, details, or the conflict document, then returns to the last Commit, Log, Branch or Stash tab. Commit & Push opens the Push tab after committing; pushing requires a separate review.
- Press `r` for repository filtering (in Branch, use `:` → Roots because `r` renames a branch): `j/k` moves, `Space` toggles, `a` selects all, `Enter` applies, and `Esc` cancels. The header has no Console, Roots, Actions, or Help buttons. Use `:` for Actions and its Console entry, or `?` / `F1` for help. `F5` refreshes the workspace and updates Conflicts after external Git changes. Actions → Open / rescan workspace reruns discovery.
- In normal mode, `Tab` / `Shift-Tab` cycles panes within the current tab (`F6` is equivalent). Commit cycles Changes, Preview (if open), and Message; Log cycles Branches, Commits, Files, and Details. Branch cycles Repositories and the branch tree when Roots includes multiple repositories; Stash cycles Repositories and Files. Hidden sidebars are skipped. `Ctrl-W` followed by `h/j/k/l` focuses the adjacent left/down/up/right pane and stops at the outside edge. Browsing focus is independent of file checkboxes.
- `Ctrl-W` followed by `>` / `<` increases/decreases the current pane's width; `+` / `-` changes height. A count can follow the prefix, such as `Ctrl-W 10 +`. Drag a target border directly; movement stops at its bounds and ends on release. Text selection and file dragging still belong to the area where the gesture began. `Ctrl-W =` restores default proportions for the active tab. Single-column mode has no vertical divider.
- Editors, filters, help, and review retain their own Tab and input rules. Shell review dialogs receive events before panels. `Tab` switches buttons, `Enter` activates the focused button, and `Esc` cancels. Push, Stash, Pull, and Fetch reviews open with the available primary action focused. Other shared confirmations start on **Cancel** and also accept `y/n`. Long review content scrolls.
- **Text selection.** Log commit details / Diff, Commit message editing / Diff preview, and Push Details / Diff support left-button dragging with a blue highlight. Release copies once and clears the selection. Empty selections and ordinary clicks do not copy; dragging does not activate links. The editing caret stays at the drag endpoint after release. Keyboard selections support `Ctrl-C`, `Ctrl-Insert`, and `y` in read-only text; `Ctrl-A` selects the current text area. In read-only panes, `Esc` clears a selection before its normal back action. Selection refers to original text and does not split wide Unicode characters, combining characters, or emoji. Soft wraps and decorative separators are excluded from the clipboard; original blank lines, indentation, and line breaks remain. Drag and keyboard selections survive scrolling/resizing; changing the object or hiding the area clears stale selection. Dialogs take event priority; canceling help restores keyboard selection. Parent commit links activate on release only without dragging. The Commit editor supports Shift+Arrow / Home / End; typing/pasting replaces keyboard selection, while Backspace/Delete removes it. The wheel scrolls long messages; keyboard movement scrolls only when the caret leaves the viewport. Text dragging does not change file checkboxes, groups, or Git state.
- **Context help.** `?` / `F1` captures the current focus or dialog as context. It follows lazydb's help style: centered, at most 74 columns × 34 rows, rounded borders, dark blue background, cyan `KEYMAP // area` title and Search, bold blue keys, white descriptions, and a gray footer. The list has no group headings or count row. Each item uses one line, with `>` for the current candidate and descriptions clipped by terminal cell width. Search ignores case; each space-separated term must match characters in order within a shortcut, description, or area name. Matches may contain gaps. Filtering and highlighting use the same matcher, preferring contiguous occurrences over a scattered match in each field. Matched characters in shortcuts and descriptions use bold amber text, or bold underlined text with `NO_COLOR`; whole Unicode graphemes remain intact and a clipping ellipsis is never highlighted. Rows matching only their area name may have no highlighted characters in the visible row. At normal widths, the key column is 18 cells; below 60 columns, it uses the longest shortcut plus two cells, capped at 18, to leave more room for descriptions. The footer selects a complete hint that fits and retains Esc to close. Search opens with a caret; ordinary characters including j/k/g/G enter text. Left/right, Home/End, Backspace/Delete, and Ctrl-W/U/A/E edit. ↑/↓ cycles candidates; PgUp/PgDn changes selection by a page. Clicking only selects, while the wheel/scrollbar changes the viewport; selection stays visible. Enter closes help, restores the original area or dialog, then executes the candidate's static key sequence. With no results, Enter leaves help open. Available actions appear in current-area, panel-wide, then global order; duplicate keys keep the higher-priority entry. Switch focus to see another area's actions. Input fields and dialogs list only applicable actions; F1 does not cancel input. Esc/F1 cancels help and restores state unchanged. Commit, push, and merge application keep their existing review paths.
- **Key sequence hints.** Normal-mode `Ctrl-W`, Branch `g`, Log commit-list `g` / `y`, and Merge conflict-document `[` / `]` open a bottom overlay after 200ms without another key. It lists only valid continuation keys and descriptions for the current prefix. Typing a continuation executes immediately; completing/canceling during the delay never shows the hint, and a new prefix restarts the timer. Once visible, arrows select by displayed columns; Enter or clicking executes. Candidate navigation is not intercepted before the overlay appears. Highlighting a candidate does not change the selected commit/conflict block. Esc/Ctrl-C cancels only the prefix and keeps Merge's document open. Candidates use one to three columns based on actual cell width, without covering the bottom status line. Borders, padding, and wheel events block underlying controls; clicking outside cancels and passes through that click. Other actions, focus changes, overlays, paste, loss of focus, or resizing clear the prefix. Text input never opens candidates.
- Recommended size: **120×36 cells**. Below 44 columns or 12 rows, Shell shows a terminal-too-small message. All panels and Roots use Commit's [shared palette](../src/ui/idea.rs): dark gray background `#1C1D22`, body text `#CFD2DB`, secondary text `#8B8F9A`, blue focus `#74A4F8`, and selected-row background `#314675`. Blue marks the browsing row; checkboxes indicate inclusion. Push and Commit patches use green/red for additions/deletions. Merge uses amber warnings for unresolved conflicts and green completion markers for resolved blocks. A shared bottom line displays `IGit v<VERSION>` and update status, followed by action hints or the latest message.

**Language and text width.** Product-owned labels, messages, help, and documentation use English. Use short action labels and measure terminal cell widths when positioning text or hit areas. User-provided repository names, paths, messages, and content retain Unicode support.

**Icons and fonts.** All panels and Roots share repository, submodule, file, and selection markers. Default `nerd-font` uses named glyphs; `--icons unicode` uses standard symbols; `--icons ascii` uses text markers. Icon widths are not fixed: ASCII `[x]` occupies three cells, all of which form its hit area. When a file marker cannot fit, preserve the filename first. The input caret, current row, and committable state each have independent meanings. Exact glyphs in diagrams vary by mode.

File lists and existing file-preview titles share the [file type mapping](../src/ui/icons.rs). Icon color indicates type, while filename color still indicates Git status. Java uses a coffee cup, Python its language marker, and React JSX / TSX a React marker (cyan/blue respectively); ordinary JS / TS retain their own icons. Common JVM, systems, scripting, and mobile languages, XML, and build files have dedicated markers. Detection checks filenames and compound suffixes such as `pom.xml` and `build.gradle.kts` before extensions. Matching generally ignores ASCII case, while preserving the `.C` / `.H` convention for C++. Detection uses paths only, without reading content. Unknown types use a generic file icon; Unicode / ASCII modes retain readable abbreviations.

### Update Project and Fetch

In normal mode, `p` opens Update Project and `f` opens Fetch Repositories. Both are also in Actions. Each review follows Push's visual hierarchy: a contextual title strip, blue pane heading, compact toolbar, tinted repository icons, bold names, aligned target/status columns, blue browsing row, and a separated footer with one primary action. Pull shows the current branch receiving its upstream; Fetch lists remotes without an integration arrow. Both show Branch's green `↗N` / blue `↙N` counters beside the current branch (ASCII `^N` / `vN`). The comparison uses the configured upstream, including differently named branches, and local refs; opening review performs no fetch. Zero and unavailable counts stay hidden. Counter space is reserved before clipping branch names. Wide layouts retain separate branch and submodule columns. Narrow layouts combine the branch and target and hide optional columns; complete branch/upstream names, counters, targets, and reasons remain in wrapped Details below the list. Repositories that cannot run the operation show an amber Skipped status and its reason. Global Roots still defines the entire batch; browsing is independent of scope, and Log tab filters and Push checkboxes do not change this list.

Pull starts with **Merge incoming changes** selected; `r` selects **Rebase current branches**, and `m` switches back. Click either toolbar option to select it; labels shorten to Merge / Rebase on narrow terminals. Fetch has no integration choices. Browse repositories with `j/k`, arrows, PageUp/PageDown, Home/End (`G` also selects the last row), row clicks, or the wheel. Details follows that row; scroll long content using `J/K` or the wheel over Details. Pull / Fetch starts focused when available; otherwise Cancel has focus. Only the focused button has a blue background; the other button has a neutral background, and neither uses a focus underline. Use `h/l`, `Tab`, or `Shift-Tab` to switch between Cancel and Pull / Fetch, and `Enter` or a click to activate a button. The primary label is simply **Pull** or **Fetch**, with repository counts in the summary/footer. Wide footers show Tab/Enter hints. Pull / Fetch is disabled when no repository is ready. `Esc` or Cancel returns to the original pane without running Git. During execution, the same rows remain visible with Pending results and no action buttons. Completion refreshes the branch comparison and displays separate success/failure/skip counts and colored row statuses, initially selecting the first failed repository. Details contains its complete error. If Git leaves conflicts, close results and open the highlighted Conflicts tab.

## 1 · Log: IDEA Git Tool Window Reference

The [actual terminal render at 180×42](reference/IGit-Log-Graph.png) uses the [temporary history workspace](../examples/demo-log.sh), with `User: me` and `Paths: atlas-web`. Filter values, authors, files, and refs come from Git. The image is a reference for the existing layout. Current file markers depend on icon mode; the ASCII graph uses `*` / `@` for ordinary commits / HEAD and characters such as `|` and `-` for edges.

Log shows the branch tree on the left, the commit graph in the center, and files above details on the right. Enabled panes stay visible at all supported widths, with smaller sidebars in narrow terminals.

| Previous layout issue | Current behavior |
| --- | --- |
| Repeated Local/Remote groups per repository; long `origin/feat/…` lists were hard to scan | The left pane has fixed HEAD / Local / Remote / Tags groups. Identical full refs share one row, annotated with repository names when present in only some Roots. Remote and slash-prefix groups collapse. The current branch stays first; upstream ahead/behind counts come from Git. |
| Large title boxes, headers, and a bottom repository legend consumed rows; long refs pushed subjects off screen | Wide layout proportions are about **17 / 57 / 26%** for left/center/right, with **65 / 35%** files/details on the right. Adjacent borders are draggable; filter values stay next to the toolbar. Each commit uses one row: author, time, graph, subject, then right-aligned refs. |
| Repository and branch edges shared colors, obscuring merge origins | A centered, heavier color strip in a neutral left gutter identifies the repository and matches its Paths checkbox. Consecutive same-repository rows form a segment with shortened ends and a gap between repositories; isolated commits use an aligned short vertical stroke with space above and below, keeping one consistent shape even when repositories alternate frequently. Segment boundaries use repository IDs, even when palette colors repeat. Scrolling does not add false ends, and filtering uses visible adjacency. Standard glyphs provide the same spacing in every icon mode, with square half-line caps rather than pixel-rounded corners. Reference-name colors identify branch histories, and fragment colors distinguish their merged ancestors; unnamed heads use gold. Colors stay attached to the history as lanes move. Light blue marks history reachable from that repository's HEAD; selection uses brighter blue outside the gutter. Merge commit text is dimmed. Identical SHAs in different repositories never connect. |
| Commit nodes sat beside rails, or vertical edges shifted while scrolling | Every real parent edge stays independent until its endpoint; lines crossing between commits do not join. IDEA's reference/DFS order determines left-to-right placement, and each row compacts its active edges. Lane changes connect across adjacent rows, including when a long edge disappears into an arrow. Filled ordinary nodes and ring-shaped HEAD nodes stay on their tracks; compatible terminals use connected dots and diagonal mid-line glyphs. Generic Unicode/ASCII approximate these paths on the cell grid; pixel-perfect antialiasing and two colors inside one cell are unavailable. All rows share one viewport origin. At narrow widths, `‹/›` means additional rails exist; `◀/▶` means that row's commit is off screen. Selecting an off-screen commit pans the whole graph. |
| Flat folders and details that put the hash before the subject | The upper right shows repository → module → compact directories → files, with container counts. Lower-right details begin with the full subject, then SHA, author email, local date/minute, and containing branches, followed by body and parent commits. |

The upper-right Diff file tree uses filename colors for change types: green for added/copied, blue for modified, red for deleted, and gold for renamed/type changes. No `A/M/D/R/C/T` letters or reserved status-letter width follow filenames. File type icons keep their separate type colors.

For a merge commit, the Diff tree starts with a combined file group, followed by `Changes to <parent hash> <subject>` for every parent. The combined group contains files changed against all parents; a parent-only change remains under that parent. Each group has its own folds, counts and file identity, even when a path appears several times. Ordinary commits retain the repository/module tree.

Opening any file from a two-parent merge, including every `Changes to` group, fills the workspace with a read-only **Repository Diff**: first parent on the left, the committed result in the center, and second parent on the right. Lock icons and immutable hashes label the three versions. Source line numbers, blank alignment rows, blue changed blocks, stronger character highlights, basic code colors and four-line context folds follow the conflict editor's visual conventions; historical differences are never presented as unresolved conflicts. Mode and line-ending notes distinguish empty/absent files and mode-only changes. `n/p` or the toolbar moves between changes; `Tab/Shift-Tab` switches the focused revision; `j/k`, paging and the wheel scroll all panes together; `h/l` or each pane's scrollbar scrolls that revision horizontally. `f` folds/unfolds context, clicking a fold opens it, and `z` zooms the focused revision. Ordinary commits use two equally sized panes: **Before** (parent) and **After** (selected commit), with the same alignment, highlighting, folding and scrolling. Initial commits use an explicitly empty Before side; added/deleted files show an absent side. Below 72 columns for three panes or 48 for two, one revision is shown at a time with Tab cycling only the available versions. Dragging text copies on release; `Ctrl-A` selects the focused revision, including folded context, and `Ctrl-C`/`y` copies the selection. Without a selection, `y` copies the complete focused revision. Copy preserves original tabs, Unicode and line endings without line numbers, fold labels or alignment padding. `Esc` clears a selection before closing. The viewer provides no editing, staging, Apply or Continue actions.

`[/]` and the Diff toolbar browse file comparisons in tree order, including repeated paths under different parents. The commit parent count determines the viewer, independently of the selected tree group; opening a parent-only path still includes both parent versions and the committed result. Binary/non-UTF-8 blobs, entries other than blobs, files above 512 KiB per revision, and merges with more than two parents show an explicit explanation and the Git patch; individual parent groups remain available. Renames retain the existing no-renames representation (deleted and added paths). Terminal cells approximate IDEA's separators and folding lines; graphical curves and pixel-level rendering are outside the TUI.

Commit references use IDEA-style outlined tag icons: HEAD yellow, remote purple, local green (including the current branch), and Git tags warm orange. HEAD plus a local branch on the same commit shows green + yellow; matching upstream adds purple first, with the name combined as `origin & main`. Icons are separated by one cell. Narrow columns shorten names first and preserve the first reference's whole icon group; compact refs and left/right placement use the same rule. Detached HEAD has only a yellow icon and does not invent a local branch. Unicode / ASCII use `◇` / `#` instead of outlined tags.

The lower-right containing-branches list is always fully expanded, one branch per line with the same icons/colors as commit rows. Only HEAD is yellow; all local branches, including the current one, are green, and remote branches are purple. The count remains, but Show all / Show less and Space expansion are removed. Overflow scrolls with details; long names wrap.

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Find a branch or history | Click Branch or tag on the left or Text or hash in the center; `.*` toggles regex and `Cc` toggles case sensitivity | `/` searches commit text or hash. In the branch tree, `S` searches branch/tag names and `Enter` or `s` selects the highlighted ref. While editing text search, `Alt-R` / `Alt-C` toggle options; Enter applies and Esc cancels. |
| Filter | Click Branch, User, Date, or Paths and select beneath that button. Clear removes only that filter. | Press `Space`, then `b/u/d/p` for Branch/User/Date/Paths. Wait after `Space` to see the choices. `x` clears filters. User `me` matches each repository's Git author email. Paths accepts lines of text or repository/directory/file checkboxes. |
| Browse and review | Click a commit to update the right pane; double-click a file for Diff. Containing branches remain expanded; click parents to navigate. | Commit list: `j/k` browses, `gg/G` selects first/last, `H/M/L` selects top/middle/bottom on screen, `Ctrl-F/B` pages, `Ctrl-D/U` half-pages, and `PageDown/PageUp` pages. `h/l` collapses/expands or changes pane; `Tab/F6` changes pane. In files, `z/Z` collapses/expands all, Enter opens a patch or the merge comparison, and Esc returns. In details, `j/k`, paging, or the wheel browses all branches. |
| Copy a commit | — | In the commit list, `yy` copies the full ID, `ym` the complete subject/body, and `ya` `Name <email>`. A notification reports success/failure. |
| Act on a commit | Right-click its row, or click `...` for the selected row. | `a` opens Commit Actions. Use `j/k` or arrows and Enter. `F2` opens Edit Commit Message directly. |
| Adjust the view | Drag visible dividers to resize. The top-right eye toggles branch tree, details, or reference labels under Show, and current-branch highlighting under Highlight. | `B/I` hides/restores left/right sidebars; `+` appends history. `H/L` in the commit list navigates within the screen. Below 82 columns, top tabs select the visible area. |
| Keep a filter set | Top `+` opens a Log tab; click tabs to switch, or `×` to close. Global Roots remains shared. | `Ctrl-T` creates, `Ctrl-W c` closes, and `[/]` switches. Returning restores that tab's filters and visible selected SHA. |

The text search field's `.*` and `Cc` controls share its background, with the right border after `Cc`. Enabled options remain blue; both outer borders turn blue while editing.

### Commit actions

The menu captures the commit and repository from the row. The review shows the repository, commit and effect. Cancel starts focused; use Tab then Enter, or click Confirm. Esc cancels. Browse and double-click for Diff as before.

| Action | Result |
| --- | --- |
| New Branch / New Tag | Enter a name and review. The new local branch or lightweight tag points to the selected commit. Existing refs and the checkout stay in place. |
| Checkout Revision | Check out the selected commit in detached HEAD. Git protects local changes and ignored files that would be overwritten. |
| Undo Commit | Move the current branch tip back one commit. The real index and working files stay intact; the undone changes are available in Commit. Requires the tip to have one parent. |
| Revert Commit | Add a Git-generated inverse commit on the checked-out branch. Requires a clean tracked worktree and index and a non-merge target. If Git stops with conflicts, use Conflicts to resolve and continue. |
| Cherry-pick Commit | Apply the selected non-merge commit on the current local branch after review. Requires a clean tracked worktree/index; protected local paths and stale reviews stop execution. Native conflicts remain in Conflicts for separately reviewed Continue, Abort, or Skip. |
| Drop Commit | Remove the selected commit from the current branch and replay later commits. The range must be linear and the tracked worktree and index clean. If Git stops, use Conflicts to resolve and continue, or review Abort there. |
| Edit Commit Message | Edit the prefilled subject and body. Enter inserts a newline; F2 or Ctrl-S opens review. Editing the current tip preserves staged and unstaged work; editing an earlier commit replays a clean linear range. Commits reachable from local remote-tracking refs cannot be edited. |

Drop and Undo act locally. The review calls out a known remote-tracking ref when rewriting history that may already have been pushed.

Clicking outside an open filter menu cancels unapplied input and still performs the click: User switches to User, a commit row selects it, and Roots opens repository selection. Clicking the original filter button simply closes the menu. Clicking the border or input field does not select a commit underneath.

**Branch** opens below its button. Main entries are `Select...`, `Favorites` (when more than one favorite exists), gold-starred branches, a separator, `Local`, and each `remote/...` group. Existing main/master branches, remote default branches, and HEAD are starred by default. Hovering/clicking a group opens full branch names. Favorites show a filled star; others show an outline only when highlighted. Clicking the star or pressing Space changes only the favorite; clicking the name or pressing Enter applies the filter. Arrows or `j/k` navigate, `→` expands, `←` / `Esc` returns one level, and the wheel scrolls. When two columns cannot fit, the submenu includes a back row.

[Terminal screenshot: expanded remote branches](reference/IGit-Log-Branch.png). Grouping and star click behavior follow [IntelliJ's Branch filter](https://github.com/JetBrains/intellij-community/blob/master/platform/vcs-log/impl/src/com/intellij/vcs/log/ui/filter/BranchFilterPopupComponent.java).

Typing directly in Branch searches. `Select...` supports `|` or pasted lines to combine branches, `-branch` to exclude reachable history, `A..B` for the difference between branches, and expressions such as `origin/feat/.*` to match names. Tab completes; Enter or Ctrl-Enter applies; Esc discards the draft. Use `refs/heads/…` or `refs/tags/…` to distinguish a branch and tag with the same name. `Favorites` applies the currently visible favorite set at once. Clearing Branch removes the filter but preserves favorites.

Paths contains **Select...**, **Select in Tree...**, and the **Roots** repository list, in that order.

- **Select...** opens multiline input. Each line is a file or directory relative to a repository root; commits changing any listed path are included. `api/src` targets the repository named `api`; `api/` selects the whole repository. To match a path literally named `api/src` in each repository, use `./api/src`. Multiline paste and historical paths deleted from the working tree are supported. `Enter` adds a line; `F2`, `Ctrl-S`, `Ctrl-Enter`, or **Apply** confirms. Applying empty input clears paths. Absolute paths or lines containing `..` report their position and preserve the draft.
- **Select in Tree...** starts with collapsed repositories and locations within global scope. Arrows, `h/l`, or left/right expand/collapse. Checkboxes or Space toggle selection; parents show partial selection. One selection may combine repositories, directories, and files. Repositories unchecked in this tab's Roots are unavailable. `Enter` or **OK** applies. Both selectors accept `Esc` or **Cancel** to discard changes.
- **Roots** initially checks all repositories in global scope. Clicking a row or pressing Space after up/down navigation updates the tab immediately and leaves the menu open. The heading shows selected/listed counts; clicking it or `Ctrl-A` toggles the entire group. Unchecking all produces an empty history. Typing searches repositories; Enter on an exact repository name selects only it, while other text is treated as a path. Clearing Paths resets this tab's repositories and paths while retaining other filters and global Roots.

The left reference tree intersects global Roots, this tab's checked Roots, and the repositories addressed by applied paths. Local branches, remote branches, and tags are grouped only within that scope, including their repository annotations, current/default-branch markers, and ahead/behind counts. Roots toggles update the tree immediately; text/tree drafts update it only after confirmation. Unqualified paths keep every checked repository eligible. A repository's refs remain available even when no commits match its paths. Reference search and folder states still apply, and clearing Paths or switching Log tabs rebuilds the tree for the restored scope.

Date has exactly three entries: `Select...`, `Last 24 hours`, and `Last 7 days`, with no search field. Initial selection is `Select...`, or the currently applied preset. Arrows or `j/k` move; Enter or click applies. `Select...` opens From / To fields in `YYYY-MM-DD` format. Tab / Shift-Tab or clicking changes field; Ctrl-U clears it. Enter / Apply applies, and Esc / Cancel cancels. Custom ranges include local 00:00:00 on the start date through 23:59:59 on the end date, including same-day ranges. Invalid/missing dates or a start after the end preserve the draft and report the error. Cancel leaves the active condition unchanged; reopening restores it. Relative periods end at query time and are recalculated on refresh. Clearing dates preserves other filters.

Default history starts from local branches, remote-tracking branches, tags, and valid HEAD; the existence of a stash does not add its index snapshot. Text, hashes, and reference names are searched within loaded results; author, date, and path conditions are passed to Git. Missing/filtered parents produce a notice, never a jump across repositories.

Holding `j/k` or up/down moves the highlight, right-pane title, and hash with selection. Once movement stops, files, body, and containing branches load for that row. Previous-row files are not shown while waiting. Enter can still open the current commit's Diff; Esc can cancel the wait.

The commit list keeps its vertical viewport, scrolling down only after selection passes its last visible commit, or up after its first. `H/M/L` selects only within the current screen without scrolling; partial screens use actual commits to determine middle/bottom. Full/half pages move both viewport and selection by the usable rows after toolbar/error space, clamped to history bounds; half a page is at least one row. Resize, filtering, and parent jumps keep selection visible. A lone `g` or `y` waits for a continuation; other actions, focus changes, clicks, or overlays cancel the sequence. `g/y` in input fields remains text. Copy always targets the current commit's repository without waiting for preview; an empty list leaves the clipboard untouched.

The [actual 180×42 terminal menu](reference/IGit-Log-View-Options.png) shows the eye menu and Columns submenu. Corresponding 48×12, 80×24, and 120×36 terminal walkthroughs and mouse hit areas have also been verified.

**Eye menu (View Options).** A dedicated eye at the far right of the graph toolbar has reserved space at all widths. `o` opens/closes; `j/k`, up/down, and the wheel select; `Space/Enter` toggles immediately. The main menu has Show / Highlight sections. Click, hover, or `l/Right` opens Columns; `h/Left` or `Esc` returns, then `Esc` closes. The menu prefers below the trigger. Submenus use the right or left side, or replace the main menu when neither fits. Short windows scroll around selection. Borders/padding block underlying rows; clicking outside closes and performs that click. Click a workspace tab to switch panels. Close the menu and press `r` to change repository scope.

Show, Highlight, and Columns use Commit-list checkboxes: checked is bold blue `#629DFF`, unchecked gray `#8B8F9A`. Checkbox colors are independent of labels and survive blue row highlights; unavailable labels stay dim. The Columns entry has only aligned blank space and a submenu arrow.

**Column order.** Columns initially lists Author, Hash, Date, GPG Signature, Graph, and Message. Top-to-bottom corresponds to left-to-right in history; moving Date above Author puts dates to the left. `K/J` or `Shift+↑/↓` moves the selected column one position earlier/later; right-edge arrows do the same. Highlight follows the moved column; first/last items cannot wrap. Clicking a checkbox or name only toggles visibility. Hidden columns remain in the menu for advance positioning. Repository markers stay at the start. Graph can move between any columns, with its jump arrows. Message moves/hides its subject and refs together and takes the width left by other columns.

Show's first item, **Branch or Tag**, controls the left branch tree. When visible, it starts at the left edge; hiding it gives that space to commits while retaining branch filters and selected commit. Focus returns from a hidden branch tree to commits. The eye menu or `B` restores the tree. Commit Details controls the right-side files/details; Reference Labels controls commit-row refs.

Root Names adds repository names beside the color strip. Compact References View keeps the first ref plus a remaining count. Tag Names controls only text; icons remain when disabled. With Long Edges off, edges spanning at least 30 visible row intervals have arrows one row from each endpoint, and hidden middle sections occupy no columns. Enabling Long Edges restores those connections while retaining endpoint navigation arrows; edges spanning at least 1,000 intervals still fold, retaining 250 rows at each end. These are IDEA's visibility thresholds. Separate edges to the same parent are evaluated independently. Filtering never invents a parent/child link across a missing commit. Commit Timestamp switches author/commit time without changing topology or sort order. References on the Left moves refs before subjects. All six columns toggle independently. Narrow views prioritize enabled subjects and graph; hiding them releases space for other columns.

Under Highlight, My Commits matches each repository's Git author email and uses bold. Merge Commits dims merge text. Current Branch uses blue for HEAD-reachable history; selection is always brighter. Not Cherry-Picked Commits becomes available only when Branch selects another local/remote branch, and dims equivalent applied patches using `git cherry`; otherwise, the menu asks for another Branch. Defaults enable Branch or Tag, Commit Details, Reference Labels, Author, Date, Graph, Message, and the first three highlights. Other Show options and cherry highlighting are off. All Log tabs share options for the session; restart restores defaults. Dates default to author time. Signatures are checked only when the column is enabled and display Good, Unsigned, Untrusted, Bad, Expired, Key exp., Revoked, Unchecked, or Unknown; the last two do not indicate successful verification. Implementation: [display.rs](../src/log/display.rs). Semantics reference: [official IDEA Log documentation](https://www.jetbrains.com/help/idea/log-tab.html).

### File History in Neovim and the Terminal

`igit log <file>` adds a closeable `History: <filename>` tab beside the normal
Log tab and starts on it. The left half contains a compact Branch toolbar and
the existing author/date/graph/message rows. The right half previews the selected
file immediately, without an overlay or an intermediate Files tree. Its toolbar
contains change navigation, folding, zoom, and a change count. The revision row
contains immutable hashes and the result filename. Two-parent merges show
Parent 1, Result, and Parent 2; ordinary/root/deletion commits show Before/After.
Syntax colors, changed text, line numbers, context folding, and synchronized
scrolling use the same read-only revision renderer as the normal Log Diff.

`j/k` and row clicks select commits and replace the preview after the navigation
debounce. `J/K` scrolls the preview down/up from either pane without changing the
selected commit. `Enter` focuses Diff; `Tab`/`Shift-Tab` and `Ctrl-W h/l` move
between the two panes. Drag the divider or use `Ctrl-W >/<` to resize. In Diff, `[/]` changes
the focused revision, `n/p` changes the selected difference, `f` toggles folding,
`z` zooms, and `Esc` clears a selection before returning to the history list.
In this context, `p/f` belong to the preview. Global workspace tabs remain
available. Narrow previews show one revision at a time; `[/]` reaches every
source. The history tab closes with its header close control or `Ctrl-W c`.

The file identity and owning repository are fixed for that history tab. Branch
and text filters remain independent of the ordinary Log tab; Reset retains the
file identity. Global Roots can hide its repository without selecting another
repository automatically. Deleted and renamed files read immutable blobs at
their historical paths. Binary, oversized, and unsupported comparisons display
an explanation and the existing patch fallback. No preview reads from the index
or current working-file contents.

`igit.nvim` embeds the same TUI in float, left, right, bottom, or tab-page windows.
Its `:Igit` command forwards panel arguments and resolves `%` from the originating
file buffer. Matching sessions are reused, while different requests keep their
own running buffers. The reference IDE screenshot defines the history/Diff
composition; rendering uses terminal cells and the user's terminal font/background,
so it does not reproduce IDE pixels or translucent wallpaper.

Refreshes, Git-filter changes and Log-tab switching read history, optional
presentation metadata and graph projections on a background worker. A visible
loading status distinguishes cached rows from current results. Cached rows can
be navigated, while commit actions and new comparisons wait for the current
Roots/filter generation. A later request supersedes an earlier result.

## 2 · Commit: IDEA Changes List

The [three icon modes](reference/IGit-Icon-Modes.png) show output from the same [practice repository](../examples/demo-commit.sh): 11 tracked changes and 8 untracked files. The [110×36 flat-layout reference](reference/IGit-Commit-Flat.png) and example below show Module grouping with Directory disabled. Preview starts enabled. At 92+ columns, the list and preview appear side by side. Narrow terminals show Changes first and switch to full-width preview with `v`, Enter on a file, or Tab.

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

Both groups start expanded. In the flat list, files sort case-insensitively by filename, then parent directory for duplicate names. Modified is blue, added green, and untracked red. Repositories and directories use expanded/collapsed folder icons; repositories retain their color markers, directories use warm yellow, and submodules retain a dedicated marker. Narrow layouts preserve folder markers before truncating names. Branch names use gray badges. Only newly encountered committable Changes files start checked. Blue rows indicate browsing, checkboxes indicate commit candidates; gray parent paths and markers do not change identity. Log and Commit use the same type markers/colors for a file; Commit can additionally show Git XY status.

With unmerged files in Roots, **Changes** counts them and shows one expanded **Merge Conflicts** child before ordinary changes. Its flat file rows retain type icons and show red filenames and gray `repository/parent` hints, regardless of Directory and Module settings. The screenshot-style blue marks on conflict-only rows are read-only; group checkboxes still affect only eligible files. The focused conflict row uses a gray highlight. Git XY appears when requested or in `NO_COLOR` mode, which uses unavailable marks for conflicts. Existing changelist membership is retained for after resolution.

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Browse and check | Row names browse, arrows expand/collapse, boxes check, and the wheel scrolls. Toolbar hover shows action/shortcut. Each target has its own hit area. | `j/k` browses, `PgUp/PgDn` pages, `h/l` collapses/expands or visits parent/child, `Space` checks; `u` undoes only the last checkbox change. |
| Open conflict resolution | Click the blue, underlined **Resolve** link on Merge Conflicts. Clicking the label browses; clicking the arrow folds. | `Enter` on Merge Conflicts opens the Conflicts tab. Use `h/l` to fold or expand the group. |
| Preview a patch or scope | Double-click a file; the toolbar preview button opens the current row. In wide mode, keep clicking other left-side rows. Use close/back-to-list to return. | `Enter` on a file opens Diff; `v` toggles preview. `J/K` scrolls a visible file preview down/up without moving focus or selection. Groups, repositories, and directories preview their visible scope. At 92+ columns, preview is side by side; below that, it fills the tool window. `Esc` returns to the original row; `Tab/F6` switches available areas. |
| Change layout or inspect raw status | The eye opens View Options; final two buttons expand/collapse all; Diff previews the current row. | `o` opens the menu, `t` toggles directories, `T` toggles Git XY status, and `z/Z` collapses/expands all. Layout does not change identity or checkboxes. |
| Manage groups | Add creates a group; Move, right-click, or dragging tracked files moves them. | `n/m` creates/moves groups. Untracked files remain separate; regrouping does not run git add. |
| Add untracked files to Git | Highlight a file or container in Changes. | `a` or `Insert` immediately starts adding the highlighted untracked file or the untracked members of its group, repository, or directory, including folded children, without confirmation. An inline `Adding N files…` indicator appears in existing Changes/footer space before background Git work. Wider layouts show Checking / Staging / Updating, the current repository, and actual completed repository counts. Browsing, Help, tab switching, and message editing remain available; write controls are disabled and repeated Add requests reuse the current job. No percentage or staged file color is invented while waiting. Completion updates affected Changes rows and reports separate results; checkboxes and the message are retained. Failed files remain available for retry, with full errors in Console. `M` toggles Amend. |
| Write messages and commit | Click the message area to edit directly in the bottom Commit pane. The composer has one shared message and no repository tabs. Its action area contains Commit, Commit and Push..., and a secondary Generate message button with the Resolve Automatically wand icon; narrow panes wrap actions or shorten labels. Clicking another control saves the shared draft first. **Commit** starts the local commit immediately. **Commit and Push...** does the same, then opens Push for a separate review. Amend remains available through `M` and is indicated in the selection summary. | `[/]` browses repositories in Changes in normal mode. From Changes or Preview, `c` focuses Message and starts editing. In the editor, Enter adds a newline, `Ctrl-W` deletes the previous word and trailing whitespace, and `Ctrl-U` deletes to the logical line start. Both deletion keys remove a selection first. `Esc` and `F2/Ctrl-S` save and leave Message focused. `Tab/Shift-Tab/F6` saves and moves focus. In Message, Left/Right moves the blue selection between Commit, Commit and Push..., and Generate message. Commit starts selected when focus enters the pane. Enter runs the selected action; `i` reopens the editor. `M` toggles Amend for the highlighted file/group's repository (a group spanning repositories requires a more specific row); `P` commits and opens Push. `A` generates and fills the shared message; `S` opens AI settings. |

The **Overview** lists files by repository. Each path uses its Git status color and a file type icon when space permits, without a status letter column.

The inline editor uses the message pane's existing rectangle, including in narrow terminals; it does not cover Changes or Git Log. The active Message pane keeps its blue border. Long messages scroll to the caret and support wheel scrolling, Unicode selection, and drag-to-copy. Resize the Changes / Message divider in normal mode for more writing space. Clicking another workspace pane saves before moving focus; a save failure keeps the input active and blocks navigation or submission. F1 help preserves the input, caret, and selection. Changelist and AI settings editors retain their dialogs; AI failures use a dismissible error dialog.

Commit uses native file events and per-repository debounce: a 300ms quiet period, capped at two seconds during continuous changes. One background worker reads only affected repositories in Roots and refreshes the current Diff only when its repository is scanned. Tab entry and terminal focus process queued changes without forcing a full scan. A 30-second Roots scan reconciles missed or unsupported events. Unchanged scans retain preview selection and scrolling; changes preserve checked paths, folds, cursor identity, the shared draft, and caret. Reviews, menus, and drags defer updates without discarding queued changes. Late results from before an explicit refresh or ignored-files toggle are discarded. F5 still refreshes the entire workspace.

**The commit target is the actual patch.** If a file has staged content, preview and commit use its index version; later working-tree edits remain. Selection is per file, without line-level splitting. Each selected repository receives a separate commit with the shared message. Generate message / `A` immediately sends one combined request for checked files in the current Roots. The button shows a rotating progress indicator. Success fills and saves the editable shared draft directly; failure opens an error dialog and preserves the draft. Edits made while waiting are kept, and repeated clicks do not send another request. Endpoint, model and prompt can be set in `[ai]`; see [configuration](configuration.md#ai-commit-messages). Implementation: [commit.rs](../src/commit.rs), [commit/backend.rs](../src/commit/backend.rs).

**Display menu.** Under `Group By`, Directory starts on and Module (Git repository) starts off. They toggle independently. The default directory tree keeps identically named directories in different repositories distinct. Module alone groups flat files by repository. With both off, files appear in a single flat list within each changelist, with repository names retained in parent paths. Menu checkboxes match the changes list: checked is bold blue, unchecked is gray. Switching preserves the current file and checkboxes where possible. If a container disappears, selection moves to an existing parent. Only ancestors needed for the current row expand; other collapse preferences remain.

`Show` offers Ignored Files and Git Status, both off by default. Ignored Files adds gray, read-only entries with unavailable markers. Ignored directories remain single folder entries without recursive content reads; preview only explains their ignored status. Ignored items cannot move groups, be checked, or enter commit/AI plans. Toggling display preserves checkboxes and the last-checkbox undo record. Options last only for the session. The menu opens below its trigger and stays within the window; short windows scroll to selection. `j/k`, arrows, Tab, wheel, or hover moves; `Space/Enter` toggles immediately; `Esc/o` closes. Borders/padding block underlying rows. Clicking outside closes and performs that click; workspace tabs switch panels. Close the menu and press `r` to change repository scope.

**Create stash (`s`).** A centered message editor opens without changing the
commit draft. Enter, Ctrl-S, F2, or **Review** opens a confirmation containing
one shared message, repository names/branches, every affected path, staged
and working-tree patches, and any skipped-repository errors. This action uses
all changes in global Roots, including unchecked and untracked files; ignored
files remain untouched. Cancel initially has focus. `Esc` cancels, while `y`
confirms the batch. Each repository is rechecked and saved independently.
Results appear in the status line and Session console; reopening `s` retains
the stash message and skips clean repositories. A stash that was saved but
could not be cleaned is reported with its hash and blocked from repeated
saving until the residual working-tree changes are resolved. `6` opens the
saved entries and their read-only comparisons.

**Rollback Changes.** `d` in Changes opens a centered modal with all changes in the current Roots. The highlighted file, directory, repository, or changelist sets the initial checks independently of Commit checkboxes. Changelists (including Changes and Unversioned Files) precheck only their own eligible members. Directory/repository preselection crosses changelists and includes collapsed children. Other changes start unchecked and can be added from the same tree. Preview and message panes retain their own input behavior. `commit.discard` can be remapped and appears in contextual help.

The review uses the Commit tree's filename colors, icons, disclosures, and tri-state checkboxes. It starts with repository and directory grouping; the eye / `o` toggles grouping independently of the main list. `j/k` or arrows browse, `h/l` fold or navigate, `z/Z` collapse/expand all, and Space or a checkbox click changes selection. Clicking a label highlights it. Use `v` or Diff to inspect both staged and working-tree patches for the highlighted file. The footer shows counts for checked files and explicitly labels untracked files that will be deleted. **Delete local copies of added files** is off initially and is available only for checked additions with local copies; click it or press `a` to toggle it.

**Close** is focused initially. `Tab` changes focus, `Enter` activates the focused control, `Esc` backs out of Diff/grouping or closes the modal, and `y` or **Rollback** applies. Rollback is disabled with no checked files. Tracked files return to HEAD in the index and working tree; untracked files are removed individually, and added files are unstaged with their local copies kept unless deletion is enabled. There is no undo. Hidden repositories are excluded. Ignored files shown by Commit, conflicts, and other unavailable paths stay read-only in the tree; Diff explains why they cannot be rolled back. Unchecked paths are preserved, and a changed checked snapshot requires another review. Results refresh the workspace; partial failure reports completed files without clearing the remaining index entries.

### Edit the highlighted working file

`e` / `E` in Commit Changes opens the highlighted file's working copy,
independently of checkboxes and container selection. A container asks the user
to choose a file; a deleted path or submodule directory reports that its working
file is unavailable. Message input, previews, menus and review dialogs retain
their own keys. The action is configurable as `commit.edit-file` and appears in
contextual Help.

Standalone execution uses Git's configured editor and temporarily restores the
normal terminal, including mouse, focus, paste and enhanced-key modes. Returning
from the editor reacquires the TUI and refreshes status while retaining the
current row, draft and checked paths. Errors also restore the TUI.

In `igit.nvim`, the same key switches the source editing window to the target
buffer. Loaded buffers retain unsaved content. Floating windows hide after a
successful switch; split windows remain visible and tab sessions remain open.
IGit keeps running, and its watcher processes saved working-copy changes.
Modified-buffer guards or unavailable targets leave the IGit window available
with an error. Opening a file changes neither HEAD nor the index.

**Hunk staging.** `Ctrl-G` from Changes/Preview or the Hunks toolbar control
opens a file-local Staged/Unstaged view. Tab switches sides; `u/s` selects a side;
`n/p` visits complete hunks; `j/k`, paging and the wheel scroll. Space/Enter or
the primary button opens a separate Stage/Unstage review with Cancel focused.
Both sides and navigation/review/close controls have mouse targets at 44, 80,
and 120 columns. Esc closes the view. Index changes preserve working files,
unrelated staging, checkboxes and the message. Unsupported conversions and
special files explain the need for whole-file or external staging.

**Commit progress.** Confirmed batches run in the background and display the
current repository/total in the Commit status area. Normal browsing and pane
navigation remain available. Ctrl-X stops unstarted repositories after the
current repository finishes; completed SHAs stay recorded. Other writes,
workspace replacement, Roots changes, and quitting wait for batch completion.

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
 [ ] Follow tags  [x] Hooks                         Cancel  Push
```

The Push tab has one contextual title: `Push Commits to <repository>` for a single repository, or a repository count for a batch. Pane headings and toolbars align. Target / Fetch sit above outgoing commits; Retry appears there only after a push failure in visible scope. Tree grouping/details sit on the right, with compact expand/collapse icons at its far edge. Both lists meet a shared footer separator rather than separate bottom borders. The footer uses quiet inline options, a secondary Cancel button, and one highlighted Push button. At narrow widths, actions move onto a separate row; hidden checked repositories retain a separate notice. Toolbars keep text labels where possible; pane headings shorten to preserve complete counts. Push is disabled when no visible, checked repository is eligible or while a remote operation runs. Cancel returns to the last Commit, Log, Branch or Stash tab, including when Details is open; target editing must first be saved or canceled. During a remote operation it reads Close and the operation continues in the background.

Opening Push shows `Loading outgoing commits…` immediately. On later visits,
existing rows and previews remain visible under `Refreshing outgoing commits…`.
Cancel, tab switching, browsing, and quit remain available during these reads;
Push, Retry, Fetch, and target editing wait for the current snapshot. File lists
and Diff load in the background with their own loading text. Newer Roots, target,
or preview selections supersede pending results. Refresh retains checkboxes and
uses commit identity to preserve the highlighted row; an unchanged snapshot also
retains the selected file, folds, and Diff scroll.

Left/right proportions are about **50%** each. The left sorts by repository name, with separate left-aligned columns for name, source branch, arrow, target, outgoing count, and submodule. Success/failure markers do not shift branch columns. Pushable targets are blue; repositories with no outgoing commits are dim. Every repository retains its checkbox: blue when checked, gray when unchecked, including when there are no outgoing commits. Rows with no outgoing commits cannot be toggled or pushed. Narrow layouts hide secondary columns first. Long text clips only within its own column; Target and details still expose the full destination.

The right defaults to a full-height repository → directory → file tree, avoiding a repeated content-root name. Consecutive single-child directories compress to paths such as `dev/iframe-proxy`; directories show descendant file counts. Icons share Log / Commit type colors. Filenames use change colors: added/copied green, modified blue, deleted red, renamed/type changes amber, without trailing status letters. Details summarizes the current scope and splits the right side about 60/40%.

Click the eye or press `o` for the Group By menu. Directory and Repository start enabled; Module (the repository's content root) starts disabled and can be enabled independently. All eight combinations remain available. Without Directory, files sort flat by name, then repository-relative path, with gray parent paths at the end. Disabling Repository or Module only removes that level. Switching preserves the selected file; if a selected container is hidden, a still-visible parent is preferred. Grouping persists across repository/commit changes, refresh, and reopening Push within the workspace session.

Repository rows compare the last fetched target with the local branch tip. Commit rows compare the first parent with that commit, including merges. A new target or initial commit has an empty Before side. If the current repository has no outgoing commits or no row is selected, the file area centers `No commits selected` without empty repository nodes; read failures still show specific reasons. The push unit is always the repository's target branch; an intermediate commit cannot be pushed alone. At most 60 recent commits per repository are displayed, but counts and execution use the full branch.

Opening a file fills the workspace with Log's read-only **Repository Diff**.
Before and After share aligned line numbers, syntax and change highlighting,
context folds, and synchronized vertical scrolling. Below 48 columns, Tab
switches the visible revision. Binary, large, and other unsupported revisions
show a Git patch with a reason. Closing restores the file tree with the last
viewed file highlighted.

| Task | Mouse | Vim / keyboard |
| --- | --- | --- |
| Select and inspect | Boxes check repositories, triangles expand/collapse, rows update the right side, and files open Diff. | `j/k` browses; `h/l` or left/right collapses/expands; `h` in files can visit the parent. `Space` checks the owning repository; `Enter` expands/collapses containers or opens files; `Tab/F6` changes area. |
| Change file grouping | Click Group By on the right, then options; the menu stays open for further changes. | `o` opens, `j/k` or ↑/↓ selects, `Space/Enter` toggles, and `Esc/o` closes. Space in the menu changes only grouping. |
| File tree and details | Toolbar Expand / Collapse changes all nodes. Details opens Diff on a selected file or toggles the scope summary. | `[/]` collapses/expands all in files; `d` runs Details. |
| Inspect a Diff | Use the toolbar to navigate changes and files, fold context, zoom, or close. | `n/p` moves between changes, `[/]` moves between visible files, `f` folds context, `z` zooms, and `Esc` closes after clearing any text selection. |
| Review target and options | Click the blue target or outgoing toolbar's Target to edit; Follow tags / Hooks toggles options; Fetch updates Push targets in scope. | `t` edits remote:branch; Enter saves and Esc cancels. `v/x` toggles Follow tags/hooks; `F` fetches Push targets. |
| Execute and handle failures | Push → review each source SHA and target → Confirm; inspect failed rows and click Retry. | `P` opens review with Confirm focused; `Enter` confirms and `Esc` cancels. `R` reviews only failed repositories still in scope with outgoing work. |

On first direct entry, a repository with outgoing commits is selected. Entry from Commit & Push sets checkboxes from the repositories that just committed successfully. Success is not rolled back because another repository failed. Changed remote tips, non-fast-forwards, and unverifiable changed submodule pointers show reasons, not success. Implementation: [push.rs](../src/push.rs), [push/submodules.rs](../src/push/submodules.rs).

**First-push tracking.** Push's `t` target editor includes an unchecked Set
upstream after push option. Tab/Shift-Tab or clicking toggles it; Enter saves
and Esc cancels. Review describes the old and requested upstream. Tracking
publication failures show PUSH SUCCEEDED; UPSTREAM NOT SET without retrying the
successful remote update.

## 4 · Conflicts: Apply File and Continue Repository Are Separate

```text
 Prepare Simple Drafts [S]                                      Full Paths
 ┌ File                             │ Yours (main)      │ Theirs (origin/main) ┐
 │ ▾ × Unresolved   2 files                                                      │
 │     example.js  app · src        │ Modified           │ Modified              │
 │     README.md   app              │ Modified   Accept  │ Modified    Accept   │
 │                                                                              │
 └──────────────────────────────────────────────────────────────────────────────┘
                                             Close    Continue    Resolve Manually
```

In a single merge, the status columns show Yours and Theirs, with branch names when available. Rebase shows Rebased and Replay instead. When the list spans several repositories, the shared headings are Current and Incoming. Side statuses come from Git's index stages, and file rows show a file icon, name, and a muted repository/path hint.

Unresolved uses the same background and focus highlight as file rows. Select it with `j/k` or the arrow keys, then press `Enter` to fold or expand it. Clicking the row or pressing `Space` also toggles the group and leaves it selected. **Full Paths** (`o`) toggles repository-relative paths. **Resolve Manually** (`Enter` on a file or double-click) opens the selected file. **Accept** on either side (`Y` / `T`) previews that entire version for the file; use **Apply Changes** after reviewing the result. **Prepare Simple Drafts** (`S`) adds only unambiguous blocks to session drafts in the selected repository and leaves overlapping edits for manual resolution. Unsupported files remain listed with their stage status. At narrow widths, side status and Accept labels shorten.

The **Ready to Continue** section shows repositories whose operation is still pending after the last file is staged. Select one to enable **Continue** (`C` or `Enter`). Close returns to the prior Commit or Log tab. No list control writes the working tree or index before the corresponding confirmation.

The selected repository's native operation also offers Abort (`B`) and, for a
stopped rebase/cherry-pick/revert, Skip (`N`). Mouse buttons and contextual help
use the same routes. Each has its own Cancel-focused review explaining Git's
repository-wide discard semantics; changed operation metadata, refs, files,
index or configuration invalidate it. Stash conflicts offer neither action.
Unsupported native filenames remain visible as External without suppressing
other conflicts or the repository's pending operation.

Stash apply/pop conflicts open through the same Enter and mouse actions. Standard
EOL attributes such as `eol=lf` are accepted when checkout preserves the source
bytes; actual line-ending conversion still requires external resolution. A stash
conflict has no Continue operation. After Apply Changes, the notice directs the
user to review the result in Commit, and the Conflicts tab disappears once no
scoped conflict or pending operation remains. Applying a resolved file leaves the
retained stash untouched.

```text
 ← Merge Revisions for /workspace/app/example.js
 ↑  ↓  ⇄  ⋈  Apply non-conflicting changes: » Left  »« All  « Right  ⚡ Resolve Automatically (3) [m]  ↶   3 changes, 2 conflicts  ⚙
 Changes from main     Show Details   Result example.js     Changes from origin/main    Show Details
 ───────────────────────────────────────────────────────────────────────────────────────────────────
    left change      │ × »  2       2  base text           │  2  « × │ right change
 ∿∿ 13 unchanged lines ∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿
    left conflict    │ × » 24      24  result              │ 24  « × │ right conflict
 Accept Left   Accept Right                                             Save and Close   Apply Changes
```

| Task | Mouse | Vim / keyboard |
| --- | --- | --- |
| Navigate and compare | The top arrows visit pending changes. Click a fold to expand it, or the fold tool to toggle all. Use Compare Contents and Show Details to inspect the versions. | `[c` / `]c` jumps unresolved conflicts, `F7` / `Shift-F7` visits pending changes, `o` toggles folds, and `v` opens Compare Contents. |
| Resolve changes | Use Left / All / Right for non-conflicting changes. The inner gutter arrows accept one side, and × ignores it. For a conflict, accepting both sides appends the second. Accept Left / Right in the footer previews an entire file version. | `a` applies safe blocks, `H/L` resolves a block with one side, `b` takes both, `Y/T` accepts a whole version, and `u` undoes. |
| Resolve automatically | The wand / lightning button shows its label, eligible block count, and configured shortcut. It applies pending non-conflicting changes and disjoint edits on the same line, leaving overlapping conflicts and manual decisions intact. | `m` resolves the batch; `u` restores it in one step. Left / All / Right remains limited to non-conflicting blocks. |
| Edit and save | Click text in Result to edit it inline. Save and Close keeps a session draft and returns to the list. | `i` edits the selected block, `Ctrl-S` or `Ctrl-Enter` accepts, `Esc` cancels the edit, and `s` saves and closes. |
| Write and continue | Apply Changes opens a review after all blocks are decided. Confirmation stages only the displayed path and opens the next conflict in the same repository, if any. Review Continue separately in the list. | `A` reviews application; `C` in the list reviews continuation. `Esc` or `F` from the editor returns to the list. |

Automatic resolution has an accent label and button background when available, and dims when its count reaches zero or Result is being edited. Narrow toolbars shorten the label to **Auto** and reserve its space and Undo before optional controls. Applying the batch advances from a completed block to a remaining change and reports the applied count; it updates only the in-memory result until Apply Changes is reviewed.

The editor fills the terminal. Thin dividers separate the three aligned panes, with a wider Result in the center. Modified lines are blue, conflicts are red, and changed characters have a stronger tint. Insertions and deletions leave alignment space in the shorter pane. Long unchanged stretches start folded with four lines of context on either side. Vertical scrolling is shared, and the horizontal scrollbar and Left/Right keys scroll a single pane. Below **72 columns** or with zoom (`z`), only the focused pane is shown; Tab still reaches all three. Rebase labels identify the rebased HEAD and replayed commit. A working file that exactly matches the common ancestor opens with decisions still pending. Other edits to the working file block opening; edits made after opening or changes to the index block Apply. Unsupported binary, delete/modify, and special-attribute cases require external handling. Implementation: [merge/view.rs](../src/merge/view.rs), [merge/engine.rs](../src/merge/engine.rs).

## 5 · Branch: Repository Selector and Branch Tree

Branch is a persistent workspace tab between Push and Stash. `4` opens it.
Push and Conflicts can return to Branch as the last main tab. When Roots
includes one repository, its branch tree fills the tab without a repository
header or path. With multiple repositories, a narrow **Repositories** pane on
the left selects the tree on the right. The tiled layout works at all supported
terminal sizes and uses the standard divider and pane shortcuts. The tree has
action controls above it and details for the selected row below; with several
repositories, details also show the selected repository path. Controls wrap
at narrow widths. Mouse row selection never starts a Git write.

Branch paints before Git reads finish. The first visit shows **Loading branches…**;
returning to the same Roots keeps the cached tree browsable with **Refreshing branches…**.
Tabs, Actions, help, and quit remain available while reading. Branch actions are
disabled until the current list is ready. Changing Roots hides the old tree
immediately and discards any result from the previous scope.

The selected repository's tree has **Local Branches** and **Remote Branches**.
The left list uses Push's repository icon colors, submodule glyph, and bold
names. Each row right-aligns the current branch and its upstream counters.
The green tag precedes the branch name, and a gray chevron follows the branch
and counters at the right edge. Narrow rows may omit these markers to fit names
and counts.
Detached HEAD shows `detached@<short-id>`; an unborn branch shows its name
without counters. Long names clip to leave room for the repository label and
complete counters. The selected repository stays marked when focus moves to
the tree.
The eye at the upper right of the tree toolbar and `o` open **Group By**.
**Branch Prefix (/)** starts checked; toggle it with a click or Space/Enter.
Unchecked, both sections show full branch names at one level. Turning the
option back on reveals the selected branch in its folder path. Esc or a click
outside closes the menu. The option applies to the Branch tab for the session.

Groups show a folder glyph, disclosure arrow, and branch count. Indent guides
separate levels and give way to names in narrow panes. Blue identifies the
focused row. Both local and remote sections start expanded, including all
nested prefix folders. Branch selection and manual folds are remembered per
repository and survive refresh and temporary Roots changes.
Current-branch folders come first recursively and the current leaf is marked
`(current)`. Default-branch folders come next, then other siblings sort by
name. To identify the default branch, Branch prefers `origin/HEAD`, then
another configured remote's locally stored HEAD alias, and uses a same-name
local branch when available. Remote symbolic HEAD aliases are omitted from
the list. Local leaves append the actual upstream, with `[gone]` for missing
refs. The current branch uses Log's green tag glyph in both panes, with the
same text color as other local branches.
Other branches use the branch glyph, green for local and purple for remote.
Upstreams occupy a common column independent of indentation, label width, and current
markers; long labels clip by terminal cells. Below 64 usable columns in the
branch pane, the tree prioritizes names and the details area shows the selected
upstream on its own line. Details also identify a detached HEAD; unborn
branches show `[no commits]`.

The **Last Commit** column aligns the tip commit's committer time for local
and remote leaves as `YYYY/MM/DD, HH:MM` in local time. Folders and branches
without a commit have no time. Below 48 usable columns in the branch pane,
the column gives way to branch names.

Local branch names are followed by compact tracking counters: green `↗N` for
commits ahead of the configured upstream, then blue `↙N` for commits behind.
Diverged branches show both. Zero counts, untracked branches, missing upstreams,
and remote rows have no counters. Nerd Font uses the shared Log arrow glyphs;
Unicode uses `↗` / `↙`, and ASCII uses `^` / `v`. Name clipping reserves space
for the counters before the current/worktree suffix. Both grouped and flat
upstream-column measurements include counters on folded branches, keeping the
column stable during navigation. Selected-row details repeat the counters,
including in narrow panes. Counts come from locally stored upstream refs in
the background scan. Counts and commit times refresh on entry, F5, focus
return, and completed Fetch.

Click a repository or move the cursor in the left pane to update the right pane
from the cache. Both lists scroll independently; the wheel moves selection in
the pane under the pointer. `Tab` moves between panes, and `Enter` or `l` in
the left pane focuses the branch tree. With one repository, focus stays on the tree.
`j/k` and arrows move the focused cursor; `gg/G` or Home/End selects the first/last row.
`H/M/L` selects the first/middle/last visible row without scrolling. `Ctrl-F/B`
or PgDn/PgUp moves a full page down/up; `Ctrl-D/U` moves half a page. Both the
viewport and cursor move by the usable height of the focused list, clamped at the ends.
`g` uses the shared delayed sequence hint and cancellation lifecycle.
In the tree, `h/l` collapses/expands or moves to the parent/child; Space and
clicking a folder toggle it. Enter on a branch reviews
checkout; remote checkout asks for a local tracking name unless a matching
local name already tracks that ref. `a` asks for a new local name at the focused
commit without switching; `r` renames a local branch; `d` requests safe local
deletion; `m` merges into the currently checked-out local branch. The name
editor keeps Unicode input separate from global shortcuts and shows validation
errors without losing the draft. Toolbar actions that do not apply to the
focused row are dimmed and have no mouse action; keyboard validation still
explains invalid requests. Enter proceeds to the shared Cancel-first
review; Esc cancels. Mouse controls follow these same paths.

Reviews identify one repository path, source/destination branches and commits.
Delete and rename operate locally. Merge requires a clean index/worktree;
existing Git operations must be completed first. No force, discard, autostash,
or recursive submodule checkout is used. A failed merge refreshes Conflicts.
`r` belongs to rename in this tab; repository filtering remains in `:` → Roots.

## 6 · Stash: Saved Changes by Repository

Stash is a workspace tab, opened with `5`. Repositories in global Roots with
saved stashes appear first and start expanded. Each newest-first entry uses a
single row: message on the left, short selector and saved date/time on the
right. Narrow panes give the message priority and omit metadata as space runs
out. The row highlight identifies the current entry. Repositories with no
stashes are listed under one collapsed summary at the bottom. Failed reads
have separate repository rows; selecting one shows its error on the right.
Selection is retained by repository and object ID after a refresh, even when a
newer stash changes the selector number.

The right pane shows the selected repository and full stash selector above
Log's **Diff / Expand / Collapse** toolbar and directory tree. Narrow panes
can put the repository and selector on separate lines; roomier panes also
show the original subject and timestamp with its recorded UTC offset.
Selecting a stash loads the tree in the background after navigation settles.
Moving to another stash clears the previous files immediately;
results from older selections or Roots are discarded. `Tab` cycles the two
panes; `Ctrl-W`, divider dragging, and pane maximize follow workspace rules.
`Enter` on an entry focuses Files. `Enter`, Space, and `h/l` fold or unfold
repository groups and the empty repository summary.
Within Files, Enter on a directory toggles it; Enter on a file or a double-click
opens Log's read-only Before/After comparison. `z/Z` folds/unfolds the file tree.

Tracked files compare the first parent (base) with the saved worktree.
The optional third-parent snapshot contributes saved untracked/ignored files
with an empty Before side. The comparison reads immutable trees and does not
apply, pop, or drop stashes. Creation starts from Commit with `s`. Diff navigation, context folding, zoom,
copy, narrow-screen behavior, and binary/large-file fallback match Log.
`F5`, entering Stash, and regaining terminal focus refresh the list; a visible
comparison keeps its snapshot until closed. Tests cover 120×36, 80×25, and 44×12.

`a` (Apply), `P` (Pop), and `d` (Drop) operate on the selected stash from
Repositories or Files. Repository headers, empty rows, and the summary have no
action target. The Diff overlay owns its input and exposes no writes. The status bar
and F1 help show the configured bindings; lowercase `p` retains global Pull.

The confirmation opens with a fixed summary of what happens to the saved entry.
Apply is blue, Pop is amber, and Drop is red with an explicit undo warning.
Aligned rows show the repository, stash selector, message, and, for Apply/Pop,
the destination branch and saved staging option. Details shows the timestamp,
full path, stash object ID, and destination HEAD when applicable. The body wraps
to terminal width and scrolls with `j/k`, arrows, or the wheel while the summary
and buttons stay visible. The footer names the operation and underlines the
focused button. The action starts focused; `Tab` switches buttons, `Enter`
activates the focused button, and `Esc` cancels.
A stale list, changed destination/index, or removed Roots membership stops the
operation. Saved paths entering another repository or traversing a directory
symlink require external handling. Pop applies the immutable snapshot first and removes its entry only
after success and another list check. Conflicts keep the stash and appear in
Conflicts. If application succeeded but removal failed, the result directs the
user to inspect and Drop instead of applying twice. Drop changes only the
selected saved entry and cannot be undone from IGit. Refresh selects a nearby
entry in the same repository. When the last entry is removed, the cursor goes
to that repository in the empty group, or to the summary if it is collapsed.

## Commit Comparison Acceptance

1. Run the [Commit practice script](../examples/demo-commit.sh) and start IGit with the printed path. At 120×36 and 80×24, confirm all 19 items can be reached by scrolling the Changes pane. Distinguish checked, partial, unchecked, unavailable, and independent row highlighting. With `--icons ascii`, click both ends of a three-cell checkbox: either end must toggle only that item; clicking a filename must only browse.
2. In the default directory tree, open both `index.vue` files and verify their paths/patches. Collapse/expand ancestors, then switch to flat mode while retaining the same file. Container previews show only their scope. Browsing, collapsing, and layout switching must not change checkboxes or the index.
3. Check only `scripts/iframe-proxy.mjs` and open Diff. It has both staged and unstaged changes; the preview shows the staged version. Click Commit once. Compare HEAD, the real index, and the working tree to verify that only the staged version was committed and later edits remain.
4. Recheck repository scope with the [multi-repository practice script](../examples/demo-workspace.sh): hide a checked repository, commit, and verify that only checked files in visible Roots were committed. See [PRD A2 / A7 / A9](prd.md).
