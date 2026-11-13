use crate::core::{git_bytes, Context};
use anyhow::{ensure, Context as _, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub oid: String,
    pub selector: String,
    pub saved: String,
    pub subject: String,
}

impl Entry {
    pub(super) fn message(&self) -> &str {
        // `git stash push -m` prefixes a custom message with its source branch.
        // Keep the original reflog subject for details and reviewed operations.
        self.subject
            .strip_prefix("On ")
            .and_then(|rest| rest.split_once(": "))
            .filter(|(branch, message)| {
                !branch.is_empty()
                    && !branch.chars().any(char::is_whitespace)
                    && !message.is_empty()
            })
            .map_or(&self.subject, |(_, message)| message)
    }

    pub(super) fn number(&self) -> Option<&str> {
        self.selector.strip_prefix("stash@{")?.strip_suffix('}')
    }
}

pub(super) struct Repository {
    pub root: usize,
    pub entries: Vec<Entry>,
    pub error: Option<String>,
}

impl Repository {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.error.is_none()
    }
}

pub(super) fn label(text: &str) -> String {
    text.chars()
        .flat_map(|ch| {
            if ch.is_control() {
                ch.escape_default().collect::<Vec<_>>()
            } else {
                vec![ch]
            }
        })
        .collect()
}

pub(super) fn entries(path: &std::path::Path) -> Result<Vec<Entry>> {
    let raw = git_bytes(
        path,
        &[
            "stash",
            "list",
            "--no-show-signature",
            "--no-notes",
            "--format=%H%x00%gd%x00%ci%x00%gs",
            "-z",
        ],
    )?;
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let mut fields = raw
        .strip_suffix(&[0])
        .unwrap_or(&raw)
        .split(|&byte| byte == 0);
    let mut entries = Vec::new();
    while let Some(oid) = fields.next() {
        let selector = fields.next().context("Missing stash selector")?;
        let saved = fields.next().context("Missing stash timestamp")?;
        let subject = fields.next().context("Missing stash subject")?;
        let oid = std::str::from_utf8(oid)?;
        ensure!(
            matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid stash object ID"
        );
        entries.push(Entry {
            oid: oid.into(),
            selector: label(&String::from_utf8_lossy(selector)),
            saved: label(&String::from_utf8_lossy(saved)),
            subject: label(&String::from_utf8_lossy(subject)),
        });
    }
    Ok(entries)
}

pub(super) fn load_repository(root: usize, path: &std::path::Path) -> Repository {
    let (entries, error) = match entries(path) {
        Ok(entries) => (entries, None),
        Err(error) => (vec![], Some(format!("{error:#}"))),
    };
    Repository {
        root,
        entries,
        error,
    }
}

pub(super) fn load(c: &Context) -> Vec<Repository> {
    c.scope
        .iter()
        .filter_map(|&root| {
            let repo = c.repos.get(root)?;
            Some(load_repository(root, &repo.path))
        })
        .collect()
}
