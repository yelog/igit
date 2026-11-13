# IGit Product Requirements and Acceptance

Applies to version 0.1.0. This document defines the audience, scope, and completion criteria. See [README](../README.md) for installation and actual shortcuts, [interaction design](design.md) for workspace behavior, and [architecture](architecture.md) for Git commands, state, and module boundaries.

## Product Goal

IGit serves developers familiar with IntelliJ IDEA's Git tools who want to work in a terminal. Running `igit` or `ig` from the current directory opens one workspace for history, change review, commits, pushes, and existing conflict resolution, even if that directory is not itself a repository. Mouse users should not need to memorize commands first; Vim users should complete common tasks without reaching for a mouse.

A typical task spans a frontend, a backend, and a shared submodule. The developer finds relevant commits across all three histories, groups the files to commit from the two ordinary repositories, writes one shared message, and starts a batch commit. Next, they review two push targets. If one repository is rejected, the successful result remains and only the failure needs attention. When the submodule has conflicts, the interface makes the content sources and editable result clear.

Product-owned interface text, help, errors, examples, and documentation use English. Repository names, paths, user messages, and file content continue to support Unicode.

## Requirements and Delivery Status

**Remote management (R1/R7/R8, 2026-11-11).** The Remote tab (`6`,
`igit remote`) lists all configured remotes under every repository in Roots,
including repositories with no remotes. A Name / URL table and toolbar provide
keyboard/mouse Add, Edit, and Remove. The editor supports names and ordered
multiple fetch/push URLs; omitted push URLs inherit fetch URLs. Shared reviews
start on Cancel, identify one exact repository, and show before/after values.
Cancel retains drafts; validation errors do not discard input.

Writes are local Git operations, with no server requests. Renaming/removal
follows Git's tracking-ref and upstream behavior; removal leaves the remote
server repository intact. Confirmation rechecks Roots, repository identity,
configuration, and tracking refs. Custom refspecs/unrelated options are
preserved; externally owned/unsupported configuration requires external editing.
Credentials are redacted from lists, reviews, and diagnostics. Existing `1`–`5`
shortcuts remain unchanged; `6` opens Remote and `7` opens Conflicts when available.
All seven header tabs stay clickable at 44 columns.
Acceptance uses disposable ordinary/submodule repositories, duplicate names
across repositories, Unicode, multiple endpoints, stale configuration and
Roots changes, canceled reviews, narrow terminals, and byte checks for HEAD,
index, and working-tree preservation. Record actual verification separately.

**Commit/workspace comparison and single-group apply (R1/R4/R7/R8,
2026-11-12).** `D` in the ordinary Log commit list and **Compare with Working
Tree** in Commit Actions compare the highlighted immutable commit with that
repository's actual working tree. The cumulative file list includes subsequent
commits, staged/unstaged working contents, and non-ignored untracked files,
independently of cherry-pick checks or history filters. An independent
workspace follows Branch's comparison title/direction pattern, reusing Log's
file tree beside the shared Commit / Working tree Diff. Narrow panes retain a
switchable revision. `J/K` scroll the right preview independently of the
highlighted file. Generation-safe background reads clear stale lists/previews
after Roots changes. `F5` updates local contents while retaining the original
commit and available selected file. `Esc` clears search/selection first and
returns to the retained Log state.

For a supported preview, `L`, toolbar **Apply**, or the aligned change group's
`»` (`>>` in ASCII icon mode) control restores that complete group from the
captured commit to the current previewed working file, inserting or deleting
lines as needed. This workspace has no Apply All. Only that file and group
change; unrelated local hunks, exact real-index bytes including the previewed
file's staged content and unselected staged siblings, other files or
repositories, HEAD, refs, and reflogs remain unchanged. Writes recheck Roots
and repository identity, the captured commit and file identity, current bytes
and permissions, Git conversion attributes/config, and path safety without
symlinks or nested repositories. They support UTF-8 text with uniform LF or
CRLF and preserve the working file's line endings. Unsupported entries keep
explained read-only previews. After an apply, the preview rebuilds and the
changed-file list refreshes asynchronously; closing after applied hunks
refreshes the normal workspace. Browsing or canceling writes nothing. File
History's existing file-specific `D` and separate single-group plus reviewed
Apply All controls remain unchanged.

**Update before Push (R1/R3/R4/R7/R8, 2026-11-10).** Push shows Branch-style
green ahead / blue behind counts against its selected Push target, including
custom remote/branch targets that differ from the configured upstream. Zero
counts are omitted. Opening and browsing use cached local refs; Fetch refreshes
them. The outgoing toolbar shows **Update strategy: Merge/Rebase** at its right
edge. `s` or a click toggles and saves the application-wide default immediately,
initially Merge, atomically in the global state's `update-strategy.json`.
This is outside Git config and `settings.toml`. Pull inherits it, retains
temporary `m/r` overrides, and uses the same `s` selector to save a default.
Branch's `p` / `U` remains fast-forward-only.

**Push toolbar and shortcuts (R4/R7/R8, 2026-11-12).** The lowercase `fetch`
control sits at the left of the outgoing toolbar and uses `f`; the redundant
Target button is removed.
Per-row targets and `t` retain target editing. `F` toggles Follow tags, `H`
toggles Hooks, and `P` opens Push review. Their mnemonic letters, including the
strategy's `s`, use Commit's gold, bold, underlined style while available.
All panes retain complete focus-colored bottom borders above a compact footer.
Narrow layouts shorten the strategy label and wrap footer actions without
overlapping hit areas or dropping hidden-selection notices. Verify all three
icon modes, Details, maximization, configured bindings, busy guards, failed
strategy saves, and unchanged repository state during browsing and toggling.

A single Push review fixes the source, actual target, options, and strategy,
and authorizes explicit Fetch from the actual push endpoint, integration only
when needed, then immediate publication without another confirmation.
Integration requires a clean tracked index/worktree, with no autostash;
ahead-only pushes preserve dirty staged and working content. Rebase preserves
merge topology and disables autostash, update-refs, autosquash, and fork-point.
Shared path preflight preserves ignored/untracked collision content and
skip-worktree/assume-unchanged paths. Integration rejects active Git operations
and unmerged indexes, disables directory rename inference, and bypasses local
replacement refs/grafts for ancestry and Merge/Rebase.
Strategy changes invalidate review and are blocked during execution.
The fetched target OID is explicit; conventional tracking refs update only
when fetch/push URLs match. Existing source/configuration rechecks, exact-OID
publication, ancestry, exact-tip leases, and submodule availability guards remain.

Progress identifies Fetching, Merging/Rebasing, and Pushing. The active
repository shows the Commit Generate-message spinner after its name until its
result replaces it; only the repository currently being processed animates.
Results distinguish updated and published, up-to-date, update conflict, and
updated-but-push-failed.
Conflicts retain native Git state and stop automatic publication. `C` or
clickable Conflicts in Push opens the existing resolver; Apply and Continue
each require their own review. Finishing resolution requires another
user-initiated Push, with no delayed automatic publication. Only reviewed
checked repositories in Roots participate; independent repositories continue,
failed children skip dependent parents, and retries freshly review failures
without repeating successes. A11a–A11d specify acceptance; record actual test
runs separately from these requirements.

**Commit split file preview (R1/R4/R7/R8, 2026-11-09).** Highlighting a file
shows file history's read-only Before/After comparison in the existing preview
pane, with syntax, line numbers, aligned changes, folded context, synchronized
scrolling, source-text copying, and focused-revision search. Staged files compare
HEAD with Index; otherwise Index compares with Working tree. Empty sources
represent additions/deletions and unborn HEAD. Containers retain Overview.
Narrow panes show one revision with a side switch; unsupported content retains
an explained patch fallback. Refresh preserves unchanged comparison navigation.
Acceptance uses disposable mixed-staging, untracked, deleted, Unicode-path,
binary, and oversized fixtures and verifies unchanged HEAD, index bytes,
unchecked staged siblings, working bytes, checks, and shared drafts.

**Immediate quit (R7/R8, 2026-11-09).** Normal-mode `q`, `Ctrl-C` without a
text selection, and Actions → Quit close the workspace without confirmation.
Configured quit bindings and contextual help execute the same guarded exit.
Running Git writes and software installation still prevent quitting; retry
after completion. Text copying, editor input, and overlay/sequence cancellation
keep priority. Acceptance covers built-in and remapped keys, Actions, narrow
terminals, background reads, and unchanged HEAD, index, and working files.

**Pane search (R7/R8 extension).** `/` finds literal text in Commit Changes and
the file Diff preview, Log's Diff file tree, Push's outgoing and
changed file trees, Branch's repositories and branches, and Stash's list and
Diff file tree. All visible occurrences are highlighted. `n/N` wraps through
matches, including offscreen text and folded rows in the loaded scope. A jump
moves only the browsing cursor and reveals the result; Commit and Push checks
and Git state are unchanged. In Log's branch tree, `/` focuses the existing
Branch or tag input; `S` and clicking that input use the same live name filter.
Enter retains the query and returns to the tree; Esc cancels editing. Only
selecting a ref filters history. The branch tree has no separate footer
search or `n/N` match navigation. Log's commit list keeps its `/` text filter.

**Log pane focus (R7/R8, 2026-11-11).** Enter in the ordinary Log commit list
focuses the right-hand Diff file tree, revealing the inspector if hidden.
Opening a file comparison requires Enter on a file or the existing mouse
action. Escape in the branch tree, Diff file tree, and commit details retains
focus while preserving search cancellation and text-selection clearing.
Acceptance covers pending previews, hidden inspectors, contextual Help, and
120×36, 80×24, and 44×12 terminals without changing Git state.

**Workflow completeness (R1/R2/R3/R7/R8 extension, 2026-11-08).** Commit offers
reviewed Stage/Unstage for one tracked text hunk from its Staged/Unstaged view
(`Ctrl-G` / Hunks), preserving working bytes, unrelated staging, checkboxes,
and the shared draft. Confirmed commit batches execute on a background worker;
`Ctrl-X` stops remaining repositories after the current repository completes.
Log runtime refreshes, Git-filter changes, and tab reloads also run in the
background with generation/scope checks and cached browsing. Loaded-page search
and the 3,200-commit limit remain unchanged.

Log supports reviewed single-commit and same-repository batch Cherry-pick.
Conflicts offers separately reviewed native Abort (`B`) and supported Skip
(`N`) without changing Apply/Continue boundaries. Native conflict paths retain
their byte identity; unsupported names remain visible individually. Push target
editing offers an explicit Set upstream after push choice, retaining published
success if the subsequent local configuration update fails. Windows state
resolution no longer depends on HOME, and bounded untracked previews announce
truncation. Individual-line staging, advanced rebase editing, universal command
cancellation, persistent merge drafts, and a workspace dashboard remain follow-up
scope. Tests use disposable repositories and local remotes.

**Edit working files (R1/R7/R8 extension).** `e` / `E` in Commit Changes edits
the highlighted file independently of checked commit candidates. Standalone
execution uses Git's configured editor and restores the TUI after exit/errors.
The Neovim companion opens the file in its parent editor, retains existing
unsaved buffers, hides floating windows after success and preserves the running
Git session. Splits and tab sessions remain available. Deleted files and
containers require an existing file target; editing does not stage changes.
Acceptance uses temporary repositories with mixed staged/unstaged content,
unselected staged siblings, literal Unicode paths and a preserved index/HEAD.

**Neovim integration and file history (R1/R7/R8 extension).** Native commands
`igit commit/log/push/branch/stash/remote` select a startup panel. `igit log <file>`
adds a file-scoped History tab beside Log, with a commit graph/list on the left
and an automatically updated read-only Diff on the right. Ordinary commits use
Before/After; two-parent merges use Parent 1/Result/Parent 2. Tabs preserve their
own filters and selection, Roots still governs repository visibility, and
historical blobs support deleted paths and Git-detected renames. Preview reads
leave HEAD, index, and working files unchanged. Binary/oversized files use the
existing explained patch fallback. The composition follows the IDE reference;
terminal cells do not provide pixel-identical IDE typography or transparency.

**Show File at Revision.** `e` in File History opens the selected commit's full
file at its historical path, independently of the focused Diff side or pending
preview. The Neovim companion presents a read-only, syntax-highlighted buffer
named `filename (short hash)` and retains the terminal session. Standalone mode
uses a disposable read-only snapshot with Git's configured editor. The operation
does not change HEAD, index, working files, or existing editor buffer contents.
Empty files are supported; absent, binary, non-UTF-8, oversized (over 512 KiB),
and submodule entries report an explanation. The shortcut is remappable and
available through contextual Help.

