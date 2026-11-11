//! A review binds one Git operation to one commit and working tree.
use super::model::{plain, Rev};
use crate::core::{
    paths::protect as protect_paths, revpath, update::published_graph as command, Context, Repo,
};
use crate::ui::NoticeLevel;
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Output,
};
use tempfile::NamedTempFile;

mod reword;
mod tip;
mod transaction;

fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    checked(command(repo).args(args).output()?)
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(repo, args)?)?
        .trim_end_matches(['\r', '\n'])
        .to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Branch,
    Tag,
    Checkout,
    Undo,
    Revert,
    Drop,
    Edit,
    CherryPick,
}

impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Branch,
        Self::Tag,
        Self::Checkout,
        Self::Undo,
        Self::Revert,
        Self::Drop,
        Self::Edit,
        Self::CherryPick,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Branch => "New Branch",
            Self::Tag => "New Tag",
            Self::Checkout => "Checkout Revision",
            Self::CherryPick => "Cherry-pick Commit",
            Self::Undo => "Undo Commit",
            Self::Revert => "Revert Commit",
            Self::Drop => "Drop Commit",
            Self::Edit => "Edit Commit Message",
        }
    }
}

#[derive(Clone)]
pub(super) struct Target {
    pub root: usize,
    pub(super) path: PathBuf,
    pub oid: String,
    pub subject: String,
}

impl Target {
    pub fn new(c: &Context, rev: &Rev) -> Result<Self> {
        let repo = c.repos.get(rev.root).context("Repository unavailable")?;
        Ok(Self {
            root: rev.root,
            path: repo.path.clone(),
            oid: rev.oid.clone(),
            subject: rev.subject.clone(),
        })
    }

    pub fn short(&self) -> &str {
        self.oid.get(..8).unwrap_or(&self.oid)
    }

    fn repo<'a>(&self, c: &'a Context) -> Result<&'a Repo> {
        ensure!(c.scope.contains(&self.root), "Repository is outside Roots");
        let repo = c
            .repos
            .get(self.root)
            .filter(|r| r.path == self.path)
            .context("Workspace changed; refresh Log")?;
        ensure!(
            revpath(&repo.path, "--show-toplevel").as_deref() == Some(repo.path.as_path()),
            "Worktree unavailable; rescan the workspace"
        );
        ensure!(
            git(
                &repo.path,
                &["rev-parse", "--verify", &format!("{}^{{commit}}", self.oid)]
            )? == self.oid,
            "Selected commit is no longer available"
        );
        Ok(repo)
    }

    pub fn message(&self, c: &Context) -> Result<String> {
        let repo = self.repo(c)?;
        Plan::read(&repo.path, &self.oid, Kind::Edit, "")?;
        message(&repo.path, &self.oid)
    }
}

