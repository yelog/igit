use super::model::{Branch, Snapshot};
use crate::core::{command, git, revpath, Context, Repo};
use anyhow::{bail, ensure, Context as _, Result};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Create,
    Delete,
    Checkout,
    Track,
    Merge,
    Rename,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Create => "Create branch",
            Self::Delete => "Delete local branch",
            Self::Checkout => "Switch branch",
            Self::Track => "Track remote branch",
            Self::Merge => "Merge branch",
            Self::Rename => "Rename local branch",
        }
    }
}

pub(super) struct Review {
    root: usize,
    path: PathBuf,
    snapshot: Snapshot,
    source: Branch,
    operation: Operation,
    name: String,
    pub body: String,
}

fn ready(repo: &Repo, operation: Operation) -> Result<()> {
    ensure!(
        revpath(&repo.path, "--show-toplevel").as_deref() == Some(repo.path.as_path()),
        "Worktree unavailable; rescan the workspace"
    );
    let dir = revpath(&repo.path, "--absolute-git-dir").context("Git directory unavailable")?;
    ensure!(
        ![
            "MERGE_HEAD",
            "rebase-merge",
            "rebase-apply",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "sequencer"
        ]
        .iter()
        .any(|name| dir.join(name).exists())
            && git(&repo.path, &["ls-files", "--unmerged", "-z"])?.is_empty(),
        "Finish the current Git operation in Conflicts first"
    );
    if operation == Operation::Merge {
        ensure!(
            git(
                &repo.path,
                &["status", "--porcelain=v1", "-z", "--untracked-files=normal"]
            )?
            .is_empty(),
            "Commit or stash working-tree and index changes before merging"
        );
    }
    Ok(())
}

impl Review {
    pub fn prepare(
        c: &Context,
        root: usize,
        source: &Branch,
        operation: Operation,
        name: &str,
    ) -> Result<Self> {
        ensure!(c.scope.contains(&root), "Repository is outside Roots");
        let repo = c.repos.get(root).context("Repository is unavailable")?;
        ready(repo, operation)?;
        let snapshot = Snapshot::read(repo)?;
        ensure!(
            snapshot.branches.get(&source.reference) == Some(source),
            "Branch changed; refresh and review again"
        );
        ensure!(!source.oid.is_empty(), "This branch has no commits yet");
        if matches!(
            operation,
            Operation::Delete | Operation::Rename | Operation::Checkout
        ) {
            ensure!(
                !source.remote,
                "Delete and rename are available for local branches only"
            );
            ensure!(
                source.worktree.is_empty() || source.current,
                "Branch is checked out in another worktree: {}",
                source.worktree
            );
        }
        if matches!(
            operation,
            Operation::Create | Operation::Track | Operation::Rename
        ) {
            ensure!(
                !name.is_empty() && !name.starts_with('-'),
                "Enter a valid branch name"
            );
            let checked = git(&repo.path, &["check-ref-format", "--branch", name])
                .context("Invalid branch name")?;
            ensure!(checked == name, "Enter a literal branch name");
            ensure!(
                !snapshot
                    .branches
                    .contains_key(&format!("refs/heads/{name}")),
                "Local branch already exists: {name}"
            );
        }
        match operation {
            Operation::Delete => ensure!(!source.current, "Cannot delete the current branch"),
            Operation::Checkout => ensure!(!source.current, "Already on this branch"),
            Operation::Track => {
                ensure!(
                    source.remote && snapshot.local_name(source).is_some(),
                    "Remote is not configured; fetch or refresh first"
                );
            }
            Operation::Merge => {
                ensure!(
                    snapshot.head.is_some() && snapshot.oid.is_some(),
                    "Switch to a local branch before merging"
                );
                ensure!(
                    !source.current,
                    "Cannot merge the current branch into itself"
                );
            }
            _ => (),
        }
        let head = snapshot.head_label();
        let detail = match operation {
            Operation::Create => format!("Create local branch: {name}\nFrom: {}\nStay on: {head}\nThe new branch has no upstream.", source.name),
            Operation::Delete => format!("Delete local branch: {}\nStay on: {head}\nGit's safe deletion check must pass. Remote branches are kept.", source.name),
            Operation::Checkout => format!("Switch from: {head}\nTo local branch: {}\nGit will refuse to overwrite local changes or ignored files.", source.name),
            Operation::Track => format!("Create and switch to: {name}\nTrack: {}\nCurrent branch: {head}\nGit will refuse to overwrite local changes or ignored files.", source.name),
            Operation::Merge => format!("Merge source: {}\nInto current branch: {head}\nGit may fast-forward or create a merge commit. Conflicts remain available in Conflicts.", source.name),
            Operation::Rename => format!("Rename local branch: {}\nTo: {name}\nUpstream configuration is retained. Remote branches are kept.", source.name),
        };
        let body = format!(
            "Repository: {}\nPath: {}\n\n{detail}\n\nSource commit: {}\nCurrent commit: {}",
            repo.name,
            repo.path.display(),
            source.oid,
            snapshot.oid.as_deref().unwrap_or("(no commits)")
        );
        Ok(Self {
            root,
            path: repo.path.clone(),
            snapshot,
            source: source.clone(),
            operation,
            name: name.into(),
            body,
        })
    }

    pub fn execute(self, c: &Context) -> Result<String> {
        ensure!(
            c.scope.contains(&self.root),
            "Repository is outside Roots; review again"
        );
        let repo = c
            .repos
            .get(self.root)
            .filter(|r| r.path == self.path)
            .context("Workspace changed; review again")?;
        ready(repo, self.operation)?;
        ensure!(
            Snapshot::read(repo)? == self.snapshot,
            "Branches or HEAD changed; refresh and review again"
        );
        let mut cmd = command(&repo.path);
        // A single-repository review never authorizes recursive submodule writes.
        cmd.args(["-c", "submodule.recurse=false"]);
        match self.operation {
            Operation::Create => {
                cmd.args(["branch", "--no-track", "--", &self.name, &self.source.oid]);
            }
            Operation::Delete => {
                cmd.args(["branch", "-d", "--", &self.source.name]);
            }
            Operation::Checkout => {
                cmd.args([
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                    "--",
                    &self.source.name,
                ]);
            }
            Operation::Track => {
                cmd.args([
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                    "--track",
                    "-c",
                    &self.name,
                    "--",
                    &self.source.reference,
                ]);
            }
            Operation::Merge => {
                cmd.args([
                    "-c",
                    "merge.autoStash=false",
                    "merge",
                    "--no-edit",
                    "--no-autostash",
                    "--no-overwrite-ignore",
                    "--",
                    &self.source.oid,
                ]);
            }
            Operation::Rename => {
                cmd.args(["branch", "-m", "--", &self.source.name, &self.name]);
            }
        }
        let output = cmd.output().context("Cannot start Git")?;
        if !output.status.success() {
            let detail = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
            bail!(
                "{} failed in {}: {}",
                self.operation.title(),
                repo.name,
                detail.trim()
            );
        }
        Ok(format!(
            "{} completed in {}: {}{}",
            self.operation.title(),
            repo.name,
            self.source.name,
            if self.name.is_empty() {
                String::new()
            } else {
                format!(" → {}", self.name)
            }
        ))
    }
}
