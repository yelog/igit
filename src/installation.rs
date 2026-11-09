//! Explicit software updates are separate from workspace Pull/Fetch operations.
mod inspection;
mod progress;
use anyhow::{bail, Context, Result};
pub(crate) use inspection::{inspect, install, restart_path, Inspection, Status};
pub(crate) use progress::Progress;
use std::{ffi::OsString, fs, path::Path, process::Command};

const HELP: &str = "Usage: igit update [OPTIONS]  (alias: ig update)

  --check                    check without installing
  --channel stable|beta      select the release channel explicitly
  --allow-downgrade          allow an explicit downgrade
  --json                     print one machine-readable report
  -h, --help                 show this help

Native installations retain their channel. Cargo and manual installations must
be updated using their original installation method.
Installation guide: https://igit.yelog.org/";

#[derive(Default)]
struct Options {
    check: bool,
    channel: Option<String>,
    downgrade: bool,
    json: bool,
    help: bool,
    unmanaged: bool,
    version: Option<String>,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--check") => options.check = true,
            Some("--json") => options.json = true,
            Some("--allow-downgrade") => options.downgrade = true,
            Some("--help" | "-h") => options.help = true,
            Some("--channel") => {
                options.channel = Some(
                    args.next()
                        .and_then(|value| value.into_string().ok())
                        .filter(|value| matches!(value.as_str(), "stable" | "beta"))
                        .context("--channel requires stable or beta")?,
                );
            }
            _ => bail!(
                "Unknown update option: {}\n{HELP}",
                argument.to_string_lossy()
            ),
        }
    }
    Ok(options)
}

fn native_root(executable: &Path) -> Option<&Path> {
    let releases = executable.parent()?.parent()?;
    (releases.file_name()? == "releases")
        .then(|| releases.parent())
        .flatten()
        .filter(|root| root.join("install.json").is_file())
}

pub(crate) fn run(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let options = parse(args)?;
    if options.help {
        println!("{HELP}");
        return Ok(());
    }
    let executable = std::env::current_exe().context("locate the running IGit executable")?;
    #[cfg(unix)]
    let executable = executable
        .canonicalize()
        .context("resolve the native launcher")?;
    if native_root(&executable).is_none() {
        let action = "This executable is not managed by the IGit installer. Update it with its original package manager, rebuild it, or use https://igit.yelog.org/.";
        if options.json {
            println!(
                "{}",
                serde_json::json!({
                    "schema": 1, "product": "igit", "status": "manager_action_required",
                    "current_version": env!("CARGO_PKG_VERSION"), "action": action,
                })
            );
        } else {
            println!("{action}");
        }
        if !options.check {
            bail!("No files were changed; use the installation's owning manager")
        }
        return Ok(());
    }
    let temporary = tempfile::tempdir().context("create an isolated update helper")?;
    let mut command = helper(temporary.path(), &executable, &options)?;
    // Installed ownership determines the paths and default channel. Ambient
    // installer variables must not redirect an update to another installation.
    clean_environment(&mut command);
    let status = command.status().context("start the native update helper")?;
    if !status.success() {
        bail!("IGit update failed; see the installer report above")
    }
    Ok(())
}

fn clean_environment(command: &mut Command) {
    for name in [
        "IGIT_INSTALL_DIR",
        "IGIT_INSTALL_ROOT",
        "IGIT_VERSION",
        "IGIT_CHANNEL",
        "IGIT_CHANNEL_LOCKED",
    ] {
        command.env_remove(name);
    }
}

#[cfg(unix)]
fn helper(directory: &Path, executable: &Path, options: &Options) -> Result<Command> {
    let script = directory.join("install.py");
    fs::write(&script, include_str!("../scripts/install/install.py"))?;
    let mut command = Command::new("python3");
    command
        .arg(&script)
        .args(["--running-version", env!("CARGO_PKG_VERSION")]);
    if options.unmanaged {
        command.arg("--unmanaged");
    } else {
        command.arg("--from-executable").arg(executable);
    }
    for (enabled, flag) in [
        (options.check, "--check"),
        (options.json, "--json"),
        (options.downgrade, "--allow-downgrade"),
    ] {
        if enabled {
            command.arg(flag);
        }
    }
    if let Some(channel) = &options.channel {
        command.args(["--channel", channel]);
    }
    if let Some(version) = &options.version {
        command.args(["--version", version]);
    }
    Ok(command)
}

#[cfg(windows)]
fn helper(directory: &Path, executable: &Path, options: &Options) -> Result<Command> {
    let script = directory.join("install.ps1");
    fs::write(&script, include_str!("../pages/install.ps1"))?;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(["-RunningVersion", env!("CARGO_PKG_VERSION")]);
    if options.unmanaged {
        command.arg("-Unmanaged");
    } else {
        command.arg("-FromExecutable").arg(executable);
    }
    for (enabled, flag) in [
        (options.check, "-Check"),
        (options.json, "-Json"),
        (options.downgrade, "-AllowDowngrade"),
    ] {
        if enabled {
            command.arg(flag);
        }
    }
    if let Some(channel) = &options.channel {
        command.args(["-Channel", channel]);
    }
    if let Some(version) = &options.version {
        command.args(["-Version", version]);
    }
    Ok(command)
}