fn message(repo: &Path, oid: &str) -> Result<String> {
    let text = git_bytes(
        repo,
        &[
            "show",
            "-s",
            "--encoding=UTF-8",
            "--no-show-signature",
            "--no-notes",
            "--format=format:%B",
            oid,
            "--",
        ],
    )?;
    let text = String::from_utf8(text).context("Commit message is not valid UTF-8")?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

fn optional(repo: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = command(repo).args(args).output()?;
    if output.status.code() == Some(1) && output.stderr.is_empty() {
        return Ok(None);
    }
    let bytes = checked(output)?;
    Ok(Some(
        String::from_utf8_lossy(&bytes)
            .trim_end_matches(['\r', '\n'])
            .to_owned(),
    ))
}

#[derive(Debug, PartialEq, Eq)]
struct Head {
    branch: Option<String>,
    oid: String,
}

impl Head {
    fn read(repo: &Path) -> Result<Self> {
        Ok(Self {
            branch: optional(repo, &["symbolic-ref", "--quiet", "HEAD"])?,
            oid: optional(repo, &["rev-parse", "--verify", "--quiet", "HEAD"])?.unwrap_or_default(),
        })
    }

    fn label(&self) -> &str {
        self.branch
            .as_deref()
            .map(|name| name.strip_prefix("refs/heads/").unwrap_or(name))
            .unwrap_or("Detached HEAD")
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Ref,
    Checkout(Head),
    CherryPick(Head),
    RemoveTip {
        head: Head,
        parent: Option<String>,
        merge: bool,
    },
    Revert(Head),
    Reword {
        head: Head,
        commits: Vec<String>,
    },
}

fn ensure_idle(repo: &Path) -> Result<()> {
    let dir = revpath(repo, "--absolute-git-dir").context("Git directory unavailable")?;
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
        .any(|marker| dir.join(marker).exists())
            && git_bytes(repo, &["ls-files", "--unmerged", "-z"])?.is_empty(),
        "Finish the current Git operation first"
    );
    Ok(())
}

fn clean(repo: &Path) -> Result<()> {
    ensure!(
        git_bytes(
            repo,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=no",
                "--ignore-submodules=dirty",
            ],
        )?
        .is_empty(),
        "Commit or stash tracked working tree and index changes first"
    );
    Ok(())
}

fn ensure_unpublished(repo: &Path, oid: &str) -> Result<()> {
    ensure!(
        git(
            repo,
            &[
                "for-each-ref",
                &format!("--contains={oid}"),
                "--count=1",
                "--format=%(refname)",
                "refs/remotes",
            ],
        )?
        .is_empty(),
        "A remote-tracking branch already contains this commit; fetch and review again if needed"
    );
    Ok(())
}

impl Plan {
    fn read(repo: &Path, oid: &str, kind: Kind, input: &str) -> Result<Self> {
        if matches!(kind, Kind::Branch | Kind::Tag) {
            ensure!(
                !input.is_empty() && !input.starts_with('-'),
                "Enter a valid name"
            );
            let namespace = if kind == Kind::Branch {
                ensure!(
                    git(repo, &["check-ref-format", "--branch", input])? == input,
                    "Enter a literal branch name"
                );
                "heads"
            } else {
                "tags"
            };
            let reference = format!("refs/{namespace}/{input}");
            git(repo, &["check-ref-format", &reference]).context("Invalid ref name")?;
            ensure!(
                optional(repo, &["rev-parse", "--verify", "--quiet", &reference])?.is_none(),
                "{} already exists: {input}",
                if kind == Kind::Branch {
                    "Branch"
                } else {
                    "Tag"
                }
            );
            return Ok(Self::Ref);
        }
        ensure_idle(repo)?;
        let head = Head::read(repo)?;
        if kind == Kind::Checkout {
            ensure!(
                head.branch.is_some() || head.oid != oid,
                "Already at this revision"
            );
            return Ok(Self::Checkout(head));
        }
        ensure!(
            head.branch
                .as_deref()
                .is_some_and(|b| b.starts_with("refs/heads/"))
                && !head.oid.is_empty(),
            "Check out a local branch first"
        );
        let info = git(repo, &["rev-list", "--parents", "-1", oid, "--"])?;
        let parents = info.split_whitespace().skip(1).collect::<Vec<_>>();
        ensure!(
            !parents.is_empty() || git(repo, &["rev-parse", "--is-shallow-repository"])? == "false",
            "Cannot change history at a shallow clone boundary"
        );
        if matches!(kind, Kind::CherryPick | Kind::Revert) {
            ensure!(
                parents.len() <= 1,
                "{} a merge requires choosing a mainline parent",
                if kind == Kind::CherryPick {
                    "Cherry-picking"
                } else {
                    "Reverting"
                }
            );
            clean(repo)?;
            protect_paths(repo, oid, true)?;
            return Ok(if kind == Kind::CherryPick {
                Self::CherryPick(head)
            } else {
                Self::Revert(head)
            });
        }
        if matches!(kind, Kind::Undo | Kind::Drop) {
            ensure!(
                head.oid == oid,
                "{} is only available for the current branch tip",
                kind.title()
            );
        }
        ensure_unpublished(repo, oid)?;
        if matches!(kind, Kind::Undo | Kind::Drop) {
            if kind == Kind::Drop {
                clean(repo)?;
                protect_paths(repo, oid, true)?;
            }
            return Ok(Self::RemoveTip {
                head,
                parent: parents.first().map(|parent| (*parent).to_owned()),
                merge: parents.len() > 1,
            });
        }
        let ancestor = command(repo)
            .args(["merge-base", "--is-ancestor", oid, &head.oid])
            .output()?;
        ensure!(
            ancestor.status.code() != Some(1),
            "Selected commit is outside the current branch"
        );
        checked(ancestor)?;
        let suffix = git(
            repo,
            &[
                "rev-list",
                "--reverse",
                "--topo-order",
                "--ancestry-path",
                &format!("{oid}..{}", head.oid),
                "--",
            ],
        )?;
        let mut commits = vec![oid.to_owned()];
        commits.extend(suffix.lines().map(str::to_owned));
        Ok(Self::Reword { head, commits })
    }
}

