pub(super) mod checkout;
mod view;

use super::model::{Branch, Snapshot};
use super::sync::Sync;
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
    Pull,
    Push,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Create => "Create branch",
            Self::Delete => "Delete local branch",
            Self::Checkout => "Switch branch",
            Self::Track => "Track remote branch",
            Self::Merge => "Merge branch",
            Self::Pull => "Pull local branch",
            Self::Push => "Push local branch",
            Self::Rename => "Rename local branch",
        }
    }
}

pub(super) struct Review {
    root: usize,
    repository: String,
    path: PathBuf,
    snapshot: Snapshot,
    source: Branch,
    operation: Operation,
    name: String,
    sync: Option<Sync>,
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
        let sync = matches!(operation, Operation::Pull | Operation::Push)
            .then(|| Sync::prepare(repo, root, source, operation))
            .transpose()?;
        let head = snapshot.head_label();
        let detail = match operation {
            Operation::Pull | Operation::Push => sync.as_ref().unwrap().detail(source),
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
            repository: repo.name.clone(),
            path: repo.path.clone(),
            snapshot,
            source: source.clone(),
            operation,
            name: name.into(),
            sync,
            body,
        })
    }

    pub fn is_sync(&self) -> bool {
        self.sync.is_some()
    }

    pub(crate) fn checkout_outcome(&self) -> Result<super::CheckoutDone> {
        let (reference, name, upstream) = if self.operation == Operation::Track {
            (
                format!("refs/heads/{}", self.name),
                self.name.clone(),
                self.source.reference.clone(),
            )
        } else {
            (
                self.source.reference.clone(),
                self.source.name.clone(),
                self.source.upstream.clone(),
            )
        };
        let actual = git(&self.path, &["symbolic-ref", "-q", "HEAD"])?;
        ensure!(
            actual == reference,
            "Git left HEAD on {actual}, expected {reference}"
        );
        let head = git(&self.path, &["rev-parse", "--verify", "HEAD"])?;
        let worktree = git(&self.path, &["rev-parse", "--show-toplevel"])?;
        let last_used = super::recent::read(
            &self.path,
            self.snapshot
                .branches
                .keys()
                .map(String::as_str)
                .chain(std::iter::once(reference.as_str())),
        );
        Ok(super::CheckoutDone {
            root: self.root,
            path: self.path.clone(),
            worktree,
            reference,
            branch: name,
            head,
            upstream,
            last_used,
        })
    }

    pub(crate) fn execute_switch(&self) -> Result<String> {
        let head = git(&self.path, &["symbolic-ref", "-q", "HEAD"]).ok();
        let oid = git(&self.path, &["rev-parse", "--verify", "HEAD"]).ok();
        ensure!(
            head == self.snapshot.head
                && oid == self.snapshot.oid
                && git(
                    &self.path,
                    &["rev-parse", "--verify", &self.source.reference]
                )? == self.source.oid,
            "Branch or HEAD changed; review again"
        );
        self.run(false)
    }

    fn validate<'a>(&self, c: &'a Context) -> Result<&'a Repo> {
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
        Ok(repo)
    }

    pub fn execute(&self, c: &Context) -> Result<String> {
        let repo = self.validate(c)?;
        if let Some(sync) = &self.sync {
            sync.execute(repo, &self.source, || {
                ready(repo, self.operation)?;
                let snapshot = Snapshot::read(repo)?;
                ensure!(
                    snapshot.head == self.snapshot.head
                        && snapshot.oid == self.snapshot.oid
                        && snapshot
                            .branches
                            .get(&self.source.reference)
                            .is_some_and(|branch| {
                                branch.current == self.source.current
                                    && branch.worktree == self.source.worktree
                                    && branch.oid == self.source.oid
                            }),
                    "Branch or HEAD changed during fetch; review again"
                );
                Ok(())
            })?;
            return Ok(format!(
                "{} completed in {}: {}",
                self.operation.title(),
                repo.name,
                self.source.name
            ));
        }
        self.run(false)
    }

    fn run(&self, force: bool) -> Result<String> {
        let mut cmd = command(&self.path);
        // A single-repository review never authorizes recursive submodule writes.
        cmd.args(["-c", "submodule.recurse=false"]);
        match self.operation {
            Operation::Pull | Operation::Push => unreachable!("sync operations return above"),
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
                ]);
                if force {
                    cmd.arg("--discard-changes");
                }
                cmd.args(["--", &self.source.name]);
            }
            Operation::Track => {
                cmd.args([
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                ]);
                if force {
                    cmd.arg("--discard-changes");
                }
                cmd.args(["--track", "-c", &self.name, "--", &self.source.reference]);
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
                self.repository,
                detail.trim()
            );
        }
        Ok(format!(
            "{} completed in {}: {}{}",
            self.operation.title(),
            self.repository,
            self.source.name,
            if self.name.is_empty() {
                String::new()
            } else {
                format!(" → {}", self.name)
            }
        ))
    }
}
