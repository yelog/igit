use super::*;
use crate::core::revision;
use anyhow::{ensure, Context as _};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::{Command, Output},
};

fn reader(repo: &Path) -> Command {
    let mut command = crate::core::command(repo);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_LITERAL_PATHSPECS", "1");
    command
}

fn checked(output: Output) -> Result<Vec<u8>> {
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

pub(super) fn files(repo: &Path, oid: &str) -> Result<Vec<model::File>> {
    let temporary = tempfile::tempdir()?;
    let index = temporary.path().join("index");
    let private = || {
        let mut command = reader(repo);
        command
            .env("GIT_INDEX_FILE", &index)
            .args([
                "-c",
                "core.sparseCheckout=false",
                "-c",
                "index.sparse=false",
            ])
            .arg("-c")
            .arg(format!("core.hooksPath={}", temporary.path().display()));
        command
    };
    checked(private().args(["read-tree", oid]).output()?)?;
    let base = checked(private().args(["ls-files", "--cached", "-z"]).output()?)?;
    let base: BTreeSet<_> = base
        .split(|&byte| byte == 0)
        .filter(|path| !path.is_empty())
        .collect();
    let changed = checked(
        private()
            .args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--ignore-submodules=dirty",
                "--name-status",
                "-z",
                oid,
                "--",
            ])
            .output()?,
    )?;
    let mut fields = changed.split(|&byte| byte == 0);
    let mut files = BTreeMap::new();
    while let Some(status) = fields.next().filter(|field| !field.is_empty()) {
        let raw = fields.next().context("Incomplete workspace diff path")?;
        files.insert(
            raw.to_vec(),
            model::File {
                raw: raw.to_vec(),
                status: model::plain(status),
                parent: None,
            },
        );
    }
    let current = checked(
        reader(repo)
            .args([
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ])
            .output()?,
    )?;
    for raw in current
        .split(|&byte| byte == 0)
        .filter(|path| !path.is_empty() && !base.contains(path))
    {
        let path = model::path_from_bytes(raw)?;
        if fs::symlink_metadata(repo.join(path)).is_ok() {
            files.entry(raw.to_vec()).or_insert_with(|| model::File {
                raw: raw.to_vec(),
                status: "A".into(),
                parent: None,
            });
        }
    }
    Ok(files.into_values().collect())
}

pub(super) fn diff(repo: &Path, oid: &str, file: &model::File) -> Result<DiffView> {
    let path = model::path_from_bytes(&file.raw)?;
    let name = model::plain(&file.raw);
    let sources = (|| -> Result<_> {
        let before = revision::source(repo, oid, path)?;
        let entries = checked(
            reader(repo)
                .args(["ls-files", "--stage", "-z", "--"])
                .arg(path)
                .output()?,
        )?;
        let mode = entries.split(|&byte| byte == 0).find_map(|entry| {
            let tab = entry.iter().position(|&byte| byte == b'\t')?;
            (&entry[tab + 1..] == file.raw.as_slice()).then_some(&entry[..tab])
        });
        let mode = mode
            .map(std::str::from_utf8)
            .transpose()?
            .and_then(|metadata| metadata.split(' ').next());
        let after = revision::working(repo, path, mode.is_none(), mode)?;
        Ok([before, after])
    })();
    let mut diff = DiffView {
        name: name.clone(),
        ..Default::default()
    };
    match sources {
        Ok(sources) => {
            let annotations = sources.each_ref().map(|source| source.description.clone());
            diff.comparison = Some(crate::merge::RevisionDiff::working_tree(
                name,
                oid.to_owned(),
                sources.map(|source| source.text),
                annotations,
            ));
        }
        Err(error) => {
            let explanation = format!("{error:#}")
                .replace("; showing the bounded Git patch", "")
                .replace("; showing the Git patch", "")
                .replace(", showing the Git patch", "");
            diff.lines = vec![
                format!("Cannot preview {name}"),
                explanation,
                String::new(),
                format!("Commit: {}", &oid[..8.min(oid.len())]),
                "Working tree comparison is read-only.".into(),
            ];
        }
    }
    Ok(diff)
}
