//! Whole-repository stash reviews; the real index is left to native Git.
use super::{
    branch_ref, checked, cmd, diff, g, oid, raw, safe, status, txt,
    working::{self, Working},
    Change, Outcome,
};
use anyhow::{bail, Context as _, Result};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(PartialEq, Eq)]
struct Snapshot {
    head: String,
    branch: String,
    index: Vec<u8>,
    files: Vec<Change>,
    working: Vec<Working>,
}

pub struct Plan {
    pub root: usize,
    pub repo: PathBuf,
    pub message: String,
    pub preview: String,
    snapshot: Snapshot,
}

fn reflog(repo: &Path) -> Result<Vec<u8>> {
    g(
        repo,
        None,
        &[
            "stash",
            "list",
            "--no-show-signature",
            "--no-notes",
            "--format=%H",
            "-z",
        ],
    )
}

#[cfg(test)]
fn tip(repo: &Path) -> Result<Option<String>> {
    Ok(reflog(repo)?
        .split(|b| *b == 0)
        .find(|entry| !entry.is_empty())
        .map(|entry| txt(entry.to_vec())))
}

impl Snapshot {
    fn read(repo: &Path) -> Result<Option<Self>> {
        let files = status(repo)?;
        if files.is_empty() {
            return Ok(None);
        }
        safe(repo)?;
        let head = oid(repo)?.context("Create an initial commit before creating a stash")?;
        let index = g(repo, None, &["ls-files", "--stage", "-z"])?;
        let tree = g(repo, None, &["ls-tree", "-r", "-z", &head])?;
        let gitlinks: BTreeSet<_> = index
            .split(|b| *b == 0)
            .chain(tree.split(|b| *b == 0))
            .filter(|record| record.starts_with(b"160000 "))
            .filter_map(|record| {
                record
                    .iter()
                    .position(|b| *b == b'\t')
                    .map(|tab| PathBuf::from(raw(&record[tab + 1..])))
            })
            .collect();
        let mut working = Vec::with_capacity(files.len());
        for file in &files {
            if gitlinks.contains(&file.path) {
                bail!("{} is a changed submodule pointer; handle it separately before stashing this repository", file.label());
            }
            working.push(working::read(repo, &file.path)?);
        }
        Ok(Some(Self {
            head,
            branch: branch_ref(repo)?,
            index,
            files,
            working,
        }))
    }
}

impl Plan {
    pub fn files(&self) -> &[Change] {
        &self.snapshot.files
    }

    pub fn branch(&self) -> &str {
        self.snapshot
            .branch
            .strip_prefix("refs/heads/")
            .unwrap_or(&self.snapshot.branch)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if Snapshot::read(&self.repo)?.as_ref() != Some(&self.snapshot) {
            bail!("Files, index, branch, or HEAD changed. Review stash again");
        }
        Ok(())
    }
}

pub fn plan(root: usize, repo: &Path, message: &str) -> Result<Option<Plan>> {
    if message.trim().is_empty() {
        bail!("Stash message is required");
    }
    if message.contains('\0') {
        bail!("Stash message cannot contain NUL characters");
    }
    let Some(snapshot) = Snapshot::read(repo)? else {
        return Ok(None);
    };
    let mut preview = String::new();
    let mut lines = 0;
    'files: for (file, working) in snapshot.files.iter().zip(&snapshot.working) {
        let mut versions = Vec::new();
        if file.staged() {
            versions.push(("Staged", file.clone()));
        }
        let mut work = file.clone();
        if file.xy[0] == b'D' && !matches!(working, Working::Missing) {
            work.xy = *b"??";
        } else if file.xy != *b"??" {
            work.xy[0] = b' ';
        }
        versions.push(("Working tree / untracked", work));
        for (label, version) in versions {
            let patch = diff(repo, &version)?;
            if patch.is_empty() {
                continue;
            }
            preview.push_str(&format!("\n{label}: {}\n", file.label()));
            for line in patch.lines() {
                if lines >= 500 || preview.len() > 48_000 {
                    preview.push_str("… Preview truncated; all affected paths are listed above.\n");
                    break 'files;
                }
                preview.extend(line.chars().take(1800));
                preview.push('\n');
                lines += 1;
            }
        }
    }
    let plan = Plan {
        root,
        repo: repo.into(),
        message: message.into(),
        preview,
        snapshot,
    };
    plan.validate()?;
    Ok(Some(plan))
}

pub fn execute(plan: &Plan) -> Result<Outcome> {
    plan.validate()?;
    // Linked worktrees share the stash reflog. Read it here, after earlier
    // repositories in the batch have saved, and detect even identical OIDs.
    let before = reflog(&plan.repo)?;
    // Explicitly suppress recursive resets: each repository in Roots has its
    // own review, and hidden submodule worktrees must never be cleaned here.
    let output = cmd(&plan.repo, None)
        // No caller-supplied pathspecs are involved. Git's internal cleanup
        // constructs its own escaped paths, which require default semantics.
        .env_remove("GIT_LITERAL_PATHSPECS")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS")
        .args([
            "-c",
            "submodule.recurse=false",
            "stash",
            "push",
            "--include-untracked",
            "--message",
        ])
        .arg(&plan.message)
        .output()?;
    let after = reflog(&plan.repo)?;
    if after == before {
        checked(output)?;
        bail!("Git did not create a stash. Refresh and review again");
    }
    let hash = after
        .split(|b| *b == 0)
        .find(|entry| !entry.is_empty())
        .context("Stash result is unavailable; inspect the Stash tab before retrying")?;
    let hash = txt(hash.to_vec());
    // Stash publication precedes worktree cleanup. A cleanup error must keep
    // the saved object visible and must not be treated as a failed save/retry.
    let warning = match checked(output) {
        Err(error) => Some(format!("Saved, but cleanup failed: {error}")),
        Ok(_) => match status(&plan.repo) {
            Ok(files) if files.is_empty() => None,
            Ok(_) => Some(
                "Saved, but changes remain in the working tree; inspect them before continuing"
                    .into(),
            ),
            Err(error) => Some(format!("Saved, but cannot verify cleanup: {error}")),
        },
    };
    Ok(Outcome { hash, warning })
}
