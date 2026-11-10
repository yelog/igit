use super::{command, git, git_bytes, revpath, strategy::Strategy};
use anyhow::{bail, ensure, Context as _, Result};
use std::{path::Path, process::Command};

#[cfg(test)]
mod tests;

pub(crate) fn published_graph(repo: &Path) -> Command {
    let mut cmd = command(repo);
    cmd.env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", "");
    cmd
}

pub(crate) fn is_ancestor(repo: &Path, ancestor: &str, commit: &str) -> Result<bool> {
    match published_graph(repo)
        .args(["merge-base", "--is-ancestor", ancestor, commit])
        .output()?
        .status
        .code()
    {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!("Cannot compare stored commit history; fetch and review again"),
    }
}

pub(crate) fn conflicts(repo: &Path) -> bool {
    git_bytes(repo, &["ls-files", "--unmerged", "-z"]).is_ok_and(|output| !output.is_empty())
}

pub(crate) fn ready(repo: &Path) -> Result<()> {
    let directory = revpath(repo, "--absolute-git-dir").context("Git directory unavailable")?;
    ensure!(
        ![
            "MERGE_HEAD",
            "rebase-merge",
            "rebase-apply",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "sequencer",
            "BISECT_START",
        ]
        .iter()
        .any(|name| directory.join(name).exists())
            && git_bytes(repo, &["ls-files", "--unmerged", "-z"])?.is_empty(),
        "Finish the current Git operation before updating or pushing"
    );
    Ok(())
}

pub(crate) fn integrate(repo: &Path, target: &str, strategy: Strategy) -> Result<()> {
    let branch = git(repo, &["symbolic-ref", "-q", "HEAD"]).context("Detached HEAD")?;
    let head = git(repo, &["rev-parse", "--verify", "HEAD"])?;
    integrate_reviewed(repo, target, strategy, &branch, &head).map(|_| ())
}

pub(crate) fn integrate_reviewed(
    repo: &Path,
    target: &str,
    strategy: Strategy,
    branch: &str,
    head: &str,
) -> Result<String> {
    ready(repo)?;
    ensure!(
        git(repo, &["symbolic-ref", "-q", "HEAD"])? == branch
            && git(repo, &["rev-parse", "HEAD"])? == head,
        "Branch or HEAD changed; review again"
    );
    if is_ancestor(repo, target, head)? {
        return Ok(head.to_owned());
    }
    ensure!(
        git_bytes(
            repo,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=no"
            ]
        )?
        .is_empty(),
        "Update requires a clean index and tracked working tree; commit or stash changes first"
    );
    super::paths::protect(repo, &format!("{head}...{target}"), false)?;
    ensure!(
        git(repo, &["symbolic-ref", "-q", "HEAD"])? == branch
            && git(repo, &["rev-parse", "HEAD"])? == head,
        "Branch or HEAD changed; review again"
    );
    let mut cmd = published_graph(repo);
    cmd.args([
        "-c",
        "submodule.recurse=false",
        "-c",
        "rerere.autoupdate=false",
        "-c",
        "merge.directoryRenames=false",
    ]);
    match strategy {
        Strategy::Merge => {
            cmd.args([
                "merge",
                "--ff",
                "--no-edit",
                "--no-autostash",
                "--no-overwrite-ignore",
                "--",
                target,
            ]);
        }
        Strategy::Rebase => {
            cmd.args([
                "-c",
                "core.logAllRefUpdates=true",
                "rebase",
                "--rebase-merges",
                "--no-autostash",
                "--no-update-refs",
                "--no-autosquash",
                "--no-fork-point",
                "--",
                target,
            ]);
        }
    }
    let output = cmd.output().context("Cannot launch Git update")?;
    if !output.status.success() {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim()
        );
        ensure!(
            !detail.trim().is_empty(),
            "Git exited with {}",
            output.status
        );
        bail!("{}", detail.trim());
    }
    ready(repo)?;
    let updated = git(repo, &["rev-parse", "--verify", "HEAD"])?;
    ensure!(
        git(repo, &["symbolic-ref", "-q", "HEAD"])? == branch,
        "Branch changed during update; inspect the repository before pushing"
    );
    if strategy == Strategy::Rebase {
        let previous = format!("{branch}@{{1}}");
        ensure!(
            git(repo, &["rev-parse", "--verify", "ORIG_HEAD"]).is_ok_and(|value| value == head)
                && git(repo, &["rev-parse", "--verify", &previous])
                    .is_ok_and(|value| value == head),
            "Rebase result changed outside the reviewed update; inspect before pushing"
        );
    }
    ensure!(
        git_bytes(
            repo,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=no"
            ]
        )?
        .is_empty(),
        "Tracked files changed during update; inspect the repository before pushing"
    );
    ensure!(
        is_ancestor(repo, target, &updated)?,
        "Updated branch does not contain the fetched target; inspect before pushing"
    );
    if strategy == Strategy::Merge {
        if is_ancestor(repo, head, target)? {
            ensure!(
                updated == target,
                "HEAD changed during update; review before pushing"
            );
        } else {
            let output = published_graph(repo)
                .args(["show", "-s", "--format=%P", &updated, "--"])
                .output()?;
            ensure!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == format!("{head} {target}"),
                "Merge result changed outside the reviewed update; inspect before pushing"
            );
        }
    }
    ensure!(
        git(repo, &["symbolic-ref", "-q", "HEAD"])? == branch
            && git(repo, &["rev-parse", "HEAD"])? == updated,
        "Branch or HEAD changed after update; review before pushing"
    );
    Ok(updated)
}
