# IGit Product Requirements and Acceptance

Applies to version 0.1.0. This document defines the audience, scope, and completion criteria. See [README](../README.md) for installation and actual shortcuts, [interaction design](design.md) for workspace behavior, and [architecture](architecture.md) for Git commands, state, and module boundaries.

## Product Goal

IGit serves developers familiar with IntelliJ IDEA's Git tools who want to work in a terminal. Running `igit` or `ig` from the current directory opens one workspace for history, change review, commits, pushes, and existing conflict resolution, even if that directory is not itself a repository. Mouse users should not need to memorize commands first; Vim users should complete common tasks without reaching for a mouse.

A typical task spans a frontend, a backend, and a shared submodule. The developer finds relevant commits across all three histories, groups the files to commit from the two ordinary repositories, writes a suitable message for each, and starts a batch commit. Next, they review two push targets. If one repository is rejected, the successful result remains and only the failure needs attention. When the submodule has conflicts, the interface makes the content sources and editable result clear.

Product-owned interface text, help, errors, examples, and documentation use English. Repository names, paths, user messages, and file content continue to support Unicode.

## Requirements and Delivery Status

**Pane maximize (R7/R8 extension).** `Ctrl-W f` maximizes/restores the focused
pane without changing saved split sizes. Tab retains access to all enabled
panes; normal panel shortcuts remain available. `MAX` identifies the state.
Optional `smart-toggle-pane-maximized` synchronizes Kitty's owning tab with
stack/previous layout, using layout groups to avoid counting overlays as splits.
Single-group and non-Kitty sessions toggle locally. Verified remote outcomes
set the local state explicitly; failures preserve it. Editors/reviews and held
key repeats cannot trigger synchronization. Configuration examples include an
`IS_IGIT`-scoped `Cmd+Ctrl+F` mapping.

The table records the 0.1.0 delivery baseline. Items marked partial or next steps are not release claims. See P0, A7, and A9 for the current Commit changes and observed checks, and A11 for Push comparison.

| ID | Original requirement | Delivered behavior and limits |
| --- | --- | --- |
| R1 | Reproduce IDEA Git layouts, buttons, and workflows where practical | **Partial.** Log follows IDEA screenshots for three-column proportions, colored rails, trailing refs, inline filters (including date ranges/presets), eye menu, file tree, and details; see A8. Commit defaults to a full-width flat list with on-demand file Diff/container summary; see A9. Push aligns repository, branch, target, count, and submodule columns, with a full-height colored tree and on-demand Diff; see A11. Commit sits above Log at equal default heights, adjustable by keyboard or divider dragging. Push and Merge use centered dialogs rather than global tabs; see A14. Commit's changes tree and preview follow P0 below and have passed A7. |
| R2 | Resolve conflicts in three panes; apply non-conflicting changes in one action | **Supported for ordinary text.** Sides are read-only; the center result is editable. Take left, right, both, undo, or edit manually. Apply file and Continue merge/rebase require separate reviews. External edits and unsupported conflict types prevent overwriting. |
| R3 | Discover multiple repositories, combine history, and selectively batch commits/pushes | **Supported.** Ordinary repositories, initialized submodules, and linked worktrees have independent identities. History, commit, and push panels share repository scope; graphs stay repository-specific, messages are per repository, and push targets/results are reviewed separately. A batch is not an atomic transaction across repositories. |
| R4 | Prioritize design: improve fonts, colors, layout, and motion | **Partial.** All panels share icon semantics with Nerd Font, Unicode, and ASCII modes. Selection states, file types, and commit graphs remain recognizable without a dedicated font. Types include Java, Python, React JSX / TSX, ordinary JS / TS, common development languages, and build files; unknown types keep generic markers. Commit retains repository colors, gray parent paths, Git XY status, and row highlighting independent of checkboxes. Brief feedback animations remain a candidate enhancement. |
| R5 | Custom changelists, with the default group initially checked | **File-level groups supported.** Newly encountered ordinary committable items start checked in Changes. Conflicted files cannot be committed; untracked files remain separate. Groups can be created and files moved. Group membership and selection are distinct; regrouping does not run git add. Commit's display menu independently controls directory/repository grouping, ignored items, and XY status. Ignored items default hidden and are read-only when shown. |
| R6 | Configure an LLM and prompt to generate commit messages | **Supported and optional.** Configure a chat-completions-compatible endpoint, model, and prompt. Candidates use the actual checked patch per repository and replace drafts only after user acceptance. Git workflows work without a model service. |
| R7 | Vim shortcuts | **Core tasks supported in all four panels.** Browse, select, edit, review, and resolve conflicts with the keyboard. Pending key-sequence prefixes show continuation candidates that can be typed or selected. `?` / `F1` help focuses Search, prioritizes the current area's actions, and supports filtering, up/down selection, and Enter execution. F1 works during input/review and restores state when closed. Keys are context-sensitive; text input retains letters. The goal is task access, not a complete Vim editor. |
| R8 | Smooth mouse workflows | **Main operations supported.** Tabs, repository scope, rows/checkboxes, group dragging, merge gutters, and confirmation buttons are clickable. Log details/Diff, Commit message editor/preview, and Push Details / Diff support left-button drag selection, copy on release, and selection clearing, plus select-all and keyboard copying; see A13. New controls need a keyboard path and terminal-cell hit-area verification. |