**Compare with Local.** `D` in File History opens the selected revision and the
current working file in the shared aligned two-pane renderer, labeled
**Revision** and **Local**. The comparison follows the historical path across
renames and converts between uniform LF and CRLF to match the working file.
`n/p` visits changed blocks; `L`, the toolbar **Apply**, or the block's `»`
control writes the complete aligned change group to the working file. `A` or
**All** opens a review that applies every revision block, but is disabled when
the selected revision deleted the file. Writes are bounded to UTF-8 regular
files, recheck the repository scope, opened snapshot, permissions, ancestor
paths, nested repositories, and Git conversion attributes immediately before
replacement, and never stage content. Binary, non-UTF-8, symlink, oversized,
externally changed, custom-filter, and mixed-line-ending files fail without
modifying bytes. Closing the comparison refreshes the workspace. Acceptance
uses temporary repositories with multiple independent local edits, renames,
deletions, CRLF files, Unicode paths, nested-repository or symlink path
replacements, custom attributes, and stale working files, and verifies HEAD,
index bytes, file permissions, and unaffected working blocks.

**Current-file diff against HEAD (R1/R7/R8 extension, 2026-11-12).**
`igit diff [--line N] -- <file>` opens the shared aligned two-pane comparison
directly, labeled HEAD and Local, without loading a history list. The optional
one-based Local line is revealed through alignment and context folding and
centered after viewport sizing, clamped at file boundaries. `n/p` navigates
changed groups, whose complete current group is highlighted in both panes
with a visible position in the toolbar; `L`, Apply, and `»` restore one
complete group from HEAD. `A` / All retains a separate review, and `u` / `U`
undoes the latest restore without closing the viewer. F1 supplies contextual help.
Standalone restores use the existing guarded working-file replacement and
preserve HEAD, the real index, unrelated staged files, permissions, and
unaffected hunks. HEAD and conversion attributes are checked again before
each restore. An existing HEAD commit and supported UTF-8 text are required.

The companion's `:Igit diff`, `:IgitDiff`, and `file_diff()` open a floating
viewer for the source buffer, including unsaved content and its current cursor
line. Native writes target a private snapshot. Closing or hiding the viewer
imports restored text as an undoable source-buffer change without saving or
staging. Source-buffer identity and changedtick protect newer edits; a stale
import keeps the restored result available for recovery. Reopening captures a
fresh snapshot and cursor position. Acceptance covers mixed staged/unstaged
content with an unselected staged sibling, unsaved buffers and new files,
LF/CRLF, Unicode paths, stale HEAD/files/attributes/snapshots, folded unchanged
lines, in-viewer undo with the same stale-write guards, and 160×36, 80×24, and
44×12 views.

The separate `igit.nvim` checkout supplies `:Igit [panel]`, `:Igit log <file>`,
current-file `%`, and a lowercase interactive abbreviation. Configuration selects
float, left, right, bottom, or tab-page windows. Argument arrays preserve literal
paths; repeated requests reuse a session and different requests retain separate
buffers. Neovim 0.10+ and a native build with these CLI arguments are required.
Acceptance covers all layouts, literal/Unicode paths, tab lifecycle, normal and
merge comparisons, narrow terminals, and preservation of Git state.

**Responsive startup (R1/R7).** Paint the TUI before repository discovery and Git
reads. The first Commit frame uses the actual workspace header/footer and pane
geometry, with local tree/preview placeholders and disabled actions. A 120 ms
spinner continues even when the initialization stage does not change. Background
initialization reports its stage, discovered root count, and elapsed time after
five seconds, and permits resizing and quitting. It does not invent progress
percentages, file selections, or an empty-workspace result. Direct panel launches
retain the requested tab. Commit and the conflict indicator become available together, without
waiting for history from an unopened Log tab. The first Log load is asynchronous;
workspace navigation remains available and stale results are discarded after a
Roots/ref refresh. Keyboard enhancement setup must not wait for a terminal
capability reply. CLI listing and existing write reviews retain their behavior.

**Release automation.** The repository release skill and GitHub Actions workflows
prepare Beta/Stable metadata and test/package both `igit` and `ig` for macOS
Intel/Apple Silicon, Linux x86_64/ARM64, and Windows x86_64. Publication requires
consistent Cargo/Changelog metadata, an annotated tag at the tested commit,
passing checks on all five targets, and verified archive checksums. This records
configured automation, not a published release or completed interactive platform
acceptance. See [Releasing](releasing.md) for scope and recovery.

**Native installation and updates.** Installers target the same five platforms
at `igit.yelog.org`, with separate stable/beta manifests. They install both
commands, verify checksums and executable versions, and retain the previous
version if installation fails. `igit update` / `ig update` check or update an
owned native installation without starting the TUI; channel changes and
downgrades are explicit. Cargo/manual commands remain owned by their original
installation method. CI covers isolated archive installation and update checks,
plus Windows PowerShell 5.1/7 contracts. Public availability requires Pages/DNS
deployment and its online matrix. The published `v0.1.0-beta.1` predates the
updater and must be updated by rerunning its installer. See
[Installation](installation.md) for requirements and recovery.

**Software Update Center.** The bottom-left version/status label, `F9` in normal
mode, and Actions open one keyboard/mouse dialog. Startup checks begin after
1.5 seconds and cache successful results for 24 hours across workspaces.
Automatic checks notify when an update is available, but automatically quiet
no-update and failure results. Manual check results and installation completion
notify. Both settings are configurable. Native updates retain
their channel and install only the reviewed version after explicit activation;
Not now is focused initially when a write or restart is offered. The compact
dialog shows real download progress and the current installation stage. The
worker continues if the dialog closes, and completion offers a separate
restart. Cargo/manual copies can check releases and open guidance for their
original installation method. Software checks and installation do not change
repository scope, Git data, or workspace drafts.

**Pane maximize (R7/R8 extension).** `Ctrl-W f` maximizes/restores the focused
pane in Commit, Push, and Conflicts without changing saved split sizes. Tab
retains access to all enabled panes. Log keeps all
enabled panes tiled at every supported size.
Optional `smart-toggle-pane-maximized` synchronizes Kitty's owning tab with
stack/previous layout, using layout groups to avoid counting overlays as splits.
Single-group and non-Kitty sessions toggle locally where supported. Verified remote outcomes
set the local state explicitly; failures preserve it. Editors/reviews and held
key repeats cannot trigger synchronization. Configuration examples include an
`IS_IGIT`-scoped `Cmd+Ctrl+F` mapping.

The table records the 0.1.0 delivery baseline. Items marked partial or next steps are not release claims. See P0, A7, and A9 for the current Commit changes and observed checks, and A11 for Push comparison.

**Workspace header and footer (R1/R7/R8).** One header row combines Log's filter tabs and `+` (or Push's contextual title) on the left with workspace tabs on the right. The old workspace name and Console / Roots / Actions / Help toolbar are removed; those actions remain accessible through shortcuts and Actions. The footer starts with `IGit v<VERSION>` and update status, followed by contextual hints. Transient notices appear in the upper-right and the footer stays available for hints, search, and progress. At 120×36, 80×25, and 44×12, content starts immediately below the header, Log tab controls stay separate from workspace tab hit areas, and existing input/review guards still apply.

| ID | Original requirement | Delivered behavior and limits |
| --- | --- | --- |
| R1 | Reproduce IDEA Git layouts, buttons, and workflows where practical | **Partial.** Log follows IDEA screenshots for three-column proportions, colored rails, trailing refs, inline filters (including date ranges/presets), eye menu, file tree, and details; see A8. Merge commits have combined/per-parent file groups and a read-only parent/result/parent comparison from every file group; ordinary commits use Before/After panes. See A16. Commit defaults to a repository → directory tree with enabled file Diff/container summary preview; see A9. Messages edit inline in the bottom Commit pane; see A15. Push aligns repository, branch, target, count, and submodule columns, with a full-height colored tree and Log's Before/After Diff; see A11. Commit, Log, Push, Branch, and Stash use right-aligned tabs; Conflicts appears in amber when any repository in Roots has unmerged files or an operation awaiting Continue. See A14. Commit's changes tree and preview follow P0 below; see A7. |
| R2 | Resolve conflicts in three panes; apply non-conflicting changes in one action | **Supported for ordinary text.** The merge window has aligned read-only sources around an inline result editor, folds for unchanged lines, change and conflict colors, and accept/ignore controls for each side. Apply safe changes by source or all at once. Resolve Automatically shows the eligible count and applies pending safe changes plus disjoint single-line edits in one undoable batch. Apply Changes and Continue require separate reviews. External edits and unsupported conflict types prevent overwriting. |
| R3 | Discover multiple repositories, combine history, and selectively batch commits/pushes | **Supported.** Ordinary repositories, initialized submodules, and linked worktrees have independent identities. History, commit, and push panels share repository scope; graphs stay repository-specific, checked repositories use one shared commit message. Commit starts the local batch from the panel; push targets are reviewed separately. A batch is not an atomic transaction across repositories. |
| R4 | Prioritize design: improve fonts, colors, layout, and motion | **Partial.** All panels share icon semantics with Nerd Font, Unicode, and ASCII modes. Selection states, file types, and commit graphs remain recognizable without a dedicated font. Types include Java, Python, React JSX / TSX, ordinary JS / TS, common development languages, and build files; unknown types keep generic markers. Commit retains repository colors, gray parent paths, Git XY status, and row highlighting independent of checkboxes. Brief feedback animations remain a candidate enhancement. |
| R5 | Custom changelists, with the default group initially checked | **File-level groups supported.** Newly encountered ordinary committable items start checked in Changes. Scoped unmerged files appear under Changes → Merge Conflicts with red filenames and a Resolve link to Conflicts. They remain ineligible for commits, AI patches, and rollback. Untracked files remain separate. Groups can be created and files moved. Group membership and selection are distinct; regrouping does not run git add. Commit's display menu independently controls Directory, Repository, compatibility Module (one shared repository level), ignored items, and XY status. Ignored items default hidden and are read-only when shown. |
| R6 | Configure an LLM and prompt to generate commit messages | **Supported and optional.** Configure a chat-completions-compatible endpoint, model, prompt, credential environment variable, serialized input budget, request cap, and timeouts in `settings.toml` under `[ai]`. Generate message / `A` prepares exact checked snapshots in the background; small inputs use one request and large patches use complete chunk summaries plus bounded reductions before final generation. Progress animates and success fills the shared draft directly. Failures, changed selections, and changed source versions preserve the draft. Verification uses local mocks; external-model quality remains service-dependent. Git workflows work without a model service. |
| R7 | Vim shortcuts | **Core tasks supported in all panels.** Browse, select, edit, review, and resolve conflicts with the keyboard. Pending key-sequence prefixes show continuation candidates that can be typed or selected. `?` / `F1` help focuses Search, prioritizes the current area's actions, and supports filtering, up/down selection, and Enter execution. F1 works during input/review and restores state when closed. Notification History has a configurable global shortcut plus search, navigation, copy, and clear confirmation. Keys are context-sensitive; text input retains letters. The goal is task access, not a complete Vim editor. |
| R8 | Smooth mouse workflows | **Main operations supported.** Tabs, repository scope, rows/checkboxes, group dragging, merge gutters, and confirmation buttons are clickable. Log details/Diff, Commit message editor/preview, Push Details / Diff, and Notification History preview/detail support left-button drag selection, copy on release, and selection clearing, plus select-all and keyboard copying; see A13 and A19. Notification cards can be dismissed or opened into History, with hit areas that do not activate controls beneath them. New controls need a keyboard path and terminal-cell hit-area verification. |

Commit rollback: `d` in Changes opens the full tree for the current Roots and prechecks only the highlighted file, directory, repository, or changelist. Other eligible rows can be checked before confirming. Parent checks and footer counts reflect selection, while the tree lists all visible changes. With repository grouping active, directory targets are repository-relative. Rollback restores checked tracked paths to HEAD, deletes checked untracked paths, and unstages additions with optional deletion of their local copies. Rollback is focused initially so Enter directly confirms the checked files; Esc cancels and there is no undo. Hidden roots, read-only rows, and unchecked paths are preserved. See [interaction design](design.md) and `python3 scripts/cargo_serial.py test --locked --lib discard`.

