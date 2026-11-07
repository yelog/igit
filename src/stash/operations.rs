//! Reviewed operations on one exact stash entry in one repository.
use super::model::{self, Entry};
use crate::core::{command, git, git_bytes, revpath, Context, Repo};
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    path::{Path, PathBuf},
    process::Output,
};

mod scope;
#[cfg(test)]
mod tests;
mod view;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Apply,
    Pop,
    Drop,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Apply => "Apply stash",
            Self::Pop => "Pop stash",
            Self::Drop => "Drop stash",
        }
    }

    fn summary(self) -> [&'static str; 2] {
        match self {
            Self::Apply => ["Keeps the stash", "Applies changes to the worktree."],
            Self::Pop => [
                "Drops after a successful apply",
                "A failed apply keeps the stash.",
            ],
            Self::Drop => [
                "Removes the stash without applying",
                "Cannot be undone from IGit.",
            ],
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Apply | Self::Pop => "Git refuses to overwrite local edits. Check Commit and Conflicts for partial changes before retrying.",
            Self::Drop => "Working files and the index stay unchanged.",
        }
    }
}

#[derive(PartialEq, Eq)]
struct Destination {
    head: String,
    branch: String,
    index: Vec<u8>,
    restore_index: bool,
}

fn optional(repo: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = command(repo).args(args).output()?;
    match output.status.code() {
        Some(0) => Ok(Some(String::from_utf8(output.stdout)?.trim_end().into())),
        Some(1) => Ok(None),
        _ => bail!("{}", detail(&output)),
    }
}

fn detail(output: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if text.trim().is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        model::label(text.trim())
    }
}

fn ready(repo: &Repo, operation: Operation) -> Result<()> {
    ensure!(
        revpath(&repo.path, "--show-toplevel").as_deref() == Some(repo.path.as_path()),
        "Worktree unavailable; rescan the workspace"
    );
    if operation == Operation::Drop {
        return Ok(());
    }
    let dir = revpath(&repo.path, "--absolute-git-dir").context("Git directory unavailable")?;
    ensure!(
        ![
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer"
        ]
        .iter()
        .any(|name| dir.join(name).exists())
            && git_bytes(&repo.path, &["ls-files", "--unmerged", "-z"])?.is_empty(),
        "Finish the current Git operation or resolve conflicts before applying a stash"
    );
    Ok(())
}

impl Destination {
    fn read(repo: &Path) -> Result<Self> {
        Ok(Self {
            head: git(repo, &["rev-parse", "--verify", "HEAD"])
                .context("A current commit is required to apply a stash")?,
            branch: optional(repo, &["symbolic-ref", "-q", "HEAD"])?
                .unwrap_or_else(|| "HEAD (detached)".into()),
            index: git_bytes(repo, &["ls-files", "--stage", "-z"])?,
            restore_index: optional(repo, &["config", "--type=bool", "--get", "stash.index"])?
                .as_deref()
                == Some("true"),
        })
    }

    fn branch_label(&self) -> String {
        model::label(
            self.branch
                .strip_prefix("refs/heads/")
                .unwrap_or(&self.branch),
        )
    }

    fn staging_label(&self) -> &'static str {
        if self.restore_index {
            "Restore (stash.index)"
        } else {
            "Do not restore"
        }
    }
}

pub(super) struct Review {
    root: usize,
    repo_name: String,
    path: PathBuf,
    source: Entry,
    entries: Vec<Entry>,
    destination: Option<Destination>,
    paths: Vec<PathBuf>,
    operation: Operation,
}

impl Review {
    pub fn prepare(
        c: &Context,
        root: usize,
        source: &Entry,
        expected: &[Entry],
        operation: Operation,
    ) -> Result<Self> {
        ensure!(c.scope.contains(&root), "Repository is outside Roots");
        let repo = c.repos.get(root).context("Repository is unavailable")?;
        ready(repo, operation)?;
        let entries = model::entries(&repo.path)?;
        ensure!(
            entries == expected && entries.contains(source),
            "Stash list changed; refresh and review again"
        );
        let destination = if operation == Operation::Drop {
            None
        } else {
            Some(Destination::read(&repo.path)?)
        };
        let paths = if let Some(target) = &destination {
            let paths = scope::paths(&repo.path, &source.oid, target.restore_index)?;
            scope::check(&repo.path, &paths, &target.index)?;
            paths
        } else {
            vec![]
        };
        Ok(Self {
            root,
            repo_name: model::label(&repo.name),
            path: repo.path.clone(),
            source: source.clone(),
            entries,
            destination,
            paths,
            operation,
        })
    }