## Required Operation Semantics

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
tool dialogs cannot resize the background workspace. Defaults remain unbound.

- **Scope, preview, selection, and execution are separate.** Roots changes only visible/executable scope; filtered-out repositories cannot join the current commit/push. The row cursor identifies what is being viewed; checkboxes identify candidates. Review shows actual repositories, files, or targets. Hiding and restoring a selected repository preserves its session selection.
- **The committed version must be explainable.** A selected file with staged changes commits its index version; only files without a staged version use working-tree changes. Changes to files, staging, or HEAD that affect a reviewed plan require review again. Unchecked staged items remain untouched. Partial success must not be reported as total failure or automatically rolled back. Real hooks may rewrite content; report actual results.
- **Pull and Fetch review all repositories in global Roots.** `p` opens Pull with Merge or Rebase; `f` opens Fetch, which refreshes remote refs without integrating changes. Log tab filters and Push checkboxes do not affect the batch. Repositories with unavailable targets are shown as skipped. Cancel performs no Git operation; confirmation runs eligible repositories independently and reports each result. Pull conflicts are left for Merge.
- **Push checks availability.** Repositories are execution units; previewing a commit does not allow skipping its ancestors. A remote target changed after review stops execution; no history-overwriting force push is offered. Added/changed gitlinks, or changes only to their declared sources, require verification that each child commit is available from the source in that version. If verification fails, identify the missing repository/commit without silently expanding selection.
- **Merge does not guess content.** Side labels identify the actual versions for the Git operation, especially during rebase, where fixed local/remote labels are misleading. Automatically apply only definite non-conflicting changes. The center supports undo; externally edited conflict files are preserved. Supported input is ordinary UTF-8 text up to 2 MiB per file. Binary files, delete/modify conflicts, special modes/conversions, and similar cases require external tools.
- **AI does not decide for the user.** Before sending, show the service, repositories, selected files, and patch size. Request per repository without unselected files. Keys come from environment variables and must not appear in documentation examples, project configuration, or logs. Cancel/failure must preserve drafts, and model responses must not trigger commit/push.

## Current Priorities

**P0: Make Commit readable at a glance and correct the preview target.** In the user's 0.1.0 baseline screenshot, `Unversioned Files` was highlighted while the right side showed one repository's `notes.txt`. Counts, status, and branch indicators were ambiguous, as were partial selection and cursor state. The [current implementation](../src/commit.rs) previews container scope summaries and file patches. Recheck the following conditions.

| Scenario | Acceptance criteria |
| --- | --- |
| Select a file, group, repository, or directory | With preview open, a file row shows that exact path and Diff. Other nodes show their own scope/count summary rather than treating the first descendant as selected. Collapse, repository changes, and refresh must not leave stale previews. |
| Scan a multi-repository tree | Distinguish group, repository/branch, directory, file, count, and Git status on one screen. Repository colors have names; status has symbols/text; focus and checkbox state are independent. Read-only conflicts do not affect parent committable-selection states. |
| Click and operate precisely | Row names browse, arrows expand/collapse, and boxes check. Complete select one file → read Diff → filter repositories → write message using mouse-only and keyboard-only paths; review lists must match. |
| Long paths and different sizes | Repeat at recommended 120×36 and narrower 80×24. Preserve filenames, selection states, and reachable commit actions first. Truncated paths retain a way to view the full path. Matching icons alone must not require a dedicated font. |

