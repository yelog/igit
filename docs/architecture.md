# IGit Architecture: From User Action to Git

IGit is a Rust terminal application. Both entry points, [igit](../src/bin/igit.rs) and [ig](../src/bin/ig.rs), call [igit::run](../src/lib.rs). Crossterm reads input, Ratatui renders the interface, and system `git` reads/writes repositories. Git's working tree, index, objects, and refs are the source of truth; the interface holds user selections, drafts, and previews of repository state at a particular time.

## Where to Read the Code

| Question | Entry points and responsibilities |
| --- | --- |
| Which repositories exist under the current directory? Where do Git commands run? | [core.rs](../src/core.rs): `Context`, `Repo`, discovery, scope, and the Git subprocess environment. |
| Who handles shortcuts, mouse events, dialogs, and panel transitions? | [shell.rs](../src/shell.rs): `Shell`, event loop, Roots, and confirmations. [ui.rs](../src/ui.rs): `Panel`, `Action`, and shared styles. |
| Where are session notifications stored and rendered? | [ui/notifications.rs](../src/ui/notifications.rs) defines notification severity and identity. [shell.rs](../src/shell.rs) owns bounded history, live-card lifetime, dismissal, and History routing; [ui/notifications](../src/ui/notifications/) renders cards and the history view. |
| Why does help prioritize this area? | Internal panel `keymap` methods in [log/keymap.rs](../src/log/keymap.rs), [commit/keymap.rs](../src/commit/keymap.rs), [push/keymap.rs](../src/push/keymap.rs), and [merge/keymap.rs](../src/merge/keymap.rs) provide shortcuts for the current focus/editing state. [ui/help.rs](../src/ui/help.rs) owns Search, candidate selection, compact rendering, and hit areas. |
| Why does a file or button use this icon? | [ui/icons.rs](../src/ui/icons.rs) maps semantics, filenames, and extensions to glyphs. Shell passes startup `IconMode` to all panels; layout and hit testing use actual terminal cell widths. Icon preferences are not written to repositories, drafts, or Git configuration. [graph/render.rs](../src/log/model/graph/render.rs) draws graph rails/nodes from topology. |
| Why does a commit appear here? | [log/model.rs](../src/log/model.rs) reads real refs, commits, reachability, and patches; [graph.rs](../src/log/model/graph.rs) allocates rails. [log.rs](../src/log.rs) coordinates selection/events. [view.rs](../src/log/view.rs), [refs.rs](../src/log/refs.rs), and [inspector.rs](../src/log/inspector.rs) render the timeline, branch tree, and right pane. [filters.rs](../src/log/filters.rs) manages filters and session tabs; [authors.rs](../src/log/authors.rs) renders the User picker and maintains its viewport and navigation without additional Git reads. |
| What does this commit include? | [commit.rs](../src/commit.rs): groups, checkboxes, the shared message, and batch commits. [commit/ai.rs](../src/commit/ai.rs): background AI jobs and shared message generation. [commit/tree.rs](../src/commit/tree.rs): flat/compact-directory projections and native file identity. [commit/rows.rs](../src/commit/rows.rs): row rendering and shared arrow/checkbox coordinates. [commit/view.rs](../src/commit/view.rs): changes list, messages, and default-open preview. [commit/refresh.rs](../src/commit/refresh.rs) owns background status and current-patch refresh. [commit/backend.rs](../src/commit/backend.rs): status, snapshots, plans, and execution. |
| Where will this push go, and will the target have the submodule commit? | [push.rs](../src/push.rs): outgoing commits, targets, plans, background jobs, and per-repository results. [push/update.rs](../src/push/update.rs) fetches the actual push target and coordinates integration/publication. [push/loading.rs](../src/push/loading.rs) schedules outgoing commit and preview reads; [push/preview.rs](../src/push/preview.rs) reads immutable file lists and comparisons. [push/diff.rs](../src/push/diff.rs) connects the current file tree to Log's revision viewer. [push/files.rs](../src/push/files.rs) parses changes and derives grouped rows from one tree; [push/display.rs](../src/push/display.rs) handles the session Group By menu; [push/view.rs](../src/push/view.rs) renders columns/hit areas. [push/submodules.rs](../src/push/submodules.rs) verifies remote availability of gitlinks in outgoing history. |
| How do branch lists load and branch writes get reviewed? | [branch/loading.rs](../src/branch/loading.rs) reads refs and prepares tree rows in the background. [branch/operations.rs](../src/branch/operations.rs) revalidates one repository before a reviewed write; checkout runs on a worker and merges its verified HEAD into the cached target repository. [core/loading.rs](../src/core/loading.rs) provides the bounded reader shared with Push. |
| How are remotes listed and edited? | [remote.rs](../src/remote.rs) owns scoped repository groups, selection, and reviews. [remote/model.rs](../src/remote/model.rs) reads configuration and redacts display text. [remote/editor.rs](../src/remote/editor.rs) edits names and URL lists; [remote/view.rs](../src/remote/view.rs) renders table and hit areas. [remote/operations.rs](../src/remote/operations.rs) validates and rechecks one repository before native Git remote/config commands. |
| What will a workspace Pull or Fetch update? | [shell/update.rs](../src/shell/update.rs) owns the Roots review, selected strategy, Git worker, and structured per-repository results. [shell/update/view.rs](../src/shell/update/view.rs) renders the shared review/running/results list, responsive columns, Details, and action hit areas. |
| Where is the shared Merge/Rebase default stored and applied? | [core/strategy.rs](../src/core/strategy.rs) loads and atomically saves the global preference. Push sends a direct toggle action to Shell; [ui/strategy.rs](../src/ui/strategy.rs) owns Pull's keyboard/mouse selector. [core/update.rs](../src/core/update.rs) integrates an explicit target OID with the selected strategy. |
| How are local paths protected before integration or a Log rewrite? | [core/paths.rs](../src/core/paths.rs) provides the collision preflight shared by [Log operations](../src/log/operations.rs) and [update integration](../src/core/update.rs), protecting ignored/untracked content and skip-worktree/assume-unchanged paths. |
| How is the application itself installed and updated? | [installation.rs](../src/installation.rs) dispatches `update` before Shell starts. [install.py](../scripts/install/install.py) and [install.ps1](../pages/install.ps1) own native installation validation, download, staging, and activation. [pages.py](../scripts/release/pages.py) builds public channel manifests from verified published releases. |
| When can smart merge choose automatically? | [merge.rs](../src/merge.rs): conflict actions, editing, and drafts. [merge/view.rs](../src/merge/view.rs): full-screen editor, controls, and aligned viewport. [merge/engine.rs](../src/merge/engine.rs): index stages, three-way comparison, file writing/staging, and Git continuation. |

## Remote Configuration

Remote reads every configured fetch and explicit push URL for repositories in
Roots, including empty repositories. Reads and writes contact no endpoint.
Linked worktrees share the same common Git directory and remote configuration;
the review states that shared ownership.
The panel editor retains exact strings and URL order; display/review/diagnostic
text redacts URL userinfo. Shell supplies configurable normal-mode commands,
the `6` shortcut, `igit remote`, shared confirmation, Git-write exclusion,
notifications, and workspace refresh after an attempted operation.

A review captures the repository identity and effective configuration before
showing the exact add/edit/remove scope. Confirmation checks Roots, worktree
and Git directory identity, configuration, and tracking refs before writing.
Git's native remote rename/removal maintains tracking and upstream semantics;
URL changes preserve other remote settings and custom fetch refspecs. External
or unsupported configuration ownership is rejected rather than overwritten.
Failures after a partial native operation report the partial result and trigger
a refresh. HEAD, the real index, and working files are outside these operations.
[remote/loading.rs](../src/remote/loading.rs) enables runtime background reads
after startup. It keeps the last successful repository rows on read failure,
clears rows immediately when Roots changes, installs only current generations,
and exposes pending/error state to block configuration writes. An explicit
refresh retries failed repositories.

## Native Installation and Software Updates

Both binaries dispatch a first argument of `update` before configuration,
repository discovery, or terminal setup. Cargo/manual copies provide owner
guidance; native copies invoke an embedded installer helper with structured
arguments, identifying the actual running executable. Ambient install-path and
channel variables cannot redirect that update. Unix needs Python 3.9+ and curl;
Windows uses Windows PowerShell and curl.exe. No Rust runtime dependency is added.

[Software Update Center](../src/shell/software_update.rs) owns a single background
check/install worker, footer status, and dialog controls independently of
workspace Pull/Fetch. It starts an optional check 1.5 seconds after the first
workspace frame. [The cache](../src/shell/software_update/cache.rs) atomically
stores successful typed reports in the global state directory and validates
their age, executable identity, native ownership record, and channel/version.
Manual checks bypass it. Automatic availability notifies; automatic no-update
and error results stay quiet. Manual outcomes and installation completion
notify, while Update Center retains its detailed dialog status. Opening another
workspace retains the worker and result.

[Typed installer access](../src/installation/inspection.rs) captures structured
reports from the embedded helper. Read-only unmanaged inspection compares the
running version against its channel without reading another installation's
state. Native installation pins the reviewed channel/version and keeps all
existing ownership/checksum/staging checks. During a dialog install, the worker
[samples stage markers and download bytes](../src/installation/progress.rs) in
its private helper directory. It uses the final response length when available;
the installer still returns one JSON result. The visible dialog animates while
work has no known total. Shell prevents quit during an install and prevents
restart during text editing or remote operations. An
explicit restart resolves the activated native executable, restores terminal
modes, and reuses the original CLI arguments.

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

[Pane search](../src/ui/search.rs) owns single-line input, literal match ranges,
wrapping navigation, and highlighted spans. Shell displays the prompt in the
footer and routes text through the panel's editing guard. The panes search their
loaded row labels or patch text and map each match back to a native row key or
source line. Searching does not filter a Git snapshot or fetch more data. Jumps
use existing selection and preview paths and reveal only the matched row's
ancestors. Stash reuses Log's inspector search through `SnapshotPreview`.
Queries are session state; replacing a snapshot clears old match positions
without changing the query. Commit's split file preview searches the focused
source revision using the revision renderer's line mapping; jumps expand hidden
context and pan horizontally. Its patch fallback retains the existing selectable
text layout. Neither presentation changes clipboard bytes.