Commit Add to Git: `a` or `Insert` in Changes immediately starts adding the highlighted untracked
file or the untracked files under its group, directory, or repository, including
folded children. The action uses only that row's scope within Roots and is
independent of commit checkboxes. No confirmation dialog opens. Feedback appears
before background Git work. Inline progress shows the file count, repository,
phase, and actual completed repository count without estimated percentages.
Browsing, Help, tab switching, and message editing remain usable; duplicate
additions and conflicting writes cannot overlap. The worker stages the captured
contents and reads affected repository statuses. Completion updates only those
Changes rows, retaining checkboxes and the shared message, and reports
added/failed counts. Other index entries, working files, and HEAD remain intact.
Changed content, staging, branch, or Git conversion stops the affected addition;
repositories report separate results. Amend uses `M`. Acceptance:
`cargo test --locked add_to_git` covers prompt feedback under a delayed filter,
scope, stale snapshots, partial failures and retry, mixed staging, literal paths,
shortcut remapping, and narrow layouts in all icon modes.

## Required Operation Semantics

**Log commit web pages (R3/R7, 2026-11-11).** Delivered: `O` in Log's commit
list opens the highlighted repository's full commit ID in the default browser,
including File History and branch comparisons. Help and configurable
`keybindings.log.open-in-browser` expose the same action. GitHub, GitLab,
Bitbucket Cloud/Data Center, Gitee, Gitea/Forgejo/Codeberg, and Azure DevOps
have built-in routes; repository settings choose a remote or override the
complete HTTP(S) URL template for custom hosting.
Discovery prefers locally known containing remotes, then the current
upstream, origin, or sole remote, and rejects ambiguity. It uses the fetch
URL and Git rewrites without fetching or pushing. Resolution and browser
startup run in the background; errors omit credentials. Input, menus, other
panes, key releases/repeats, empty lists, and stale or out-of-scope history
cannot launch a browser. Preview loading and cherry-pick checkboxes do not
change the target. Native launch success does not prove the commit is
published or the browser is authenticated. Acceptance: run
`python3 scripts/cargo_serial.py test --locked --lib core::hosting::` and
`python3 scripts/cargo_serial.py test --locked --lib browser` to exercise
provider URLs, remotes, custom templates, same-OID repository isolation,
remapping/help, launch failures, and unchanged HEAD/index/worktree.

**Log commit actions (R1/R3/R7/R8, updated 2026-11-11).** The Log list supports reviewed local branch
and lightweight tag creation, detached checkout, Undo of the checked-out tip,
Revert as a new inverse commit, Cherry-pick of one or more non-merge commits,
Drop of the checked-out unpublished tip, and editing the full message of any
unpublished commit on the current branch, including initial and merge commits.
`d/u/R/F2` open Drop/Undo/Revert/Edit review paths from the commit list.
Actions address only the highlighted commit's repository, independently of
cherry-pick checkboxes. Drop and Undo require the current tip and reject commits
contained by any local remote-tracking branch, as does message editing.
Publication checks use stored ancestry and repeat at execution without fetching.
Undo preserves the real index and working files, making the removed changes
staged relative to the parent. Drop requires clean tracked files and index.
Merge tip removal uses the first parent; initial tip removal returns the branch
to its unborn state. Message edits rebuild only affected commit objects and
parent IDs, preserving all file trees, merge topology, author/committer metadata,
index bytes, and working files. Other branches and tags remain unchanged;
invalidated signatures are removed and commit hooks do not run.
Pending operations block checkout and history changes. Stale reviews and local
file collisions prevent the affected write.
Git conflicts stay available in Conflicts. See A17.

Cherry-pick validates each selected immutable commit, current local branch/HEAD,
clean tracked index/worktree, pending operations, Roots, and untracked/ignored
collisions before execution. It leaves native conflict state for a separate
Continue/Abort/Skip review. A batch stays within one repository. Review shows
the destination branch and all selected commits in application order, including
selections hidden by filters.

**Stash browsing (R1/R3/R7/R8 extension).** The Stash tab (`5`) starts with
expanded repositories that have stashes, shows each newest-first entry on one
line, and groups repositories without stashes in a collapsed summary. A narrow
list preserves the message before optional date and selector columns. The
selected repository and full selector identify the Log-style Diff tree on the
right. Enter/double-click on a file opens the same read-only
Before/After comparison as an ordinary Log commit. Tracked changes compare
the saved base and worktree; saved untracked/ignored files from `-u`/`-a` are
included with an empty Before side. Binary and oversized content uses the
existing patch fallback. Repository errors are local, stale background
results are discarded, and refresh retains selection by immutable identity.
The tab supports mouse, Vim navigation, remapped keys, global Roots, help,
pane resizing/maximize, and refresh on entry/F5/terminal focus. The Stash tab
offers reviewed Apply/Pop/Drop on the selected entry; creation is available
from Commit as described below.

**Selected stash operations.** `a` applies, `P` pops, and `d` drops one selected
stash after confirmation, from either the repository list or file tree.
Repository headers and the read-only Diff cannot trigger these operations.
The reviewed identity includes the complete entry list so shifted selectors
and duplicate object IDs cannot silently select another entry. Confirmation
rechecks Roots, repository, list, and the apply destination/index. Apply keeps
the entry; Pop removes it only after successful application and a repeated
list check. Failed or conflicting application keeps the stash and refreshes
Commit/Conflicts. A successful apply followed by a failed drop reports the
partial result without replaying application. Drop affects no working files
or index entries and requires a destructive-action review. Tests cover
cancellation, stale reviews, local edits, conflicts, locks, linked worktrees,
saved paths crossing submodule/nested-repository/symlink boundaries, duplicate
objects, remapping, supported terminal sizes, and selection after
removal. Shelf management remains outside this delivery.

**Stash creation from Commit.** `s` opens one message editor and a separate
review for all staged, unstaged, and untracked changes in the current Roots.
This operation uses whole-repository scope; checkboxes do not filter it. Ignored files and repositories outside Roots are preserved.
Confirmation creates a native stash per changed repository with the same
message, preserves the commit draft, and reports independent saved/failed/
skipped results. Changed review inputs stop that repository. Successful saves
are not repeated on retry; saved objects survive cleanup failures and their
remaining working-tree changes must be resolved before another save.
Conflicts, active Git operations, unborn HEAD, changed submodule pointers,
and unsupported file layouts are reported before writing that repository.

Creation acceptance: `cargo test --locked stash` covers shared messages,
unchecked staged files plus later working edits, untracked/ignored content,
review/cancel, Roots changes, partial failures/retry, cleanup failures,
linked worktrees, hidden dirty submodules, remapping, and narrow dialogs.
All Git writes in regression tests use temporary repositories.

Stash acceptance: `cargo test --locked stash` covers multi-repository and
empty/unborn lists, the empty summary, saved timestamps, selection context at
120×36/80×25/44×12, keyboard/mouse
Diff entry, unchanged HEAD/index/worktree/reflog, mixed staged/unstaged files,
deletions, untracked Unicode and byte paths, binary/large fallback, selector
renumbering, stale workers, Roots, and custom bindings. Fixture repositories
are disposable; ordinary browsing never uses models or real user repositories
for regression tests.

**Branch management (R1/R4/R7/R8 extension).** The Branch tab (`4`) shows the
selected repository's Local Branches and Remote Branches groups. With one
repository in Roots, the tree fills the tab without a repository label. With
multiple repositories, the left list selects which tree appears on the right.
Each repository row shows its current branch and upstream divergence on the
right, followed by a gray chevron. The branch tree shows the local time of
each local or remote branch's tip commit in a Last Commit column when space
permits. Action shortcut hints appear only in the workspace footer; the tree
starts with column headings and the View Options eye, without an action toolbar.
Slash segments form nested folders by default. The eye menu switches between
prefix folders and full names, retaining the selected ref. The tab opens before
the ref scan finishes. View Options also offers Recently Used (default) and
Name (retaining default-branch priority) sorting. Both keep the current local
branch and its ancestors first. Recently Used reads this worktree's HEAD
reflog, uses event order for same-second ties, and puts unknown branches after
known ones in name order. Verified checkout immediately updates the target
repository's cached usage on the existing background path, so switching back
puts the previous branch next without a full Branch reload. Explicit Name
sorting remains active across checkout. Folders use their latest-used descendant; remote
ordering is unchanged. Sorting, grouping, folds, and selection survive refresh;
options are session-only and create no persistent state. Missing or unreadable
reflogs do not prevent branch browsing or operations.
During a refresh, cached rows for the same Roots stay browsable and branch
actions wait for current results. Superseded scans cannot publish old Roots.
Current local branches are marked and sort first,
including their ancestor folders. Local branches display the configured
upstream and missing-ref status. Both branch sections and all nested folders
start expanded. User fold choices survive refresh and repository switches.
Repository icons use Push's colors and submodule glyph; branches
use Log's green and purple. Folder glyphs, indent guides, counts, and aligned
upstreams make groups easier to scan. Local branch names append green `↗N`
and blue `↙N` for commits ahead of and behind their configured upstream;
diverged branches show both, and zero/unavailable counts stay hidden. Counts
use local upstream refs, load in the background, survive name clipping, and
repeat in selected-row details. ASCII uses `^N` / `vN`. Click a repository or
use the keyboard in the left pane to switch; Tab moves between panes.
`gg/G`, `H/M/L`, and full/half-page Ctrl-F/B/D/U motions use the focused list's
viewport. Remote symbolic HEAD aliases are omitted.
Focused-row actions create (`a`), safely delete (`d`), switch (`Enter`), merge
into the current local branch (`m`), and rename (`r`). Switching starts a
single-repository background job and immediately reports `Switching branch…`.
Navigation, Help, and tab changes remain responsive. Roots and overlapping writes
are blocked while Git runs. Success is reported only after Git exits and the final
HEAD is verified; IGit then merges the result into that repository's cached
snapshot without rescanning the Branch list. Selection, folds, grouping, and sort
remain stable, and other repositories are untouched. Uncertain or failed states
fall back to a safe refresh or checkout review. A refused overwrite opens **Git Checkout
Problem** for that repository: **Smart Checkout** saves all reviewed staged,
unstaged, and untracked changes to a named stash, switches, and restores changes
and staging; **Don't Checkout** cancels; **Force Checkout** requires a separate
Cancel-first destructive confirmation. Smart Checkout starts focused. Both
local and new/existing remote-tracking branches use this flow. Restoration
conflicts open Conflicts and keep the backup; incomplete untracked restoration
keeps both the destination file and saved local version. Complete restoration
drops only an unchanged exact stash entry. Changed files/index/refs/HEAD/Roots
invalidate review. Protected ignored paths, nested repositories, submodules,
and special index flags block unsafe saves or discards.
Create, delete, merge, and rename open a single-repository review. Creation does not switch. Remote checkout creates a named local tracking
branch or switches to the same-name branch already tracking that remote ref.
Delete/rename are local-only. Roots is available through Actions while `r` is
used for rename. `p` and `P` review Pull and Push for one focused local branch
with a configured remote upstream. Both require fast-forward updates. The shared
Merge/Rebase preference does not change Branch Pull; `U` remains its compatibility alias. Pull
leaves the active worktree intact when the target branch is not checked out,
and refuses a branch checked out in another worktree. Failed merges remain
available in Conflicts.

**Focused branch Pull (R1/R4/R7/R8, 2026-11-12).** In the Branch browser,
lowercase `p` opens a single-branch review using the same frame and controls
as `P`. Purple From identifies the configured upstream; green To identifies
the focused local branch. Fast-forward only is emphasized, with repository,
path, short commit ID, and any retained checkout shown as secondary context.
Confirm starts focused. Enter, y/n, mouse controls, scrolling, and cancellation
match Branch Push. Reviewing and canceling do not fetch or write Git state.
Confirmation updates only the reviewed branch in the selected repository.
Repository, group, and remote rows never fall back to a workspace update.
Actions → Pull repositories retains the global review, and `p` retains its
workspace meaning in other panels.

**Branch comparison (R1/R4/R7/R8, 2026-11-11).** A non-current local or remote
branch offers `Compare with <current branch>` by right-click and `c`.
The Branch workspace shows two stacked Log views: selected-only commits above,
current-only commits below. Both use the same immutable tip pair and the
selected repository only. Independent filters, selection, graph columns,
changed-file trees, details, and ordinary/merge Diff reuse Log. The aligned
columns, two row heights, and separate inspector splits are resizable through
the standard keyboard/mouse routes. An empty side explains containment;
filtered-empty, loading, missing-ref, and hidden-Roots states are distinct.
`+` loads additional pages, and refresh updates both original named endpoints
while retaining applied filters and available selected commits. No fetch,
checkout, index change, or commit action occurs. Esc returns to the saved
Branch selection after closing any active editor, selection, or Diff.