**P1 candidates:** After core information is readable, assess brief highlighting after accepting a diff block, contrast on low-color terminals, and optional motion. Splitting one file across groups, selecting unstaged lines, Shelf/Stash management, and further branch operations are outside this delivery and require separate use cases/acceptance. IGit does not manage hosting services, automatically clone child repositories, provide atomic multi-repository commits, or let models resolve code conflicts automatically.

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
| A1 Startup and history (R1, R3, R7) | Enter Log through either command. Hide sdk-core in Roots, browse/open a commit Diff, then restore scope. | Three working trees appear; bare remotes do not become a fourth. Filtered repositories disappear from history. Details follow the current commit, with no parent/child edges across repositories. |
| A2 Groups and batch commits (R3, R5, R7, R8) | Create Review in Commit. Move only one ordinary repository's `src/config.txt`; write different messages for both. Hide the other ordinary repository in Roots, enter review, cancel, and restore it. Choose Commit and Push, review both repositories, and confirm. | Untracked `notes.txt` starts unchecked; regrouping preserves its prior selection state. Hidden repositories do not appear in review. After restoration, each repository gets one local commit with its own message, then the app stops at Push preview. Verify with commands such as `git -C "$ws/atlas-web" show --name-only -1`. |
| A3 Push results and rejection (R3) | Continue A2: check each target and confirm, then compare local HEAD with each bare remote's main. Run `cargo test --locked two_remotes_partial_failure_and_retry_scope` for an invalidated plan. | Successful repositories match their remote branches. In the failure case, success remains recorded and retry includes only failed repositories in visible scope. |
| Workspace updates (R3, R7, R8) | Run `cargo test --locked shell::update_tests` and `cargo test --locked log_space_filters`. Open `p/f` from normal mode, cancel, then confirm against local bare remotes. Press `Space b/u/d/p` and `/` from Log. | Cancel leaves refs and working trees unchanged. Fetch updates refs only in Roots; Pull uses the selected strategy. Changed targets fail safely, conflicts remain available in Merge, and other repositories still complete. `Space p` opens Paths; standalone `p` opens Pull. |
| A4 Submodules and index isolation (R3, R5) | Run `cargo test --locked mixed_and_unrelated_index_survive` and `cargo test --locked unpublished_intermediate_child_and_absent_remote_stop_parent`. | A selected file with staged/unstaged changes commits only its staged version; unselected staged entries remain. An unavailable child commit in history prevents the parent push from publishing that pointer. |
| A5 Merge, undo, and Continue (R2, R7, R8) | In Merge, open sdk-core's `transport.conf`. Apply non-conflicting changes, take left, undo, then select the intended transport. Apply file first; continue separately. | path and retries merge automatically; transport requires a user choice. After Apply file, `git -C "$ws/atlas-web/deps/sdk-core" ls-files -u` is empty, but MERGE_HEAD remains. Only Continue creates the merge commit. |
| A6 AI boundaries (R6) | Select one file and put a recognizable unselected marker in another. Keep a handwritten draft. Inspect requests using a local compatible test endpoint: cancel once, then confirm, and choose whether to accept the candidate. | Cancel sends nothing. The request includes model, custom prompt, and selected patch, excluding the unselected marker. Until acceptance, the handwritten draft and HEAD remain unchanged. This case requires a test endpoint; routine Rust tests do not establish external-model validation. |
| A7 IDEA comparison (R1, R4, R7, R8) | Use the demo's two identically named `notes.txt` files to repeat the four P0 scenarios with mouse only and keyboard only. | **Verified 2026-11-04.** Mouse and keyboard walkthroughs at 120×36 and 80×24. After browsing all four node types, select the API repository's notes.txt, write its message, and filter out another checked repository. All four reviews contained only the API repository's notes.txt. After canceling, all three repositories' HEADs, index entries, and working-tree states were unchanged. |
| A8 IDEA Log comparison (R1, R3, R4, R7, R8) | Run the [Log practice script](../examples/demo-log.sh), set User to me and Paths to atlas-web. Inspect trailing refs and merge branches at wide size; open a selected file's Diff. Clear one filter, switch independent Log tabs, then narrow to 80×24. | Three-column layout and selection match the [terminal screenshot](reference/IGit-Log-Graph.png); all four areas remain reachable in narrow mode. Clearing User retains Paths; returning to a tab retains filters/commit. Identical SHAs in different repositories never connect. All containing branches stay expanded and scrollable, with HEAD yellow, local green, remote purple, and no Show all / Show less. The current local branch keeps green + yellow icons, or purple + green + yellow with matching upstream. Narrow/compact modes preserve the first reference's icons. |
| A9 IDEA Commit list (R1, R4, R5, R7, R8) | Start with the [Commit practice script](../examples/demo-commit.sh), press `2`, and compare the [screenshot](reference/IGit-Commit-Flat.png). Switch flat/directory modes, open both `index.vue` files, then select only `iframe-proxy.mjs`, review, and cancel. | The 11 checked tracked changes and 8 unchecked untracked files start expanded. At 120×36 and 80×24, the top half scrolls to the last file; closing preview restores the original row. Duplicate names open their own patches; the mixed-state script reviews only staged content. Cancel leaves HEAD, real index, and working-tree files unchanged. |
| A10 Icon fallback (R4, R8) | Start the multi-repository demo in default, `--icons unicode`, and `--icons ascii` modes. Visit all panels, Roots, and a conflict file. In ASCII mode, click both ends of Commit/Push checkboxes. | **Verified 2026-11-04.** All three modes were walked through in pseudo-terminals at 120×36, 100×30, and 80×24. Neither fallback used private-use glyphs. Three-cell checkboxes toggled and restored selection. HEADs, index entries, and working-tree diffs matched before/after. |
| A11 IDEA Push comparison (R1, R3, R4, R7, R8) | Run the [Push practice script](../examples/demo-push.sh), press `3`, and compare the [file tree screenshot](reference/IGit-Push-Tree.png). Check branch/submodule columns for long/short names, compact directories/counts, and added/modified colors. Expand/collapse with mouse and `h/l`, `[/]`; open Diff, scroll to the end, review, and cancel. | **UI and read-only behavior verified 2026-11-04.** Six pseudo-terminal walkthroughs: Nerd Font at 180×48 / 120×36, Unicode and ASCII each at 120×36 / 80×24. Eleven repositories sorted by name with aligned columns; 20 files expanded by directory, with independent type/status colors. Browsing, collapsing, and canceling review left all 11 HEADs, index entries, and working-tree diffs unchanged. Real Git tests additionally cover deletion, rename, duplicate names, and merge commit Diff. |
| A12 Commit display options (R1, R4, R5, R7, R8) | At 44×12, 80×24, and 120×36, use the eye or `o` to toggle Directory, Module, Ignored Files, and Git Status in a multi-repository workspace with duplicate names/ignored directories. Select a mixed-staging file, leave another staged file unchecked, switch options, and review/commit. | Repositories/directories have expanded/collapsed folder markers; submodules keep dedicated icons. Grouping options operate independently and retain identity, checkboxes, and scope. Menu borders do not trigger underlying controls; mouse/keyboard results match. Ignored items cannot be checked, moved, or sent to commit/AI; directories are not read recursively. The commit tree includes only selected staged content, preserving unselected staging and later working-tree edits. Run `cargo test --locked commit::`. |
| A14 Shared workspace (R1, R7, R8) | After startup, cycle across splits with Tab / Shift-Tab and move directionally with Ctrl-W then hjkl. Resize widths/heights with keyboard and dragging; reset with Ctrl-W =. Repeat in Push, Merge, input, and review. | Default split heights are equal or differ by one row. The input pane has a blue border. Resizing does not change focus, checkboxes, HEAD, or index. Hidden sidebars are skipped; dialogs block background operations and restore the original pane when closed. |

