//! Historical comparisons use immutable trees, never the index or working file.
use super::{git_bytes, path_from_bytes, plain, File, Parent, Rev};
use crate::core::revision::{self, Source};
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

fn source(repo: &Path, oid: &str, raw: &[u8]) -> Result<Source> {
    revision::source(repo, oid, path_from_bytes(raw)?)
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
        let old_path = rev.file.as_ref().and_then(|f| f.before.as_deref());
        source(repo, parent, old_path.unwrap_or(&file.raw))?
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
