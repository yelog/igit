# Installing and Updating IGit

Both `igit` and `ig` open the same workspace. Install Git separately and use a
terminal with ANSI support. The native installer supports these release targets:

| System | CPUs | Requirements |
| --- | --- | --- |
| macOS | Intel, Apple Silicon | Python 3.9+, curl, Git |
| Linux | x86_64, ARM64 | Python 3.9+, curl, Git; binaries use musl |
| Windows | x86_64 | Windows PowerShell 5.1 or PowerShell 7, curl.exe, Git |

## macOS and Linux

Beta channel:

```sh
curl -fsSL https://igit.yelog.org/install-beta.sh | sh
```

Stable channel:

```sh
curl -fsSL https://igit.yelog.org/install.sh | sh
```

The stable channel becomes available after the first stable release. It does
not fall back to a beta. To inspect the script first, download it to a file,
read it, then run `sh FILE`.

The default command directory is `~/.local/bin`. Versioned releases and
ownership metadata live under `${XDG_DATA_HOME:-$HOME/.local/share}/igit`.
The installer adds a marked PATH entry to the current shell's bash, zsh, fish,
or POSIX shell profile and prints a command for the current terminal. It avoids
replacing an existing profile block or following a symlinked profile. Open a
new terminal afterward, or run the printed command.

For a custom location or manual PATH management:

```sh
curl -fsSL https://igit.yelog.org/install-beta.sh | sh -s -- \
  --install-root "$HOME/.local/share/igit" \
  --install-dir "$HOME/.local/bin" --no-modify-path
```

`IGIT_INSTALL_ROOT` and `IGIT_INSTALL_DIR` provide the same defaults. Set them
on the installer side of a pipeline, for example `curl ... | env
IGIT_INSTALL_DIR=/path/to/bin sh`. Do not run the installer with `sudo`.

## Windows

Run the beta installer in PowerShell:

```powershell
$env:IGIT_CHANNEL = 'beta'
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "irm https://igit.yelog.org/install.ps1.txt -ErrorAction Stop | iex"
```

For the stable channel, set `$env:IGIT_CHANNEL = 'stable'` instead. PowerShell 7
can use `pwsh` in place of `powershell.exe`. Open a new terminal after installing
so it sees the updated user PATH.

To inspect the script or supply options explicitly:

```powershell
Invoke-WebRequest https://igit.yelog.org/install.ps1.txt -OutFile install-igit.ps1
Get-Content ./install-igit.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./install-igit.ps1 -Channel beta -NoModifyPath
```

The default root is `%LOCALAPPDATA%\IGit`; its `bin` directory contains the
`igit.cmd` and `ig.cmd` launchers and their PowerShell helpers. `-InstallRoot PATH` and `-InstallDir PATH`
override those locations. Releases retain their own EXEs, so an update does
not overwrite an executable that is still running. `-NoModifyPath` skips the
user PATH update; add the command directory yourself.

## Updating

Native installations remember the active release channel. These commands run
before TUI startup and do not load workspace configuration or modify Git data:

```sh
igit update --check
igit update
ig update
igit update --check --json
igit update --channel beta
igit update --channel stable
```

Use `--allow-downgrade` to opt into a lower version, including a channel switch
whose newest version is older than the installed one. Without this flag, a
same-channel check never downgrades and a lower-version channel switch fails.
The TUI checks for updates in the background 1.5 seconds after its first
workspace frame. Successful checks are cached for 24 hours across workspaces;
automatic failures stay quiet. Installation always requires your confirmation.
Configure or disable startup checks in [settings](configuration.md#update-checks).

Press `F9` in normal mode, click the bottom-left version/update label, or choose
**Update IGit** in Actions. Update Center shows the running version, channel,
available version, and installation result. **Not now** starts focused when an
update or restart is offered. `Tab` selects another button, `Enter` activates
it, and `Esc` closes the window. Buttons and shortcut hints are clickable.
`r` checks again after a current or failed check; `j/k` or the wheel scrolls
details.

Native copies offer **Update**, retaining their channel and validating the
exact reviewed version. The dialog shows download progress and the installation
stage. **Run in background** closes the dialog while the update continues.
Once complete, choose **Restart now** or relaunch later; finish text editing
and remote operations before restarting. Restart reuses the original command
arguments. Cargo/manual copies check their running version's stable/beta channel
and offer **How to update** when a newer release is available. Update these copies
using their original installation method.

**The published `v0.1.0-beta.1` predates the built-in update command.** Rerun the
appropriate installation command above to update that version. Once a later
release includes the command, both aliases support it. Re-running an installer
with `--check --json` on Unix or `-Check -Json` on Windows checks without writing;
omit the check flag to install the available version.

JSON reports use `schema: 1` and `product: "igit"`, with `channel`,
`current_version`, `target_version`, and `status`. Native statuses are
`installed`, `updated`, `up_to_date`, and `update_available`. Validation or
download errors return a nonzero exit code and `status: "error"`. A command
installed through Cargo or copied manually reports `manager_action_required`;
`--check` succeeds, while an attempted update fails without replacing files.
Update those installations with their original method.

`update` is reserved as the first command-line argument. To open a workspace
directory named `update`, use `igit ./update` or its absolute path.

## Verification and recovery

The installer selects the OS/CPU archive from the requested channel, checks its
SHA-256, rejects unsafe archive paths and links, and verifies the versions of
both commands before activating them together. It retains the previous version.
A failed download, extraction, or version check leaves the old selection active.
An existing unowned command, changed launcher, or modified installed file stops
the update rather than being overwritten. Use a different command directory or
resolve the existing installation through its owner before retrying.

If an installer is interrupted and reports `.install.lock`, first make sure no
other installation is running. Inspect the lock and installation state before
removing that stale lock. The installer never steals an existing lock.

Check the selected commands with `command -v igit ig` on Unix or
`Get-Command igit, ig` in PowerShell, then run `igit --version` and `ig --version`.
Another directory earlier on PATH may still contain an older command.

For offline/manual installation, download the matching archive and `SHA256SUMS`
from [GitHub Releases](https://github.com/yelog/igit/releases), verify its
checksum, then place both commands on PATH. Manual copies are not native-managed
installations. See [Releasing](releasing.md) for archive details and maintainer
instructions for deploying `igit.yelog.org`.