Push grouping recheck (A11): toggle Directory, Repository, and Module in the eye menu. Directory / Repository start enabled and Module starts disabled, removing the repeated root; all eight combinations must change only the right-side hierarchy. Without Directory, parent paths distinguish three `system.ts` files. Open one Diff and change grouping; the same file's patch remains. Repository/commit changes preserve grouping, without affecting left-side checkboxes or review scope. Run `cargo test --locked push::` for grouping combinations, collapsed navigation, and real Git patches.

Push design recheck (A11, R1/R4/R8): one contextual window title, aligned pane toolbars, outgoing Target / Fetch and conditional Retry, and a shared footer with quiet options plus Cancel / Push N. File type icons retain their independent colors; filenames carry change colors without appended status letters. At 44×12, 80×24, 120×36, and 180×36 in each icon mode, controls must fit without overlapping; narrow footers wrap actions and retain hidden-selection notices. Push N counts only visible eligible checked repositories and is disabled while empty or busy. Cancel from the tree or Details restores the original workspace pane without Git writes. See the [design plan](plans/2026-11-05-push-dialog-design.md) for verification.

Graph recheck: compare the same repository Paths; without Paths, multiple Roots add commits and rails. `straight_commits_sit_on_the_rail` checks that nodes in both terminal glyph modes share their rail's column and that compatible terminals connect only real visible parent/child pairs. `many_tips_pointing_at_one_parent` checks that 20 tips with one common parent occupy only two rails. `dense_graph_keeps_one_origin` checks shared coordinates, edge hints, and selection in an overwide merge graph at 80, 120, and 180 columns. Run each with `cargo test --locked <test-name-prefix>`.

