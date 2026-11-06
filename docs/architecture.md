# IGit Architecture: From User Action to Git

IGit is a Rust terminal application. Both entry points, [igit](../src/bin/igit.rs) and [ig](../src/bin/ig.rs), call [igit::run](../src/lib.rs). Crossterm reads input, Ratatui renders the interface, and system `git` reads/writes repositories. Git's working tree, index, objects, and refs are the source of truth; the interface holds user selections, drafts, and previews of repository state at a particular time.

## Where to Read the Code

| Question | Entry points and responsibilities |
| --- | --- |
| Which repositories exist under the current directory? Where do Git commands run? | [core.rs](../src/core.rs): `Context`, `Repo`, discovery, scope, and the Git subprocess environment. |
| Who handles shortcuts, mouse events, dialogs, and panel transitions? | [shell.rs](../src/shell.rs): `Shell`, event loop, Roots, and confirmations. [ui.rs](../src/ui.rs): `Panel`, `Action`, and shared styles. |
| Why does help prioritize this area? | Internal panel `keymap` methods in [log/keymap.rs](../src/log/keymap.rs), [commit/keymap.rs](../src/commit/keymap.rs), [push/keymap.rs](../src/push/keymap.rs), and [merge/keymap.rs](../src/merge/keymap.rs) provide shortcuts for the current focus/editing state. [ui/help.rs](../src/ui/help.rs) owns Search, candidate selection, compact rendering, and hit areas. |
| Why does a file or button use this icon? | [ui/icons.rs](../src/ui/icons.rs) maps semantics, filenames, and extensions to glyphs. Shell passes startup `IconMode` to all panels; layout and hit testing use actual terminal cell widths. Icon preferences are not written to repositories, drafts, or Git configuration. [graph/render.rs](../src/log/model/graph/render.rs) draws graph rails/nodes from topology. |
| Why does a commit appear here? | [log/model.rs](../src/log/model.rs) reads real refs, commits, reachability, and patches; [graph.rs](../src/log/model/graph.rs) allocates rails. [log.rs](../src/log.rs) coordinates selection/events. [view.rs](../src/log/view.rs), [refs.rs](../src/log/refs.rs), and [inspector.rs](../src/log/inspector.rs) render the timeline, branch tree, and right pane. [filters.rs](../src/log/filters.rs) manages filters and session tabs. |
| What does this commit include? | [commit.rs](../src/commit.rs): groups, checkboxes, the shared message, and batch commits. [commit/ai.rs](../src/commit/ai.rs): reviewed AI requests and candidate adoption. [commit/tree.rs](../src/commit/tree.rs): flat/compact-directory projections and native file identity. [commit/rows.rs](../src/commit/rows.rs): row rendering and shared arrow/checkbox coordinates. [commit/view.rs](../src/commit/view.rs): changes list, messages, and default-open preview. [commit/refresh.rs](../src/commit/refresh.rs) owns background status and current-patch refresh. [commit/backend.rs](../src/commit/backend.rs): status, snapshots, plans, and execution. |
| Where will this push go, and will the target have the submodule commit? | [push.rs](../src/push.rs): outgoing commits, targets, plans, background jobs, and per-repository results. [push/loading.rs](../src/push/loading.rs) schedules outgoing commit and preview reads; [push/preview.rs](../src/push/preview.rs) reads immutable file lists and patches. [push/files.rs](../src/push/files.rs) parses changes and derives grouped rows from one tree; [push/display.rs](../src/push/display.rs) handles the session Group By menu; [push/view.rs](../src/push/view.rs) renders columns/hit areas. [push/submodules.rs](../src/push/submodules.rs) verifies remote availability of gitlinks in outgoing history. |
| How do branch lists load and branch writes get reviewed? | [branch/loading.rs](../src/branch/loading.rs) reads refs and prepares tree rows in the background. [branch/operations.rs](../src/branch/operations.rs) revalidates one repository before a reviewed write. [core/loading.rs](../src/core/loading.rs) provides the bounded reader shared with Push. |
| What will a workspace Pull or Fetch update? | [shell/update.rs](../src/shell/update.rs) owns the Roots review, selected strategy, Git worker, and structured per-repository results. [shell/update/view.rs](../src/shell/update/view.rs) renders the shared review/running/results list, responsive columns, Details, and action hit areas. |
| How is the application itself installed and updated? | [installation.rs](../src/installation.rs) dispatches `update` before Shell starts. [install.py](../scripts/install/install.py) and [install.ps1](../pages/install.ps1) own native installation validation, download, staging, and activation. [pages.py](../scripts/release/pages.py) builds public channel manifests from verified published releases. |
| When can smart merge choose automatically? | [merge.rs](../src/merge.rs): conflict actions, editing, and drafts. [merge/view.rs](../src/merge/view.rs): full-screen editor, controls, and aligned viewport. [merge/engine.rs](../src/merge/engine.rs): index stages, three-way comparison, file writing/staging, and Git continuation. |

## Native Installation and Software Updates

Both binaries dispatch a first argument of `update` before configuration,
repository discovery, or terminal setup. Cargo/manual copies provide owner
guidance; native copies invoke an embedded installer helper with structured
arguments, identifying the actual running executable. Ambient install-path and
channel variables cannot redirect that update. Unix needs Python 3.9+ and curl;
Windows uses Windows PowerShell and curl.exe. No Rust runtime dependency is added.

