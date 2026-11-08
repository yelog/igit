//! Read-only fingerprints shared by whole-file write reviews.
use super::{checked, cmd, txt};
use anyhow::{bail, ensure, Result};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(PartialEq, Eq)]
pub(super) enum Working {
    Missing,
    File { hash: String, mode: u32 },
    Symlink(PathBuf),
}

pub(super) fn read(repo: &Path, path: &Path) -> Result<Working> {
    Ok(read_many(repo, &[path])?.remove(0))
}

pub(super) fn read_many(repo: &Path, paths: &[&Path]) -> Result<Vec<Working>> {
    let mut values = paths
        .iter()
        .map(|path| metadata(repo, path))
        .collect::<Result<Vec<_>>>()?;
    let files: Vec<_> = paths
        .iter()
        .enumerate()
        .filter(|(i, _)| matches!(values[*i], Working::File { .. }))
        .collect();
    let mut remaining = files.as_slice();
    while !remaining.is_empty() {
        // Bound native argv size as well as file count, including long Windows paths.
        let mut bytes = 0;
        let count = remaining
            .iter()
            .take(128)
            .take_while(|(_, path)| {
                bytes += path.as_os_str().as_encoded_bytes().len() * 2 + 2;
                bytes <= 16 * 1024
            })
            .count()
            .max(1);
        let (batch, rest) = remaining.split_at(count);
        let hashes = txt(checked(
            cmd(repo, None)
                .args(["hash-object", "--no-filters", "--"])
                .args(batch.iter().map(|(_, path)| path.as_os_str()))
                .output()?,
        )?);
        let hashes: Vec<_> = hashes.lines().collect();
        ensure!(
            hashes.len() == batch.len(),
            "Git returned incomplete working-file fingerprints"
        );
        for ((index, _), value) in batch.iter().zip(hashes) {
            if let Working::File { hash, .. } = &mut values[*index] {
                *hash = value.into();
            }
        }
        remaining = rest;
    }
    Ok(values)
}

fn metadata(repo: &Path, path: &Path) -> Result<Working> {
    // Never follow a directory symlink or enter another working tree. Leaf
    // symlinks are safe: Git replaces the link, and remove_file unlinks it.
    let mut parent = repo.to_path_buf();
    let parts: Vec<_> = path.components().collect();
    if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
        bail!("Invalid working-tree path")
    }
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(m) if m.is_dir() && !parent.join(".git").exists() => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => bail!(
                "Unsafe parent of {}; handle file/directory replacements externally",
                path.display()
            ),
        }
    }
    let file = repo.join(path);
    let meta = match fs::symlink_metadata(&file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Working::Missing),
        Err(e) => return Err(e.into()),
    };
    if meta.is_symlink() {
        return Ok(Working::Symlink(fs::read_link(file)?));
    }
    if !meta.is_file() {
        bail!("{} is a directory or special file; handle file/directory replacements and nested repositories externally", path.display())
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = u32::from(meta.permissions().readonly());
    Ok(Working::File {
        hash: String::new(),
        mode,
    })
}
