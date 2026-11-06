# Configuration

IGit uses an optional `settings.toml`, following LazyDB's application-settings
model: embedded defaults plus a small user override file. Settings load once at
startup and stay in effect when opening another workspace. Restart after editing.

## Location and precedence

The settings file is selected in this order:

1. `igit --config /path/to/settings.toml` (or `--config=/path/to/settings.toml`).
2. `$IGIT_CONFIG_HOME/settings.toml` when `IGIT_CONFIG_HOME` is nonempty.
3. On macOS/Linux, `$XDG_CONFIG_HOME/igit/settings.toml`, otherwise
   `~/.config/igit/settings.toml`.
4. On Windows, `%APPDATA%\igit\settings.toml`.

`ig` accepts the same options. `--config` points to a file; `IGIT_CONFIG_HOME`
points to a directory. There is no repository-local settings discovery. Unlike
LazyDB's installation root, this directory contains only application preferences.
IGit does not create or rewrite it automatically.

Precedence for values is **explicit CLI option → user settings → embedded
defaults**. Tables merge recursively; arrays replace the complete default array.
Omitted values inherit future built-in defaults, so copy only settings you want
to change. The schema version is `1`; it may be omitted.

A missing default-location file uses built-in settings. A missing explicit
`--config` file, an unreadable file, invalid TOML, unknown fields/commands, or
invalid bindings stop startup with a diagnostic before terminal initialization
or repository discovery. Settings are validated even for `--list-roots`.
`--help`, `--version`, and `--print-default-config` work without loading settings.

```sh
# Inspect all supported commands and their exact defaults.
igit --print-default-config

# Check the selected file without opening a terminal UI or touching a repository.
igit --check-config
igit --config /path/to/settings.toml --check-config

# Override presentation for this run.
igit --icons ascii --mouse off /path/to/workspace
```

The complete reference is [`config/default.toml`](../config/default.toml).

## Example

Create the selected directory and save a `settings.toml` containing just the
overrides you need:

```toml
version = 1

[terminal]
mouse = true

[ui]
icons = "unicode" # "nerd-font", "unicode", or "ascii"

[keybindings]
hint_delay_ms = 300

[keybindings.global]
focus-commit = ["1", "F8"]
quit = ["q", "Ctrl-c"]

[keybindings.panes]
focus-left = ["Ctrl-w h", "Alt-h"]
focus-down = ["Ctrl-w j", "Alt-j"]
focus-up = ["Ctrl-w k", "Alt-k"]
focus-right = ["Ctrl-w l", "Alt-l"]

[keybindings.log]
copy-id = ["y y", "Alt-y"]
copy-author = ["g a"]

[keybindings.commit]
review = ["Ctrl-r"]
ai-draft = []

[keybindings.push]
review = ["Ctrl-r"]

[keybindings.merge]
apply = ["Ctrl-r"]
```

Here `Ctrl-r` has a different meaning in each panel. Commit's old `c`, Push's
old `p`, and Merge's old `A` / `Shift-Enter` bindings are replaced. Keep an old
key by listing it alongside the new one. `ai-draft = []` removes its keyboard
binding; the action's mouse button remains available.

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `version` | `1` | Configuration schema version |
| `terminal.mouse` | `true` | Enable mouse capture and IGit mouse interactions; CLI `--mouse on/off` overrides it |
| `ui.icons` | `"nerd-font"` | Icon mode; CLI `--icons` overrides it |
| `ai.endpoint` | `""` | Full OpenAI-compatible chat completions URL; HTTPS or localhost HTTP with a port |
| `ai.model` | `""` | Model identifier sent to the configured service |
| `ai.prompt` | See defaults | System prompt used to generate one message for all selected changes |
| `ai.api_key_env` | `"IGIT_LLM_API_KEY"` | Environment variable containing the optional bearer token |
| `ai.timeout_seconds` | `60` | Total request timeout, from `1` to `600` seconds |
| `keybindings.hint_delay_ms` | `200` | Delay before showing sequence continuations, from `0` to `10000` milliseconds |
| `keybindings.global` | See defaults | Switch workspace tabs, review Pull/Fetch, Roots, Actions, refresh, help, quit |
| `keybindings.panes` | See defaults | Focus/cycle panes, resize/reset sizes, maximize/restore, optional Kitty synchronization, close a Log tab |
| `keybindings.log` | See defaults | Movement, paging, copying, filtering, tabs, display options |
| `keybindings.commit` | See defaults | Movement, checkboxes, groups, preview, messages, AI and commit review |
| `keybindings.push` | See defaults | Movement, repository selection, targets, fetch, push review and retry |
| `keybindings.merge` | See defaults | Conflicts tab: navigation, resolutions, undo, drafts, Apply and Continue review |