`install.json` records fixed native ownership and paths. Each immutable
`releases/VERSION_TARGET` directory has a file-hash record. On Unix both command
symlinks resolve through one atomically replaced `current` symlink. Windows CMD
launchers use ASCII PowerShell helpers to read one atomically replaced
`current.txt`, preserving Unicode installation paths; their versioned EXEs are
never overwritten in place. The active version determines the retained channel.
Installers serialize writes, validate ownership and archive checksums, stage and
check both executables, then activate. Failures before activation preserve the
old version and remove only this operation's new files. App configuration and
workspace Git state are outside this installation root.

Public `stable.json` and `beta.json` manifests are generated independently from
published Releases, excluding drafts. A missing stable release leaves its channel
unavailable. Pages serves only generated manifests, standalone installers, and
the installation landing page. The installer helpers are embedded in new binary
builds; deploying Pages does not add `update` to an already published binary.

## The Path of a Keypress or Click

[Configuration](../src/config.rs) loads optional user TOML over the embedded
[defaults](../config/default.toml) before repository discovery or raw mode.
Unknown fields, invalid values, and duplicate/prefix conflicts in overlapping
normal-mode contexts fail startup. CLI presentation overrides apply afterward.
The [command catalogue](../src/config/keys.rs) owns stable command names,
contexts, key parsing/normalization, and trusted built-in execution sequences.
Preferences stay separate from workspace state and are carried across rescans.