**User configuration (R7/R8 extension).** Optional TOML settings support partial
overrides of embedded defaults, global/pane/panel normal-mode shortcuts, aliases,
multi-key sequences, unbinding, icons, mouse capture, and sequence hint delay.
Invalid settings and overlapping binding/prefix conflicts fail before terminal
startup. CLI presentation options take precedence. Context help and sequence
hints follow effective keys; editors and confirmation controls retain their
existing input rules. No repository-local configuration or live reload is
included. See the [configuration reference](configuration.md).

**Kitty smart focus (R7/R8 extension).** Four optional modified-chord commands
match LazyDB's `smart-focus-pane-*` names. Navigate actual visible pane geometry
first; at an edge, request the adjacent Kitty window using the source window's
ID. Default bindings remain unchanged. Editors and reviews retain focus and
input; navigation never changes Git state. Kitty errors are nonfatal and
bounded by a timeout. Neovim continues to use its existing smart-splits mappings.

**Kitty smart resize (R7/R8 extension).** Optional `smart-resize-pane-*` commands
move visible dividers by three cells, accepting key repeats. Directional
movement is consistent from both sides of a divider. A missing or clamped
divider hands off to Kitty through IGit's installed resize helper; target
window identity and focus are preserved. Editors/reviews block resizing, and
only panes within the active tab can be resized. Defaults remain unbound.

- **Scope, preview, selection, and execution are separate.** Roots changes only visible/executable scope; filtered-out repositories cannot join the current commit/push. The row cursor identifies what is being viewed; checkboxes identify candidates. The Commit panel shows the local selection and message before submission. Push review shows the actual repositories and targets. Hiding and restoring a selected repository preserves its session selection.
- **The committed version must be explainable.** A selected file with staged changes commits its index version; only files without a staged version use working-tree changes. A stale branch, HEAD, or selected file snapshot stops the affected commit before writing; refresh before retrying. Unchecked staged items remain untouched. Partial success must not be reported as total failure or automatically rolled back. Real hooks may rewrite content; report actual results.
- **Workspace Pull and Fetch review all repositories in global Roots.** Outside the Branch browser, `p` opens workspace Pull; Actions → Pull repositories opens it from any panel. Branch's `p` reviews only the focused local branch. Workspace Pull inherits the saved Merge/Rebase default; `m/r` overrides only that review and `s` persists the shared Pull/Push default. `f` opens Fetch, which refreshes remote refs without integrating changes. Both workspace reviews use Push-style title bars, colored repository icons, aligned target/status columns, and a separated action footer. Current branches show Branch's ahead/behind counters against their configured upstream, using local refs without fetching on review. Counts survive branch-name clipping, omit zero/unavailable values, and refresh after execution. Details retains the complete comparison. Highlighting a row only selects its wrapped Details; it never changes the batch. Log tab filters and Push checkboxes do not affect the batch. Repositories with unavailable targets are shown as skipped. Pull / Fetch starts focused when available, with Cancel focused otherwise; a blue background marks only the focused button. Tab/Shift-Tab switch between Cancel and Pull / Fetch, and Enter activates only the focused button. Action labels omit repository counts. Cancel performs no Git operation; confirmation runs up to four eligible repositories concurrently and reports each result in the same list. Parent/child workspaces and worktrees sharing Git storage are serialized. Rows change from Pending to Running and then to their own result as workers complete. Pull conflicts appear in the highlighted Conflicts tab.
- **Push authorizes update and checks availability.** Repositories are execution units; previewing a commit does not allow skipping its ancestors. Review covers explicit Fetch of the actual push target, any required Merge/Rebase, and immediate exact-OID publication with the fixed strategy. A remote target changed after that Fetch stops publication through rechecks and its exact-tip lease; no history-overwriting force push is offered. External source/configuration changes invalidate execution. Added/changed gitlinks, or changes only to their declared sources, require verification that each child commit is available from the source in that version, including after integration. If verification fails, identify the missing repository/commit without silently expanding selection. Single-failure notifications prioritize explicit server errors over hook progress banners; Push Details and notification history retain captured Git error output per failed repository.
- **Merge does not guess content.** Side labels identify the actual versions for the Git operation, especially during rebase, where fixed local/remote labels are misleading. Automatically apply only definite non-conflicting changes. The center supports undo. A working file that exactly matches the common ancestor opens with decisions still pending; other external edits are preserved. Supported input is ordinary UTF-8 text up to 2 MiB per file, including standard EOL attributes when checkout preserves all three source versions. Stash apply/pop conflicts follow the same reviewed Apply flow, direct users to Commit without Continue, and retain the stash. Binary files, delete/modify conflicts, special modes, actual checkout conversions, and similar cases require external tools.
 - **AI generates an editable message.** Generate message / `A` starts background preparation of checked files in the current Roots, without hidden roots or unselected files. Budget the serialized input; send small selections directly and summarize every large-patch fragment before final generation, reducing summaries when needed. Preserve a complete file manifest and enforce request count, timeout, and resource limits without silent truncation. Animate progress and fill the shared message directly on success. Changes to selection/Roots cancel the obsolete job, and final source checks reject stale versions. Errors appear as Error notifications and preserve the existing draft and focus; edits made during generation are kept. Settings are opened explicitly with `S`. Keys come from environment variables and must not appear in documentation examples, project configuration, or logs. Model responses must not trigger commit/push.

## Current Priorities

**P0: Make Commit readable at a glance and correct the preview target.** In the user's 0.1.0 baseline screenshot, `Unversioned Files` was highlighted while the right side showed one repository's `notes.txt`. Counts, status, and branch indicators were ambiguous, as were partial selection and cursor state. The [current implementation](../src/commit.rs) previews container scope summaries and file patches. Recheck the following conditions.

| Scenario | Acceptance criteria |
| --- | --- |
| Select a file, group, repository, or directory | With preview open, a file row shows that exact path and Diff. Other nodes show their own scope/count summary rather than treating the first descendant as selected. Collapse, repository changes, and refresh must not leave stale previews. |
| Scan a multi-repository tree | Distinguish group, repository/branch, directory, file, count, and Git status on one screen. Repository colors have names; status has symbols/text; focus and checkbox state are independent. Read-only conflicts do not affect parent committable-selection states. |
| Click and operate precisely | Row names browse, arrows expand/collapse, and boxes check. Select one file, read Diff, filter repositories, and write a message. Repeat with the mouse and keyboard in separate practice workspaces; the same files must be committed. |
| Long paths and different sizes | Repeat at recommended 120×36 and narrower 80×24. Preserve filenames, selection states, and reachable commit actions first. Truncated paths retain a way to view the full path. Matching icons alone must not require a dedicated font. |

**P1 candidates:** After core information is readable, assess brief highlighting after accepting a diff block, contrast on low-color terminals, and optional motion. Splitting one file across groups, selecting unstaged lines, Shelf management and further branch operations are outside this delivery and require separate use cases/acceptance. IGit does not manage hosting services, automatically clone child repositories, provide atomic multi-repository commits, or let models resolve code conflicts automatically.

## Reproducible Acceptance

Use a **practice workspace**. Run these commands from the source root; each script invocation creates two ordinary repositories, one submodule, and local bare remotes. For write checks, run the script again to get fresh directories instead of resetting a developer's repositories.

```sh
cargo build --locked
ws="$(sh examples/demo-workspace.sh)"
./target/debug/ig --list-roots "$ws"
./target/debug/igit "$ws"
```

