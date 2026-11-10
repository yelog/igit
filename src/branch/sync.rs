use super::model::Branch;
use super::operations::Operation;
use crate::{
    core::{command, git, redact_remote_error, Repo},
    push::FocusedPush,
};
use anyhow::{ensure, Result};

#[derive(PartialEq, Eq)]
struct Upstream {
    remote: String,
    reference: String,
    tracking: String,
}

impl Upstream {
    fn read(repo: &Repo, branch: &Branch) -> Result<Self> {
        ensure!(!branch.remote, "Select a local branch");
        let output = git(
            &repo.path,
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)%09%(upstream:remoteref)%09%(upstream)",
                &branch.reference,
            ],
        )?;
        let fields: Vec<_> = output.split('\t').collect();
        let [remote, reference, tracking] = fields.as_slice() else {
            anyhow::bail!("Configure a remote upstream for this branch first");
        };
        ensure!(
            !remote.starts_with('-')
                && git(&repo.path, &["remote"])?.lines().any(|r| r == *remote)
                && reference.starts_with("refs/heads/")
                && tracking.starts_with("refs/remotes/"),
            "Configure a remote upstream for this branch first"
        );
        Ok(Self {
            remote: (*remote).into(),
            reference: (*reference).into(),
            tracking: (*tracking).into(),
        })
    }
}

enum Transfer {
    Pull { fetch_url: String },
    Push(Box<FocusedPush>),
}

pub(super) struct Sync {
    upstream: Upstream,
    transfer: Transfer,
}

impl Sync {
    pub fn prepare(
        repo: &Repo,
        root: usize,
        branch: &Branch,
        operation: Operation,
    ) -> Result<Self> {
        let upstream = Upstream::read(repo, branch)?;
        let transfer = if operation == Operation::Pull {
            ensure!(
                branch.current || branch.worktree.is_empty(),
                "Branch is checked out in another worktree: {}",
                branch.worktree
            );
            Transfer::Pull {
                fetch_url: Self::fetch_url(repo, &upstream)?,
            }
        } else {
            Transfer::Push(Box::new(FocusedPush::prepare(
                repo,
                root,
                &branch.reference,
                &branch.oid,
                &upstream.remote,
                upstream.reference.strip_prefix("refs/heads/").unwrap(),
                &upstream.tracking,
            )?))
        };
        Ok(Self { upstream, transfer })
    }

    fn fetch_url(repo: &Repo, upstream: &Upstream) -> Result<String> {
        git(&repo.path, &["remote", "get-url", "--", &upstream.remote])
            .map_err(|error| anyhow::anyhow!(redact_remote_error(&error.to_string(), "")))
    }

    pub fn detail(&self, branch: &Branch) -> String {
        let upstream = format!("{}:{}", self.upstream.remote, self.upstream.reference);
        if matches!(self.transfer, Transfer::Push(_)) {
            format!(
                "Push local branch: {}\nTo: {upstream}\nOnly fast-forward updates are allowed.",
                branch.name
            )
        } else {
            format!(
                "Pull local branch: {}\nFrom: {upstream}\nFast-forward only. {}",
                branch.name,
                if branch.current {
                    "Updates the current worktree."
                } else {
                    "Keeps the current branch and worktree."
                }
            )
        }
    }

    pub fn execute(
        &self,
        repo: &Repo,
        branch: &Branch,
        revalidate: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            Upstream::read(repo, branch)? == self.upstream,
            "Upstream changed; review again"
        );
        let fetch_url = match &self.transfer {
            Transfer::Push(push) => return push.execute(),
            Transfer::Pull { fetch_url } => fetch_url,
        };
        let url = Self::fetch_url(repo, &self.upstream)?;
        ensure!(fetch_url == &url, "Remote URL changed; review again");

        let tracking = format!("{}:{}", self.upstream.reference, self.upstream.tracking);
        let fetch = [
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-prune-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--refmap=",
            "--",
        ];
        let mut args = fetch.to_vec();
        args.extend([&self.upstream.remote, &tracking].map(String::as_str));
        git(&repo.path, &args)
            .map_err(|error| anyhow::anyhow!(redact_remote_error(&error.to_string(), &url)))?;
        revalidate()?;
        ensure!(
            Upstream::read(repo, branch)? == self.upstream,
            "Upstream changed during fetch; review again"
        );
        let tip = git(
            &repo.path,
            &["rev-parse", "--verify", &self.upstream.tracking],
        )?;
        if branch.current {
            git(
                &repo.path,
                &[
                    "-c",
                    "submodule.recurse=false",
                    "-c",
                    "merge.autostash=false",
                    "merge",
                    "--ff-only",
                    "--no-autostash",
                    "--no-overwrite-ignore",
                    "--",
                    &tip,
                ],
            )?;
        } else {
            match command(&repo.path)
                .args(["merge-base", "--is-ancestor", &tip, &branch.oid])
                .output()?
                .status
                .code()
            {
                Some(0) => return Ok(()), // The local branch already contains the upstream.
                Some(1) => (),
                _ => anyhow::bail!("Cannot compare the local and upstream commits"),
            }
            // Git's fetch checks fast-forward and refuses destinations checked out
            // in any worktree. Transfer only the OID fetched above, from this repo.
            let local = format!("{tip}:{}", branch.reference);
            let mut args = fetch.to_vec();
            args.extend([".", &local]);
            git(&repo.path, &args)?;
        }
        Ok(())
    }
}
