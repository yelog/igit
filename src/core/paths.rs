use super::{git, git_bytes};
use anyhow::{ensure, Result};
use std::{
    collections::BTreeSet,
    ops::Bound::{Included, Unbounded},
    path::Path,
};

fn key(path: &[u8], ignore_case: bool) -> Vec<u8> {
    if !ignore_case {
        return path.to_owned();
    }
    std::str::from_utf8(path).map_or_else(
        |_| path.to_ascii_lowercase(),
        |path| path.to_lowercase().into_bytes(),
    )
}

fn overlaps(paths: &BTreeSet<Vec<u8>>, path: &[u8]) -> bool {
    if path
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'/')
        .map(|(index, _)| &path[..index])
        .chain([path])
        .any(|prefix| paths.contains(prefix))
    {
        return true;
    }
    let prefix = [path, b"/"].concat();
    paths
        .range::<[u8], _>((Included(prefix.as_slice()), Unbounded))
        .next()
        .is_some_and(|name| name.starts_with(&prefix))
}

pub(crate) fn protect(repo: &Path, span: &str, single: bool) -> Result<()> {
    let others = git_bytes(
        repo,
        &[
            "ls-files",
            "--others",
            "--directory",
            "--no-empty-directory",
            "-z",
        ],
    )?;
    let indexed = git_bytes(repo, &["ls-files", "-v", "-z"])?;
    let ignore_case = cfg!(any(target_os = "macos", windows))
        || git(repo, &["config", "--bool", "--get", "core.ignoreCase"])
            .is_ok_and(|value| value == "true");
    let paths = others
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| path.strip_suffix(b"/").unwrap_or(path))
        .chain(
            indexed
                .split(|byte| *byte == 0)
                .filter_map(|entry| match entry {
                    [b'S' | b'a'..=b'z', b' ', path @ ..] => Some(path),
                    _ => None,
                }),
        )
        .map(|path| key(path, ignore_case))
        .collect::<BTreeSet<_>>();
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec![
        "log",
        "--format=",
        "--no-show-signature",
        "--no-notes",
        "--no-ext-diff",
        "--no-renames",
        "--ignore-submodules=none",
        "--diff-merges=first-parent",
        "--name-only",
        "--root",
        "-z",
    ];
    if single {
        args.push("-1");
    }
    args.extend([span, "--"]);
    let output = super::update::published_graph(repo).args(args).output()?;
    ensure!(
        output.status.success(),
        "Cannot inspect paths in the stored history"
    );
    let touched = output.stdout;
    for path in touched
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let display = String::from_utf8_lossy(path)
            .chars()
            .flat_map(|character| {
                if character.is_control() {
                    character.escape_default().collect::<Vec<_>>()
                } else {
                    vec![character]
                }
            })
            .collect::<String>();
        ensure!(
            !overlaps(&paths, &key(path, ignore_case)),
            "Local file may be overwritten at {}; move it aside or check its index flags",
            display
        );
    }
    Ok(())
}