pub(super) struct Review {
    target: Target,
    kind: Kind,
    input: String,
    plan: Plan,
    picks: Vec<Target>,
}

impl Review {
    pub fn is_cherry_pick(&self) -> bool {
        self.kind == Kind::CherryPick
    }

    pub fn prepare_picks(c: &Context, mut picks: Vec<Target>) -> Result<(Self, String)> {
        let target = picks.first().context("No commits selected")?.clone();
        let repo = target.repo(c)?;
        let plan = Plan::read(&repo.path, &target.oid, Kind::CherryPick, "")?;
        for pick in &picks[1..] {
            ensure!(
                pick.root == target.root,
                "Select commits from one repository"
            );
            pick.repo(c)?;
            ensure!(
                Plan::read(&repo.path, &pick.oid, Kind::CherryPick, "")? == plan,
                "Branch or HEAD changed; review again"
            );
        }
        if picks.len() > 1 {
            // Walk all source tips together so even ancestry through an unselected
            // commit constrains execution order. Ignore unrelated history in the review.
            let mut args = vec!["rev-list", "--date-order", "--reverse"];
            args.extend(picks.iter().map(|pick| pick.oid.as_str()));
            args.push("--");
            let order = git(&repo.path, &args)?;
            let positions: std::collections::HashMap<_, _> =
                order.lines().enumerate().map(|(i, oid)| (oid, i)).collect();
            picks.sort_by_key(|pick| positions.get(pick.oid.as_str()).copied());
        }
        let Plan::CherryPick(head) = &plan else {
            unreachable!()
        };
        let mut body = format!(
            "Apply to {} / {}\nCurrent branch\n",
            plain(repo.name.as_bytes()),
            plain(head.label().as_bytes())
        );
        for (i, pick) in picks.iter().enumerate() {
            use std::fmt::Write as _;
            writeln!(
                body,
                "{}  {}  {}",
                i + 1,
                pick.short(),
                plain(pick.subject.as_bytes())
            )?;
        }
        Ok((
            Self {
                target,
                kind: Kind::CherryPick,
                input: String::new(),
                plan,
                picks,
            },
            body,
        ))
    }

