//! Read committed file revisions without involving the working tree or index.
use super::{command, git_bytes};
use anyhow::{bail, Context as _, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

pub(crate) struct Source {
    pub text: String,
    pub description: String,
}

pub(crate) fn source(repo: &Path, oid: &str, path: &Path) -> Result<Source> {
    let output = command(repo)
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
        return Ok(Source {
            text: String::new(),
            description: "Absent in this revision".into(),
        });
    };
    let meta = std::str::from_utf8(entry)?;
    let mut fields = meta.split(' ');
    let mode = fields.next().context("missing tree mode")?;
    let kind = fields.next().context("missing object type")?;
    let blob = fields.next().context("missing object ID")?;
    if kind != "blob" {
        bail!("Split preview is unavailable for {kind} entries; showing the Git patch");
    }
    let size = git_bytes(repo, &["cat-file", "-s", blob])?;
    let size: usize = std::str::from_utf8(&size)?.trim().parse()?;
    if size > 512 * 1024 {
        bail!("Split preview is limited to 512 KiB per revision; showing the Git patch");
    }
    let bytes = git_bytes(repo, &["cat-file", "blob", blob])?;
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        bail!("Binary or non-UTF-8 content; showing the Git patch");
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
    Ok(Source {
        text,
        description: format!("{mode} · {ending}"),
    })
}

pub(crate) fn read_patch(mut cmd: Command) -> Result<(Vec<u8>, bool)> {
    const LIMIT: usize = 512 * 1024;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .context("git diff")?;
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
        bail!("git diff failed ({status})")
    }
    raw.truncate(LIMIT);
    Ok((raw, cut))
}
