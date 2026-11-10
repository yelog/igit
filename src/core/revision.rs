//! Read bounded text revisions without filters or writes.
use super::command;
use anyhow::{bail, Context as _, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

pub(crate) const LIMIT: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub text: String,
    pub description: String,
}

pub(crate) fn source(repo: &Path, oid: &str, path: &Path) -> Result<Source> {
    Ok(file(repo, oid, path)?.unwrap_or_else(|| Source {
        text: String::new(),
        description: "Absent in this revision".into(),
    }))
}

pub(crate) fn file(repo: &Path, oid: &str, path: &Path) -> Result<Option<Source>> {
    let output = reader(repo)
        .args(["--literal-pathspecs", "ls-tree", "-z", oid, "--"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        bail!("Cannot read revision tree {oid}");
    }
    let raw = path.as_os_str().as_encoded_bytes();
    let entry = output.stdout.split(|&b| b == 0).find_map(|entry| {
        let tab = entry.iter().position(|&b| b == b'\t')?;
        (entry[tab + 1..] == *raw).then_some(&entry[..tab])
    });
    let Some(entry) = entry else {
        return Ok(None);
    };
    let meta = std::str::from_utf8(entry)?;
    let mut fields = meta.split(' ');
    let mode = fields.next().context("missing tree mode")?;
    let kind = fields.next().context("missing object type")?;
    let blob = fields.next().context("missing object ID")?;
    if kind != "blob" {
        bail!("File revision is unavailable for {kind} entries");
    }
    self::blob(repo, blob, mode).map(Some)
}

fn reader(repo: &Path) -> Command {
    let mut cmd = command(repo);
    cmd.env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1");
    cmd
}

pub(crate) fn blob(repo: &Path, oid: &str, mode: &str) -> Result<Source> {
    mode_supported(mode)?;
    let mut cmd = reader(repo);
    cmd.args(["cat-file", "blob", oid]);
    let (bytes, cut) = read_output(cmd, "git cat-file")?;
    if cut {
        bail!("File revision is limited to 512 KiB");
    }
    text(bytes, mode)
}

fn mode_supported(mode: &str) -> Result<()> {
    if !matches!(mode, "100644" | "100755" | "120000") {
        bail!("File revision is unavailable for mode {mode} (submodule or special entry)");
    }
    Ok(())
}

pub(crate) fn text(bytes: Vec<u8>, mode: &str) -> Result<Source> {
    mode_supported(mode)?;
    if bytes.len() > LIMIT {
        bail!("File revision is limited to 512 KiB");
    }
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        bail!("Binary or non-UTF-8 content");
    }
    let text = String::from_utf8(bytes)?;
    let ending = if text.is_empty() {
        "Empty file"
    } else if !text.ends_with('\n') {
        "No newline at end of file"
    } else if text.contains("\r\n") {
        "CRLF"
    } else {
        "LF"
    };
    let link = if mode == "120000" {
        " · Symlink (link text)"
    } else {
        ""
    };
    Ok(Source {
        text,
        description: format!("{mode}{link} · {ending}"),
    })
}

pub(crate) fn read_patch(cmd: Command) -> Result<(Vec<u8>, bool)> {
    read_output(cmd, "git diff")
}

fn read_output(mut cmd: Command, operation: &str) -> Result<(Vec<u8>, bool)> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| operation.to_owned())?;
    let mut raw = vec![];
    let result = child
        .stdout
        .take()
        .unwrap()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut raw);
    let cut = raw.len() > LIMIT;
    if cut || result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    if !cut && !status.success() {
        bail!("{operation} failed ({status})")
    }
    raw.truncate(LIMIT);
    Ok((raw, cut))
}