Branch menu recheck (A8): open Branch in a multi-repository workspace and hover Local/remote groups. Stars change only favorites and persist when reopened; names filter history. Favorites combines favorite branches. `master..topic` in Select... shows only commits unique to topic. Check narrow-screen back navigation, scrolling to the final branch, and one-click switching to User. Run `cargo test --locked branch_menu`.

Paths multi-repository recheck (A8): enter a directory and a file containing `[]` on separate lines in Paths → Select...; results should combine their histories. In Select in Tree..., expand two repositories and check one file in one and a directory in the other. Before confirmation, history stays unchanged; afterward, identically named paths in other repositories are excluded. Reopening restores checkboxes; cancel preserves results. Roots toggles update immediately; unchecking all shows empty state. Switching away/back retains tab repository/path filters while global Roots stays unchanged. Run `cargo test --locked paths_`.

Write boundary recheck (A3, A4, A6): use `cargo test --locked <name-fragment>` for `path_scope_`, `ai_review_of_a_replaced_file`, `external_branch_switch`, `source_only_change`, and `cannot_turn_a_non_fast_forward_into_a_push`. See the [fix and regression plan](plans/2026-11-04-git-write-safety.md) for scenarios/results.

Date filter recheck (R1, R7, R8): Date must list `Select...`, `Last 24 hours`, and `Last 7 days` in order. Use mouse and keyboard to enter/apply custom dates; verify inclusive boundary dates, rejection of invalid/reversed ranges, and cancel preserving the prior condition. Clearing dates retains User / Branch / Paths; returning to a Log tab restores dates. Run `cargo test --locked date_` for real-history relative ranges, future-commit exclusion, ancestors with reversed timestamps, date boundaries, and menu/input hit areas at 48, 80, 120, and 180 columns.

Log eye menu recheck (R1, R4, R7, R8): top-right eye or `o` opens Show / Highlight. Toggle Branch or Tag: wide mode gives the left space to commits; narrow mode hides Refs. Restore through the menu or `B`, retaining filters and selection. Columns expands Author, Hash, Date, GPG Signature, Graph, and Message; toggles immediately affect rows. Check root/tag names, compact refs, ref position, both timestamps, and three default highlights. After selecting another branch, Not Cherry-Picked Commits dims equivalent applied commits. With Long Edges off, arrows jump to real endpoints. Author, Date, Graph, and Message default visible; Hash/signatures default hidden. Options are shared only within the session. `cargo test --locked view_options` covers three icon modes, 48/80/120/180 columns, short windows, hover, keyboard/mouse, and tab persistence. `cargo test --locked log::model::presentation` covers real signatures, dual dates, per-repository identity, and cherry equivalence while checking unchanged HEAD, index entries, and working tree.

Log column order recheck (A8): move Date before Author in Columns, then use row-end arrows to move unchecked Hash first. History should show date then author; enabling Hash should place it before both. First/last items cannot wrap. Reordering must not change checkboxes or reload history; switching Log tabs preserves order. Move Message before Date: long subjects/refs stay within that column. Move Graph last: arrows still jump to the same commits; hiding Graph disables those hit areas. Narrow views can hide these columns independently. Run `cargo test --locked view_options_reorder`.

