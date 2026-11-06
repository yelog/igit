//! Saved paths must not restore files inside a different working tree.
use super::*;
use std::{collections::BTreeSet, ffi::OsString, fs, path::Component};

fn path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec()).into()
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).into_owned()).into()
    }
}

pub(super) fn paths(repo: &Path, oid: &str, restore_index: bool) -> Result<Vec<PathBuf>> {
    let base = format!("{oid}^1");
    let mut names = BTreeSet::new();
    let mut trees = vec![oid.to_string()];
    if restore_index {
        trees.push(format!("{oid}^2"));
    }
    for tree in trees {
        let bytes = git_bytes(
            repo,
            &[
                "diff-tree",
                "--no-commit-id",
                "-r",
                "--name-only",
                "-z",
                "--no-renames",
                "--ignore-submodules=none",
                &base,
                &tree,
            ],
        )?;
        names.extend(bytes.split(|b| *b == 0).filter(|p| !p.is_empty()).map(path));
    }
    if let Some(untracked) = optional(repo, &["rev-parse", "--verify", "-q", &format!("{oid}^3")])?
    {
        let bytes = git_bytes(repo, &["ls-tree", "-r", "--name-only", "-z", &untracked])?;
        names.extend(bytes.split(|b| *b == 0).filter(|p| !p.is_empty()).map(path));
    }
    Ok(names.into_iter().collect())
}

pub(super) fn check(repo: &Path, paths: &[PathBuf], index: &[u8]) -> Result<()> {
    let modules: BTreeSet<_> = index
        .split(|b| *b == 0)
        .filter(|record| record.starts_with(b"160000 "))
        .filter_map(|record| {
            record
                .iter()
                .position(|b| *b == b'\t')
                .map(|tab| path(&record[tab + 1..]))
        })
        .collect();
    for path in paths {
        let parts: Vec<_> = path.components().collect();
        ensure!(
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|part| matches!(part, Component::Normal(_))),
            "Invalid saved path"
        );
        ensure!(
            !parts.iter().any(|part| part
                .as_os_str()
                .as_encoded_bytes()
                .eq_ignore_ascii_case(b".git")),
            "Saved paths cannot enter a Git directory"
        );
        ensure!(
            !path.ancestors().skip(1).any(|parent| modules.contains(parent)),
            "Saved path {} is inside a submodule; apply this stash outside IGit after reviewing repository boundaries", model::label(&path.to_string_lossy())
        );
        let mut current = repo.to_path_buf();
        for (index, part) in parts.iter().enumerate() {
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(meta) => {
                    ensure!(!meta.is_dir() || fs::symlink_metadata(current.join(".git")).is_err(),
                        "Saved path {} enters another repository; apply this stash outside IGit after reviewing repository boundaries", model::label(&path.to_string_lossy()));
                    ensure!(
                        !meta.is_symlink() || index + 1 == parts.len(),
                        "Saved path {} passes through a directory symlink; handle it outside IGit",
                        model::label(&path.to_string_lossy())
                    );
                    if !meta.is_dir() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}
