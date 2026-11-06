//! Historical comparisons use immutable trees, never the index or working file.
use super::{git_bytes, path_from_bytes, plain, File, Parent, Rev};
use anyhow::{bail, Context, Result};
use std::path::Path;

pub(in crate::log) fn changed_files(path: &Path, rev: &Rev) -> Result<(Vec<File>, Vec<Parent>)> {
    let read = |args: &[&str], parent| -> Result<Vec<File>> {
        let raw = git_bytes(path, args)?;
        let mut fields = raw.split(|&b| b == 0);
        let mut files = Vec::new();
        while let Some(status) = fields.next().filter(|s| !s.is_empty()) {
            let name = fields.next().context("incomplete changed-file record")?;
            files.push(File {
                raw: name.to_vec(),
                status: plain(status),
                parent,
            });
        }
        Ok(files)
    };
    let mut files = if rev.parents.len() == 1 {
        read(
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--name-status",
                "-z",
                &rev.parents[0],
                &rev.oid,
                "--",
            ],
            None,
        )?
    } else {
        // Combined diffs list paths changed against *every* parent. Parent-only
        // changes remain accessible in the individual groups below.
        read(
            &[
                "diff-tree",
                "--root",
                "-r",
                "-c",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--name-status",
                "-z",
                "--no-commit-id",
                &rev.oid,
                "--",
            ],
            None,
        )?
    };
    let mut parents = Vec::new();
    if rev.parents.len() > 1 {
        for (index, oid) in rev.parents.iter().enumerate() {
            let start = files.len();
            files.extend(read(
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    "--name-status",
                    "-z",
                    oid,
                    &rev.oid,
                    "--",
                ],
                Some(index),
            )?);
            let subject = git_bytes(
                path,
                &[
                    "show",
                    "-s",
                    "--no-show-signature",
                    "--no-notes",
                    "--format=%s",
                    oid,
                    "--",
                ],
            )?;
            parents.push(Parent {
                oid: oid.clone(),
                subject: plain(subject.strip_suffix(b"\n").unwrap_or(&subject)),
                files: start..files.len(),
            });
        }
    }
    Ok((files, parents))
}

pub(in crate::log) struct Source {
    pub text: String,
    pub description: String,
}

fn source(repo: &Path, oid: &str, raw: &[u8]) -> Result<Source> {
    let path = path_from_bytes(raw)?;
    let output = crate::core::command(repo)
        .args(["--literal-pathspecs", "ls-tree", "-z", oid, "--"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        bail!("Cannot read revision tree {oid}");
    }
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

pub(in crate::log) fn sources(repo: &Path, rev: &Rev, file: &File) -> Result<[Source; 3]> {
    if rev.parents.len() != 2 {
        bail!("Three-pane preview requires two parents; use the individual parent comparisons");
    }
    Ok([
        source(repo, &rev.parents[0], &file.raw)?,
        source(repo, &rev.oid, &file.raw)?,
        source(repo, &rev.parents[1], &file.raw)?,
    ])
}

pub(in crate::log) fn pair(repo: &Path, rev: &Rev, file: &File) -> Result<[Source; 2]> {
    let before = if let Some(parent) = rev.parents.first() {
        source(repo, parent, &file.raw)?
    } else {
        Source {
            text: String::new(),
            description: "No parent · initial commit".into(),
        }
    };
    Ok([before, source(repo, &rev.oid, &file.raw)?])
}

#[cfg(test)]
pub(in crate::log) mod tests;
