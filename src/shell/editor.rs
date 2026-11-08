//! Working-file editing is a terminal/host handoff, never a Git index write.
use crate::core;
use anyhow::{bail, ensure, Context as _, Result};
use serde::Serialize;
use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const NVIM_SESSION: &str = "IGIT_NVIM_SESSION";

#[derive(Debug)]
pub(super) struct Request {
    pub repo: PathBuf,
    pub path: PathBuf,
}

pub(super) enum Prepared {
    Neovim(String),
    External(Box<Command>),
}

impl Request {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.path.is_absolute()
                && self.path.starts_with(&self.repo)
                && !self
                    .path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir)),
            "Choose a file inside the selected repository"
        );
        let metadata = self
            .path
            .metadata()
            .context("Working file is unavailable; deleted files cannot be edited")?;
        ensure!(
            metadata.is_file(),
            "Choose a working file, not a directory or submodule"
        );
        Ok(())
    }

    pub fn prepare(&self, session: Option<&str>) -> Result<Prepared> {
        self.validate()?;
        if let Some(session) = session.filter(|s| !s.is_empty()) {
            #[derive(Serialize)]
            struct Edit<'a> {
                version: u8,
                session: &'a str,
                path: &'a str,
            }
            let payload = serde_json::to_string(&Edit {
                version: 1,
                session,
                path: self
                    .path
                    .to_str()
                    .context("Neovim editing requires a UTF-8 file name")?,
            })?;
            return Ok(Prepared::Neovim(format!("\x1b]51;igit-edit;{payload}\x07")));
        }
        let editor = configured_editor(&self.repo, std::env::var_os("GIT_EDITOR").as_deref())?;
        Ok(Prepared::External(Box::new(self.command(editor))))
    }

    fn command(&self, mut editor: OsString) -> Command {
        // Git editor commands are shell expressions. Only the trusted editor
        // expression enters shell code; the literal file is a positional argument.
        editor.push(" \"$@\"");
        let mut command = Command::new(shell(&self.repo));
        command
            .args([OsStr::new("-c"), &editor, OsStr::new("igit-editor")])
            .arg(&self.path)
            .current_dir(&self.repo)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        command
    }
}

fn configured_editor(repo: &Path, inherited: Option<&OsStr>) -> Result<OsString> {
    let mut command = core::command(repo);
    // Shared Git operations deliberately set GIT_EDITOR=true. Restore the
    // caller's value here so git var can apply Git's real editor precedence.
    command.env_remove("GIT_EDITOR");
    if let Some(value) = inherited {
        command.env("GIT_EDITOR", value);
    }
    let output = command
        .args(["var", "GIT_EDITOR"])
        .output()
        .context("Cannot query the configured editor")?;
    if !output.status.success() {
        bail!("No editor available; configure core.editor, VISUAL, or EDITOR");
    }
    let mut bytes = output.stdout;
    while matches!(bytes.last(), Some(b'\r' | b'\n')) {
        bytes.pop();
    }
    ensure!(!bytes.is_empty(), "Configured editor must not be empty");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    Ok(OsString::from(String::from_utf8(bytes)?))
}

fn shell(_repo: &Path) -> OsString {
    // Git for Windows ships sh, but its bin directory is not always on PATH.
    #[cfg(windows)]
    if let Ok(path) = core::git(_repo, &["--exec-path"]) {
        for ancestor in Path::new(&path).ancestors() {
            let candidate = ancestor.join("bin/sh.exe");
            if candidate.is_file() {
                return candidate.into_os_string();
            }
        }
    }
    "sh".into()
}

pub(super) fn send_request(out: &mut impl Write, sequence: &str) -> Result<()> {
    out.write_all(sequence.as_bytes())?;
    out.flush()?;
    Ok(())
}

pub(super) fn execute(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .context("Cannot start the configured editor")?;
    ensure!(status.success(), "Editor exited with {status}");
    Ok(())
}

#[cfg(test)]
mod tests;