Log's branch tree uses its existing `Field::Refs` editor for `/`, `S`, and
input clicks. Its query filters the in-memory scoped ref tree and reveals
matching paths while retaining ordinary folds. Enter saves that tab's
`ref_query`; Esc discards the draft. The tree has no pane-search controller,
footer prompt, or match navigation. Log's `find` and `next/previous-match`
commands apply only to its Diff file tree; `search-refs` owns the branch input.

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

Pane maximize is a session-only Shell flag applied to Commit, Branch, Stash,
Push, and Conflicts. Log always registers its enabled pane rectangles and dividers.
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

1. [shell::run](../src/shell.rs) enables alternate screen, raw mode, mouse reporting, and bracketed paste, then paints the workbench before any repository discovery. [startup.rs](../src/shell/startup.rs) uses a render-only `LoadingView` containing a default Commit panel, requested tab, icon mode and workspace path. It shares header rendering and footer identity with [workspace.rs](../src/shell/workspace.rs), without constructing a `Shell`, discovering a `Context`, or performing filesystem/Git reads on the terminal thread. [Commit loading](../src/commit/loading.rs) shares pane geometry with [view.rs](../src/commit/view.rs), rendering disabled composer actions, tree placeholders and a waiting preview. The startup worker discovers repositories and initializes Commit and Conflicts, reporting stages and the discovered root count over `mpsc`. The terminal thread handles resize and cancellation while polling, and redraws the spinner every 120 ms independently of stage changes; errors and cancellation restore terminal modes. Discovered branch labels are reused without another branch scan. The loaded `Shell` replaces the loading view once ready; pending input is never replayed. `--list-roots` remains a synchronous plain-output command.
2. [Workspace layout](../src/shell/workspace.rs) records the positions of right-aligned workspace tabs, the left-side Log tab strip in the same header row, and the active panel. The footer prefixes contextual hints with `IGit v<VERSION>` and software-update status. `Shell::dispatch` handles top-level overlays such as help, notification history, and reviews before routing input to the active tab. It also resolves the default `g n` notification prefix before panel routing. The prefix waits indefinitely and shares its first key with panel `g g` navigation: it forwards the first `g` to the affected list so a single `g` does nothing, the second `g` completes `first-item`, and `G` remains the independent last-item action. Log's Space filter prefix remains immediate and does not claim Notifications. If `notification-history` is rebound to a Space suffix, Commit Changes and Push outgoing rows apply their configured standalone Space check immediately; completing that chord restores only that check. In other panels, a second Space replays the standalone Space action. The configured hint delay only controls when a continuation overlay appears. Panels report text input through `editing()` so ordinary characters do not trigger global commands. [Pane geometry](../src/ui/panes.rs) registers pane rectangles/dividers for the active tab; adjustable sizes are stored in session character cells. Keyboard and mouse share bounds and size preferences; restoring defaults reallocates against the current terminal size. [Pane routing](../src/shell/panes.rs) handles Tab in registration order, finds directional neighbors by actual edges/overlap, and keeps an entire drag with the divider that received the press. Text selection and file dragging keep their original panel event routes.
3. The active panel receives `Panel::event` and returns an `Action`. It owns its selected row, focus, and scroll position. Shell owns panel switching, notification history and live-card state, and confirmation. The notification center retains up to 500 stable-ID entries per process and presents four newest live cards; workspace rescans do not clear this session state. Notification cards are separate from footer hints and operation progress. The `notification-history` global binding and Actions entry open the history view. A card body opens its matching history entry only when no dialog or text input is active; close hit areas remain active over dialogs and block clicks beneath. To request confirmation, the panel stores a concrete plan and returns `Confirm { title, body, token }`. Shell records the requesting panel, initially selects Cancel, and calls that panel's `confirm(token, context)` only after confirmation.
4. The notification view adapts at 72 columns: wider layouts show a list and detail pane in a maximum 100×18 frame; narrow layouts show a selected notification detail on Enter. Search, copy, and clear act on history entries, while closing a live card leaves its history record intact. Severity determines icon, palette color, TTL, and a creation timestamp on each card. Panel operations report explicit severity, but detailed review, loading, progress, input validation, and per-repository outcomes remain in their owning views. Update availability (including automatic checks) and installation completion become notifications; manual checks also report up-to-date results and errors, while automatic no-update and check errors stay quiet. AI request errors become notifications and leave the editable draft/focus in place.
5. For **Commit & Push**: the button or shortcut starts the local batch → Commit builds per-repository plans from the panel's selection and commits each repository individually → `Action::Committed` returns the successful repository set → Shell refreshes the workspace, switches to Push, and initially checks those repositories → user separately reviews and starts pushing. A local commit created and a remote updated are independent results.

[ui/review.rs](../src/ui/review.rs) supplies the compact review frame shared by
Branch Merge/Pull/Push and Push's batch confirmation. It owns content-based
sizing, the dimmed backdrop, border, wrapped-row scrolling, and fixed action hit
areas. Panels supply styled content from their reviewed plans;
[push/review.rs](../src/push/review.rs) renders Push's sources, destinations,
upstream choices, and update options. Shell retains confirmation focus, event
routing, and execution authorization; the renderer performs no Git operations.

All panels remain resident in the same Shell. Startup deliberately keeps its
first Commit and Conflicts reads synchronous, then calls
[enable_background_loading](../src/shell/refresh.rs) before entering the launch
tab. Startup still defers Log history until the first Log entry; that initial
load runs on a worker while workspace navigation and quit remain available.
[refresh.rs](../src/shell/refresh.rs) records one dirty bit per main tab and
processes it only when that panel has no blocking review/editor/selection/drag,
the Git operation is idle, and no overlay or key sequence owns input. Queued
work survives focus loss, overlays, and navigation; inactive tabs are refreshed
when entered. A terminal focus return invalidates every runtime list and reloads
an already loaded Log while preserving its browsing state.

F5 and completed Git writes use the same invalidation path instead of scanning
every panel on the terminal thread. Commit receives an explicit full scan; Log,
Branch, Push, Stash, Remote, and Conflicts are queued. The background worker
notifies "Refreshing workspace…" rather than claiming completion. A changed
Roots generation discards pending results. A same-Roots refresh retains cached
rows and applies the latest cursor, folds, drafts, and checks when their
identities remain valid. Log, Branch, Push, Stash, Remote, and Merge also reject
superseded requests through their generation/scope checks. Stash, Remote, and
Merge preserve a failed repository's last successful rows, expose a local
error, and block dependent writes until an explicit refresh succeeds. Merge
keeps an open document and its draft, marks it stale when its stages or native
operation change, and refuses Apply without changing the document's existing
execution-time revalidation.

Shell derives Conflicts visibility from the conflict list, which includes
unmerged index entries and operations ready for Continue. It checks again after
background installation; if Conflicts disappears while active, Shell returns
to the last main tab. Push refreshes on entry and while active; Branch queues
reads on entry and refresh. Branch, Push, Stash, Remote, and Merge use bounded
repository readers, with one latest request per panel and at most four
repository reads in a batch. A new request stops older work from publishing.
Runtime Log filters, tab reloads, optional presentation metadata, and graph
preparation use [loading.rs](../src/log/loading.rs) with the same generation
semantics. Actions and new comparisons wait for current history. The initial
synchronous Log read runs on the startup worker, then enables runtime
background loading; direct Panel callers keep synchronous refresh semantics.
Opening/rescanning from Actions still synchronously rebuilds context and
panels, then restores the prior runtime background mode.

Commit batches use [execution.rs](../src/commit/execution.rs), reporting
repository progress and independent outcomes while the terminal remains
responsive. Ctrl-X stops only repositories that have not started. Merge writes,
commit snapshot preparation, and explicit patch opening remain synchronous.
Read-only Git operations still have no general timeout or cancellation layer
for a hung child process; a disconnected worker is handled separately from a
worker that never returns. The panels use standard threads and `mpsc`, without
a global asynchronous scheduler.

[ui/selection.rs](../src/ui/selection.rs) provides shared text layout/selection for Log details/Diff, Commit message editing/preview, and Push Details / Diff. Each display line retains its original UTF-8 byte range. Cell hit testing, soft wrapping, scrolling, and highlighting share one projection; copy slices original text directly, while decorative rows have no source. Shared mouse handling returns `Action::Copy` on left-button release and collapses selection to its endpoint; the panel forwards the action to Shell. Clicks return no action; repeated releases do not copy again. Panels own selection lifetimes/focus. Log links reuse the same line layout and activate on release only without copying/dragging. `Panel::selected_text` exposes only valid selection in the current context, letting Shell send Ctrl-C to keyboard selection before quitting. Copy still uses `Action::Copy` → [ui/clipboard.rs](../src/ui/clipboard.rs); failures show reasons without logging text. The Commit editor keeps the post-copy caret endpoint and synchronizes source between consecutive events so input before redraw cannot misalign selection. Selection/viewport state stays in memory, outside draft files and Git plans. Shell tests capture copying through a thread-local receiver without modifying the system clipboard.

Shell routes quit shortcuts, Actions, and help through `request_quit`. It checks
software installation and running Git writes, then sets the event-loop exit flag
directly. Quitting has no confirmation token or popup; existing terminal cleanup
restores the user's terminal on return.

Commit's [preview adapter](../src/commit/preview.rs) feeds the same
[read-only revision renderer](../src/merge/view/revision.rs) as file history.
The [source reader](../src/commit/backend/preview.rs) uses literal native paths,
stage-0 index entries, bounded blobs, and bounded working-file reads. Staged
files compare HEAD (or an empty unborn tree) to Index; other files compare
Index to Working tree. Missing sides stay explicit. Git conversions and
unsupported content use an explained patch fallback. Source reads never stage
files or create Git objects. The existing background refresh worker returns
the selected file's comparison only for its scanned repository, checks the
current target, and preserves the viewer when its sources are unchanged.
Commit keeps its own workspace pane cycle while `[` / `]` selects the revision;
focused difference/fold controls take priority over workspace Pull/Fetch.

When help opens, Shell captures the original area's `HelpContent` without extending the public `Panel` trait. Panels provide only single-action shortcuts available in context. Internal ordering remains area, panel-wide, then global; the UI deduplicates them into a compact lazydb-style list. Search always receives text, ↑/↓ changes candidates, and Enter returns `HelpCommand`. Shell restores the covered dialog, validates original context, then sends the candidate's statically registered key sequence back through `dispatch`. Sequences/modifiers use the original event path. Search text never becomes an executable sequence; the original panel still produces review/validation for commit, push, and merge writes. F1 is handled before ordinary dispatch and does not cancel input. Canceling help leaves drafts, checkboxes, buttons, and scroll positions in their original objects. Search/candidate/scroll state stays in memory.

