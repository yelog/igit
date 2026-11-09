//! Structured access to the same installer used by `igit update`.
use super::{clean_environment, helper, native_root, progress, Options, Progress};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Output};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    UpToDate,
    UpdateAvailable,
    ManagerActionRequired,
    Updated,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Inspection {
    pub schema: u32,
    pub product: String,
    pub channel: String,
    pub current_version: String,
    pub target_version: String,
    pub status: Status,
    pub action: Option<String>,
}

impl Inspection {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.product == "igit",
            "Invalid update report identity"
        );
        ensure!(
            self.current_version == env!("CARGO_PKG_VERSION"),
            "Update report belongs to a different running version"
        );
        let version = regex_lite::Regex::new(
            r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.[1-9][0-9]*)?$",
        )?;
        ensure!(
            version.is_match(&self.target_version),
            "Invalid update target version"
        );
        ensure!(
            self.channel
                == if self.target_version.contains('-') {
                    "beta"
                } else {
                    "stable"
                },
            "Update report channel mismatch"
        );
        Ok(())
    }
}

fn decode(output: Output) -> Result<Inspection> {
    if !output.status.success() {
        let report = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
        let reason = report
            .as_ref()
            .and_then(|value| value["error"].as_str())
            .filter(|message| !message.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned());
        anyhow::bail!(
            "{}",
            if reason.is_empty() {
                "Update helper failed"
            } else {
                &reason
            }
        );
    }
    let inspection: Inspection =
        serde_json::from_slice(&output.stdout).context("Invalid update helper report")?;
    inspection.validate()?;
    Ok(inspection)
}

fn execute(options: &Options, notify: Option<&mut dyn FnMut(Progress)>) -> Result<Inspection> {
    let executable = std::env::current_exe()
        .context("Locate the running executable")?
        .canonicalize()
        .context("Resolve the running executable")?;
    let temporary = tempfile::tempdir().context("Create an isolated update helper")?;
    let mut command = helper(temporary.path(), &executable, options)?;
    clean_environment(&mut command);
    let output = match notify {
        Some(notify) => progress::capture(&mut command, temporary.path(), notify)?,
        None => command.output().context("Start the update helper")?,
    };
    decode(output)
}

pub(crate) fn inspect() -> Result<Inspection> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let inspection = execute(
        &Options {
            check: true,
            json: true,
            unmanaged: native_root(&executable).is_none(),
            ..Options::default()
        },
        None,
    )?;
    ensure!(
        inspection.status != Status::Updated,
        "Check unexpectedly installed an update"
    );
    Ok(inspection)
}

pub(crate) fn install(
    reviewed: &Inspection,
    mut notify: impl FnMut(Progress),
) -> Result<Inspection> {
    reviewed.validate()?;
    ensure!(
        reviewed.status == Status::UpdateAvailable,
        "No native update was reviewed"
    );
    let executable = std::env::current_exe()?.canonicalize()?;
    ensure!(
        native_root(&executable).is_some(),
        "Use the original installation method to update this executable"
    );
    let inspection = execute(
        &Options {
            json: true,
            channel: Some(reviewed.channel.clone()),
            version: Some(reviewed.target_version.clone()),
            ..Options::default()
        },
        Some(&mut notify),
    )?;
    ensure!(
        inspection.target_version == reviewed.target_version
            && inspection.channel == reviewed.channel
            && matches!(inspection.status, Status::Updated | Status::UpToDate),
        "Installation result differs from the reviewed update"
    );
    Ok(inspection)
}

pub(crate) fn restart_path(inspection: &Inspection) -> Result<PathBuf> {
    inspection.validate()?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let root = native_root(&executable)
        .context("Native installation unavailable; relaunch IGit manually")?;
    #[cfg(unix)]
    let active = root.join("current").join(executable.file_name().unwrap());
    #[cfg(windows)]
    let active = root
        .join("releases")
        .join(std::fs::read_to_string(root.join("current.txt"))?.trim())
        .join(executable.file_name().unwrap());
    let active = active
        .canonicalize()
        .context("Updated executable unavailable")?;
    ensure!(
        active.parent().and_then(|path| path.parent()) == Some(root.join("releases").as_path())
            && active
                .parent()
                .and_then(|path| path.file_name())
                .is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with(&format!("{}_", inspection.target_version))
                }),
        "Active installation changed; relaunch IGit manually"
    );
    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn report(status: Status) -> Inspection {
        Inspection {
            schema: 1,
            product: "igit".into(),
            channel: "beta".into(),
            current_version: env!("CARGO_PKG_VERSION").into(),
            target_version: "0.2.0-beta.1".into(),
            status,
            action: None,
        }
    }

    #[test]
    fn reports_reject_wrong_identity_version_and_channel() {
        let valid = report(Status::UpdateAvailable);
        valid.validate().unwrap();
        for invalid in [
            Inspection {
                schema: 2,
                ..valid.clone()
            },
            Inspection {
                product: "lazydb".into(),
                ..valid.clone()
            },
            Inspection {
                current_version: "9.9.9".into(),
                ..valid.clone()
            },
            Inspection {
                target_version: "../release".into(),
                ..valid.clone()
            },
            Inspection {
                channel: "stable".into(),
                ..valid
            },
        ] {
            assert!(invalid.validate().is_err(), "{invalid:?}");
        }
    }
}
