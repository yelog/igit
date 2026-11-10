use super::Action;
use crate::core::git;
use anyhow::Result;
use std::path::Path;

pub(crate) fn copy(
    repo: &Path,
    oid: &str,
    author: Option<(&str, &str)>,
    suffix: char,
) -> Result<Action> {
    let show = |format| {
        git(
            repo,
            &[
                "show",
                "-s",
                "--no-show-signature",
                "--no-notes",
                format,
                oid,
                "--",
            ],
        )
    };
    let (text, label) = match suffix {
        'y' => (oid.to_owned(), "commit ID"),
        'm' => (show("--format=%B")?, "commit message"),
        'a' => (
            match author {
                Some((name, email)) => format!("{name} <{email}>"),
                None => show("--format=%an <%ae>")?,
            },
            "author details",
        ),
        _ => return Ok(Action::None),
    };
    Ok(Action::Copy { text, label })
}