Mouse gestures separately record press coordinates and whether actual dragging occurred. Only `Drag(Left)` across cells starts selection. Ordinary `Down → Up` retains the press-time caret without remapping on release. Closing a notification, scrolling, or relayout therefore cannot turn a click into copying. Release, keypress, clearing selection, and source changes end unfinished gestures. `cargo test --locked click_after_copy --lib` covers another click after copying, movement within one cell, and redraws between press/release.

### Opening Immutable File Revisions

File History's `show-file-at-revision` action uses the selected `Rev` and its
recorded historical filename, without depending on the asynchronous preview.
`Action::ShowFileRevision` enters Shell's existing pending editor handoff.
The request validates a full commit object ID and a relative path, then uses
`core::revision::file` to distinguish an absent file from an empty blob and
enforce the existing 512 KiB UTF-8 text limit. All reads target immutable trees;
no checkout, index write, or working-file read is involved.

Embedded requests emit OSC 51 `igit-show-revision` with JSON fields `version`
(1), `session`, `repo` (absolute), `path` (historical, repository-relative), and
`revision` (full commit ID). The companion validates the managed terminal token,
reads the immutable object asynchronously, and opens a read-only buffer with a
`filename (short hash)` suffix. It preserves the existing terminal/focus lifecycle
and keeps full repository/path/revision identity separate from the short title.
Standalone requests hold a private `TempDir` beside the editor command for the
entire synchronous editor invocation, containing only a read-only snapshot.
Dropping it after the editor returns removes the snapshot, including after launch
failure. Existing working-file requests keep their OSC name and schema.

### Direct Panel Startup and Embedded File History

[CLI parsing](../src/shell/cli.rs) separates startup presentation options, a panel
command, and an optional directory/file target before opening the terminal.
`log <file>` locates an existing ancestor for discovery, allowing deleted paths.
Startup sets Log's file intent before its initial background load. Branch,
Push, and Stash use their existing tab-entry paths; the native updater and
noninteractive configuration/listing commands retain their own behavior.