    pub fn prepare(
        c: &Context,
        target: Target,
        kind: Kind,
        input: String,
    ) -> Result<(Self, String)> {
        let repo = target.repo(c)?;
        let plan = Plan::read(&repo.path, &target.oid, kind, &input)?;
        if kind == Kind::Edit {
            ensure!(
                !input.trim().is_empty() && !input.contains('\0'),
                "Enter a commit message"
            );
            ensure!(
                input != message(&repo.path, &target.oid)?,
                "Commit message is unchanged"
            );
        }
        let effect = match &plan {
            Plan::Ref => format!(
                "Create {}: {input}\nPoint it at the selected commit.",
                if kind == Kind::Branch { "local branch" } else { "lightweight tag" },
            ),
            Plan::Checkout(head) => format!(
                "From: {}\nCheck out the selected commit in detached HEAD.",
                head.label(),
            ),
            Plan::CherryPick(head) => format!(
                "Apply the selected commit as a new commit on {}.\nCurrent HEAD: {}\nSource commit: {}\nIf Git stops, review Continue or Abort separately in Conflicts.",
                head.label(),
                head.oid,
                target.oid,
            ),
            Plan::RemoveTip { head, parent, merge } => {
                let effect = if kind == Kind::Drop {
                    "Remove this commit and discard its committed changes.\nTracked files and index return to the previous tree."
                } else {
                    "Remove this commit and keep its changes staged.\nExisting index and working files stay intact."
                };
                let destination = parent.as_ref().map_or_else(
                    || "The branch returns to its initial, unborn state.".to_owned(),
                    |parent| format!("New tip: {parent}"),
                );
                let mainline = if *merge { "\nThis merge is removed relative to its first parent." } else { "" };
                format!("Branch: {}\n{effect}\n{destination}{mainline}", head.label())
            }
            Plan::Revert(head) => format!(
                "Create a new commit on {} that reverses the selected commit.\nIf Git stops, resolve the files in Conflicts.",
                head.label(),
            ),
            Plan::Reword { head, commits } => {
                let later = commits.len() - 1;
                format!(
                    "Edit the message on {} and rewrite {later} descendant commit(s).\nFile trees, merge topology, index and working files stay intact.\nOther branches and tags stay in place. Rewritten signatures are removed.\n\n{input}",
                    head.label(),
                )
            }
        };
        let publication = if matches!(kind, Kind::Undo | Kind::Drop | Kind::Edit) {
            "\n\nUnpublished according to local remote-tracking refs; this does not fetch."
        } else {
            ""
        };
        let body = format!(
            "Repository: {}\nCommit: {} · {}\n{effect}{publication}\n\nPath: {}",
            repo.name,
            target.short(),
            target.subject,
            repo.path.display(),
        );
        Ok((
            Self {
                target,
                kind,
                input,
                plan,
                picks: Vec::new(),
            },
            body,
        ))
    }