These settings do not store repository selections, message drafts, pane sizes,
or display-menu choices. Existing workspace state remains
under `IGIT_STATE_DIR` / `XDG_STATE_HOME/igit`; see [README](../README.md#ai-commit-messages-optional).
API keys come only from the configured environmtent variable.

## AI commit messages

Add `[ai]` to the same user settings file, or select your file with
`igit --config /path/to/settings.toml`. For a local compatible service:

```toml
[ai]
endpoint = "http://localhost:11434/v1/chat/completions"
model = "your-local-model"
prompt = """
Write one Conventional Commit message covering all supplied repositories.
Use a short subject and a body describing the changes when useful.
Return only the commit message, without Markdown fences.
"""
api_key_env = "MY_LLM_API_KEY"
timeout_seconds = 120
```

The endpoint must include the complete chat completions path. Remote services
require HTTPS; local HTTP accepts `localhost`, `127.0.0.1`, or `[::1]` with an
explicit port. Set the named environment variable before launching IGit if the
service requires authentication. An unset or empty variable sends no bearer
header, which supports local services without authentication. Do not put keys
in the URL or settings file; inline `api_key` fields are not accepted.

`prompt` is the system message and supports multiline Unicode text. IGit appends
the reviewed patches as a separate user message: a JSON array of objects with
`repository` and `patch` fields. No template placeholders are required. The
response must provide a nonempty `choices[0].message.content`. Streaming and
provider-specific native APIs are not used; configure a compatible endpoint.

When a user `[ai]` table is present, its merged values take precedence over the
workspace's old AI settings at startup. Without that table, saved workspace
endpoint/model/prompt remain available. `S` edits those three values for the
current session and saves the workspace fallback; it does not rewrite the user
settings file. On restart, `[ai]` takes precedence again. Timeout and credential
variable settings come from the user file or defaults.

**Generate message** (`A`) first reviews the destination, model, file lists and
patches. Confirmation sends one request containing only checked files in the
current Roots, up to 160 KB of patch content in total. A staged file sends its
index version; later unstaged edits and unchecked files are excluded. Accept
the returned candidate to replace the shared draft, then edit it normally.
Request errors and canceled reviews preserve the original draft. Each selected
repository gets a separate Git commit with that same message after commit review.

## Key syntax and contexts

Each command takes an array of strings. A string is one chord or a sequence of
up to four chords separated by spaces:

- Characters are case-sensitive: `j`, `J`, `/`, `+`, `-`, and Unicode characters.
- Named keys include `Space`, `Enter`, `Tab`, `Shift-Tab`, `Up`, `Down`, `Left`,
  `Right`, `Home`, `End`, `PageUp` / `PgUp`, `PageDown` / `PgDn`, `Insert`,
  `Delete`, `Backspace`, and `F2` through `F24`.
- Modifiers are `Ctrl` / `Control`, `Alt` / `Option`, `Shift`, and
  `Super` / `Cmd` / `Command`, joined with `-` or `+`: `Ctrl-r`, `Alt+Enter`.
- Write `g g`, `y m`, `[ c`, `Space b`, and `Ctrl-w h` as separate sequence steps;
  compact help labels such as `gg` are not configuration syntax.
- `Shift-j` and `J` normalize to the same key. Shift-Tab and BackTab encodings
  also match. The terminal must deliver the selected chord; OS/terminal-reserved
  shortcuts are not forwarded by IGit.

Global `pull` and `fetch` default to `p` and `f`. Push's own `review` and `fetch`
default to `P` and `F`, and Merge's `back-to-files` defaults to `F`. If an
override still uses `p` or `f` for one of those panel commands, change that
override or rebind the conflicting global command.

`global.focus-branch` defaults to `5`. `[keybindings.branch]` controls `create`
(`a`), `delete` (`d`), `checkout` (`Enter`), `merge` (`m`), `rename` (`r`), and
tree navigation. The global `roots` binding is inactive in Branch because `r`
renames; use Actions → Roots there. Branch name entry and confirmation retain
their fixed controls. For example:

```toml
[keybindings.global]
focus-branch = ["F8"]

[keybindings.branch]
create = ["n"]
rename = ["R"]
```

`commit.stash` defaults to `s`: it opens the shared stash-message editor for
all changes in Roots, including untracked files. `commit.toggle-status` now
defaults to `T`; the same option remains in View Options. Remap either under
`[keybindings.commit]`, for example `stash = ["F9"]`. Within the stash editor,
Enter/Ctrl-S/F2 reviews and Esc cancels; typed characters bypass normal-mode
bindings. If an existing custom `toggle-status` binding uses `s`, move or
unbind it when enabling the default stash shortcut.

`global.focus-stash` defaults to `6`. `[keybindings.stash]` provides
`move-down`, `move-up`, `collapse`, `expand`, `toggle`, `open`, `first-item`,
`last-item`, `page-down`, and `page-up` for the repository and file trees.
`collapse-all` (`z`) and `expand-all` (`Z`) apply only to Files. For example,
`open = ["v"]` changes the normal-mode file comparison key. The open Diff
retains the same fixed revision controls as Log, including `Esc` to close.

In the merge editor, `auto-resolve` (`a`) applies only non-conflicting changes
(the toolbar's All action). `resolve-simple` (`m`) runs **Resolve Automatically**:
all pending non-conflicting changes plus disjoint single-line edits. These
existing command names remain stable for custom bindings; the automatic button
shows the configured shortcut.

Bindings apply in **normal mode**, in their command's original context. For
example, Log copy commands operate in the commit list, Commit's `toggle`
operates in Changes, Push's `edit-target` operates in Repos and Commits, and
Merge's `apply` operates in an open conflict document. Menus, search fields,
Diff overlays, editors, Roots, and confirmation dialogs retain their existing
controls. A command cannot be moved into a text-entry or confirmation context.

`F1` always opens/closes contextual help. `Esc` remains cancellation/back.
Text selection keeps `Ctrl-a`, `Ctrl-c`, `Ctrl-Insert`, and read-only `y`;
selection copy takes precedence over normal commands. The configurable
`global.help` command controls the normal-mode `?` shortcut. The configurable
`global.quit` command includes `Ctrl-c` by default, but never overrides copying
or cancellation of a pending sequence.

Exact duplicates and complete-key/prefix conflicts in overlapping contexts are
errors. For example, binding Commit review to `i` also requires changing or
disabling `commit.edit-message`. Different panels may reuse a chord. Two
sequences may share a prefix (`g g` and `g a`), but `g` and `g a` cannot both
be complete commands in the same context.

Sequence prefixes remain pending until completion, cancellation, or context
change; the hint delay is not a timeout. Visible hints support arrows, Enter,
and mouse selection. An explicitly configured continuation takes precedence
over picker navigation. Esc/Ctrl-c cancels only the pending sequence. Switching
focus, opening a dialog, resizing, pasting, or losing terminal focus clears it.
Digits following `Ctrl-w` are reserved for window counts (`Ctrl-w 3 +`).

Help displays/searches the effective bindings and executes the same reviewed
action. Rebinding Commit, Push, Merge Apply, or Continue never skips review.
Shortcut changes do not change repository scope, checkbox selections, or retry
semantics. Mouse buttons remain usable when a command is unbound.

## Shared focus shortcuts with Kitty and Neovim

The opt-in `smart-focus-pane-*` commands use the same names as LazyDB. They
focus the nearest visible IGit pane in the requested direction, then ask Kitty
to focus a neighboring window when there is no internal neighbor. They do not
wrap. The original `focus-*` commands remain internal-only.

For the same macOS chords as a Neovim `smart-splits.nvim` setup, add:

```toml
[keybindings.panes]
smart-focus-pane-left = ["Cmd+Ctrl+h"]
smart-focus-pane-down = ["Cmd+Ctrl+j"]
smart-focus-pane-up = ["Cmd+Ctrl+k"]
smart-focus-pane-right = ["Cmd+Ctrl+l"]
```

Each smart-focus binding must be a single chord with Ctrl, Alt, or Super/Cmd.
All four are unbound by default. Aliases and `[]` work as for other commands.
Text editors, menus, help, Diff overlays, and confirmation dialogs consume
these chords without moving focus or changing input; finish or close the
overlay first. Push and Merge expose only their own panes. Help in normal mode
lists the configured commands and can execute them. Repeated key events are
ignored so holding a chord does not run across multiple windows.

In `kitty.conf`, retain your existing Neovim/LazyDB rules and add IGit's rules:

```conf
# Required for application-to-Kitty focus requests. Reuse an existing socket.
allow_remote_control yes
listen_on unix:/tmp/kitty
kitty_mod cmd+ctrl

map kitty_mod+h neighboring_window left
map kitty_mod+j neighboring_window down
map kitty_mod+k neighboring_window up
map kitty_mod+l neighboring_window right

# Pass these chords to IGit while its smart focus integration is active.
map --when-focus-on var:IS_IGIT kitty_mod+h
map --when-focus-on var:IS_IGIT kitty_mod+j
map --when-focus-on var:IS_IGIT kitty_mod+k
map --when-focus-on var:IS_IGIT kitty_mod+l
```

Kitty must expose nonempty `KITTY_LISTEN_ON` and `KITTY_WINDOW_ID`, and `kitten`
must be on the application's PATH. Restart Kitty if newly enabling its remote
control socket; reload its configuration for mapping changes. Restart IGit to
load the TOML file. IGit sets `IS_IGIT` while opted in and connected to Kitty,
then clears it on exit. It negotiates the enhanced keyboard protocol when
supported, allowing Cmd/Super chords to reach the application.

Neovim should keep the same `<D-C-h/j/k/l>` mappings to
`smart-splits.move_cursor_left/down/up/right`, plus its existing `IS_NVIM`
pass-through rules. No changes to Neovim or LazyDB are needed when those
already work. Without Kitty, smart focus still moves within IGit and stays
put at the edge. Remote failures appear as a nonfatal status message; requests
have a one-second timeout and target the originating Kitty window.

See Kitty's [remote control](https://sw.kovidgoyal.net/kitty/remote-control/)
and [keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
documentation for terminal-side details.

## Shared resize shortcuts with Kitty and Neovim

The optional `smart-resize-pane-*` commands follow LazyDB's directional resize
behavior. Add these entries to your existing `[keybindings.panes]` table:

```toml
smart-resize-pane-left = ["Cmd+Ctrl+Shift+h"]
smart-resize-pane-down = ["Cmd+Ctrl+Shift+j"]
smart-resize-pane-up = ["Cmd+Ctrl+Shift+k"]
smart-resize-pane-right = ["Cmd+Ctrl+Shift+l"]
```

Each press moves a visible divider **three terminal cells** left/down/up/right.
Holding the chord repeats the adjustment. The nearest right/bottom divider is
preferred; a pane at the right/bottom edge uses its left/top divider. For
example, moving right expands the left pane and shrinks the right pane.
The focused pane stays focused. When no internal divider exists on that axis,
or the selected divider is at its size limit, IGit asks Kitty to resize its
window in the same direction. A compact Log with hidden sidebars therefore
hands horizontal resizing directly to Kitty. Without Kitty, it stays at the
internal size limit.

Push and Merge resize only their own visible panes. Editors, menus, help, Diff
overlays, and confirmation dialogs consume the chords without changing size,
focus, or input. Normal-mode help can execute a configured resize action.
All four commands are unbound by default and require a single Ctrl, Alt, or
Super/Cmd modified chord. Any configured smart pane command enables `IS_IGIT`.

Install the [IGit resize helper](../contrib/kitty/igit_resize.py) into Kitty's
configuration directory. For the default directory, run from the IGit source:

```sh
install -m 644 contrib/kitty/igit_resize.py ~/.config/kitty/igit_resize.py
```

For a custom Kitty configuration directory, use that directory instead. This
helper resolves neighbors and resizes the originating window by ID, keeping
focus unchanged even if it has moved while the request was pending. A stack
layout or a window with no neighbor on the requested axis stays unchanged.
The helper does not depend on Neovim's `relative_resize.py` or LazyDB's helper.

Add the following to `kitty.conf`, alongside your existing resize mappings:

```conf
map --when-focus-on var:IS_IGIT kitty_mod+shift+h
map --when-focus-on var:IS_IGIT kitty_mod+shift+j
map --when-focus-on var:IS_IGIT kitty_mod+shift+k
map --when-focus-on var:IS_IGIT kitty_mod+shift+l
```

Use the same remote-control prerequisites as smart focus. Reload Kitty's
configuration and restart IGit. Existing Neovim `<D-C-S-h/j/k/l>` mappings to
`smart-splits.resize_left/down/up/right`, `IS_NVIM` pass-through rules, and
ordinary Kitty resize mappings can continue to use their existing setup.
If a remote resize fails, check the status message, remote-control settings,
and that `igit_resize.py` is installed. Requests retain the one-second timeout.

## Pane maximize and Kitty synchronization

`toggle-pane-maximized` defaults to `Ctrl-w f` in Commit, Stash, Push, and Conflicts.
It fills the workspace with the focused pane. Tab continues through enabled
panes; `1` / `2` / `5` / `6` / `3` / `4` switches to Commit, Log, Branch, Stash, Push, or Conflicts when
available. Log stays tiled. The next press in a supported tab restores the saved split sizes.
Maximizing is session-only. Local divider resize/reset and mouse resizing are
disabled while maximized; smart resize can still hand off to Kitty.

The optional `smart-toggle-pane-maximized` command follows LazyDB's behavior:

- In a Kitty tab with multiple layout groups, switch to `stack` and maximize
  IGit's focused pane where supported. From `stack`, restore the previous Kitty
  layout and the IGit split layout. Log keeps its panes tiled in both Kitty layouts.
- With one Kitty group, or outside Kitty, toggle only the IGit pane where
  supported. Overlays belonging to the same Kitty group do not count as extra splits.
- The originating `KITTY_WINDOW_ID` identifies the tab, even if another tab
  currently has focus. IGit verifies Kitty's resulting layout before updating
  its own state. Failures show a status message and leave IGit's layout intact.
- Editors, menus, help, Diff overlays, and review dialogs consume the shortcut
  without changing input or layout. Held-key repeats do not toggle repeatedly.

Add this to the existing `[keybindings.panes]` table in `settings.toml`:

```toml
smart-toggle-pane-maximized = ["Cmd+Ctrl+f"]
```

Alongside the existing remote-control settings in `kitty.conf`, use:

```conf
allow_remote_control yes
listen_on unix:/tmp/kitty
enabled_layouts splits,stack
map --when-focus-on var:IS_IGIT cmd+ctrl+f
```

Keep your existing socket and enabled layouts if already configured; include
`stack` in the layout list. This binding synchronizes **pane maximize**, not
macOS window fullscreen. It applies only while IGit owns the terminal, leaving
LazyDB, Neovim, and ordinary Kitty mappings available in their own contexts.
No additional kitten script is required. Reload Kitty's configuration and
restart IGit after building/installing the version supporting these commands.
The empty Kitty mapping passes the original key event through, preserving
press/repeat/release information so a held shortcut cannot toggle repeatedly.