[File history](../src/log/file_history.rs) resolves the deepest discovered owning
worktree and adds a tab-local target to `Filter`. The target is separate from
ordinary path-filter drafts and remains fixed across resets. The history query
uses literal path arguments, `--follow --full-history --diff-merges=separate`,
NUL-delimited metadata/name-status records, and deduplicates merge rows by OID.
Each revision retains actual parents plus the historical filename and rename
source; comparisons use immutable blobs, including the old path before a rename.
Rename traversal inherits [Git's single-file follow semantics](https://git-scm.com/docs/git-log#Documentation/git-log.txt---follow),
including its limitations for complex nonlinear rename histories.

The existing preview worker also builds the inline file comparison; generation
checks reject obsolete results after navigation, refresh, or tab/scope changes.
The normal Log Diff and inline History Diff share the comparison builder and
[revision renderer](../src/merge/view/revision.rs). Inline rendering removes
modal chrome and registers two Log panes with its own resizable divider. It
does not report text editing; focused revision keys are routed ahead of global
Pull/Fetch, while workspace navigation, pane controls, and review guards remain
available. The Lua companion plugin uses argv-based Neovim terminal jobs and
window-local options; it performs no Git operations itself.

File History's `D` action opens **Compare with Local** using the selected
immutable revision and a working-file snapshot from
[compare.rs](../src/log/file_history/compare.rs). The snapshot records bytes and
permissions, rejects symlinks, binaries, non-UTF-8 content, and files over
512 KiB, and becomes the stale-write guard for later applies. The shared
[RevisionDiff](../src/merge/view/revision.rs) local-merge mode renders
`Revision` and `Local`, exposes `»`, `Apply`, and `All`, and builds replacement
text by taking a complete aligned change group from the revision side. Writes
recheck the repository scope, complete snapshot, permissions, ancestor paths,
nested repositories, and Git conversion attributes; they use a same-directory
temporary file, preserve uniform LF/CRLF layout, and never update the index.
Slices with custom filters, working-tree encodings, or mixed line endings are
rejected. A separate confirmation covers **All**, which is unavailable when
the revision omitted the file; each successful apply rebuilds the comparison
from the new working file; closing it
returns `Action::Refresh` so Commit and the workspace see the changed bytes.
HEAD and immutable objects are never modified.

The focused `igit diff --line N -- <file>` route uses
[file_diff.rs](../src/shell/file_diff.rs) to load the owning workspace and HEAD
comparison on a background worker, then runs only the revision viewer.
[FileDiff](../src/log/file_diff.rs) reuses `LocalCompare` source validation,
line-ending conversion, stale-write checks, aligned hunk reconstruction, and
the shared review dialog. It checks that HEAD still identifies the opened
commit before restoring. Each successful single-group or reviewed full restore
records the previous Local text and selected change position in a session-local
undo stack. `u` / `U` writes the latest entry back through the same snapshot and
stale-write guards, rebuilds the comparison, and restores focus to that change
position; closing the viewer discards the stack. `RevisionDiff` carries a
pending one-based Local line until its first usable draw, finds that line in
the aligned row map, expands its context fold if needed, and centers the row
using the actual code height. The pending location is consumed once so later
scrolling remains independent.

The Neovim companion supplies an absolute private UTF-8 snapshot through
`IGIT_DIFF_BUFFER`. This snapshot remains outside discovered Git working trees.
The native viewer reads HEAD from the real repository and Local from the
snapshot; restores replace only that snapshot using checked temporary-file
replacement. The companion imports reviewed text on exit or hide, checking the
original buffer name and changedtick and providing normal editor undo. A stale
buffer keeps its newer contents and the reviewed result remains recoverable.
Neither side automatically saves the source buffer or updates the index.
Standalone File Diff omits the snapshot and uses the original guarded
working-file write path.

### Editing working files

Commit returns `Action::EditFile` for the highlighted native repository/path,
independently of commit checkboxes. Shell queues the request and handles it
outside drawing and panel dispatch. [editor.rs](../src/shell/editor.rs) validates
an existing working file, resolves `git var GIT_EDITOR`, and forwards the literal
absolute path as a shell positional argument. The query restores the inherited
`GIT_EDITOR`, overriding the shared Git adapter's deliberate noninteractive
`true`. Configured editor expressions retain Git's shell semantics; filenames
never become shell code. Git for Windows supplies the required `sh`.

The terminal guard releases and reacquires raw mode, alternate screen, mouse,
focus, paste, enhanced-key flags and the Kitty marker around a foreground editor.
Exit/launch failures also restore the TUI; returning refreshes the workspace.

The Neovim plugin advertises `IGIT_NVIM_SESSION` per managed terminal. A native
file action emits OSC 51 `igit-edit` with versioned JSON containing that session
and an absolute UTF-8 path. JSON escaping protects terminal control characters.
The plugin accepts matching requests only from its own terminal buffer and
schedules changes outside TermRequest, supporting both Nvim 0.10's string data
and later `.sequence` data. Background/stale requests cannot steal focus.
The parent reuses buffers through a normal guarded buffer switch, hides floats
after success, and retains split/tab sessions and their jobs. There is no RPC
server, nested editor, staging or application-owned file writer in this path.

## Repositories, Scope, and Persistence

Workspace Pull and Fetch build a read-only plan from global Roots. The dialog
opens immediately with repository names from the existing Context; the plan and
display comparison are prepared on a background worker with at most four bounded
readers from [core/loading.rs](../src/core/loading.rs). Rows remain `Checking`
and confirmation stays disabled until the complete snapshot arrives in input
order. Canceling invalidates late results. Each worktree is checked again before
running Git. Pull also revalidates the branch, HEAD, upstream, and resolved
remote URL; Fetch revalidates the configured remotes.
After confirmation, Pull explicitly fetches its reviewed upstream and calls
[core/update.rs](../src/core/update.rs) to integrate the fetched commit with
Merge or Rebase. Workspace Fetch runs `git fetch --all`; both disable submodule
recursion. Pull inherits the saved application default, while `m/r` overrides
only the current review. Its `s` selector saves the shared default. Execution
captures the strategy and the update dialog stays active through completion so
another workspace write cannot overlap it.

Presentation rows retain the reviewed identity, target labels, and repository
tint. A separate display comparison reads the live current branch, resolves its
configured local upstream ref, and uses `rev-list --left-right --count` for
ahead/behind values. This metadata never changes plan equality or performs
network access; unavailable comparisons omit counters. Branch, Push, and update
reviews share arrow/color spans in [ui/idea.rs](../src/ui/idea.rs), but Push
compares its selected target rather than the configured upstream. The worker
rereads comparisons after execution, returning structured outcomes in plan
order. Row selection and Details scrolling cannot change the batch. Shell
refreshes Log after Fetch and the workspace after Pull. Failures and Git conflict
state remain per repository.

- **Discovery.** `Context::discover` first finds the working tree containing the startup directory, then traverses below that directory. Git resolves and deduplicates working trees with `.git` files/directories, ignoring bare repositories and common build/dependency directories. `--show-superproject-working-tree` identifies submodules; `Repo.parent` exists only when the parent working tree is also discovered. Ordinary nested repositories have no such dependency. Uninitialized submodules are not usable working trees.
- **Identity.** `repos: Vec<Repo>` is ordered by canonical path. `scope: BTreeSet<usize>` contains repository indices currently allowed for display/operations, initially all. IDs are valid only for that discovery result; old indices cannot restore identity after a rescan. Display names, abbreviated SHAs, and file row numbers are not persistent keys either. Narrowing Roots must not let hidden repositories silently join the next write.
- **Local state.** Nonempty `$IGIT_STATE_DIR` and `$XDG_STATE_HOME/igit` take precedence. Unix then uses `$HOME/.local/state/igit`; Windows uses `LOCALAPPDATA/igit/state`, `APPDATA/igit/state`, `USERPROFILE/AppData/Local/igit/state`, then Unix-style HOME as a compatibility fallback. The pure resolver in [core/state.rs](../src/core/state.rs) accepts injected environment/platform inputs for portable testing. A hash of the canonical workspace path selects the workspace subdirectory. Its `commit.json` stores groups, file membership, one shared `message`, retained legacy `drafts`, and workspace AI endpoint/model/prompt, replaced through a temporary file in the same directory. Internal keys encode repository/path bytes. Moving the workspace creates a new state directory; concurrent instances have no draft-merging mechanism. Checkboxes, focus, pending plans, push results, and merge undo stacks are excluded.
- **Shared update strategy.** [core/strategy.rs](../src/core/strategy.rs) stores the serialized Merge/Rebase choice in `update-strategy.json` directly under that global state root, outside hashed workspace directories. Missing state defaults to Merge. Saves flush a temporary file in the same directory and atomically replace the destination; the UI adopts a new default only after saving succeeds. Read/parse errors are reported without overwriting the file, and failed saves retain the prior default. The preference survives restarts and workspace changes without writing Git config or `settings.toml`. Push captures it in the reviewed plan; changing it invalidates the review, and busy guards prevent changes during execution. Pull reads the same default when opening a review, retaining temporary `m/r` overrides.
- **Command environment.** Git processes derived from `core::command` use `git -C <repo>`, clear inherited overrides for working tree, index, object directory, and similar settings, and disable interactive terminal prompts. Callers explicitly inject private indexes. Configured Git identity, hooks, signing, and credential helpers still affect results; interactive credential entry is outside the TUI. Paths are parsed from NUL-delimited bytes; lossy display strings must never be sent back to Git as paths.

## Core Data Paths

**Stash.** [stash.rs](../src/stash.rs) owns repository grouping, selection,
folding, and focus. The empty repository summary has a session-local fold
state; read failures remain separate rows. [model.rs](../src/stash/model.rs)
reads stash reflogs with NUL-separated immutable object IDs, selectors,
timestamps, and subjects. [view.rs](../src/stash/view.rs) renders the compact
rows and identifies the current selection above the file inspector. List reads
run only on tab entry, explicit refresh, or terminal focus, leaving startup
unaffected. After startup enables runtime loading,
[stash/loading.rs](../src/stash/loading.rs) keeps cached rows for the same Roots,
clears them immediately on scope change, installs with the latest cursor/folds,
and retains a failed repository's last successful entries while blocking
stash writes. [preview.rs](../src/stash/preview.rs) debounces selection for
60ms, reuses a successful snapshot for the same repository/object ID, runs one
snapshot worker at a time, and rejects stale generations.
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
`stash.index` option. [operations/view.rs](../src/stash/operations/view.rs)
renders the reviewed snapshot; Shell handles confirmation input and dispatch.
Native apply uses the immutable object ID and suppresses
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

**Branch.** [branch.rs](../src/branch.rs) owns repository selection, pane focus,
name input, and the pending shell review. [model.rs](../src/branch/model.rs) reads
local and remote refs, commit IDs, symbolic HEAD, upstreams, and worktree ownership
for reviewed operations. Background display reads add
`%(upstream:track,nobracket)` and the tip's local committer time to the same
NUL-delimited `for-each-ref` query. Counts and times are cached by full ref
outside the reviewed snapshots; operation revalidation does not request them.
[recent.rs](../src/branch/recent.rs) adds display-only last-used metadata from
the selected worktree's `git reflog show HEAD --date=unix` output. It parses
checkout destinations and event times, uses reflog sequence to break
same-second ties, follows rename records backwards, and keeps only existing
local refs. Missing or unreadable reflogs yield an empty usage map without
failing the ref scan. No usage data is persisted or shared across worktrees.
Verified checkout results also carry fresh usage read on the checkout worker,
using the reviewed ref names plus the destination ref for new tracking branches.
The UI installs that map only in the matching repository cache, before projecting
its tree. No additional refs scan or terminal-thread Git call is needed.
[loading.rs](../src/branch/loading.rs) reads scoped snapshots
and builds the selected tree in the background. Changing repositories projects
rows from the cache; a completed refresh uses the latest selection and folds.
The same ref scan reads remote HEAD aliases to prioritize the default branch;
this display metadata stays separate from reviewed branch state.
[tree.rs](../src/branch/tree.rs) projects grouped or flat rows for one
repository with stable fold and cursor identities. Groups start expanded;
the tree records manually collapsed folder IDs per repository. The
[View Options menu](../src/branch/display.rs) toggles prefix grouping and
Name/Recently Used sorting from cached refs; Recently Used is the default.
Name preserves current/default
priority; Recently Used keeps only current priority for local refs, then
orders by switch time and sequence, with unknown refs falling back to names.
Remote order is unchanged. Folder keys inherit the best descendant key.
Sorting is copied into background and unfolded search projections; installing
a refresh reprojects if the user changed sorting meanwhile.
[navigation.rs](../src/branch/navigation.rs) maintains both
viewports and exposes `gg` to the shell's sequence picker.
[rows.rs](../src/branch/rows.rs) measures upstream columns per repository,
including counters on folded branches, and reserves a column for commit times
when there is room. Repository rows use their cached HEAD and tracking counts.
Both lists use shared Log reference colors and tracking arrows, reserving
cells for counts before clipping branch names. The selected-row details repeat
the counters. [view.rs](../src/branch/view.rs) uses the
shared pane layout for the repository list and selected branch tree, hiding
the repository list for single-repository Roots. State is session-only.
Branch Pull and Push run reviewed operations in a background worker. Pull
fetches explicit upstream refs and uses Git's fast-forward and worktree checks.
Shell routes Branch's `p` to the focused branch's reviewed operation before
the global Update Project handler. The global Pull binding excludes the Branch
browser's context; `branch.pull` defaults to `p` with `U` retained as an alias.
Actions can still open the global review. Branch Pull remains fast-forward-only
and does not use the shared Merge/Rebase preference. Its review shares the
Merge/Push renderer, reversing Push's direction to show the configured upstream
as From and the focused local branch as To.
Branch Push reuses the Push executor with a pinned local branch ref and OID.
Shell blocks overlapping writes and refreshes workspace state after completion.

[Branch comparison entry](../src/branch/compare.rs) derives one exact repository
path and two full refs from a ready Branch snapshot. Its dynamic context menu
and configured `compare` key open a session-only
[comparison workspace](../src/log/compare.rs), owned by Branch. A generation-safe
background reader resolves both refs together and validates the initial current
branch. Both embedded Log panels receive the same immutable OID pair in opposite
directions. Their normal history workers, graph projections, debounced previews,
filters, file inspectors, and read-only revision viewers are reused.

The additional Log filter binds history to the original repository path before
loading topology or refs, and supplies explicit included/excluded OIDs to Git.
It survives filter resets and participates in history-request identity, so an
older range cannot publish into a newer one. Comparison contexts intersect
global Roots; changing scope cancels old work and clears stale row identities.
Missing refs and hidden repositories produce explicit errors. Refresh resolves
the originally named pair again; no fetch or Git write occurs.

Comparison owns the tiled geometry and maps six distinct workspace pane IDs
to each embedded Log's graph/files/info panes. Shell retains standard focus,
resize, copy, help, configuration, and polling routes. Only the active child's
overlays receive input, and its Diff uses the full comparison work area.
Comparison mode prevents commit-write menus and shortcuts from entering Log's
review flow. Closing drops the readers and restores the resident Branch tree.

[operations.rs](../src/branch/operations.rs) prepares a single-repository operation. Compatible checkout and tracking run without a
confirmation dialog on a background worker; Enter returns immediately. Preparation checks Roots, worktree path, refs, HEAD, upstreams,
worktree ownership and pending Git operations. The switch worker repeats a
lightweight HEAD/source-ref check, runs Git off the terminal thread, and reads the
actual symbolic HEAD back before reporting success. Create and merge use the reviewed
immutable source OID. Local checkout uses `switch --no-guess`; remote checkout
uses explicit `--track -c`. Safe deletion is `branch -d`, rename is `branch -m`,
and merges disable autostash. Checkout/merge protect ignored files and suppress
recursive submodule updates. Git provides its normal locking and overwrite
checks; IGit does not lock out other Git processes between reads and execution.
Roots changes and overlapping writes wait while the worker runs. A verified
checkout updates only the target repository's cached snapshot, current marker,
and last-used metadata,
without rescanning the Branch list or moving the user's repository selection;
folds, grouping, sorting, and cursor identity are retained. Pull/Push completion,
uncertain checkout state, and partial conflict results use the safe workspace
refresh path. Ref scans first run on entry; F5, reentry, focus return, and completed
remote operations refresh them while Branch is active. Name drafts and pending
reviews retain their original source identity across refreshes.

[checkout.rs](../src/branch/operations/checkout.rs) turns Git's refused-overwrite
result into an IDEA-style three-action review, rendered by its
[view](../src/branch/operations/checkout/view.rs) through Shell's Branch-owned
confirmation route. It reuses the existing Commit stash backend's whole-repository
snapshot, path checks, preview, and save executor through a crate-private export.
The review lists every file that saving or forcing could affect. Special index
flags, ignored-file collisions, nested repositories, and target submodule or
directory-symlink boundaries refuse unsafe automation.
Smart Checkout runs in Branch's existing write worker: validate → named stash
including untracked files → protected switch → apply immutable stash OID.
Restoration first uses `--index`. If that fails before any writes, a normal
three-way apply exposes conflicts; a temporary index merges the original staged
patch onto the target and restores nonconflicting index entries without changing
working files or conflict stages. Incomplete staging retains the backup.
Only a fully restored result can drop the exact entry after rechecking the
entire stash reflog. Failures keep recovery instructions and the saved OID;
switch failures on the unchanged original branch attempt restoration there.
Force Checkout has its own Cancel-first destructive review and revalidates the
same local snapshot before `switch --discard-changes`. Completion refreshes all
panels; restore conflicts open Conflicts without creating a merge operation or
automatically committing. Resolving those files and dropping their backup remain
separate user reviews.

Log's [display menu](../src/log/display.rs) stores session preferences and temporary menu focus without writing project/state files. Filters and selected SHA remain per Log tab. `Rev` retains author email, author date, and commit date; sorting still uses commit time. After refresh/tab changes, [presentation metadata](../src/log/model/presentation.rs) reads per-repository identity. Loaded OIDs are queried in batches for `%G?` only when signatures are visible; `git cherry HEAD <resolved-tip>` runs only when cherry highlighting is enabled and another branch is selected. Queries use the shared Git environment, report failures per repository, and preserve history. Rendering, hover, and row navigation do not run these commands. Runtime reloads include signature verification and patch comparison in the history worker; direct synchronous Panel callers still wait for them. Shortening long edges recomputes the visible row projection and releases hidden columns; arrow hit areas retain the real visible endpoints without modifying the underlying DAG.

[Commit/workspace comparison](../src/log/workspace.rs) captures the highlighted
row's repository path and immutable OID, retaining the ordinary Log panel
while a temporary child supplies its existing file inspector. The
[backend](../src/log/workspace/backend.rs) reads the selected tree into a
temporary index and compares that index's tracked paths with current working
files. Current tracked and non-ignored untracked additions are merged by exact
NUL-delimited byte paths. This avoids treating staged content as the working
version, handles staged deletions followed by recreation, and leaves the real
index and repository metadata unchanged. List and debounced preview jobs use
independent generation-safe readers and validate the original repository
against Roots. Refresh keeps the selected commit fixed; closing restores the
resident Log state and refreshes the workspace when an apply occurred.

The [bounded working-source reader](../src/core/revision/working.rs) is shared
with Commit previews. It rejects parent symlinks/nested repositories, reads
symlink text, enforces 512 KiB tracked and 96 KiB untracked-file limits, and explains
unsupported content or conversions. Immutable commit blobs still come from
the revision reader's exact tree/blob queries. Successful sources use the
shared two-pane Diff with Commit / Working tree titles, synchronized scrolling,
alignment, folding, search, and copying. Supported previews expose one aligned
change group at a time through the revision gutter's `»` (or `>>` in ASCII)
control, `L`, and **Apply**. The write delegates to
[LocalCompare](../src/log/file_history/compare.rs) through
`prepare_working_tree`, takes the complete group from the commit side, and can
insert or delete lines. The workspace has no Apply All path.

Before replacement, the guarded writer verifies Roots, repository identity,
the captured commit OID and file path, the opened bytes and permissions
snapshot, Git conversion attributes/config, and path safety including
symlinks and nested repositories. Writable content is UTF-8 with uniform LF or
CRLF, and the working file's line-ending style is retained. Root identity
captures the canonical directory, creation metadata, and Unix device/inode;
refresh and replacement preflight recheck it. A single-group apply retains
the original working bytes for aligned context, including visually equivalent
tabs and spaces. The write changes
only that working file, preserving unrelated hunks, exact real-index bytes
including the selected file's staged content and unselected staged siblings,
other files/repositories, HEAD, refs, and reflogs. Unsupported previews remain
read-only with an explanation. A successful apply rebuilds the current preview
in place and requests an asynchronous changed-file refresh; closing after any
apply refreshes the normal workspace. File History retains its separate
Compare with Local flow with one-group apply and reviewed Apply All.

Log holds cherry-pick selections by repository and immutable object ID. Review validates all sources against the same destination HEAD and passes the reviewed order to Git in one command. Git owns the pending queue and Conflicts handles continuation, skipping, and aborting.

**Log: multiple commit graphs on one screen.** Each repository starts from local branches, remote-tracking branches, tags, and valid HEAD. `git log --date-order` produces a topologically valid sequence; sequences are then merged by their head timestamps. [Graph metadata](../src/log/model/graph/load.rs) reads full object IDs, parents, and branch/tag decorations once per history load, without loading all messages or authors. Its cached permanent ordering keeps fragment colors stable when older commits are outside the display page. [Graph topology](../src/log/model/graph.rs) matches real parents/children by repository ID + object ID. Each parent edge remains independent until its parent commit. [Layout](../src/log/model/graph/layout.rs) orders branch heads by Git reference priority, assigns DFS fragment indices, and sorts each row's node and passing edges using IDEA's endpoint comparator. Removing a node or folding a long edge changes the next row's positions; adjacent half-row segments share one boundary coordinate. The compact projection is prepared with history; the expanded projection is cached lazily. [Character rendering](../src/log/model/graph/render.rs) draws only the viewport with one horizontal origin, separating crossings from commit joins. [Colors](../src/log/model/graph/color.rs) use reference names and fragment indices, independently of screen columns. Text/hash filtering keeps the loaded topology's ordering/colors and includes only real edges with both endpoints visible. The first page is 160 commits per repository. When the cursor enters its final 24 rows, [loading.rs](../src/log/loading.rs) queues the next page on the existing generation-checked worker while cached rows remain usable. Page windows double (160, 320, 640, …) and have no total cap; `+` requests the next window immediately. Git has no stable resumable cursor for this combined multi-root ordering, so each request re-reads a longer prefix; geometric growth keeps cumulative work proportional to the final window. Selection remains keyed by repository ID and object ID. Rendering remains viewport-only. Text/hash filter changes do not reload all history; `regex-lite` matches in memory. Queries use full ref names across repositories and Git namespaces to distinguish identically named branches, remotes, and tags. Log tabs store only filters/selection; switching reuses the same panel and reloads visible data. Roots applies to all tabs.

The [Branch menu](../src/log/branches.rs) builds candidates from loaded refs within the current tab's Roots, listing each full ref once across repositories. Hover, submenu scrolling, and search do not run Git. `branch-favorites.json` in workspace state stores only manual star preferences, atomically replaced through a temporary file without changing repository configuration. Each Log tab has its own [branch expression](../src/log/model/branch_filter.rs). Full refs resolve per repository before queries; history and reachability use identical starts/exclusions. Cherry comparison is enabled only for one unambiguous branch.

[Paths](../src/log/paths.rs) stores tab-local Roots separately from its [path set](../src/log/model/path_filter.rs). Roots changes reuse loaded history. Each path holds an optional repository ID and native relative path. A repository query appends only applicable files/directories individually after `git --literal-pathspecs log … --`. Selecting a whole repository passes no paths; repositories outside all path scopes skip history queries. The [tree selector](../src/log/paths/tree.rs) reads only immediate directory children on expansion, skipping `.git`, symlinks, and other repositories. Text/tree selectors share the applied path set; drafts replace tab filters only after confirmation. A confirmed path set clears tab-local Roots and displays the Roots rows unchecked; choosing a root clears the path set and restores root scope. The menu also keeps the 10 most recent distinct non-empty path sets in panel-local session memory, newest first. Recent selection reuses the same path/runtime reload path, keeps the menu open, and writes no persistent state.

Loaded refs remain cached in `History`; the [reference tree](../src/log/refs.rs) intersects global Roots, tab-local Roots, and `PathFilter::for_root` before aggregating rows and ownership metadata. Cached Roots changes rebuild both history visibility and the reference tree without new Git queries. Applied paths, refresh, and tab restoration rebuild refs through the existing reload paths; the tree's repository scope does not depend on matching commits.

The commit-list cursor moves in memory. During continuous navigation, [detail loading](../src/log/preview.rs) waits for selection to remain stable for 60ms, then reads changed files, body, and containing branches and builds the file tree in the background. At most one detail query runs at a time, with only the latest pending row retained. Selection changes/refresh invalidate older requests. Branches reload even when the SHA remains identical across refresh. Shell handles queued consecutive movement keys in order and redraws their final position once; before other actions it redraws, so mouse hit testing sees the current row.

[Commit actions](../src/log/actions.rs) capture one row's root, worktree path and full OID independently of filters, viewport and preview. Names and messages use the [shared Log editor](../src/log/input.rs); Shell confirms a single-use [operation plan](../src/log/operations.rs). The menu and graph-focused `d/u/R/F2` paths share preparation and confirmation. Execution rechecks scope, target, branch/HEAD and operation eligibility. All operation history reads and writes bypass replacement refs and grafts. Drop, Undo, and message editing reject any commit contained by a locally known remote-tracking branch; review never fetches.

[Tip removal](../src/log/operations/tip.rs) restricts Drop/Undo to the checked-out unpublished tip. A [prepared ref transaction](../src/log/operations/transaction.rs) compares the reviewed OID, locks HEAD and its referent, then checks the branch identity and publication again before touching files. Undo updates only refs, retaining soft-reset semantics. Drop requires a clean tracked index/worktree and protected-path preflight, then uses the non-forcing two-tree `read-tree -m -u` transition while the refs remain locked. Merge tips use the first parent; initial commits use an empty tree and delete the branch ref while retaining symbolic HEAD. `ORIG_HEAD` updates in the same transaction. Failed preparations leave files/index untouched; a failed commit restores files only when HEAD still matches the reviewed state. Native Revert protects local paths and preserves conflict state for the existing Conflicts workflow. Checkout suppresses submodule recursion and protects ignored files.

[Message rewriting](../src/log/operations/reword.rs) walks only the selected commit and its descendants reachable from the current HEAD in parent-first order. Raw commit reconstruction replaces the selected message and changed parent IDs, preserving every tree, parent order, author/committer metadata, descendant message bytes, and unrelated headers. Invalidated commit signatures and merge-tag attestations are removed; an edited message uses UTF-8. No patch replay, checkout, index changes, signing, or commit hooks are involved. The same prepared transaction locks HEAD and the branch, rechecks branch/publication, and atomically records `ORIG_HEAD` and the replacement tip. Other local branches and tags stay in place. Git owns object/ref/index locking; all results refresh Log, Commit, Branch data, and Conflicts.

[Historical merge details](../src/log/model/merge_diff.rs) read the combined changed paths plus each parent-to-result path list, preserving NUL-delimited byte paths and a parent index for each leaf. The combined list follows [Git combined diff semantics](https://git-scm.com/docs/git-diff#_combined_diff_format): only paths modified against all parents appear there. File lists and parent subjects load in the existing preview worker, with the same generation invalidation. The [shared revision reader](../src/core/revision.rs) reads exact `ls-tree` entries and immutable blobs through `cat-file`, capped at 512 KiB each; no index, working file, merge operation or clean/smudge filter supplies its contents. Missing entries are empty sides with explicit labels. The selected commit determines the presentation: two parents use the same three versions from every file group; an ordinary commit compares its parent and itself in two panes. Initial commits have an empty Before source. Unsupported text/object types fall back to a bounded patch; in that fallback, parent leaves still select the corresponding parent explicitly.

Read-only revision views pair each parent or Before with Result or After. Patience matching uses trimmed line keys for anchors and bounded similarity pairs the remaining replacements. Alignment projects the original lines into sections with a fixed set of present panes. Every pane uses the same section offsets for drawing, line numbers, folding and search. Adjacent changed sections form one navigation stop. A single pane reduces a missing section to one row and maps the scroll position back to the same section when the layout changes. Source byte ranges keep copy and search tied to the original tabs, CRLF and Unicode. [Revision alignment](../src/merge/view/revision/alignment.rs) · [Revision viewer](../src/merge/view/revision.rs)

The shared renderer uses the aligned row for read-only tint and fragment comparisons. Equivalent indentation uses the same tab stops as drawing and gets a muted gutter dot. Classification retains the original body and line endings. The renderer paints empty rows as decorations and uses change colors on the overview rail. Live source changes compare against the merge base with red priority for pending conflicts. Directional token operations distinguish replacements from insertions, using character boundaries for unspaced CJK text. [Diff highlights](../src/merge/view/highlight.rs) · [Row renderer](../src/merge/view/rows.rs)

The live editor caches line pairings by section, comparison and exact text. Panes share the entries and reuse them while scrolling or resizing. Edits replace the affected entry and a new document clears the cache. Small comparisons use a fixed work limit to keep highlights stable under load. Larger comparisons use deadlines, limited lookahead and prefix and suffix sampling. Long lines use row colors when fragment analysis would exceed its limit. The display lexer includes basic single-line XML markup. [Syntax colors](../src/merge/view/syntax.rs)

[List navigation](../src/log/navigation.rs) stores vertical origin and the most recently drawn usable row count. It corrects edges after every selection change so batched input and per-key redraw have identical scrolling. Rendering corrects again for new window height. `gg` is recognized in the non-graph Log panes, while `y` commit-copy sequences are limited to the commit list; focus changes/overlays clear prefixes. `Action::Copy` routes through Shell and the [clipboard adapter](../src/ui/clipboard.rs), using command stdin rather than interpolating text into commands or notifications. Log ID/name/email come from the current row. Its `ym` runs read-only `git show` for that exact repository/OID to retrieve the full message, independent of potentially pending details.

[Commit copying](../src/ui/commit_copy.rs) shares ID, message, and author formatting between Log and Push. [Push's handler](../src/push/commit_copy.rs) resolves only the focused outgoing commit and owns its pending prefix; Shell exposes the same `CommitCopy` chooser and cancellation route. Push reuses the row's full ID and reads message/author on demand with immutable repository/OID arguments, without adding metadata to outgoing snapshots or relying on asynchronous preview results. Help and `[keybindings.push]` expose all three commands. File-tree path copying remains a separate focus context.

[Commit web URLs](../src/core/hosting.rs) resolve the captured repository and
full OID using read-only local Git calls. The optional `igit.webCommitUrl`
template takes precedence over discovery; `igit.webRemote` selects an explicit
fetch remote. Otherwise, containing remote-tracking branches narrow the
candidates before current-upstream/origin/sole-remote preferences apply.
`git remote get-url` honors Git URL rewrites. Transport parsing strips
credentials and maps recognized service URLs to commit routes; unknown
services require a template. Errors omit the original remote URL.

Log emits `Action::OpenCommit` only from its current, scoped commit-list
selection. Shell delegates to the [browser adapter](../src/ui/browser.rs),
which reuses `core::loading::Load` for one background request and polls its
result into the notification center. Native URL handlers receive a validated
HTTP(S) URL as a separate argument with detached standard streams. A handler
that remains alive after 500ms is reaped separately, releasing the request
without waiting for the browser to close. No shell
interpolation, Git writes, network discovery, or new dependency is involved.
Tests capture launches and use fake native commands without opening browsers.

[Key sequence hints](../src/ui/key_sequence.rs) derive candidates from pending `g` states in Branch, Commit, Stash, Remote, Log, and the Merge conflict list, plus Log `y` and Merge `[/]` states. The same `g` prefix can also start the global notification chord; forwarding its first key to the panel leaves a single `g` inert and lets `gg` complete `first-item`. Shell candidate state stores cursor, last-rendered hit areas, and a display deadline. The deadline participates in the event poll's minimum wait; reaching it only shows the hint, without expiring the prefix or blocking later keys. Completion/cancel clears timing, and a new prefix restarts it. The bottom overlay uses shared colors and cell-width-based columns. Once visible, arrows, Enter, and mouse events inside it run before ordinary panel/global events. Esc/Ctrl-C can cancel during the delay too. Choosing a candidate returns the continuation key to the original panel route rather than creating another Git entry point. Global panel changes, refresh, and dialogs clear both kinds of prefixes. Single-key actions without prefixes and text editing bypass the candidate overlay.

**Commit: checked content, not the entire real index.** Changes come from `git status --porcelain=v1 -z`. The tree starts with Directory and Repository grouping enabled and legacy Module disabled. Native paths drive browsing identity, checkboxes, and regrouping; parent-directory hints are display-only. The tree projects scoped unmerged paths into Merge Conflicts as a flat special child while keeping their saved changelist membership. Resolve uses the existing Shell tab action; it does not apply a resolution. Grouping and expansion changes never touch the index. Preview starts enabled; closed previews do not read patches. Open previews use the current row to choose a file Diff or in-memory container summary. Submission builds a temporary index from HEAD, or the empty tree for an initial commit. Selected files with staged content use that version; otherwise, selected working-tree content is read. A file staged and then edited again therefore commits only its staged version. `write-tree` obtains the planned tree, while HEAD, branch, and selected index records are retained. Preparing the snapshot may create Git objects but does not move HEAD or overwrite the real index.

[Commit refresh](../src/commit/refresh.rs) uses at most one status/patch worker, polled by Shell while Commit is active and no shell dialog is open. [Native watching](../src/commit/refresh/watch.rs) uses [notify](https://docs.rs/notify/latest/notify/) to share platform-specific notification handling. Watching starts before the initial status read on the startup worker; dropping the panel releases its native handles. Canonical worktrees and resolved Git/common directories cover ordinary repositories, submodules, and linked worktrees without following directory symlinks. Events map to the deepest owning worktree or Git directory; shared refs invalidate their worktrees, and submodule HEAD changes also invalidate parent gitlinks. Read events and irrelevant object/log/lock writes are filtered. A bounded per-repository dirty map merges bursts for 300ms, with a two-second maximum debounce delay. Only dirty roots in the current scope are scanned; hidden-root events remain queued. Snapshots carry root IDs so unscanned files, branches, and errors remain intact. Events arriving during a scan are retained for another scan. Tab/focus entry drains queued changes without an unconditional scan; a 30-second consistency scan of Roots covers missed events and unavailable watchers. Status and Diff use `--no-optional-locks` to avoid refresh-generated index writes. Generation checks discard results superseded by manual refresh or ignored-file toggles. Apply preserves checkboxes, folds, cursor identity, the shared draft, caret, and unchanged preview selection/scroll. Menus and mouse gestures defer application. Failed roots retain their previous files and repeated errors are reported once. Commit execution, AI requests, rollback reviews, and Log history remain independent; F5 still refreshes all panels.

Message editing renders the existing buffer and text selection inside the composer's draft rectangle in [commit/view.rs](../src/commit/view.rs). It reports text-input mode but does not claim overlay mouse events. Clicking another control saves the shared draft before processing that click; Shell does the same before leaving Commit or moving focus with Tab / Shift-Tab / F6. `c` from Changes or Preview focuses Message and opens the editor. `Esc` saves the draft and leaves Message focused. Outside text entry, Message tracks one selected button for Left/Right navigation. Enter and mouse clicks use the same action handler. F1 help preserves the editor state. Persistence failures preserve the buffer and original draft, and stop navigation or submission. Caret and scroll state remain in memory; the shared draft uses `commit.json`. Each repository plan receives a copy of that message. Partial failures and hidden checked paths retain it; a completed batch clears it. On first load, a missing shared message is seeded from the first nonempty legacy draft in repository order; legacy entries remain available for recovery and do not replace an explicitly empty shared message. Changelist and AI editors retain overlay rendering.

The [Commit display menu](../src/commit/display.rs) stores session toggles for Directory, Repository, compatibility Module, ignored items, and XY status. The [tree model](../src/commit/tree.rs) treats Repository or Module as one shared repository level beneath each changelist group, including Unversioned Files and Ignored Files; enabling both does not duplicate rows. With both off, Directory uses workspace-relative roots. With repository grouping on, Directory nests repository-relative paths beneath the shared repository level. Display state is separate from native file identity, checks, folds, current file, Roots scope, and Git write plans, and is never persisted. When repository grouping is active, directory `yr` copy paths and rollback directory targets use repository-relative roots; otherwise they retain the displayed workspace-relative directory root. `git ls-files --others --ignored --exclude-standard --directory -z` runs only when ignored items are shown, without reading their contents; directories stay single items. These `!!` entries are unavailable, and commit/AI source validation rejects them again. Menu handling precedes changes-list events. Border blocking, outside clicks, and Shell-header dismissal follow Log conventions.

For each prepared repository, `backend::execute` exclusively creates the real `index.lock`, then rechecks branch, HEAD, index records, and the recomputed tree. Actual `git commit` uses a private index and native hooks. The new real index is built from the locked index copy, preserving unrelated staged content. Hook-rewritten commits are reported for review. If HEAD updates but index publication fails, the error preserves the recovery lock. Inspect HEAD, the index, and the error before acting; do not automatically remove the lock or commit again.

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

**AI commit messages: bounded background generation fills one shared draft.**
[commit/ai.rs](../src/commit/ai.rs) captures the effective configuration, checked
native paths, Roots identity, and original draft before dispatching a worker.
No Git reads run on the terminal thread for AI preparation.
[backend/ai.rs](../src/commit/backend/ai.rs) builds one private selected index and
immutable base/tree pair per repository, retaining staged-version priority,
exact-path scope checks, and source fingerprints. Per-file name-status/numstat
and patches come from those trees with explicit one-line context and zero
inter-hunk context. Binary content remains a Git marker. Prepared patch bytes
have a 32 MiB cap. Preparation fails if the complete selection exceeds it.

[ai/input.rs](../src/commit/ai/input.rs) measures the complete serialized request,
including model, prompt, metadata, JSON escaping, and summary output limits.
Small selections use one request. Larger inputs are partitioned by file, hunk,
line, and UTF-8 fragment as needed. Internal factual-summary requests cover every
fragment. Reductions combine oversized summaries before one final request with
the unchanged custom system prompt and complete file manifest. Requests run
sequentially and reserve space within `ai.max_requests` for the final one. Empty,
incomplete, oversized, or non-shrinking intermediate output fails instead of
becoming a commit message.

[ai/transport.rs](../src/commit/ai/transport.rs) sends compatible requests through
`curl`, enforces the byte budget again, and bounds response bytes. The credential
comes from `ai.api_key_env` (default `IGIT_LLM_API_KEY`). The request and
authorization header pass through anonymous stdin. Errors omit service bodies,
curl stderr, and credentials. URL globbing is disabled so one call cannot expand
into multiple patch uploads. Git/filter failures likewise omit raw diagnostics.
[commit/process.rs](../src/commit/process.rs) uses file-backed IO and bounded
reads to avoid pipe deadlocks. Git and curl share a cancellation flag and the
`ai.total_timeout_seconds` deadline. Curl also uses the per-request
`ai.timeout_seconds`. Cancellation signals the owned subprocess group,
including active Git filters, when the workspace changes or IGit quits or restarts.

The pending job receives phase reports while Shell redraws the active Commit
pane. Roots or checked-source identity changes cancel obsolete work. Completion
performs source, branch, and HEAD validation on a worker; unrelated index changes
and unstaged edits to a staged source do not invalidate it. Active input defers
insertion and schedules a fresh validation after editing ends. Validation reports
carry the input/refresh revision and completion time; intervening editor sessions,
refreshes, or delayed polling require another check. Newer drafts are preserved.
Successful output directly replaces and persists the shared draft;
persistence failure restores the original. Other failures produce an Error
notification with no partial adoption, preserving the draft and focus.
Generation never commits or pushes.
User `[ai]` settings override the legacy workspace endpoint/model/prompt on
startup; `S` edits those three session values and their workspace fallback.
Budget/count/time settings come from the user configuration or merged defaults.
Tests use disposable repositories and local HTTP mocks, never users' code or
real models.

Commit and AI use checked-file snapshots. Both import selected staged entries
and remove selected working deletions before adding working files. This gives
Git attributes the same private-index fallback when preparing AI evidence and
when preparing the commit. Index entries match native paths exactly. Exclusions for
Diff, temporary staging, and index publication match only selected-path
descendants, escaping wildcards while preserving the gitlink itself.
File/directory replacements validate scope against actual changed paths. If
adding a file requires deleting unchecked descendants, stop and ask for a
revised selection. The plan reads the live branch ref to determine the target.

**Push reads: responsive entry and current snapshots.** The panel queues outgoing
reads on entry and retains its previous rows while refreshing. One batch runs at
a time, with at most four repository readers; each reads effective branch/push
configuration once and computes ahead/behind against the selected target's
cached local ref, omitting history queries for zero outgoing commits. This
comparison does not resolve a differently configured upstream or perform a
fetch. Zero counts are hidden; an unavailable target comparison omits the
behind count. [push/view.rs](../src/push/view.rs) uses Branch's shared green
ahead/blue behind spans, reserving count cells before clipping names and
retaining full values in Details. Fetch refreshes available target refs. A new
request invalidates the old generation, stops it from taking more repositories,
and replaces queued work. Shell polls channels at up to 16ms intervals while
reads are pending. Workspace/Roots identity also invalidates results when scope
changes while Push is inactive. File lists and file comparisons use a separate single
reader over immutable OIDs, coalescing navigation and discarding obsolete results.
Repository file lists read the complete stored tracking-tip..HEAD range in one
NUL-delimited log command, using the same first-parent merge differences as
individual commit previews. Native paths are deduplicated, independently of the
60-row history limit. Cumulative statuses and rename sources come from the
merge-base-to-HEAD diff, restricted to those outgoing paths. The cached target
tip bounds previews; execution fetches a fresh tip from the actual push endpoint.
New targets and unrelated histories
use an empty cumulative base. Paths absent from the cumulative diff retain their
latest outgoing commit for a historical comparison, so reverted changes and
intermediate rename paths remain inspectable.
For the Before side, a cumulative repository file uses the merge base and a commit
file uses its first parent; rename/copy records supply the old path. The worker
prepares the shared two-pane revision viewer or a bounded Git patch with a reason.
Closing the viewer invalidates any pending file result. No Git commands run in Push's
tab-entry or read-completion path.

Checkboxes, target overrides, and grouping belong to the UI, not the returned
snapshot. The first current snapshot checks and expands every scoped repository
with outgoing commits; later refreshes retain manual checkbox and fold choices.
Commit and Push reseeds checks only from successful repositories, independently
of expanding all outgoing repositories. If `notification-history` is bound to a
Space suffix, Shell eagerly applies standalone Space in Commit's Changes tree
and Push's repository pane when configured as their toggle, records the
previous check state in the notification prefix, and marks Space dispatched. A
completed notification chord restores only those checks; unrelated keys cannot
replay the toggle, and canceling the prefix retains the immediate change.
Selection follows repository and commit OID; unchanged snapshots retain
the selected file, folds, Diff scroll, and text selection. Commit and Push applies
its reseed when the current batch arrives. Loading and reader failures disable
Push/Retry/Fetch until a current snapshot is available, while navigation, Cancel,
and quit remain available. This read state is separate from the remote-operation
busy guard. Refresh can retry a failed reader. Explicit target validation and
push-review job construction still run synchronously. Update and publication
run on the background worker with source/URL/remote/submodule validation.

**Push: one reviewed update and publication.** Each Job records the repository,
source branch/HEAD, target remote/branch, effective configuration, push URL,
and cached target tip. The review also fixes the Merge/Rebase strategy, hooks,
tag choice, and any requested upstream publication. Confirmation authorizes
Fetch of that actual push target, integration into the local branch only when
necessary, and immediate publication without a second confirmation. Only the
reviewed checked repositories in current Roots may execute.

[push/update.rs](../src/push/update.rs) revalidates source and configuration,
rejects an unfinished Git operation or unmerged index, then queries the target branch with
`ls-remote --refs` against the actual **push URL**. It fetches that exact OID
from the same endpoint with `--no-tags --no-prune --no-prune-tags
--no-recurse-submodules --no-write-fetch-head --refmap=`. A new target has no
incoming commit to integrate. Source/configuration checks run again after
Fetch. The conventional `refs/remotes/<remote>/<branch>` ref is updated only
when the remote's fetch and push URLs match, using its previous value as a
compare-and-swap guard. With different endpoints, execution uses the fetched OID
without replacing the fetch endpoint's tracking ref. It never integrates a
differently configured upstream.
Transport validation rejects a resolved URL that would be rewritten again when
used literally. Target advertisements must match the complete refname; a
suffix-pattern match cannot supply another branch's OID.

[core/update.rs](../src/core/update.rs) owns idle-operation/unmerged-index
checks, stored-history ancestry, and explicit Merge/Rebase integration. It skips
integration when the fetched target is already an ancestor of the reviewed
source. Only integration requires a clean tracked index/worktree; ahead-only
pushes preserve dirty staged and working content. Before integration, the shared
[core/paths.rs](../src/core/paths.rs) preflight checks paths touched in the
incoming/local history against ignored and untracked paths plus index entries
marked skip-worktree or assume-unchanged. Native byte paths and case-aware
ancestor/descendant checks detect file/directory collisions. An overlap blocks
integration without overwriting content or clearing index flags; Log operations
use this same preflight.

No automatic stash is created. Merge uses `--ff --no-edit
--no-autostash --no-overwrite-ignore`; Rebase uses
`--rebase-merges --no-autostash --no-update-refs --no-autosquash --no-fork-point`.
Both integrate the explicit fetched OID and disable submodule recursion and
directory rename inference (`merge.directoryRenames=false`). Ancestry checks,
protected-path history reads, Merge, and Rebase bypass local replacement refs
and legacy grafts. Successful
integration must leave the same local branch with no pending Git operation,
clean tracked files, and the fetched target in its ancestry.

Integration rechecks the reviewed branch and source OID, returns the validated
result OID, and rejects Merge results with unexpected parents or an unexpected
fast-forward tip. Rebase records its ref movement for that command and requires
both `ORIG_HEAD` and the branch's previous reflog entry to identify the reviewed
source, so a `post-rewrite` hook cannot add or amend commits into the result
that is published automatically. The worker rechecks source/configuration
before publication. A repeated query of the push endpoint must still match
the fetched tip; stored-history ancestry must prove a fast-forward. The
target-specific `--force-with-lease=<ref>:<expected-oid>` then publishes the
exact OID, rejecting later remote races without allowing history overwrite.
In outgoing history, any gitlink change or change to the corresponding path's
source still requires proof that the child commit is available from the source
declared in that commit. The same guards apply to the integrated result;
failure blocks the parent without implicitly pushing another repository.

The background worker reports `Started` before each executable repository, then
phase changes and the final result. [core/schedule.rs](../src/core/schedule.rs)
runs at most four repositories concurrently, while reviewed ancestors wait for
all selected descendants and worktrees sharing a Git common directory never run
together. Push tracks every active repository with its own start time and 100 ms
frame index; Shell advances the shared icon-mode spinner only when at least one
frame changes. Each active row replaces its result marker with its spinner until
that repository's `Row` arrives, while skipped dependents never become active.
Phase reports identify **Fetching**, **Merging** or **Rebasing**, and
**Pushing**. Results distinguish updated and published, up-to-date, update
conflict, and updated-but-push-failed. Publication failure retains the updated
local branch.
For a single failure, the notification summary prioritizes explicit server
errors over hook progress banners and retains the update/publication phase.
Captured Git error output remains in per-repository Details and is also included
in notification history for all failed repositories. Remote hooks can reject
commit messages after local integration succeeds; retries use the current
reviewed HEAD without repeating an already completed merge.
A conflict preserves the native merge/rebase state and ends automatic flow
for that repository. Push's `C` and clickable Conflicts control route through
Shell to the existing resolver. Apply file and Continue repository keep their
separate reviews; Continue never resumes an old Push job. After finishing,
the user must initiate another Push with a fresh review.

[The conflict list](../src/merge/list.rs) groups unresolved index entries separately from repositories ready for Continue. It caches operation and source labels on refresh, derives both status columns from stages, and keeps selection by repository and path. [merge/loading.rs](../src/merge/loading.rs) reads Roots on a generation-checked background worker after startup, retains cached rows for the same Roots, and clears them immediately on scope change. A failed repository keeps its previous conflict/ready rows and source labels, adds a local refresh error, and waits for an explicit retry. An open document and draft stay in memory; changed conflict stages or native operation mark the document stale, display a warning, and block Apply until the user reopens it. Runtime installation waits for editors, menus, and drags. Its Accept controls fill an in-memory document from one side; Prepare Simple Drafts runs the same non-conflicting block rules for the selected repository and stores session drafts. Both use the existing file eligibility checks and leave all writes to the reviews below.

**Merge: preview, staging, and Continue have separate boundaries.** Read index stages 1 (base), 2 (ours), and 3 (theirs). An ordinary text conflict can open when the working file matches Git's conflict output or exactly matches stage 1, the editor's starting result. A stage 1 match leaves the same decisions pending as a fresh conflict. Compute base-to-side line differences and assemble blocks in base coordinates. Blocks changed on only one side, or with identical results on both sides, can be accepted automatically. `Chunk::automatic_result` shares eligibility between the editor's Resolve Automatically button/count, its batch action, and prepared session drafts. It accepts pending non-conflicting blocks and combines disjoint single-line edits only when neither side has been decided; touching insertions remain manual. Left / All / Right stays limited to non-conflicting blocks. An automatic batch has one undo snapshot. Stages 2/3 are Git's sides, so rebase identifies the rebased HEAD and replayed commit. Each source's pending decision and the result bytes are held in the in-memory `Document`; undo and session drafts retain both.

The editor's row layout aligns blocks across all three panes, pads shorter sides, and folds shared context in a single display coordinate system. Each pane keeps its own line numbers and horizontal offset. Intraline colors, indentation guides, and folds affect display only. Inline edits replace a result block and can be undone. Shell displays the editor without the workspace chrome and still owns Help and confirmation dialogs.

`Document::open` admits standard `eol=lf` and `eol=crlf` attributes to its
per-stage `git cat-file --filters --path` comparison. All three checkout results
must match their raw blobs, so an ordinary LF rule can open while actual CRLF
conversion remains blocked. Custom merge drivers, filters, encodings, and ident
expansion remain unsupported. The attribute/config snapshot is still rechecked
before Apply. Stash apply/pop conflicts use the same index stages with no
`Operation`; after Apply, the notice points to Commit instead of Continue, and
the stash ref is never removed by the merge editor.

After Apply confirmation, revalidate operation, HEAD, conflict stages, original file content, permissions, and conversion configuration. Write a temporary file in the original directory, replace the original, then separately run `git add -- <path>`. If staging fails and the working file still matches the just-written result, attempt to restore original content. Do not directly overwrite conflicts outside ordinary three-stage text, binary content, files requiring checkout conversion, files beyond size/line limits, or externally edited files. **Continue is per repository:** separately review/validate the entire index tree and run the corresponding `git … --continue`. Other staged files in that repository are included in its scope.

Commit's [discard controller](../src/commit/discard.rs) uses the cursor scope for initial checks independently of Commit checkboxes. Changelist rows use exact native paths from their visible members; physical directories and repositories can span changelists. The [Rollback modal](../src/commit/rollback.rs) is owned by Shell and renders all visible Changes rows with the shared tree and row renderer. Its checks, folds, grouping, Diff, and added-copy option are local to the modal. Directory targets use the displayed grouping root, matching path copying: repository-relative with repository grouping on, workspace-relative with it off. Close drops the pending plans, while F1 retains the modal and its state under Help.

The [discard backend](../src/commit/backend/discard.rs) captures eligible visible paths at open, recording HEAD/branch, exact index records, and raw working-tree hashes/modes or symlink targets. Unavailable paths stay in the tree without a checkbox target. A file Diff is loaded on demand against the snapshot. Confirm narrows the retained plans to checked paths without recapturing Git state. Discard locks the real index, revalidates the selected snapshot, and prepares a private index with only the reviewed paths reset to HEAD (or the empty tree before the initial commit). `checkout-index` restores tracked leaves; `remove_file` removes untracked leaves and added local copies only when requested. Each leaf is checked again before writing. Unrelated staging is preserved, and partial failure publishes index changes only for completed leaves. Conflicts, active Git operations, submodule pointers, nested repositories, directory symlink traversal, and file/directory replacements remain read-only. Results refresh the workspace; no undo backup or cross-repository atomicity is provided.

Commit's [Add to Git controller](../src/commit/add.rs) captures only untracked
members of the highlighted row within Roots, independently of commit checkboxes.
The UI thread captures native path and repository identities without running Git,
then one worker prepares, stages, and reads statuses for those repositories.
Phase/completion events are polled by Shell even while Help or another tab is
active. No confirmation dialog opens, and duplicate jobs/conflicting writes
cannot start. Add completion applies affected statuses directly rather than
invoking Shell's synchronous full-workspace refresh. If an in-flight scan
covered repositories that Add did not refresh, their roots remain queued rather
than being consumed by the superseded generation. A generation change rejects
older Commit reads; checks, cursor identity, and message edits are retained.
The [addition backend](../src/commit/backend/add.rs) reuses snapshot validation
and [working-file fingerprints](../src/commit/backend/working.rs). Hash reads are
batched with bounded native argv sizes; private-index additions use one
NUL-delimited literal pathspec list per snapshot. Preparation builds a Git-filtered
tree in a private index; execution rechecks branch/HEAD, exact untracked status, raw
contents, and the rebuilt tree. It locks the real index, copies its current state,
and reuses `reset_index_paths` to publish only the captured leaves, rejecting
implicit removal of other staged paths. No working file or ref is written.
Plans stay on the worker, and repositories report separate results. A failed
status read keeps cached rows and reports F5 recovery; a disconnected worker
reports uncertain outcomes and schedules a read without repeating writes.

**Reviewed hunks.** [The controller](../src/commit/staging.rs) owns file-local
Staged/Unstaged inspection without changing commit checkboxes.
[The backend](../src/commit/backend/staging.rs) retains HEAD/branch, native path,
exact selected index entries, working fingerprints, attributes/conversions, and
the original patch. Plan creation applies one hunk through a private index and
derives the reviewed result patch. Execution locks the real index, revalidates,
starts from the current locked index, checks that only the selected path changes,
then atomically publishes. Reverse application unstages; no working file is
written. Unsupported modes, flags, content and required conversions are refused.

**First-push tracking.** [push/upstream.rs](../src/push/upstream.rs) snapshots
effective branch tracking values and their origins. An explicit target-editor
choice joins the reviewed Job. The immutable OID push and its remote lease run
first; only a successful push may update the local branch.remote/merge keys.
Both keys are prepared under a Git config lock and published together, retaining
unrelated values. Inherited tracking is not silently rewritten. A local config
failure returns a distinct successful-push warning, keeping it out of Retry.

**Conflict identity and recovery.** Unmerged paths retain native PathBuf identity;
display conversion never supplies a Git argument. Unsupported filenames stay
listed individually and cannot suppress ordinary conflicts. Separately reviewed
Abort/Skip live in [merge/recovery.rs](../src/merge/recovery.rs). The reviewed
repository, directory identity, operation metadata, HEAD/branch/refs, index,
working fingerprints, and configuration are revalidated before native Git runs.
Recovery disables submodule recursion and reports any surviving operation or
conflicts, including native partial failures. Apply and Continue keep their own
reviews. Native recovery may discard local state as described in its review.

## Batch Writes and Failures

| Operation | When a repository fails |
| --- | --- |
| Commit | Execute sequentially on a background worker, deeper child paths first. Retain successful SHAs. If a parent selected a failed child's gitlink, skip that dependent parent commit. If a successful child moves the parent pointer beyond the original preview, require review again. Independent repositories may continue. Stopping keeps the current command running and skips unstarted repositories; the UI blocks overlapping writes, Roots changes, workspace replacement, and quit until completion. |
| Push | One background task schedules up to four reviewed checked repositories in Roots concurrently. Selected descendants finish first, and a failed child blocks its dependent ancestors; worktrees sharing Git storage are serialized. Failure in one chain does not stop independent repositories. Report update and publication outcomes separately, retaining successful results and any updated local branch. Retry builds a fresh review of only visible failures using current source/target/configuration/strategy; completed pushes never run again and scope is never expanded implicitly. |
| Merge | Roots may gather conflicts from several repositories. Apply targets one file; Continue targets one repository. There is no cross-repository apply-all-and-continue transaction. |

Batch commit/push means selecting multiple repositories in one operation; it does not guarantee all-or-nothing results. Push, Pull, and Fetch use at most four concurrent repository workers, but dependency and shared-Git-storage constraints can serialize them. Confirmation dialogs and busy state coordinate only this application instance. They cannot lock external terminals, other IGit instances, or remotes. Git locks, snapshot rechecks, and remote leases address different race windows.

## Changes and Acceptance

| Change | Verification entry point |
| --- | --- |
| Settings, key parsing, or contextual remapping | `cargo test --locked config::` and `cargo test --locked configuration_`; use `--check-config` for user files. |
| Discovery, duplicate names, submodules, or workspace boundaries | [tests/discovery.rs](../tests/discovery.rs); run `cargo test --locked --test discovery`. |
| Graphs, filters, keyboard/mouse coordination | `cargo test --locked log::`; check focus changes, narrowing, and hit areas after scrolling in a temporary multi-repository workspace. |
| Commit selection or snapshots | `cargo test --locked commit::`; inspect the actual commit tree and compare unchecked staging/working-tree content, not just command success. |
| Push updates, targets, remote races, or submodules | Start with `python3 scripts/cargo_serial.py test --locked --lib push::update::`; use temporary local remotes for ahead-one/behind-two Merge/Rebase, native conflicts without publication, dirty preservation, custom targets/endpoints, races, partial failures, and scoped retries. See PRD A11a–A11d. |
| Shared update preference and selector | `python3 scripts/cargo_serial.py test --locked --lib core::strategy::`, then `ui::strategy::` and `shell::update::` through the same wrapper; verify atomic persistence, temporary Pull overrides, keyboard/mouse controls, and narrow Unicode/icon layouts. |
| Three-way diff, Apply, or Continue | `cargo test --locked merge::`; verify real conflict stages, written bytes, staging, and refusal to overwrite external edits. |

Run all checks with `cargo test --locked`. For interactive acceptance, follow [README](../README.md) with the [practice workspace script](../examples/demo-workspace.sh); do not test writes in everyday repositories. Before adding a button, identify its existing action and route keyboard/mouse through that same path. Before adding a Git write, define what review includes, what execution revalidates, and how partial success appears, then connect the panel. For a runtime read, use [core/loading.rs](../src/core/loading.rs) for generation/scope rejection and register the tab in [shell/refresh.rs](../src/shell/refresh.rs) so focus, F5, and writes share one deferred invalidation path. Keep writes blocked while a required list is pending or failed. Carry workspace/request identity with results so slower old requests cannot replace current selection. This is extension guidance, not a general asynchronous scheduler.