    fn path_label(&self) -> String {
        model::label(&self.path.to_string_lossy())
    }

    pub fn body(&self) -> String {
        let [effect, explanation] = self.operation.summary();
        let source = &self.source;
        let mut body = format!(
            "{effect}\n{explanation}\n\nRepository: {}\nPath: {}\n\nStash: {}\nSaved: {}\nMessage: {}\nObject: {}\n",
            self.repo_name, self.path_label(), source.selector, source.saved, source.subject, source.oid
        );
        if let Some(target) = &self.destination {
            body.push_str(&format!(
                "\nApply to: {}\nCurrent commit: {}\nStaging: {}\n",
                target.branch_label(),
                target.head,
                target.staging_label()
            ));
        }
        body.push('\n');
        body.push_str(self.operation.note());
        body
    }

    fn check_entries(&self) -> Result<()> {
        ensure!(
            model::entries(&self.path)? == self.entries,
            "Stash list changed; refresh and review again"
        );
        Ok(())
    }

    fn drop_entry(&self) -> Result<()> {
        // A stash selector is a reflog position, not a stable object ID. Never
        // reuse it after an external push/drop, even if the selected OID remains.
        self.check_entries()?;
        let output = command(&self.path)
            .args(["stash", "drop", &self.source.selector])
            .output()?;
        ensure!(output.status.success(), "{}", detail(&output));
        Ok(())
    }

    pub fn execute(self, c: &Context) -> Result<String> {
        ensure!(
            c.scope.contains(&self.root),
            "Repository is outside Roots; review again"
        );
        let repo = c
            .repos
            .get(self.root)
            .filter(|repo| repo.path == self.path)
            .context("Workspace changed; review again")?;
        ready(repo, self.operation)?;
        self.check_entries()?;
        let label = format!(
            "{} · {} · {}",
            model::label(&repo.name),
            self.source.selector,
            &self.source.oid[..12]
        );
        if self.operation == Operation::Drop {
            self.drop_entry()?;
            return Ok(format!("Dropped stash · {label}"));
        }
        let destination = self
            .destination
            .as_ref()
            .context("Missing stash destination")?;
        ensure!(
            Destination::read(&self.path)? == *destination,
            "Branch, HEAD, index, or staging option changed; review again"
        );
        scope::check(&self.path, &self.paths, &destination.index)?;
        let mut cmd = command(&self.path);
        // Operate on immutable saved content. A Pop is an apply followed by a
        // separately revalidated drop; a changed reflog must never delete a
        // different entry after a slow apply or external Git hook.
        cmd.args([
            "-c",
            "submodule.recurse=false",
            "-c",
            "stash.index=false",
            "stash",
            "apply",
        ]);
        if destination.restore_index {
            cmd.arg("--index");
        }
        let output = cmd
            .arg(&self.source.oid)
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() {
            let conflicts = git_bytes(&self.path, &["ls-files", "--unmerged", "-z"])
                .is_ok_and(|entries| !entries.is_empty());
            bail!(
                "{} failed · {label}. Stash kept. {} {}",
                self.operation.title(),
                if conflicts {
                    "Resolve conflicts in the Conflicts tab before continuing."
                } else {
                    "Inspect working files and the index before retrying; Git may have applied some changes."
                },
                detail(&output)
            );
        }
        if self.operation == Operation::Pop {
            if let Err(error) = self.drop_entry() {
                return Ok(format!("Applied stash · {label}, but could not drop it: {error:#}. Do not apply again; refresh and review Drop to remove the saved entry."));
            }
            Ok(format!("Popped stash · {label}"))
        } else {
            Ok(format!("Applied stash · {label}; stash kept"))
        }
    }
}