    pub fn execute(self, c: &Context) -> Result<(NoticeLevel, String)> {
        let repo = self.target.repo(c)?;
        let oid = self.target.oid.as_str();
        let current = Plan::read(&repo.path, oid, self.kind, &self.input)
            .context("Repository changed; review again")?;
        ensure!(current == self.plan, "Branch or HEAD changed; review again");

        for pick in &self.picks {
            if pick.oid != oid {
                pick.repo(c)?;
                ensure!(
                    Plan::read(&repo.path, &pick.oid, self.kind, "")
                        .context("Repository changed; review again")?
                        == self.plan,
                    "Branch or HEAD changed; review again"
                );
            }
        }

        let mut cmd = command(&repo.path);
        cmd.args([
            "-c",
            "submodule.recurse=false",
            "-c",
            "merge.directoryRenames=false",
        ]);
        let result = match &self.plan {
            Plan::Reword { head, commits } => {
                reword::execute(&repo.path, head, commits, &self.input)
            }
            Plan::RemoveTip { head, parent, .. } => {
                tip::execute(&repo.path, head, parent.as_deref(), self.kind)
            }
            _ => {
                match &self.plan {
                    Plan::Ref if self.kind == Kind::Branch => {
                        cmd.args(["branch", "--no-track", "--", &self.input, oid]);
                    }
                    Plan::Ref => {
                        cmd.args(["-c", "tag.gpgSign=false", "tag", "--", &self.input, oid]);
                    }
                    Plan::Checkout(_) => {
                        cmd.args([
                            "switch",
                            "--detach",
                            "--no-overwrite-ignore",
                            "--no-recurse-submodules",
                            "--",
                            oid,
                        ]);
                    }
                    Plan::CherryPick(_) => {
                        cmd.args([
                            "-c",
                            "merge.autoStash=false",
                            "-c",
                            "rebase.autoStash=false",
                            "cherry-pick",
                            "--no-edit",
                            "--no-rerere-autoupdate",
                            "--",
                        ]);
                        if self.picks.is_empty() {
                            cmd.arg(oid);
                        } else {
                            cmd.args(self.picks.iter().map(|pick| &pick.oid));
                        }
                    }
                    Plan::Revert(_) => {
                        cmd.args(["revert", "--no-edit", "--no-rerere-autoupdate", "--", oid]);
                    }
                    Plan::RemoveTip { .. } | Plan::Reword { .. } => unreachable!(),
                }
                checked(cmd.output().context("Cannot start Git")?).map(|_| ())
            }
        };
        if let Plan::CherryPick(head) = &self.plan {
            if !self.picks.is_empty() {
                let count = self.picks.len();
                let destination = format!("{} / {}", repo.name, head.label());
                if result.is_ok() {
                    return Ok((
                        NoticeLevel::Success,
                        format!("Cherry-picked {count} onto {destination}"),
                    ));
                }
                let range = format!("{}..HEAD", head.oid);
                let applied = git(
                    &repo.path,
                    &["rev-list", "--count", "--first-parent", &range],
                )
                .ok()
                .and_then(|n| n.parse::<usize>().ok());
                let pending = revpath(&repo.path, "--absolute-git-dir")
                    .is_some_and(|dir| dir.join("CHERRY_PICK_HEAD").exists());
                if pending {
                    let stopped = if git_bytes(&repo.path, &["ls-files", "--unmerged", "-z"])
                        .is_ok_and(|paths| !paths.is_empty())
                    {
                        "1 conflicted"
                    } else {
                        "1 stopped"
                    };
                    if let Some(applied) = applied.filter(|n| *n < count) {
                        let remaining = count - applied - 1;
                        return Ok((
                            if applied > 0 {
                                NoticeLevel::Warning
                            } else {
                                NoticeLevel::Error
                            },
                            format!(
                                "{applied} applied · {stopped} · {remaining} remaining · 6 Conflicts · {destination}"
                            ),
                        ));
                    }
                    return Ok((
                        NoticeLevel::Warning,
                        format!("Cherry-pick stopped · 6 Conflicts · {destination}"),
                    ));
                }
                if let Some(applied) = applied.filter(|n| *n > 0 && *n <= count) {
                    // Do not offer the original selection again after Git has
                    // already created commits, even if it left no conflict marker.
                    let error = result.unwrap_err();
                    return Ok((
                        NoticeLevel::Warning,
                        format!(
                            "{applied}/{count} applied · Cherry-pick stopped in {destination} · {error:#}"
                        ),
                    ));
                }
            }
        }
        result.with_context(|| format!("{} failed in {}", self.kind.title(), repo.name))?;
        let detail = if matches!(self.kind, Kind::Branch | Kind::Tag) {
            &self.input
        } else {
            self.target.short()
        };
        Ok((
            NoticeLevel::Success,
            format!("{} completed in {}: {detail}", self.kind.title(), repo.name),
        ))
    }
}

fn write_temp(data: &[u8]) -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(data)?;
    Ok(file)
}

fn git_input(repo: &Path, args: &[&str], data: &[u8]) -> Result<String> {
    let file = write_temp(data)?;
    let output = checked(command(repo).args(args).stdin(file.reopen()?).output()?)?;
    Ok(String::from_utf8(output)?.trim_end().to_owned())
}

fn checked(output: Output) -> Result<Vec<u8>> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!("{}", format!("{stderr}\n{stdout}").trim());
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests;