Log navigation/copy recheck (R7, A8): browse more than one screen of history at 80, 120, and 180 columns. `j/k` scrolls only after crossing visible edges. A lone `g` does not jump; `gg/G` reaches first/last, while `H/M/L` selects screen top/middle/bottom only. `Ctrl-F/B` and `PageDown/PageUp` use actual rows for full pages; `Ctrl-D/U` uses half-pages. Bounds hold at ends, with empty lists, and after shrinking. `yy/ym/ya` copies full ID, full message, and name/email for the current row, including after switching repositories. Inputs do not trigger copying; unavailable system clipboards report errors. Run `cargo test --locked navigation_` and `cargo test --locked ui::clipboard::` for navigation, cross-repository text, and clipboard command input/failure paths. Automated tests do not change the runner's clipboard.

Key sequence hint recheck (R7, R8): in Log commits, `g` offers only `g`; `y` offers `y/m/a`. In a Merge conflict document, `[` or `]` offers `c`. Hints appear automatically at 200ms, remain hidden at 199ms, and need no further input. Early completion/cancel must prevent late display; a new prefix restarts timing. Once shown, arrows change only candidate highlighting; Enter/click uses the same action as direct completion. Esc/Ctrl-C closes hints without exiting the app/document. Panel/focus/menu/input/paste/blur/resize changes clear pending sequences. Run `cargo test --locked key_sequence` for delay boundaries, timed wakeups, quick completion, 44×12 / 80×24 / 120×36 layouts, overlay mouse blocking, candidate execution, and unchanged HEAD, conflict index entries, working-tree diffs, and file bytes before/after Merge browsing/canceling.

Context help recheck (R7, R8): open help in each area of Log, Commit, Push, and Merge. The title identifies context; current-area actions come first and Search starts focused, as in lazydb. Type to filter, select with ↑/↓, and execute a single action with Enter. Check gg sequences, modifiers, panel changes, editing carets, and Roots actions all follow original event paths. Press F1 while editing a message or viewing Diff; cancel restores drafts/focus. Choosing Commit's review action opens only review; cancel leaves HEAD, real index entries, and working-tree diffs unchanged. At narrow widths, the key column should shrink to leave room for descriptions, and the footer must retain a complete Esc-to-close hint. Run `cargo test --locked help_` for style at 120×36, 80×24, and 44×12, Unicode text editing, candidate visibility, empty results, and mouse hit areas.

Text selection/copy recheck (A13, R7/R8): at 120×36, 80×24, and 44×12, drag-select in Log commit details / Diff, Commit Edit commit message / Diff preview, and Push Details / Diff. Dragging highlights without copying; release copies exactly once and clears highlight without a shortcut. Clicks, empty selections, and repeated release do not copy. Separately check keyboard select-all with Ctrl-C / Ctrl-Insert and read-only `y`. Verify wide Unicode characters, combining characters, emoji, reverse selections, and multiline selections. Soft wraps must not add newlines; original blank lines/indentation remain, without separators/padding. Scrolling during dragging still copies the correct source; keyboard selection survives scrolling. Dragging parent hashes does not navigate, while ordinary clicks do. Hiding/changing an object removes stale selections. In the message editor, verify typing at the mouse-copy endpoint, Shift+Arrow / Home / End, select-all then type/paste/delete, F2 save, and two Esc presses to cancel. F1 help blocks copying underneath and restores keyboard selection afterward. Run `cargo test --locked ui::selection::` and `cargo test --locked text_selection`; tests capture copy actions without changing the clipboard and verify unchanged checkboxes, HEAD, index, and working tree. Clipboard command byte-input/failure paths remain covered by `cargo test --locked ui::clipboard::`.

Run `cargo test --locked` for the full regression suite. Acceptance records should identify terminal size, demo workspace, and cases actually passed. Static screenshots alone cannot prove correct preview scope, Git writes, or mouse hit areas.

Configuration recheck (R7/R8): use `--check-config` with missing, partial, invalid,
and conflicting settings. Remap Commit review, pane focus, and a Log copy
sequence; unbind another command. Verify old bindings stop, new labels appear,
help/sequence candidates execute the same action, and message/search input keeps
its characters. Cancel a remapped review with mixed staged/unstaged content and
an unchecked staged file; compare HEAD, real index, and working tree. Reopen the
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