[Shell's configuration adapter](../src/shell/configuration.rs) intercepts prefixes
affected by overrides, suppresses replaced keys, and sends complete trusted
sequences through the existing dispatch path without recursively remapping them.
Unchanged prefixes retain their panel handlers and window-count behavior. Custom
prefixes share the existing hint picker and its configurable display deadline.
Editors, menus, selection-copy controls, and confirmation dialogs retain their
context-owned input. Help carries display labels separately from static command
identity, so searching or choosing a custom binding cannot create a new Git
execution path. No settings are loaded from repositories or written by the app.

Smart focus is a typed command rather than a synthetic key sequence. Shell
consumes its configured chords before editor/review handlers, moving only in
normal mode. [Pane routing](../src/shell/panes.rs) uses current geometry before
falling back to [Kitty](../src/shell/kitty.rs). One background request at a time
executes `kitten @ action --match id:<source>`, with a one-second timeout,
cancellation on drop, and nonfatal errors polled by Shell. It never invokes Git.
The terminal guard registers/clears `IS_IGIT` for opted-in Kitty sessions and
pushes/pops keyboard enhancement flags on a best-effort basis. Following the
[protocol quickstart](https://sw.kovidgoyal.net/kitty/keyboard-protocol/#quickstart),
it requests flags directly instead of blocking on a capability query; unsupported
backends retain ordinary keyboard input. The Kitty adapter survives
workspace rescans; unit-test Shell constructors never discover a live terminal.

Pane maximize is a session-only Shell flag applied to Commit, Stash, Push, and
Conflicts. Log always registers its enabled pane rectangles and dividers.
The active panel receives the available work area; hidden panes retain their
logical Tab order with empty rectangles and clear their hit/selection geometry.
No split preferences change until the user restores and resizes. Smart maximize
uses the same worker: `ls` resolves the source window's owning tab and layout
groups, a tab-scoped `goto-layout stack` or `last-used-layout` changes the layout,
and a second `ls` verifies it. One deadline covers the operation; captured stdout
uses a temporary file with a size limit. Shell consumes input while the request
is pending to keep its focus/editor context stable, then applies the verified
target (or local toggle where supported for one group). Errors preserve local maximize state.

Smart resize is another native action, accepting press/repeat events. A missing
or clamped divider in the active tab hands off to Kitty. The same worker invokes the installed
[igit_resize.py](../contrib/kitty/igit_resize.py) helper via `kitten @ kitten
--match id:<source>`. It chooses the size increment from axis neighbors and
calls `tab.resize_window_by` with the explicit source ID, without activating
another window. Smart focus or resize enables the terminal marker. The helper
has no dependency on the user's Neovim or LazyDB helper.

1. [shell::run](../src/shell.rs) enables alternate screen, raw mode, mouse reporting, and bracketed paste, then paints a loading view before any repository discovery. [startup.rs](../src/shell/startup.rs) discovers repositories and initializes Commit and Conflicts on a worker, reporting stages over `mpsc`. The terminal thread handles resize and cancellation while polling; errors and cancellation restore terminal modes. Discovered branch labels are reused without another branch scan. The normal event loop starts when the workspace is ready. `--list-roots` remains a synchronous plain-output command.
2. [Workspace layout](../src/shell/workspace.rs) records the positions of right-aligned workspace tabs, the left-side Log tab strip in the same header row, and the active panel. The footer prefixes contextual hints or notifications with `IGit`. `Shell::dispatch` handles top-level overlays such as help/review first, then routes input to the active tab. Panels report text input through `editing()` so ordinary characters do not trigger global commands. [Pane geometry](../src/ui/panes.rs) registers pane rectangles/dividers for the active tab; adjustable sizes are stored in session character cells. Keyboard and mouse share bounds and size preferences; restoring defaults reallocates against the current terminal size. [Pane routing](../src/shell/panes.rs) handles Tab in registration order, finds directional neighbors by actual edges/overlap, and keeps an entire drag with the divider that received the press. Text selection and file dragging keep their original panel event routes.
3. The active panel receives `Panel::event` and returns an `Action`. It owns its selected row, focus, and scroll position. Shell owns panel switching, notifications, and confirmation. To request confirmation, the panel stores a concrete plan and returns `Confirm { title, body, token }`. Shell records the requesting panel, initially selects Cancel, and calls that panel's `confirm(token, context)` only after confirmation.
4. For **Commit & Push**: Commit builds per-repository plans → user reviews → repositories commit individually → `Action::Committed` returns the successful repository set → Shell refreshes the workspace, switches to Push, and initially checks those repositories → user separately reviews and starts pushing. A local commit created and a remote updated are independent results.

All panels remain resident in the same Shell. Startup defers Log history until
the first Log entry; that initial load runs on a worker while workspace navigation
and quit remain available. Log panel actions wait until its data is ready. A
workspace refresh drops any pending Log receiver, so a result from old Roots or
refs cannot replace current data; a replacement starts only if Log is active.
F5, applying Roots, and Git writes refresh Commit, already loaded Log, and the
scoped conflict list. Shell derives Conflicts visibility from that list, which
includes unmerged index entries and operations ready for Continue. If Conflicts
disappears while active, Shell returns to the last Commit, Log, Branch or Stash tab. Push refreshes
on entry and during a workspace refresh while active, without waiting for Git on
the terminal thread. Branch also queues reads on entry and refresh. Branch and
Push use a shared loader with one active batch per reader and at most four
repository reads in each batch. A new request supersedes the old generation;
an old batch stops taking repositories and cannot publish results. Branch
prepares tree rows and measures the upstream column in its worker. The terminal
installs current results using the latest cursor and fold choices. It retains
cached rows within the same Roots; a changed scope clears the old rows.
Opening/rescanning from the
Actions menu still synchronously rebuilds context and panels. Subsequent Log
history refresh and explicit patch opening, Commit snapshot/execution, and Merge
reads/writes remain synchronous. Log commit details, Branch and Push reads,
Push/Fetch, and AI requests
use standard threads and `mpsc` results polled by Shell. There is no general
asynchronous scheduler; large-repository refreshes and slow hooks may block input.

[ui/selection.rs](../src/ui/selection.rs) provides shared text layout/selection for Log details/Diff, Commit message editing/preview, and Push Details / Diff. Each display line retains its original UTF-8 byte range. Cell hit testing, soft wrapping, scrolling, and highlighting share one projection; copy slices original text directly, while decorative rows have no source. Shared mouse handling returns `Action::Copy` on left-button release and collapses selection to its endpoint; the panel forwards the action to Shell. Clicks return no action; repeated releases do not copy again. Panels own selection lifetimes/focus. Log links reuse the same line layout and activate on release only without copying/dragging. `Panel::selected_text` exposes only valid selection in the current context, letting Shell send Ctrl-C to keyboard selection before quit confirmation. Copy still uses `Action::Copy` → [ui/clipboard.rs](../src/ui/clipboard.rs); failures show reasons without logging text. The Commit editor keeps the post-copy caret endpoint and synchronizes source between consecutive events so input before redraw cannot misalign selection. Selection/viewport state stays in memory, outside draft files and Git plans. Shell tests capture copying through a thread-local receiver without modifying the system clipboard.

When help opens, Shell captures the original area's `HelpContent` without extending the public `Panel` trait. Panels provide only single-action shortcuts available in context. Internal ordering remains area, panel-wide, then global; the UI deduplicates them into a compact lazydb-style list. Search always receives text, ↑/↓ changes candidates, and Enter returns `HelpCommand`. Shell restores the covered dialog, validates original context, then sends the candidate's statically registered key sequence back through `dispatch`. Sequences/modifiers use the original event path. Search text never becomes an executable sequence; the original panel still produces review/validation for commit, push, and merge writes. F1 is handled before ordinary dispatch and does not cancel input. Canceling help leaves drafts, checkboxes, buttons, and scroll positions in their original objects. Search/candidate/scroll state stays in memory.

Mouse gestures separately record press coordinates and whether actual dragging occurred. Only `Drag(Left)` across cells starts selection. Ordinary `Down → Up` retains the press-time caret without remapping on release. Closing a notification, scrolling, or relayout therefore cannot turn a click into copying. Release, keypress, clearing selection, and source changes end unfinished gestures. `cargo test --locked click_after_copy --lib` covers another click after copying, movement within one cell, and redraws between press/release.

## Repositories, Scope, and Persistence

Workspace Pull and Fetch build a read-only plan from global Roots. Each worktree is checked again before running Git. Pull also revalidates the branch, HEAD, upstream, and resolved remote URL; Fetch revalidates the configured remotes. After confirmation, a worker runs one `git pull` or `git fetch --all` per eligible worktree, with submodule recursion disabled. Pull integrates the reviewed upstream with an explicit Merge or Rebase strategy. The update dialog remains active through completion so another workspace write cannot overlap it. Presentation rows retain the reviewed identity, branch/target labels, and repository tint; structured worker outcomes map back in plan order. Row selection and Details scrolling are view-only state and cannot change the batch. Shell refreshes Log after Fetch and the workspace after Pull. Failures and Git conflict state remain per repository.

- **Discovery.** `Context::discover` first finds the working tree containing the startup directory, then traverses below that directory. Git resolves and deduplicates working trees with `.git` files/directories, ignoring bare repositories and common build/dependency directories. `--show-superproject-working-tree` identifies submodules; `Repo.parent` exists only when the parent working tree is also discovered. Ordinary nested repositories have no such dependency. Uninitialized submodules are not usable working trees.
- **Identity.** `repos: Vec<Repo>` is ordered by canonical path. `scope: BTreeSet<usize>` contains repository indices currently allowed for display/operations, initially all. IDs are valid only for that discovery result; old indices cannot restore identity after a rescan. Display names, abbreviated SHAs, and file row numbers are not persistent keys either. Narrowing Roots must not let hidden repositories silently join the next write.
- **Local state.** State-directory precedence is `$IGIT_STATE_DIR`, `$XDG_STATE_HOME/igit`, then `$HOME/.local/state/igit`, followed by a hash of the canonical workspace path. Its `commit.json` stores groups, file membership, one shared `message`, retained legacy `drafts`, and workspace AI endpoint/model/prompt, replaced through a temporary file in the same directory. Internal keys encode repository/path bytes. Moving the workspace creates a new state directory; concurrent instances have no draft-merging mechanism. Checkboxes, focus, pending plans, push results, and merge undo stacks are excluded.
- **Command environment.** Git processes derived from `core::command` use `git -C <repo>`, clear inherited overrides for working tree, index, object directory, and similar settings, and disable interactive terminal prompts. Callers explicitly inject private indexes. Configured Git identity, hooks, signing, and credential helpers still affect results; interactive credential entry is outside the TUI. Paths are parsed from NUL-delimited bytes; lossy display strings must never be sent back to Git as paths.

## Core Data Paths

**Stash.** [stash.rs](../src/stash.rs) owns repository grouping, selection,
folding, and focus. [model.rs](../src/stash/model.rs) reads stash reflogs with
NUL-separated immutable object IDs, selectors, timestamps, and subjects;
empty and failed repositories remain visible within global Roots. List reads
run only on tab entry, explicit refresh, or terminal focus, leaving startup
unaffected. [preview.rs](../src/stash/preview.rs) debounces selection for 60ms,
runs one snapshot worker at a time, and rejects stale generations.
[log/snapshot.rs](../src/log/snapshot.rs) is a file-only adapter over Log's
inspector and historical comparison. It restricts input to file and Diff
controls, without loading history or exposing Log filters. The first parent
and stash worktree form tracked comparisons; optional third-parent files
have an empty Before side. Each file retains its source revision across
next/previous navigation and patch fallback. Existing byte paths, size limits,
and read-only revision controls are reused. Stash creation belongs to Commit
below. [operations.rs](../src/stash/operations.rs) owns single-use Apply/Pop/Drop
reviews. Each review retains the repository path, entry selector/object,
complete entry list, and (for apply/pop) HEAD, branch, index records, and the
`stash.index` option. Native apply uses the immutable object ID and suppresses
submodule recursion. [scope.rs](../src/stash/operations/scope.rs) checks saved
tracked/index/untracked paths before review and execution: a saved untracked
file must not be restored inside a newly added submodule, nested repository,
or directory symlink. This check is needed even with Git recursion disabled. Pop performs a separate list check before native drop;
conflicts or changed lists keep the entry, and successful application followed
by failed removal is reported without replaying apply. Drop also validates the
reviewed list immediately before invoking Git on the selected selector. Native
Git supplies index/ref locking and local overwrite protection. Every attempted
operation returns Shell's Refresh action, updating Stash, Commit, and Conflicts.
List refresh falls back to the nearest entry/header in the same repository.
There is no new persistent state.

**Branch.** [branch.rs](../src/branch.rs) owns tree focus, name input, and the
pending shell review. [model.rs](../src/branch/model.rs) reads local and remote
refs, commit IDs, symbolic HEAD, upstreams, and worktree ownership without
loading history. [tree.rs](../src/branch/tree.rs) builds scoped repository and
slash-folder rows with current-first traversal, branch counts, and stable
fold/focus IDs. Explicit fold overrides keep remote groups closed initially
while retaining user choices through refresh and Roots changes.
[navigation.rs](../src/branch/navigation.rs) maintains the viewport after each
motion and exposes `gg` to the shell's existing sequence picker.
[rows.rs](../src/branch/rows.rs) measures branch/upstream columns in terminal
cells and uses shared Log reference colors. [view.rs](../src/branch/view.rs)
owns responsive controls/details and row/action hit targets. State is
session-only; no new persistence or dependency is introduced.

[operations.rs](../src/branch/operations.rs) prepares a single-repository review
and rechecks Roots, worktree path, refs, HEAD, upstreams, worktree ownership and
pending Git operations before executing. Create and merge use the reviewed
immutable source OID. Local checkout uses `switch --no-guess`; remote checkout
uses explicit `--track -c`. Safe deletion is `branch -d`, rename is `branch -m`,
and merges disable autostash. Checkout/merge protect ignored files and suppress
recursive submodule updates. Git provides its normal locking and overwrite
checks; IGit does not lock out other Git processes between reads and execution.
Success and failure both refresh workspace state, including partial conflict
results. Branch reads/writes are synchronous, first loaded on entry rather than
startup. F5, reentry, focus return, and completed remote operations refresh it
while active. Name drafts and pending reviews retain their original source
identity across refreshes.

Log's [display menu](../src/log/display.rs) stores session preferences and temporary menu focus without writing project/state files. Filters and selected SHA remain per Log tab. `Rev` retains author email, author date, and commit date; sorting still uses commit time. After refresh/tab changes, [presentation metadata](../src/log/model/presentation.rs) reads per-repository identity. Loaded OIDs are queried in batches for `%G?` only when signatures are visible; `git cherry HEAD <resolved-tip>` runs only when cherry highlighting is enabled and another branch is selected. Queries use the shared Git environment, report failures per repository, and preserve history. Rendering, hover, and row navigation do not run these commands. Signature verification/patch comparison use synchronous refresh and can increase load time for large histories. Shortening long edges changes only character rendering; arrow hit areas retain true visible endpoints and do not modify the DAG.

**Log: multiple commit graphs on one screen.** Each repository starts from local branches, remote-tracking branches, tags, and valid HEAD. `git log --date-order` produces a topologically valid sequence; sequences are then merged by their head timestamps. [Rail layout](../src/log/model/graph.rs) matches real visible parents/children by repository ID + object ID. Long vertical edges store only start/end rows. Paths to a common parent may share their trailing rail; the main rail prefers the first parent. Completed rails free columns for reuse, with bends recorded at column changes. [Character rendering](../src/log/model/graph/render.rs) expands only the current viewport; all rows share a horizontal origin. The default is 160 commits per repository, capped at 3,200. Text/hash filter changes do not reload all history; `regex-lite` matches in memory. Queries use full ref names across repositories and Git namespaces to distinguish identically named branches, remotes, and tags. Log tabs store only filters/selection; switching reuses the same panel and reloads visible data. Roots applies to all tabs.

The [Branch menu](../src/log/branches.rs) builds candidates from loaded refs within the current tab's Roots, listing each full ref once across repositories. Hover, submenu scrolling, and search do not run Git. `branch-favorites.json` in workspace state stores only manual star preferences, atomically replaced through a temporary file without changing repository configuration. Each Log tab has its own [branch expression](../src/log/model/branch_filter.rs). Full refs resolve per repository before queries; history and reachability use identical starts/exclusions. Cherry comparison is enabled only for one unambiguous branch.

[Paths](../src/log/paths.rs) stores tab-local Roots separately from its [path set](../src/log/model/path_filter.rs). Roots changes reuse loaded history. Each path holds an optional repository ID and native relative path. A repository query appends only applicable files/directories individually after `git --literal-pathspecs log … --`. Selecting a whole repository passes no paths; repositories outside all path scopes skip history queries. The [tree selector](../src/log/paths/tree.rs) reads only immediate directory children on expansion, skipping `.git`, symlinks, and other repositories. Text/tree selectors share the applied path set; drafts replace tab filters only after confirmation.

Loaded refs remain cached in `History`; the [reference tree](../src/log/refs.rs) intersects global Roots, tab-local Roots, and `PathFilter::for_root` before aggregating rows and ownership metadata. Cached Roots changes rebuild both history visibility and the reference tree without new Git queries. Applied paths, refresh, and tab restoration rebuild refs through the existing reload paths; the tree's repository scope does not depend on matching commits.

The commit-list cursor moves in memory. During continuous navigation, [detail loading](../src/log/preview.rs) waits for selection to remain stable for 60ms, then reads changed files, body, and containing branches and builds the file tree in the background. At most one detail query runs at a time, with only the latest pending row retained. Selection changes/refresh invalidate older requests. Branches reload even when the SHA remains identical across refresh. Shell handles queued consecutive movement keys in order and redraws their final position once; before other actions it redraws, so mouse hit testing sees the current row.

[Historical merge details](../src/log/model/merge_diff.rs) read the combined changed paths plus each parent-to-result path list, preserving NUL-delimited byte paths and a parent index for each leaf. The combined list follows [Git combined diff semantics](https://git-scm.com/docs/git-diff#_combined_diff_format): only paths modified against all parents appear there. File lists and parent subjects load in the existing preview worker, with the same generation invalidation. Every selected file reads exact `ls-tree` entries and immutable blobs through `cat-file`, capped at 512 KiB each; no index, working file, merge operation or clean/smudge filter supplies its contents. Missing entries are empty sides with explicit labels. The selected commit determines the presentation: two parents use the same three versions from every file group; an ordinary commit compares its parent and itself in two panes. Initial commits have an empty Before source. Unsupported text/object types fall back to a bounded patch; in that fallback, parent leaves still select the corresponding parent explicitly.

The [historical viewer](../src/merge/view/revision.rs) uses `diff3(result, parent1, parent2)` to align around the committed result; two-pane comparisons use `diff3(after, before, after)` with the duplicate alignment side hidden and excluded from focus, sharing the conflict editor's chunk row map, Unicode clipping, character ranges, folding and scrollbar machinery. The shared row renderer takes chunks instead of a writable `Document`, and historical mode suppresses resolution gutters and editing hit targets. RevisionDiff owns only text, labels and viewport state; its events can navigate, copy or close, never apply or continue. All historical change blocks use the modified palette, including disagreements with both parents. Source byte ranges project visible rows through shared text selection, while an overlay highlights the existing code rendering. Folded context stays in the underlying source and alignment padding remains decorative; copying preserves original tabs, CRLF and Unicode. The small display-only code lexer colors common source extensions without a production dependency; it is not a full language parser.

[List navigation](../src/log/navigation.rs) stores vertical origin and the most recently drawn usable row count. It corrects edges after every selection change so batched input and per-key redraw have identical scrolling. Rendering corrects again for new window height. `gg` and `y` sequences are recognized only in the commit list; focus changes/overlays clear prefixes. `Action::Copy` routes through Shell and the [clipboard adapter](../src/ui/clipboard.rs), using command stdin rather than interpolating text into commands or notifications. ID/name/email come from the current row. Only `ym` runs read-only `git show` for that exact repository/OID to retrieve the full message, independent of potentially pending details.

[Key sequence hints](../src/ui/key_sequence.rs) derive candidates from pending Branch `g`, Log `g/y`, and Merge `[/]` states. Panels still recognize sequences. Shell candidate state stores cursor, last-rendered hit areas, and a 200ms display deadline. The deadline participates in the event poll's minimum wait; reaching it triggers one redraw without blocking later keys. Completion/cancel clears timing, and a new prefix restarts it. The bottom overlay uses shared colors and cell-width-based columns. Once visible, arrows, Enter, and mouse events inside it run before ordinary panel/global events. Esc/Ctrl-C can cancel during the delay too. Choosing a candidate returns the continuation key to the original panel route rather than creating another Git entry point. Global panel changes, refresh, and dialogs clear both kinds of prefixes. Single-key actions without prefixes and text editing bypass the candidate overlay.

**Commit: checked content, not the entire real index.** Changes come from `git status --porcelain=v1 -z`. The tree starts with Directory grouping enabled and Module disabled. Native paths drive browsing identity, checkboxes, and regrouping; parent-directory hints are display-only. The tree projects scoped unmerged paths into Merge Conflicts while keeping their saved changelist membership. Resolve uses the existing Shell tab action; it does not apply a resolution. Directory/expansion changes never touch the index. Preview starts enabled; closed previews do not read patches. Open previews use the current row to choose a file Diff or in-memory container summary. Review builds a temporary index from HEAD, or the empty tree for an initial commit. Selected files with staged content use that version; otherwise, selected working-tree content is read. A file staged and then edited again therefore commits only its staged version. `write-tree` obtains the planned tree, while HEAD, branch, and selected index records are retained. Review may create Git objects but does not move HEAD or overwrite the real index.

[Commit refresh](../src/commit/refresh.rs) uses at most one status/patch worker, polled by Shell while Commit is active and no shell dialog is open. [Native watching](../src/commit/refresh/watch.rs) uses [notify](https://docs.rs/notify/latest/notify/) to share platform-specific notification handling. Watching starts before the initial status read on the startup worker; dropping the panel releases its native handles. Canonical worktrees and resolved Git/common directories cover ordinary repositories, submodules, and linked worktrees without following directory symlinks. Events map to the deepest owning worktree or Git directory; shared refs invalidate their worktrees, and submodule HEAD changes also invalidate parent gitlinks. Read events and irrelevant object/log/lock writes are filtered. A bounded per-repository dirty map merges bursts for 300ms, with a two-second maximum debounce delay. Only dirty roots in the current scope are scanned; hidden-root events remain queued. Snapshots carry root IDs so unscanned files, branches, and errors remain intact. Events arriving during a scan are retained for another scan. Tab/focus entry drains queued changes without an unconditional scan; a 30-second consistency scan of Roots covers missed events and unavailable watchers. Status and Diff use `--no-optional-locks` to avoid refresh-generated index writes. Generation checks discard results superseded by manual refresh or ignored-file toggles. Apply preserves checkboxes, folds, cursor identity, the shared draft, caret, and unchanged preview selection/scroll. Menus and mouse gestures defer application. Failed roots retain their previous files and repeated errors are reported once. Commit/AI/rollback reviews and Log history remain independent; F5 still refreshes all panels.

Message editing renders the existing buffer and text selection inside the composer's draft rectangle in [commit/view.rs](../src/commit/view.rs). It reports text-input mode but does not claim overlay mouse events. Clicking another control saves the shared draft before processing that click; Shell does the same before leaving Commit or moving focus with Tab / Shift-Tab / F6. Explicit save/cancel and F1 help retain their editing semantics. Persistence failures preserve the buffer and original draft, and stop navigation/review. Caret and scroll state remain in memory; the shared draft uses `commit.json`. Each repository plan receives a copy of that message. Partial failures and hidden checked paths retain it; a completed batch clears it. On first load, a missing shared message is seeded from the first nonempty legacy draft in repository order; legacy entries remain available for recovery and do not replace an explicitly empty shared message. Changelist and AI editors retain overlay rendering.

The [Commit display menu](../src/commit/display.rs) stores session toggles for directory/repository grouping, ignored items, and XY status. The [tree model](../src/commit/tree.rs) combines both grouping options; directory mode without Module uses workspace-relative paths. Display state is separate from native file identity, checkboxes, and Git write plans. `git ls-files --others --ignored --exclude-standard --directory -z` runs only when ignored items are shown, without reading their contents; directories stay single items. These `!!` entries are unavailable, and commit/AI source validation rejects them again. Menu handling precedes changes-list events. Border blocking, outside clicks, and Shell-header dismissal follow Log conventions.

After confirmation, `backend::execute` exclusively creates the real `index.lock`, then rechecks branch, HEAD, index records, and the recomputed tree. Actual `git commit` uses a private index and native hooks. The new real index is built from the locked index copy, preserving unrelated staged content. Hook-rewritten commits are reported for review. If HEAD updates but index publication fails, the error preserves the recovery lock. Inspect HEAD, the index, and the error before acting; do not automatically remove the lock or commit again.

**Commit stash creation.** [commit/stash.rs](../src/commit/stash.rs) owns a
session-local stash message, reviewed plans, preparation errors, and saved
objects awaiting cleanup. It uses the existing input and confirmation overlays.
[backend/stash.rs](../src/commit/backend/stash.rs) rereads every repository in
Roots instead of deriving scope from checkboxes or cached rows. Reviews retain
HEAD, branch, index entries, native changed paths, file hashes/modes, and
symlink targets. The fingerprint reader is shared with rollback. Revalidation
precedes native `git stash push --include-untracked --message`; recursion into
submodules is explicitly disabled and Git's internal pathspec handling keeps
its default semantics. Linked worktrees share a stash reflog, so publication
is detected at execution time, including repeated object IDs. Git performs its
own index locking and stores staged and untracked snapshots. Results are
independent; a published object with a cleanup failure stays visible and is
not retried automatically. Commit refreshes after the batch and the Stash tab
reads saved entries on entry. This operation does not change the shared commit
message or publish anything remotely.

**AI commit messages: one reviewed request, one shared candidate.** [commit/ai.rs](../src/commit/ai.rs) snapshots the effective configuration and selected patches, labeled by repository, at review time. It limits total patch bytes to 160 KB and rechecks Roots before sending. After confirmation, a background `curl` request sends the custom system prompt and a user message containing the repository/patch array. User `[ai]` settings from [config.rs](../src/config.rs) override legacy workspace endpoint/model/prompt on startup; `S` changes the current session and saves a workspace fallback. The credential comes from `ai.api_key_env` (default `IGIT_LLM_API_KEY`) and is passed through a temporary header file, never command arguments or persisted state. HTTP failures, invalid/empty replies and incomplete responses are reported without echoing service bodies or credentials. Shell's tick consumes completion so redraw cannot swallow errors. The candidate replaces the shared draft only after acceptance; persistence failure retains both candidate and original draft. Generation never creates commits. Tests use local mock HTTP servers and temporary repositories.

Commit and AI share checked-file snapshots. Index entries match native paths exactly. Exclusions for Diff, temporary staging, and index publication match only selected-path descendants, escaping wildcards while preserving the gitlink itself. File/directory replacements validate scope against actual changed paths. If adding a file requires deleting unchecked descendants, stop and ask for a revised selection. The branch shown in Commit review comes from the same execution plan; cached branch lists do not determine the target.

**Push reads: responsive entry and current snapshots.** The panel queues outgoing
reads on entry and retains its previous rows while refreshing. One batch runs at
a time, with at most four repository readers; each reads effective branch/push
configuration once and omits history queries for zero outgoing commits. A new
request invalidates the old generation, stops it from taking more repositories,
and replaces queued work. Shell polls channels at up to 16ms intervals while
reads are pending. Workspace/Roots identity also invalidates results when scope
changes while Push is inactive. File lists and patches use a separate single
reader over immutable OIDs, coalescing navigation and discarding obsolete results.
No Git commands run in Push's tab-entry or read-completion path.

Checkboxes, target overrides, and grouping belong to the UI, not the returned
snapshot. Selection follows repository and commit OID; unchanged snapshots retain
the selected file, folds, Diff scroll, and text selection. Commit and Push applies
its reseed when the current batch arrives. Loading and reader failures disable
Push/Retry/Fetch until a current snapshot is available, while navigation, Cancel,
and quit remain available. This read state is separate from the remote-operation
busy guard. Refresh can retry a failed reader. Explicit target validation and
push-review job construction still run synchronously; write execution and its
source/URL/remote/submodule checks are unchanged.

**Push: publish reviewed objects.** Each Job records repository, source branch/HEAD, target remote/branch, push URL, and expected remote tip. Before execution, source state is rechecked and the target tip queried from the actual **push URL**. Ancestry uses parent records in Git objects, ignoring local replacement refs and legacy grafts. Only after validation does a target-specific `--force-with-lease` with the expected value push the reviewed OID, preventing concurrent remote changes after the check. In outgoing history, any gitlink change or change to the corresponding path's source triggers verification that the recorded submodule version is available from the source declared in that commit. If it cannot be proven, the parent push is rejected without implicitly pushing other repositories.

[The conflict list](../src/merge/list.rs) groups unresolved index entries separately from repositories ready for Continue. It caches operation and source labels on refresh, derives both status columns from stages, and keeps selection by repository and path. Its Accept controls fill an in-memory document from one side; Prepare Simple Drafts runs the same non-conflicting block rules for the selected repository and stores session drafts. Both use the existing file eligibility checks and leave all writes to the reviews below.

**Merge: preview, staging, and Continue have separate boundaries.** Read index stages 1 (base), 2 (ours), and 3 (theirs). An ordinary text conflict can open when the working file matches Git's conflict output or exactly matches stage 1, the editor's starting result. A stage 1 match leaves the same decisions pending as a fresh conflict. Compute base-to-side line differences and assemble blocks in base coordinates. Blocks changed on only one side, or with identical results on both sides, can be accepted automatically. `Chunk::automatic_result` shares eligibility between the editor's Resolve Automatically button/count, its batch action, and prepared session drafts. It accepts pending non-conflicting blocks and combines disjoint single-line edits only when neither side has been decided; touching insertions remain manual. Left / All / Right stays limited to non-conflicting blocks. An automatic batch has one undo snapshot. Stages 2/3 are Git's sides, so rebase identifies the rebased HEAD and replayed commit. Each source's pending decision and the result bytes are held in the in-memory `Document`; undo and session drafts retain both.

The editor's row layout aligns blocks across all three panes, pads shorter sides, and folds shared context in a single display coordinate system. Each pane keeps its own line numbers and horizontal offset. Intraline colors, indentation guides, and folds affect display only. Inline edits replace a result block and can be undone. Shell displays the editor without the workspace chrome and still owns Help and confirmation dialogs.

After Apply confirmation, revalidate operation, HEAD, conflict stages, original file content, permissions, and conversion configuration. Write a temporary file in the original directory, replace the original, then separately run `git add -- <path>`. If staging fails and the working file still matches the just-written result, attempt to restore original content. Do not directly overwrite conflicts outside ordinary three-stage text, binary content, files requiring checkout conversion, files beyond size/line limits, or externally edited files. **Continue is per repository:** separately review/validate the entire index tree and run the corresponding `git … --continue`. Other staged files in that repository are included in its scope.

Commit's [discard controller](../src/commit/discard.rs) uses the cursor scope for initial checks independently of Commit checkboxes. Changelist rows use exact native paths from their visible members; physical directories and repositories can span changelists. The [Rollback modal](../src/commit/rollback.rs) is owned by Shell and renders all visible Changes rows with the shared tree and row renderer. Its checks, folds, grouping, Diff, and added-copy option are local to the modal. Close drops the pending plans, while F1 retains the modal and its state under Help.

The [discard backend](../src/commit/backend/discard.rs) captures eligible visible paths at open, recording HEAD/branch, exact index records, and raw working-tree hashes/modes or symlink targets. Unavailable paths stay in the tree without a checkbox target. A file Diff is loaded on demand against the snapshot. Confirm narrows the retained plans to checked paths without recapturing Git state. Discard locks the real index, revalidates the selected snapshot, and prepares a private index with only the reviewed paths reset to HEAD (or the empty tree before the initial commit). `checkout-index` restores tracked leaves; `remove_file` removes untracked leaves and added local copies only when requested. Each leaf is checked again before writing. Unrelated staging is preserved, and partial failure publishes index changes only for completed leaves. Conflicts, active Git operations, submodule pointers, nested repositories, directory symlink traversal, and file/directory replacements remain read-only. Results refresh the workspace; no undo backup or cross-repository atomicity is provided.

## Batch Writes and Failures

| Operation | When a repository fails |
| --- | --- |
| Commit | Execute sequentially on the UI thread, deeper child paths first. Retain successful SHAs. If a parent selected a failed child's gitlink, skip that dependent parent commit. If a successful child moves the parent pointer beyond the original preview, require review again. Independent repositories may continue. |
| Push | One background task executes repositories sequentially. Failure blocks the dependent parent chain, not unrelated repositories. Report per-repository results. Retry builds a new review from current Roots and failure records rather than replaying old remote plans. |
| Merge | Roots may gather conflicts from several repositories. Apply targets one file; Continue targets one repository. There is no cross-repository apply-all-and-continue transaction. |

Batch commit/push means selecting multiple repositories in one operation; it guarantees neither parallelism nor all-or-nothing results. Confirmation dialogs and busy state coordinate only this application instance. They cannot lock external terminals, other IGit instances, or remotes. Git locks, snapshot rechecks, and remote leases address different race windows.

## Changes and Acceptance

| Change | Verification entry point |
| --- | --- |
| Settings, key parsing, or contextual remapping | `cargo test --locked config::` and `cargo test --locked configuration_`; use `--check-config` for user files. |
| Discovery, duplicate names, submodules, or workspace boundaries | [tests/discovery.rs](../tests/discovery.rs); run `cargo test --locked --test discovery`. |
| Graphs, filters, keyboard/mouse coordination | `cargo test --locked log::`; check focus changes, narrowing, and hit areas after scrolling in a temporary multi-repository workspace. |
| Commit selection or snapshots | `cargo test --locked commit::`; inspect the actual commit tree and compare unchecked staging/working-tree content, not just command success. |
| Push targets, remote races, or submodules | `cargo test --locked push::`; use temporary bare remotes to verify target refs, partial failures, and retry scope. |
| Three-way diff, Apply, or Continue | `cargo test --locked merge::`; verify real conflict stages, written bytes, staging, and refusal to overwrite external edits. |

Run all checks with `cargo test --locked`. For interactive acceptance, follow [README](../README.md) with the [practice workspace script](../examples/demo-workspace.sh); do not test writes in everyday repositories. Before adding a button, identify its existing action and route keyboard/mouse through that same path. Before adding a Git write, define what review includes, what execution revalidates, and how partial success appears, then connect the panel. If synchronous calls become a bottleneck, use Push's background result channel as a reference. Carry workspace/request identity with results so slower old requests cannot replace current selection. This is extension guidance, not an existing global asynchronous mechanism.