| Case / requirement | Procedure | Pass criteria |
| --- | --- | --- |
| Branch focused Pull (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib branch_pull`. In disposable repositories, press `p` on current and non-current branches with differently named upstreams. Change the selected repository, cancel and confirm, remap/disable Pull, and inspect all icon modes at 44×12, 80×24, 120×36, and 184×48. | Pull uses the same compact review and controls as Branch Push, with the remote upstream as From and the focused local branch as To. Confirm starts focused. Only the reviewed branch advances; another checkout, mixed staged/unstaged changes, and other repositories are preserved. Reviews/cancellation do not fetch. Group, remote, and repository rows never open global Pull. `U` remains an alias; Actions retains workspace Pull. Long Unicode content scrolls while controls remain visible. |
| Branch Smart Checkout (R1, R3, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib smart_checkout`. In disposable repositories, exercise mixed staged/unstaged changes, an unrelated staged sibling, untracked collisions, existing stashes, remote tracking, stale reviews, and hook failures. Inspect 44×12, 80×25, and 120×36 in all icon modes. | Compatible changes switch directly. Blocked checkout shows all three actions and a scrollable local preview. Smart restores changes and staging; conflicts open Conflicts and retain the original stash. Incomplete restore preserves both versions. Cancel preserves exact index bytes; Force needs a second Cancel-first confirmation. Hidden or different repositories never join the write. Protected paths refuse automation; changed reflogs never cause another stash to be dropped. |
| Commit/workspace comparison and apply (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib workspace_compare`. Compare an older commit with later committed, staged, unstaged, deleted/recreated, and untracked files; apply an insertion and a deletion through the gutter, `L`, and toolbar **Apply**. Include a previewed file with both staged and unstaged content plus an unselected staged sibling and another repository. Repeat with the same commit ID in two repositories, hidden Roots, stale bytes/permissions/attributes/config/scope, binary/large/literal paths, symlink/nested-repository replacements, remapped entry, and short/narrow terminals. Scroll the right preview with `J/K` from both panes. | The cumulative list and Commit / Working tree sources belong only to the highlighted repository. Current working bytes supply the right revision, independently of staging and cherry-pick checks. An apply changes only the current previewed file's complete aligned group, including inserted/deleted lines, and preserves unrelated hunks, exact real-index bytes including previewed-file staged content and unselected staged siblings, other files/repositories, HEAD, refs, and reflogs. `A` does not enable apply-all in this workspace. Stale bytes/permissions, attributes/config, identity, or scope guards refuse without writes; unsupported previews remain read-only with an explanation. The preview rebuilds and the changed-file list refreshes asynchronously; closing after an apply refreshes the normal workspace. Browsing/cancel leaves Git state unchanged. Loading/error/empty states, search, mouse, panes, resize, help, `J/K`, F5, and return work without moving the file selection. File History's separate `D`, one-group apply, and reviewed Apply All behavior remain unchanged. |
| Pane search (R7, R8) | Run `cargo test --locked --lib pane_search`. In a practice workspace, find a duplicate filename under folded directories and a repeated term far to the right in a Commit patch. Repeat at 44×12 and try searching from another pane. | Enter reveals a match; `n/N` visits all occurrences and wraps; Escape cancels or clears only the focused search. Long Unicode matches remain readable where space permits. Git, checkboxes, and unrelated folds stay unchanged. |
| Branch repository layout (R1, R7, R8) | Open Branch in a disposable workspace with two repositories, each with a distinct local branch. Switch repositories with mouse and keyboard at 120×36, 80×25, and 44×12. Apply Roots with one repository, then restore both. Review an operation in the second repository. | Both panes stay visible in the tiled layout; the right pane shows only the selected repository's branches. With one repository in Roots, the branch list uses the full width and hides the repository name. Branch selection and folds return when switching back; the review names only the selected repository. |
| Branch merge review (R1, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib branch_merge`. Inspect 44×12, 80×24, 120×36, and 184×48 in all icon modes. Review long Unicode names, switch buttons, press y/n, cancel, and confirm a merge in a disposable repository. | From/Into branches lead the hierarchy; commit IDs and repository path are secondary. Content wraps and scrolls with fixed footer buttons. Confirm starts focused; only the focused button has a blue background. Enter activates the focused button, and y/n do nothing. Cancel/Esc/close preserve HEAD, index, and working tree. A confirmed conflicting merge exposes Conflicts. |
| Branch push review (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib branch_push_review`. Inspect 44×12, 80×24, 120×36, and 184×48 in all icon modes. Review a non-current branch with a differently named upstream, mixed staged/unstaged files, and long Unicode names. Switch buttons, cancel through keyboard/mouse/close, and confirm through Enter, y, and mouse against a local bare remote. | Green From / purple To names and Fast-forward only lead the hierarchy; repository/path, short commit IDs, and retained current checkout are secondary. Content fits, wraps, and scrolls with fixed footer buttons. Confirm starts focused; Tab/Shift-Tab/arrows move focus. Existing y/n shortcuts remain active. Cancellation preserves HEAD, exact index bytes, local content, and remote refs. Confirmation pushes only the reviewed branch and preserves the current checkout, staging, working changes, and unrelated remote branches. |
| Branch tree and operations (R1, R4, R7, R8) | Open Branch with `4` in a temporary multi-repository workspace. Add nested local/remote names, a recorded remote HEAD, and an upstream whose ref is missing. Browse/fold and toggle the eye menu at 120×36, 80×25, and 44×12. Exercise direct checkout and reviewed create, rename, safe deletion and merge. Run `cargo test --locked branch` and `cargo test --locked branch_checkout_tests`. | The current branch is the first local leaf. Both branch sections and all nested folders start expanded; explicit folds are retained. The eye switches between folders and full names without changing refs or the selected branch. Verify folder icons, indentation, keyboard motions, and aligned upstreams. Remote HEAD aliases are omitted; local upstreams and `[gone]` labels reflect Git. Creation uses the focused commit without switching; remote checkout establishes tracking. A gated checkout leaves Enter responsive, marks success only after Git exits, updates only the selected repository cache, and keeps selection/folds without a Branch rescan. Cancellation, stale reviews, other-worktree branches, and refused overwrites preserve refs/index/content. Renaming preserves upstream and mixed staging. A failing merge exposes Conflicts. Branch owns `r`; Actions retains Roots, and remapped global Roots labels do not replace Rename. |
| Branch loading (R1, R3, R7, R8) | Run `cargo test --locked shell::branch_tests`. Hold a Branch read open, switch tabs, return, then change Roots before an older read finishes. | The Branch header and loading state paint before the scan completes. Help and tab switching remain responsive. Same-scope refresh keeps browsing and the latest selection/folds; new branch actions wait for the current list. Results from prior Roots never appear. |
| Branch comparison (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib compare`. Compare diverged, ancestor, identical, merged, remote and Unicode branches; apply filters, load more, open Diff, resize, switch focus, refresh, remove a ref, and change Roots during a read. Inspect 44×12, 80×25, 120×36 and 184×48 in all icon modes. | Upper/lower commit sets match `git rev-list current..target` / `target..current`. Both sides retain independent state and never include another repository. Current/detached/unborn/loading entries cannot start comparisons. Stale reads cannot repopulate hidden Roots. Menu, remapped keys, help, mouse, panes, and Esc work without loading the ordinary Log workspace. HEAD, refs, index bytes, worktree content, and reflogs remain unchanged. |
| Branch upstream divergence (R1, R4) | Run `cargo test --locked --lib branch::tests::tracking`. Use disposable repositories with ahead-only, behind-only, diverged, synchronized, untracked, and missing-upstream branches, including differently named upstreams. Inspect all icon modes at 120×36, 80×25, and 44×12. | Arrows show the correct counts and colors; both directions appear for diverged branches. Zero and unavailable counts stay hidden. Long Unicode names and flat/grouped layouts retain the counters. Folding does not move the upstream column. Refresh replaces old counts, repository switching does not mix counts, clicks retain branch identity, and browsing preserves HEAD, refs, the real index, and staged/unstaged content. |
| Branch repository status and commit times (R1, R4) | Use two repositories with different current branches and a local branch with a different upstream tip. Compare the Branch rows to Git before and after changing a tip and pressing F5. Resize to 80×25 and 44×12. | Each repository row right-aligns its own branch and counters. The Last Commit column aligns local and remote tip times, updates on refresh, and leaves folders and unborn branches blank. Narrow panes give priority to branch names and preserve row selection. |
| Branch recent usage sorting (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib branch::tests::sorting`. Switch branches at the same commit and within the same second, rename one, and use a linked worktree. Exercise Name/Recently Used with grouped and flat trees, including while a refresh is pending. | Recently Used starts active. The current branch stays first; switching `main → develop → main` puts `develop` next immediately without a full Branch reload. New tracking branches receive usage metadata; explicit Name sorting survives checkout. Local recency follows this worktree's switch events rather than commit times; unknown branches follow by name. Folders inherit descendant recency, remote order stays unchanged, and selection/folds survive refresh. Missing history is harmless. Mouse and keyboard options work in narrow terminals without Git writes or persistent settings. |
| A1 Startup and history (R1, R3, R7) | Enter Log through either command. Hide sdk-core in Roots, browse/open a commit Diff, then restore scope. | Three working trees appear; bare remotes do not become a fourth. Filtered repositories disappear from history. Details follow the current commit, with no parent/child edges across repositories. |
| A2 Groups and batch commits (R3, R5, R7, R8) | Create Review in Commit. Move only one ordinary repository's `src/config.txt`; write one shared message for both. Hide the other ordinary repository in Roots and click Commit. Restore it and choose Commit and Push. | Untracked `notes.txt` starts unchecked; regrouping preserves its prior selection state. Only the visible repository commits first. The restored repository retains its checks and the shared message, then commits once when Commit and Push is chosen. The app stops at Push preview. Verify with commands such as `git -C "$ws/atlas-web" show --name-only -1`. |
| A3 Push results and rejection (R3) | Continue A2: check each target and confirm, then compare local HEAD with each bare remote's main. Run `cargo test --locked two_remotes_partial_failure_and_retry_scope` for an invalidated plan. | Successful repositories match their remote branches. In the failure case, success remains recorded and retry includes only failed repositories in visible scope. |
| Workspace updates (R3, R7, R8) | Run `cargo test --locked shell::update` and `cargo test --locked log_space_filters`. Inspect 44×12, 80×24, 120×36, and 180×36 in all icon modes, including long names/errors, skipped repositories, and empty scope. Compare ahead-only, behind-only, diverged, synchronized, missing, and differently named upstreams. Open Pull and Fetch in a multi-repository workspace and observe the first frame before repository checks finish. Browse rows and Details, select Merge/Rebase, switch to Cancel with Tab, then reopen and confirm against local bare remotes with Enter. Press `Space b/u/d/p` and `/` from Log. | Columns and controls do not overlap; clipped targets/errors remain readable in Details. The review paints immediately with Checking rows, cancel remains responsive, Enter cannot queue work, and Pull / Fetch becomes available only after the background snapshot arrives. Branch counters retain their colors and survive clipped names; review does not fetch and results refresh the counts. Browsing does not change scope, Pull / Fetch starts focused when available, Tab/Shift-Tab move the blue background between buttons without underlines, and Enter activates the focused button. Pull / Fetch labels have no count; empty/busy actions cannot run. Cancel leaves refs, the real index, and working trees unchanged. Fetch updates refs only in Roots; Pull uses the selected strategy. Changed targets fail safely, conflicts remain available in Merge, and other repositories still complete. `Space p` opens Paths; standalone `p` opens Pull. |
| A4 Submodules and index isolation (R3, R5) | Run `cargo test --locked mixed_and_unrelated_index_survive` and `cargo test --locked unpublished_intermediate_child_and_absent_remote_stop_parent`. | A selected file with staged/unstaged changes commits only its staged version; unselected staged entries remain. An unavailable child commit in history prevents the parent push from publishing that pointer. |
| A5 Merge, undo, and Continue (R2, R7, R8) | Open sdk-core's `transport.conf`. Apply non-conflicting changes, accept one source in the gutter, and ignore the other. Undo, check the restored result, and resolve transport. Save and reopen the draft. Use Apply Changes, then review Continue separately. | The path and retries merge automatically; transport requires a decision. Drafting preserves the working file and index. After confirmed Apply Changes, `git -C "$ws/atlas-web/deps/sdk-core" ls-files -u` is empty, but MERGE_HEAD remains. Only Continue creates the merge commit. |
| A6 AI boundaries (R6) | Configure `[ai]` with a custom endpoint, model and prompt. Select files in two repositories and put a recognizable unselected marker in another. Keep a handwritten draft. Run `cargo test --locked commit::ai::tests` against local mock endpoints. Generate once, click again while pending, simulate success and failure, and edit the message while waiting. | One action starts one background job with animated preparation/summary/generation/checking progress; duplicate clicks do not start another job. Small selections use one request with the model, custom prompt, and both exact selected patches, excluding the unselected marker and hidden roots. Oversized selections summarize all UTF-8-safe fragments and, if necessary, reduce summaries before the final custom-prompt request. Every serialized request fits `input_budget_bytes`; final metadata covers all selected files. Request-count, total-time, and preparation caps fail clearly without partial adoption. Delayed filters do not block panel input. Roots/checkbox changes cancel old work; changed selected source versions stop insertion. Success fills and persists the shared message directly without opening settings or a candidate dialog. Missing configuration, invalid/empty/truncated replies, HTTP failures, and save failures produce Error notifications and preserve the existing draft and focus. Newer edits are kept. HEAD, index, and working tree remain unchanged. Run `cargo test --locked commit::ai::`, `cargo test --locked commit::backend::ai::`, and `cargo test --locked config::tests`. Local mocks exercise transport, chunking, reduction, preservation, and failure handling; they do not establish external-model quality. |
| A7 IDEA comparison (R1, R4, R7, R8) | Use the demo's two identically named `notes.txt` files to repeat the four P0 scenarios with mouse only and keyboard only. | At 120×36 and 80×24, select the API repository's notes.txt after browsing all four node types, write a message, and filter out another checked repository. Commit once. Only the selected API file is committed; other repositories keep their HEAD, index entries, and working-tree contents. |
| A8 IDEA Log comparison (R1, R3, R4, R7, R8) | Run the [Log practice script](../examples/demo-log.sh), set User to me and Paths to atlas-web. Inspect trailing refs and merge branches at wide size; open a selected file's Diff. Clear one filter, switch independent Log tabs, then narrow to 80×24. | Three-column layout and selection match the [terminal screenshot](reference/IGit-Log-Graph.png); all four areas stay tiled at 80×24 and 44×12. Paths also limits the left Local / Remote / Tags tree to the included repositories, including shared-row ownership and ahead/behind counts. Check one or multiple repositories or qualified paths, cancel a path draft, clear Paths, and switch Log tabs; tree scope follows only applied filters and global Roots. After a path selection, Roots rows are unchecked; Recent keeps the 10 newest distinct selections, marks the active one, and switches without closing the menu. Multi-path Recent rows show every complete path that fits and append a remaining count only when it fits; the Paths control and Log title summarize the same selection as first path plus remaining count. Choosing a root clears the path selection. Clearing User retains Paths; returning to a tab retains filters/commit. Identical SHAs in different repositories never connect. All containing branches stay expanded and scrollable, with HEAD yellow, local green, remote purple, and no Show all / Show less. The current local branch keeps green + yellow icons, or purple + green + yellow with matching upstream. Narrow/compact modes preserve the first reference's icons. |
| A8g IDEA graph topology (R1, R3, R4) | Run `cargo test --locked log::model::graph` and the Log viewport/arrow tests. Compare the same row order and repository scope with IDEA, including two children of one parent, a merge with an already-reached first parent, and edges spanning 29/30 and 999/1,000 rows. | Distinct parent edges meet only at their actual commits. Reference/DFS ordering and adjacent-row compaction determine lanes; hidden long-edge sections release columns. Arrows identify real endpoints and remain clickable after column reorder. Text/hash filtering preserves loaded branch colors, and identical SHAs across Roots never connect. Terminal glyphs approximate diagonals/crossings; exact pixels, fonts, and theme overrides are outside this acceptance. |
| A9 IDEA Commit list (R1, R4, R5, R7, R8) | Launch IGit with the [Commit practice script](../examples/demo-commit.sh) workspace. Start with Directory and Repository on and Module off; open both `index.vue` files in the repository → directory tree. Exercise all eight Directory/Repository/Module combinations, using the historical [flat screenshot](reference/IGit-Commit-Flat.png) only as a visual reference. Select only `iframe-proxy.mjs`, inspect its Diff, and click Commit. | The 11 checked tracked changes and 8 unchecked untracked files start expanded. Across all eight combinations, Repository or Module adds one repository row beneath each visible changelist group, including Unversioned Files and Ignored Files; both together still show one row. Directory nests repository-relative folders under that row; with both repository options off it preserves the workspace-relative tree, and with Directory off rows stay flat within the same scope. Merge Conflicts remains a flat special child. Switching grouping preserves the current file, checks, folds, and Roots scope without altering the index or Git plans. At 120×36 and 80×24, the Changes pane scrolls to the last file; closing preview restores the original row. Duplicate names open their own patches. The mixed-state file contributes only its staged content to the new commit. Later working-tree edits and unchecked index entries remain. |
| A10 Icon fallback (R4, R8) | Start the multi-repository demo in default, `--icons unicode`, and `--icons ascii` modes. Visit all panels, Roots, and a conflict file. In ASCII mode, click both ends of Commit/Push checkboxes. | **Verified 2026-11-04.** All three modes were walked through in pseudo-terminals at 120×36, 100×30, and 80×24. Neither fallback used private-use glyphs. Three-cell checkboxes toggled and restored selection. HEADs, index entries, and working-tree diffs matched before/after. |
| A11 IDEA Push comparison (R1, R3, R4, R7, R8) | Run the [Push practice script](../examples/demo-push.sh), press `3`, and compare the [file tree screenshot](reference/IGit-Push-Tree.png). Check branch/submodule columns for long/short names, compact directories/counts, and added/modified colors. Expand/collapse with mouse and `h/l`, `[/]`; open Diff, scroll to the end, review, and cancel. | **UI and read-only behavior verified 2026-11-04.** Six pseudo-terminal walkthroughs: Nerd Font at 180×48 / 120×36, Unicode and ASCII each at 120×36 / 80×24. Eleven repositories sorted by name with aligned columns; 20 files expanded by directory, with independent type/status colors. Browsing, collapsing, and canceling review left all 11 HEADs, index entries, and working-tree diffs unchanged. Real Git tests additionally cover deletion, rename, duplicate names, and merge commit Diff. |
| A11a Update and publish (R1, R3) | Use disposable repositories and local bare remotes with one local-only and two remote-only commits. Run `python3 scripts/cargo_serial.py test --locked --lib push::update::`. Confirm once with Merge, then repeat in a fresh fixture with Rebase, including local merge commits. Also cover ahead-only, up-to-date, and new targets. | Progress shows only actual Fetching/Merging/Rebasing/Pushing phases. Merge retains local commits and includes incoming history; Rebase preserves merge topology with `--rebase-merges --no-autostash --no-update-refs --no-autosquash --no-fork-point`. Successful integration immediately publishes the resulting exact OID without a second confirmation; the remote matches local HEAD. Results distinguish updated and published from up-to-date. |
| A11b Shared strategy and target counters (R1, R4, R7, R8) | Run `python3 scripts/cargo_serial.py test --locked --lib core::strategy::`, then selector and Pull checks with `ui::strategy::` and `shell::update::` through the wrapper. Toggle each strategy from Push by keyboard and mouse; save each from Pull, reopen across workspaces/restarts, cancel Pull selection, simulate invalid/unreadable state and failed saves, and exercise temporary Pull `m/r` overrides. Inspect ahead-one/behind-two, zero, unavailable, and differently named targets at 44×12, 80×24, 120×36, and 180×36 with long Unicode names in Nerd Font, Unicode, and ASCII modes. | Merge is the missing-state default; successful saves atomically replace global `update-strategy.json` without changing Git config or `settings.toml`. Pull inherits it, overrides stay temporary, and failed saves retain the prior default. Push shows right-aligned Update strategy: Merge/Rebase and toggles directly without a selector; keyboard and mouse paths agree. Strategy changes invalidate review and cannot occur while running. Counters use the selected Push target, reserve cells before clipping, omit zeros, and remain local until Fetch. Branch `U` remains fast-forward-only. |
| A11c Update guards and partial results (R3) | Against temporary local remotes, use a custom target differing from upstream and separate fetch/push URLs with different tips. Change source HEAD/branch/configuration during Fetch and change the remote again before publication. Test staged plus unstaged tracked edits, an unchecked staged sibling, and an ahead-only dirty push; compare index bytes, working files, stash list, refs, and remote OIDs. Repeat both strategies with ignored/untracked collisions, skip-worktree/assume-unchanged paths, active operations, an unmerged index, directory renames, and replacement refs/grafts. Force a publication failure after successful integration. | Fetch obtains the explicit target OID from the actual push endpoint and never integrates the unrelated upstream. Conventional tracking refs update only when fetch/push URLs match. Integration refuses dirty tracked content and protected-path collisions without autostash, content/index loss, or clearing flags; ahead-only publication preserves dirty content. Active operations and unmerged indexes block execution. Directory rename inference stays disabled; ancestry/Merge/Rebase use stored history without local replacement refs/grafts. External changes and exact-tip lease races stop unsafe publication. An updated-but-push-failed result retains the updated local branch without overwriting the remote and requires a fresh review. |
| A11d Conflicts, scope, and retries (R2, R3, R7, R8) | Create Merge and Rebase conflicts in fresh local-remote fixtures, plus an independent repository and a dependent parent/submodule chain. Hide a checked repository in Roots. Open Conflicts from Push with `C` and a click; cancel and confirm Apply/Continue separately, finish the operation, and return. Cause partial failures, change scope, and review Retry. Exercise missing child commits and changed `.gitmodules` sources. | An update conflict publishes nothing for that repository and preserves native operation/index state. Apply/Continue retain separate write boundaries; finishing never resumes a delayed Push. Only reviewed checked Roots execute. Independent repositories continue, dependent parents skip after child failure, and submodule availability guards still block unsafe publication. Retry freshly reviews only visible failures and never replays completed pushes or silently expands scope. After resolving conflicts, the user initiates another Push. |
| A12 Commit display options (R1, R4, R5, R7, R8) | At 44×12, 80×24, and 120×36, use the eye or `o` to toggle Directory, Repository, Module, Ignored Files, and Git Status in a multi-repository workspace with duplicate names/ignored directories. Exercise all eight Directory/Repository/Module combinations. Select a mixed-staging file, leave another staged file unchecked, switch options, and commit. | Directory and Repository start checked and Module unchecked. Menu checkboxes match the changes list, and all eight combinations are available. Repository or Module enables one repository level beneath each visible changelist group, including Unversioned Files and Ignored Files; both never duplicate it. Directory is repository-relative under repository grouping and workspace-relative when both repository options are off; Directory off stays flat. Merge Conflicts remains a flat special child. Every Display option is session-local; changes preserve the current file, checks, folds, and Roots scope and never alter the index or Git write plans. Repositories/directories have expanded/collapsed folder markers; submodules keep dedicated icons. Menu borders do not trigger underlying controls; mouse/keyboard results match. Ignored items cannot be checked, moved, or sent to commit/AI; directories are not read recursively. The commit tree includes only selected staged content, preserving unselected staging and later working-tree edits. Run `python3 scripts/cargo_serial.py test --locked --lib commit::`. |
| A14 Workspace tabs (R1, R2, R7, R8) | At 120×36, 80×24, and 44×12, launch on Commit and switch to Commit, Log, Push, Branch, Stash, and Remote by clicking or pressing `1` through `6`. Use `7` for Conflicts when available. Start a real conflict or use the practice workspace; change Roots and review Apply and Continue separately. Use Tab, Ctrl-W, and divider dragging within the active tab. | Commit is active on launch. Each tab fills the workspace and retains its focus and pane sizes. Log stays tiled even when the other tabs are maximized. Conflicts is absent for clean Roots, highlighted while a scoped conflict or pending operation exists, and hidden after the last Continue. Refresh picks up external changes. Reviewing a write blocks switching tabs. Browsing and resizing preserve HEAD and index. |
| A15 Inline commit messages (R1, R3, R7, R8) | At 120×36, 80×24, and 44×12, press `c` from Changes and Preview. Type/paste multiline Unicode, including c/a, then press `Esc`. With Message focused, exercise `c` (Commit), `a` (Commit and Push), and the generic `A` (Generate message), plus Left/Right and Enter. Press `i` to edit again. Check mnemonic visibility after focus changes, editing, and key remapping/disablement, including a 34×6 composer. Also exercise message-field clicks, selection/copy, Tab, Shift-Tab, F6, and a click on the Log tab. | Editing stays within the Message pane with a blue border; Changes stays visible. Enter inserts a newline, and c/a type normally. The composer exposes only Commit, Commit and Push..., and Generate message. Outside editing, plain c/a presses execute their matching action only with Message focused; `A` remains the generic AI shortcut; repeat events do not submit. Exactly one C/a letter per available default action is gold, bold, and underlined while Message has normal-mode focus. Generate message has no default dedicated binding or accent; its fixed `A` accent appears only when `generate-message` is explicitly bound to unmodified `A` in Message context. Other focus, editing, inactive panes, and disabled or remapped default keys hide these accents. Compact labels retain the letters and all three hit areas. Ctrl-W deletes the preceding word and trailing whitespace; Ctrl-U deletes to the logical line start. Either deletes selected text first. `Esc` and `F2/Ctrl-S` persist the draft and leave Message focused. Left/Right moves the blue highlight, starting on Commit, and Enter executes the selected button. Navigation away and mouse submission save the draft first. The commit buttons start a local commit directly; Commit and Push opens Push for separate confirmation. The shared draft remains the same when browsing or filtering repositories. Each selected repository receives that message; partial failure retains it for retry without repeating successful commits. No repository name or selector appears in the composer. Generate message is a secondary button with the Resolve Automatically icon. Save failures keep input active and block navigation and submission. Mixed files commit their staged version; later edits and unchecked staged files remain. Run `python3 scripts/cargo_serial.py test --locked --lib inline_message` and `python3 scripts/cargo_serial.py test --locked --lib message_action_shortcuts`. |
| Commit automatic refresh (R3, R5, R7, R8) | While Commit is active, add, edit, atomically save, stage, and remove files externally, including edits with unchanged XY status. Repeat while typing a draft, after returning from Log, and after terminal focus returns. Check a second repository, a submodule, and a linked worktree. Run `cargo test --locked commit::refresh` and `cargo test --locked shell::workspace_tests::refresh`. | Native events scan only affected repositories in Roots after 300ms of quiet, with a two-second maximum debounce delay during continuous writes. One scan runs at a time; events during scans/dialogs are retained. Idle focus/tab changes start no Git scan. A 30-second Roots scan reconciles missed events or watcher failures. New untracked paths remain unchecked. Checkboxes, folds, cursor identity, the shared draft, caret, and unchanged preview selection/scroll survive refresh. Stale results are discarded and failed roots retain their last successful tree. Refresh does not write HEAD, index, or working files. |
| A16 Historical merge inspection (R1, R4, R7, R8) | Select a two-parent merge with a resolved file and a file changed against only one parent. Open combined and parent leaves with Enter and double-click; also inspect ordinary/root/add/delete commits. Navigate files/changes, fold, scroll, resize and copy at 180×40, 120×36, 80×24 and 44×12. Keep unrelated staged and unstaged edits present. | Combined files and each parent group preserve exact byte paths and comparison identity. Every merge file group opens three locked panes with immutable first-parent/result/second-parent content; ordinary commits show two Before/After panes. Both have aligned source line numbers and folded context. Added lines have green gutters and full-width code backgrounds; modified lines have blue gutters, stronger blue replacements, green inserted fragments and light blue remaining content. Extra added lines within a replacement block are green, matching parent lines stay neutral, and deletions use gray. Historical changes never use pending-conflict red. Parent-only files remain visible. Narrow views retain all available versions through Tab; two-pane mode never focuses a hidden third version. Selection preserves original tabs, Unicode and line endings across folds and alignment gaps. Binary/large/non-UTF-8/gitlink and octopus cases explain patch fallback. No Apply/Continue actions exist; HEAD, index and working-tree bytes remain unchanged. Run `cargo test --locked log::`, `cargo test --locked merge::`, and the focused `cargo test --locked --lib merge::view::rows::tests`. |
| A16a Revision alignment (R1, R8) | Compare reformatted Java declarations and repeated comment blocks around real edits. Open an XML mapper with changed text and inserted tags. Repeat in a two-parent comparison, then zoom and narrow the terminal. Run `cargo test --locked --lib merge::view::`. | Corresponding lines share a row and retain their source line numbers. Equivalent indentation has a muted gutter dot. Changes to indentation width, body whitespace or line endings stay visible. Real additions, modifications and deletions use green, blue and gray on the overview. Empty sides have neutral padding and a thin boundary. A single pane uses one row per empty span. Navigation counts contiguous changed sections together, and search and copy use the original revision bytes. |
| A17 Log commit actions (R1, R3, R7, R8) | In disposable repositories, use right-click or `a`, direct `d/u/R`, and the `F2` editor. Cancel/confirm actions with two repositories showing the same OID. Exercise remapping, filter prefixes, dirty tips, initial/merge tips, published ancestors on a second remote, earlier messages through merges, stale reviews, and inverse-patch conflicts. | Review targets only the highlighted repository and starts on Cancel. Drop/Undo reject older or published commits. Drop removes committed changes while protecting local content; Undo leaves removed changes staged and preserves mixed staged/unstaged work. Root removal leaves an unborn branch. Revert permits published non-merges and exposes native Conflicts. Message edits preserve trees, topology, exact index bytes, metadata, and other refs while rejecting published commits. Stale reviews and unsafe paths are rejected. Run `python3 scripts/cargo_serial.py test --locked --lib log::` and `python3 scripts/cargo_serial.py test --locked --lib shell::log_tests`. |
| A18 File tree path copying (R4, R8) | In Commit, Log, Push, and Stash, highlight a nested file and a dotted directory and try `yy`, `yf`, `yn`, and `yr` on each. Repeat in Rollback and Log's Select in Tree, and try a file in Conflicts. | The clipboard receives the absolute path, name, name without the final file extension, or repository-relative path. Dotted directory names remain intact, and copying preserves the highlighted row and checkboxes. Log's commit list still copies the full commit ID with `yy`. |
| A18a Push commit copying (R7, R8) | Focus an outgoing commit in each of two repositories; try `yy/ym/ya` during preview loading, the delayed chooser, F1, and remapped Push bindings. Repeat on a repository row, the right file tree, and after canceling a prefix with focus changes or an overlay. | Copies the highlighted commit's full ID, full multiline message, or `Name <email>`, independently of cached preview data. Repository rows do not copy commit data; file-tree path copying and text selection retain their meanings. Prefixes cancel without copying after context changes. Checkboxes, cursor, HEAD, index, and working tree remain unchanged. Read failures report a reason. Run `python3 scripts/cargo_serial.py test --locked --lib push_commit_copy`; tests capture copy actions without changing the system clipboard. |
| A19 Notification Center (R1, R7, R8) | Trigger Info, Success, Warning, and Error feedback from panel operations, mixed and all-failed batches, software-update availability/completion and manual errors, quiet automatic no-update/error results, and AI generation. Open History with `g n` from every normal workspace panel, Actions, and a live card. In Commit, Stash, Remote, and the Merge conflict list, verify one `g` leaves the selection unchanged, `gg` selects the first item, and `G` selects the last. In Log, verify the first Space immediately shows lowercase b/u/d/p and the filter footer while `g n` still opens History. If Notifications is rebound to a Space suffix, verify the existing immediate checkbox and second-Space behavior. Esc cancels. Close a card and verify its history entry remains. Exercise `j/k`, `J/K`, arrows, paging, Home/End, `/`, Enter, `n/N`, `y`, clear `c` then `y/n`, Esc/q, and mouse hit areas with and without dialogs/input. Scroll the right preview with `J/K` and its wheel without moving the highlighted row; drag-select right content and copy on release; use `Ctrl-A` / `Ctrl-C`, then Esc to clear before closing. Resize across 72 columns and test 44×12, 80×24, and 120×36. Rescan Roots and restart IGit. | Shell retains at most 500 stable-ID entries for the process and shows at most four newest cards, upper-right with 24–56-cell width, 4–8-row height, and a one-row gap. Severity colors are blue/green/amber/red and TTLs are 4/3/6/8 seconds. Wide History splits list/detail within 100×18; narrow History opens detail on Enter. `J/K` scroll the right preview or full detail independently of the list selection, while list `j/k` and wheel continue to navigate. Right-content drag selects exact source text with a blue highlight and copies once on release; clicks and empty selections do not copy, and changing the entry clears stale geometry. Card body opens the matching entry only outside dialogs/input; close works over dialogs and blocks underlying clicks. Roots rescans retain history; process restart clears it. Hints, search, and progress remain visible. Mixed batch outcomes warn, all-failed batches error, AI errors preserve the draft/focus, update availability (including automatic checks) and install completion notify, manual check results notify, and automatic no-update/error results stay quiet. |

Conflict list recheck (R1, R2, R7, R8): at 120×36, 80×24, and 44×12, check that the toolbar starts the list and that the Unresolved count, side statuses, and footer controls remain visible. Unresolved shares the file row background and highlight. Move focus to it, then press Enter twice to fold and expand it without moving focus. Toggle full paths, preview either side, and prepare simple drafts in one repository of a multi-repository workspace. A whole-file preview includes only the chosen source version; draft preparation leaves overlapping changes unresolved. Compare file bytes, HEAD, and index entries before and after preview/cancel. Apply file stages only that file. Continue stays disabled for unresolved repositories and requires a separate review when ready. Run `cargo test --locked conflict_list`.

Stash conflict EOL recheck (R2, R8): run
`cargo test --locked merge::engine::tests::conversion` in temporary repositories.
Enter must open conflicts from stash apply/pop with `eol=lf`, `text eol=lf`, and
`text=auto eol=lf`, including repositories with `core.autocrlf=true` and
merge/diff3/zdiff3 markers. Opening, closing, and reviewing preserve working bytes,
index entries, HEAD, and the stash. Confirmed Apply stages only the resolved path,
preserves an unrelated file's staged and unstaged contents, and offers Commit
instead of Continue. Actual CRLF conversion and custom transformations remain
blocked; attribute/config changes after opening reject Apply without writes.

Automatic resolution recheck (R2, R4, R8): use a temporary conflict with three non-conflicting blocks (identical edits, left-only, right-only) and two overlapping conflicts. Resolve Automatically must start enabled with count 3; clicking it or pressing `m` leaves zero changes and two conflicts. A second activation is a no-op; one undo restores all five pending blocks. Check readable labels and click targets at 44, 80, 120, and 184 columns in all icon modes. Mixed batches include disjoint single-line edits, preserve manual/ignored/partly accepted blocks, and leave working bytes, index entries, and HEAD unchanged until Apply review. Run `cargo test --locked merge::engine::tests::automatic`.

Push grouping recheck (A11): toggle Directory, Repository, and Module in the eye menu. Directory / Repository start enabled and Module starts disabled, removing the repeated root; all eight combinations must change only the right-side hierarchy. Without Directory, parent paths distinguish three `system.ts` files. Open one Diff, close it, and change grouping; the same file stays selected. Repository/commit changes preserve grouping, without affecting left-side checkboxes or review scope. Run `cargo test --locked push::` for grouping combinations, collapsed navigation, and real Git comparisons.

Push outgoing file scope (A11, R3): repository Changed files is the union of paths
changed by all outgoing commits, with repeated paths counted once. A divergent
remote must not introduce remote-only changes. The repository's cumulative Diff
starts at the common ancestor with the fetched target, while individual commits
use their first parent. Reverted changes and intermediate rename paths remain
listed with their latest outgoing comparison. Include history beyond the 60
visible commits and new targets; preserve checks, HEAD, index, working files,
and refs while browsing. Run
`python3 scripts/cargo_serial.py test --locked --lib repository_preview_`.

Push selection recheck (A11, R3/R7/R8): first direct entry checks and expands every repository in current Roots with outgoing commits, leaving up-to-date, unavailable, and hidden repositories unchecked and collapsed. Preserve the initial preview target even when repository names sort differently. Manual checkbox and fold choices survive refresh and tab switching, including folds changed during a background scan. Commit and Push still checks only successful commits' repositories while expanding all outgoing repositories. In the outgoing list, one Space changes the current repository checkbox immediately, with no extra key or hint delay. Following `j/k` must not toggle it again. If `notification-history` is rebound to a Space prefix, completing that chord restores only its own checkbox change and opens History; the default `g n` never changes the checkbox. Cover custom notification suffixes, repeated Space, remapped toggles, and Space before initial loading. Run `python3 scripts/cargo_serial.py test --locked --lib push::loading::` and `python3 scripts/cargo_serial.py test --locked --lib shell::push_tests::`; compare HEAD, index entries, and working files before/after.

Push loading recheck (A11, R1/R4/R7/R8): open the 11-repository Push practice workspace and switch to Push, including while Git reads are delayed. The loading frame must appear before outgoing reads finish; Cancel, another tab, and quit stay available. Returning retains cached rows, while Push/Retry/Fetch wait for fresh results. Change Roots or target during a delayed scan and rapidly browse commits/files: only current results may appear, at most one repository batch and one preview run, with no more than four concurrent repository readers. Retain checks, grouping, and selected commit OID; an unchanged snapshot also retains the current file, folds, and Diff scroll. Check Commit and Push reseeding, reader failure/retry, no-outgoing repositories, and unchanged HEAD, real index, working files, and remote refs. Run `cargo test --locked push::` and `cargo test --locked push_entry_renders_loading`.

Push design recheck (A11, R1/R4/R8): one contextual window title, aligned pane toolbars, outgoing Fetch and conditional Retry, right-aligned Update strategy: Merge/Rebase, complete focus-colored pane borders, and a compact footer with quiet options plus Cancel / Push. File type icons retain their independent colors; filenames carry change colors without appended status letters. At 38×10, 44×12, 80×24, 120×36, and 180×36 in each icon mode, controls must fit without overlapping; narrow footers wrap actions and retain hidden-selection notices. Verify s/f/P/F/H mnemonic highlights, direct keyboard/mouse strategy toggling, remapped and disabled shortcuts, Details, and maximization. Push is enabled only when a visible, eligible repository is checked and no remote operation is running. Cancel from the tree or Details restores the original workspace pane without Git writes. A file opens Log's Before/After viewer with line numbers, highlights and folds. Check previous/next files, narrow-terminal Tab, and Close returning to the last viewed file. Verify empty Before on a new target, a bounded patch for binary content, and unchanged index and working files. See the [design plan](plans/2026-11-12-push-toolbar-shortcuts.md) for verification.

Graph recheck: compare the same repository Paths; without Paths, multiple Roots add commits and rails. `straight_commits_sit_on_the_rail` checks that nodes in both terminal glyph modes share their rail's column and that compatible terminals connect only real visible parent/child pairs. `many_tips_pointing_at_one_parent` checks that 20 tips retain independent edges until their common parent. `dense_graph_keeps_one_origin` checks shared coordinates, edge hints, and selection in an overwide merge graph at 80, 120, and 180 columns. Run each with `cargo test --locked <test-name-prefix>`.

Filter access-key recheck (A8, R7, R8): in Log, press Space once and verify accented, bold, underlined lowercase b/u/d/p initials and the filter footer appear immediately at 44×12, 80×24, 120×36, and 180×36. Verify `g n` still opens Notifications. No bottom chooser covers history when all category initials fit. Left/Right moves the category highlight without moving the selected commit; Enter or a suffix opens the corresponding filter. Esc/Ctrl-C, focus changes, and resize clear the cues. Applied Unicode values, clear-button clicks, three icon modes, and Space checkboxes in commit selection mode retain their behavior. Incomplete toolbars and File History retain the delayed chooser. Run `python3 scripts/cargo_serial.py test --locked --lib log_space_filters`.

Branch menu recheck (A8): open Branch in a multi-repository workspace and hover Local/remote groups. Stars change only favorites and persist when reopened; names filter history. Favorites combines favorite branches. `master..topic` in Select... shows only commits unique to topic. Check narrow-screen back navigation, scrolling to the final branch, and one-click switching to User. Run `cargo test --locked branch_menu`.

Paths multi-repository recheck (A8): enter a directory and a file containing `[]` on separate lines in Paths → Select...; results should combine their histories. In Select in Tree..., expand two repositories and check one file in one and a directory in the other. Before confirmation, history stays unchanged; afterward, identically named paths in other repositories are excluded. Reopening restores tree checkboxes; reopening the Paths menu shows Roots unchecked and Recent at the bottom, with the applied selection checked. Apply more than 10 distinct selections and confirm only the newest 10 remain, without duplicates; resize the menu and confirm Recent shows every complete path that fits, adding `+ N` only when that suffix also fits; switch between Recent rows and confirm the menu stays open and the checked row and history follow. Choosing a root clears the path selection and recent check; `x` clears Paths without removing Recent. Roots toggles update immediately; unchecking all shows empty state. Switching away/back retains tab path filters and shared Recent while global Roots stays unchanged. Run `cargo test --locked paths_`.

Write boundary recheck (A3, A4, A6): use `cargo test --locked <name-fragment>` for `path_scope_`, `ai_review_of_a_replaced_file`, `submission_rejects_a_branch_switch`, `source_only_change`, and `cannot_turn_a_non_fast_forward_into_a_push`. See the [fix and regression plan](plans/2026-11-04-git-write-safety.md) for scenarios/results.

User filter recheck (A8, R7, R8): open User with no condition, `me`, and a custom
author. The search starts empty; Quick filters and Authors have separate
headings, secondary descriptions, and an applied checkmark independent of the
blue navigation row. Search loaded authors, clear the draft with its button,
enter an unknown author, cancel, and reopen. Test hover, wheel, scrollbar clicks,
arrows, Tab / Shift-Tab, paging, Home/End, and direct row selection at 44×12,
80×24, 120×36, and 180×36 in all icon modes. The popup stays in bounds; hover
keeps row targets stable; no matches explains typed-author application.
Browsing and cancellation retain other conditions and Git state. Run
`python3 scripts/cargo_serial.py test --locked --lib author_menu`.

Date filter recheck (R1, R7, R8): Date must list `Select...`, `Last 24 hours`, and `Last 7 days` in order. Use mouse and keyboard to enter/apply custom dates; verify inclusive boundary dates, rejection of invalid/reversed ranges, and cancel preserving the prior condition. Clearing dates retains User / Branch / Paths; returning to a Log tab restores dates. Run `cargo test --locked date_` for real-history relative ranges, future-commit exclusion, ancestors with reversed timestamps, date boundaries, and menu/input hit areas at 48, 80, 120, and 180 columns.

Log eye menu recheck (R1, R4, R7, R8): the eye in the graph toolbar or `o` opens Show / Highlight. Toggle Branch or Tag: the commit graph takes the space previously occupied by the branch tree. Restore through the menu or `B`, retaining filters and selection. Columns expands Author, Hash, Date, GPG Signature, Graph, and Message; toggles immediately affect rows. Check root/tag names, compact refs, ref position, both timestamps, and three default highlights. After selecting another branch, Not Cherry-Picked Commits dims equivalent applied commits. With Long Edges off, arrows jump to real endpoints. Author, Date, Graph, and Message default visible; Hash/signatures default hidden. Options are shared only within the session. `cargo test --locked view_options` covers three icon modes, 48/80/120/180 columns, short windows, hover, keyboard/mouse, and tab persistence. `cargo test --locked log::model::presentation` covers real signatures, dual dates, per-repository identity, and cherry equivalence while checking unchanged HEAD, index entries, and working tree.

Log column order recheck (A8): move Date before Author in Columns, then use row-end arrows to move unchecked Hash first. History should show date then author; enabling Hash should place it before both. First/last items cannot wrap. Reordering must not change checkboxes or reload history; switching Log tabs preserves order. Move Message before Date: long subjects/refs stay within that column. Move Graph last: arrows still jump to the same commits; hiding Graph disables those hit areas. Narrow views can hide these columns independently. Run `cargo test --locked view_options_reorder`.

Log navigation/copy recheck (R7, A8): browse more than one screen of history at 80, 120, and 180 columns. `j/k` scrolls only after crossing visible edges. A lone `g` does not jump; `gg/G` reaches first/last, while `H/M/L` selects screen top/middle/bottom only. `Ctrl-F/B` and `PageDown/PageUp` use actual rows for full pages; `Ctrl-D/U` uses half-pages. Bounds hold at ends, with empty lists, and after shrinking. `yy/ym/ya` copies full ID, full message, and name/email for the current row, including after switching repositories. Inputs do not trigger copying; unavailable system clipboards report errors. Run `cargo test --locked navigation_` and `cargo test --locked ui::clipboard::` for navigation, cross-repository text, and clipboard command input/failure paths. Automated tests do not change the runner's clipboard.

Key sequence hint recheck (R7, R8): in Log commits, `g` offers only `g`; `y` offers `y/m/a`. In a Merge conflict document, `[` or `]` offers `c`. Hints appear automatically at 200ms, remain hidden at 199ms, and need no further input. Early completion/cancel must prevent late display; a new prefix restarts timing. Once shown, arrows change only candidate highlighting; Enter/click uses the same action as direct completion. Esc/Ctrl-C closes hints without exiting the app/document. Panel/focus/menu/input/paste/blur/resize changes clear pending sequences. Run `cargo test --locked key_sequence` for delay boundaries, timed wakeups, quick completion, 44×12 / 80×24 / 120×36 layouts, overlay mouse blocking, candidate execution, and unchanged HEAD, conflict index entries, working-tree diffs, and file bytes before/after Merge browsing/canceling.

Context help recheck (R7, R8): open help in each area of Log, Commit, Push, and Merge. The title identifies context; with an empty query current-area actions come first and Search starts focused, as in lazydb. Type to filter using case-insensitive, ordered character matches, select with ↑/↓, and execute a single action with Enter. Verify nonadjacent matches in both shortcuts and descriptions, multiple search terms, and area-name matches. Results rank full-field matches first, followed by field prefixes, contiguous substrings, and nonadjacent matches. Earlier match positions break ties, followed by catalogue order. In Commit / Changes, `noti` should select **Open notification history** first. Use the shared window palette for the surface, title band, focused border, selected row, shortcut keys, descriptions, footer, and search highlights; check the same colors at 120×36, 80×24, and 44×12. Highlight every matched visible character, prefer contiguous matches when available, and preserve Unicode graphemes, clipping, and `NO_COLOR` styling. Check gg sequences, modifiers, panel changes, editing carets, and Roots actions all follow original event paths. Press F1 while editing a message or viewing Diff; cancel restores drafts/focus. From Changes or Preview, choosing the message shortcut opens the editor. In Message, choosing the selected action with Commit highlighted starts the local batch. Closing Help without choosing leaves HEAD, real index entries, and working-tree diffs unchanged. At narrow widths, the key column should shrink to leave room for descriptions, and the footer must retain a complete Esc-to-close hint. Run `cargo test --locked help_` for style at 120×36, 80×24, and 44×12, Unicode text editing, candidate visibility, empty results, and mouse hit areas.

Text selection/copy recheck (A13, R7/R8): at 120×36, 80×24, and 44×12, drag-select in Log commit details / Diff, Commit inline message editor / Diff preview, and Push Details / Diff. Dragging highlights without copying; release copies exactly once and clears highlight without a shortcut. Clicks, empty selections, and repeated release do not copy. Separately check keyboard select-all with Ctrl-C / Ctrl-Insert and read-only `y`. Verify wide Unicode characters, combining characters, emoji, reverse selections, and multiline selections. Soft wraps must not add newlines; original blank lines/indentation remain, without separators/padding. Scrolling during dragging still copies the correct source; keyboard selection survives scrolling. Dragging parent hashes does not navigate, while ordinary clicks do. Hiding/changing an object removes stale selections. In the message editor, verify typing at the mouse-copy endpoint, Shift+Arrow / Home / End, select-all then type/paste/delete, F2 save, and one Esc to save and exit with Message focused even with a selection. F1 help blocks copying underneath and restores keyboard selection afterward. Run `cargo test --locked ui::selection::` and `cargo test --locked text_selection`; tests capture copy actions without changing the clipboard and verify unchanged checkboxes, HEAD, index, and working tree. Clipboard command byte-input/failure paths remain covered by `cargo test --locked ui::clipboard::`.

Run `cargo test --locked` for the full regression suite. Acceptance records should identify terminal size, demo workspace, and cases actually passed. Static screenshots alone cannot prove correct preview scope, Git writes, or mouse hit areas.

Configuration recheck (R7/R8): use `--check-config` with missing, partial, invalid,
and conflicting settings. Remap Commit message focus, pane focus, and a Log copy
sequence; unbind another command. Verify old bindings stop, new labels appear,
help/sequence candidates execute the same action, and message/search input keeps
its characters. Open the message editor with the remapped `c`, save with `Esc`,
and press Enter on Commit with a mixed file and an unchecked staged file. Compare
HEAD, the real index, and the working tree. Reopen the
workspace and verify settings persist. Run `cargo test --locked config::`,
`cargo test --locked configuration_`, and `cargo test --locked --test configuration_cli`.

Kitty focus recheck: run `cargo test --locked smart_focus` and
`cargo test --locked shell::kitty`. In an isolated Kitty tab, use the same
Cmd+Ctrl+h/j/k/l chords through IGit's internal panes and across the Kitty edge
to Neovim splits, then return. Check that `IS_IGIT` is present only during an
opted-in IGit session, clears on exit, and is absent with default settings.
Verify edits, review choices, HEAD, index, and working tree remain unchanged.

**Kitty focus verification, 2026-11-05:** Kitty 0.47.4 on macOS, in isolated
test tabs with a 57-row terminal and a disposable `demo-commit.sh` repository.
Kitty's actual mapping dispatcher forwarded Cmd+Ctrl+h/j/k/l using `IS_IGIT`.
Internal navigation and horizontal/vertical Kitty handoffs passed. Neovim's
installed smart-splits v2.0.0 passed internal navigation and the return to IGit
while Kitty was the foreground app. Message editing and quit review preserved
focus; normal exit cleared the marker and restored Kitty's mappings. Explicit
default settings did not register the marker. HEAD, index, diff, and status
were unchanged; test tabs were closed and the original tab restored.

Kitty resize recheck: run `cargo test --locked smart_resize` and
`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s contrib/kitty -v`.
Move dividers from both sides and in the middle of a three-pane row. Check
repeats, fixed/minimum dividers, narrow layouts, focus preservation, tool
isolation, and edited/reviewed content. In Kitty, exhaust internal width/height
and verify handoff, the helper's source ID after focus/tab changes, and marker
cleanup with a resize-only settings file.

**Kitty resize verification, 2026-11-05:** Kitty 0.47.4 on macOS, isolated tabs
using a fresh `demo-commit.sh` repository. Horizontal test windows began at
142×57 (IGit) and 60×57 (Neovim); vertical tests began at 203×28 (IGit).
Cmd+Ctrl+Shift+h/j/k/l passed Kitty's actual mapping dispatcher. Internal
dividers moved three cells in both axes and reversed exactly. At internal
limits, width/height adjustments reached Kitty without changing focus.
Installed smart-splits v2.0.0 resized Neovim's internal splits and its Kitty
window. The IGit helper resized an inactive source window and an inactive tab
without activating them; stack layout was unchanged. Editors and quit review
blocked resizing. Resize-only startup registered `IS_IGIT`; exit cleared it
and restored Kitty's ordinary resize mappings. HEAD, real index, diff, and
status stayed unchanged; all test tabs were closed and the original restored.

**Configuration terminal verification, 2026-11-05:** a fresh
`examples/demo-commit.sh` workspace was exercised through pseudo-terminals at
120×36, 80×24, and 44×12. F8 focused Commit; the replaced `c` did nothing;
Ctrl-r opened review; help showed Ctrl-r; a custom `Q Q` sequence displayed
its continuation and opened quit review. Explicit ASCII/mouse-off CLI options
overrode Nerd Font/mouse-on settings. Canceling review preserved HEAD, real
index entries, working-tree diffs, and status in all three runs.

**Pane maximize verification, 2026-11-06:** regression tests cover all workspace
and tool panes, Tab traversal, saved split restoration, narrow layouts,
rebound shortcuts, hidden mouse/selection targets, and failed Kitty requests.
An isolated Kitty 0.47.4 instance loaded the user's actual configuration and
ran the release binary against a fresh practice workspace. Its mapping
dispatcher passed `Cmd+Ctrl+F` through to IGit: a 60×36 split became 120×36
stack with IGit in `MAX`, and the next press restored both layouts exactly.
Repeat/release events did not toggle; editors and quit review blocked the
action. With one Kitty window, only IGit changed. The test instance was closed;
HEAD, the real index, working-tree diff, and status stayed unchanged in all
three practice repositories.

Commit Changes Space recheck (A9, A19, R7, R8): in the practice workspace, highlight a checked file, repository, directory, or Changes group and press Space once. The checkboxes and selected-file counts must update immediately without waiting for the hint delay or another key. Press Space again and verify it toggles in the other direction. Pressing j/k afterward moves the cursor without toggling that selection again. The default `g n` opens Notification History independently of the checkboxes; if `notification-history` is rebound to a Space suffix, completing that chord restores only the checks changed by the last Space and opens Notification History. Esc keeps the applied change and its undo record. Repeat with a partially checked container and with a custom Space notification suffix. Run `python3 scripts/cargo_serial.py test --locked --lib shell::notification_tests::`; before and after, HEAD, the real index, and working files must remain unchanged.
