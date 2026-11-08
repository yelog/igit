//! A review binds one Git operation to one commit and working tree.
use super::model::{plain, Rev};
use crate::core::{command, git, git_bytes, revpath, Context, Repo};
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    collections::BTreeSet,
    io::Write,
    ops::Bound::{Included, Unbounded},
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::NamedTempFile;

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
    path: PathBuf,
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
    Undo {
        head: Head,
        parent: String,
        published: bool,
    },
    Revert(Head),
    Amend(Head),
    Replay {
        head: Head,
        base: Option<String>,
        commits: Vec<String>,
        published: bool,
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

fn path_key(path: &[u8], ignore_case: bool) -> Vec<u8> {
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
        .filter(|(_, b)| **b == b'/')
        .map(|(i, _)| &path[..i])
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

// Cherry-pick, revert and rebase do not offer switch's --no-overwrite-ignore option.
fn protect_paths(repo: &Path, span: &str, single: bool) -> Result<()> {
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
        || git(repo, &["config", "--bool", "--get", "core.ignoreCase"]).is_ok_and(|s| s == "true");
    let paths = others
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| p.strip_suffix(b"/").unwrap_or(p))
        .chain(indexed.split(|b| *b == 0).filter_map(|entry| match entry {
            [b'S' | b'a'..=b'z', b' ', path @ ..] => Some(path),
            _ => None,
        }))
        .map(|p| path_key(p, ignore_case))
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
        "--name-only",
        "--root",
        "-z",
    ];
    if single {
        args.push("-1");
    }
    args.extend([span, "--"]);
    let touched = git_bytes(repo, &args)?;
    for path in touched.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        ensure!(
            !overlaps(&paths, &path_key(path, ignore_case)),
            "Local file may be overwritten at {}; move it aside or check its index flags",
            plain(path)
        );
    }
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
        if kind == Kind::Undo {
            ensure!(
                head.oid == oid,
                "Undo Commit is only available for the current branch tip"
            );
            ensure!(
                parents.len() == 1,
                "Undo Commit needs a commit with one parent"
            );
        }
        let published = !git(
            repo,
            &[
                "for-each-ref",
                &format!("--contains={oid}"),
                "--count=1",
                "--format=%(refname)",
                "refs/remotes",
            ],
        )?
        .is_empty();
        if kind == Kind::Undo {
            return Ok(Self::Undo {
                head,
                parent: parents[0].to_owned(),
                published,
            });
        }
        if kind == Kind::Edit {
            ensure!(
                !published,
                "A remote-tracking branch already contains this commit"
            );
            if head.oid == oid {
                return Ok(Self::Amend(head));
            }
        }
        ensure!(parents.len() <= 1, "Cannot replay a merge commit");
        let ancestor = command(repo)
            .args(["merge-base", "--is-ancestor", oid, &head.oid])
            .output()?;
        ensure!(
            ancestor.status.code() != Some(1),
            "Selected commit is outside the current branch"
        );
        checked(ancestor)?;
        let base = parents.first().map(|p| (*p).to_owned());
        let suffix = git(
            repo,
            &[
                "rev-list",
                "--parents",
                "--reverse",
                "--first-parent",
                &format!("{oid}..{}", head.oid),
                "--",
            ],
        )?;
        let mut commits = vec![oid.to_owned()];
        for line in suffix.lines() {
            let mut fields = line.split_whitespace();
            let commit = fields.next().context("Invalid commit range")?;
            ensure!(
                fields.next() == commits.last().map(String::as_str) && fields.next().is_none(),
                "Cannot rewrite across a merge commit"
            );
            commits.push(commit.to_owned());
        }
        ensure!(
            kind != Kind::Drop || base.is_some() || commits.len() > 1,
            "Cannot drop the only commit on this branch"
        );
        clean(repo)?;
        let span = base
            .as_ref()
            .map_or_else(|| head.oid.clone(), |p| format!("{p}..{}", head.oid));
        protect_paths(repo, &span, false)?;
        Ok(Self::Replay {
            head,
            base,
            commits,
            published,
        })
    }
}

pub(super) struct Review {
    target: Target,
    kind: Kind,
    input: String,
    plan: Plan,
}

impl Review {
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
            Plan::Undo { head, parent, .. } => format!(
                "Undo the tip of {} and keep its changes.\nIndex and working files stay intact.\nNew tip: {parent}",
                head.label()
            ),
            Plan::Revert(head) => format!(
                "Create a new commit on {} that reverses the selected commit.\nIf Git stops, resolve the files in Conflicts.",
                head.label(),
            ),
            Plan::Amend(head) => format!(
                "Rewrite the tip of {} with this message:\n\n{input}",
                head.label(),
            ),
            Plan::Replay { head, commits, .. } => {
                let later = commits.len() - 1;
                let change = if kind == Kind::Drop {
                    "Remove the selected commit and its changes.".to_owned()
                } else {
                    format!("Replace the selected commit's message:\n\n{input}")
                };
                format!(
                    "Rewrite {} and replay {later} later commit(s).\n{change}",
                    head.label(),
                )
            }
        };
        let published = if matches!(
            &plan,
            Plan::Undo {
                published: true,
                ..
            } | Plan::Replay {
                published: true,
                ..
            }
        ) {
            "\n\nA remote-tracking branch contains this commit. A regular push may be rejected."
        } else {
            ""
        };
        let body = format!(
            "Repository: {}\nCommit: {} · {}\n{effect}{published}\n\nPath: {}",
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
            },
            body,
        ))
    }

    pub fn execute(self, c: &Context) -> Result<String> {
        let repo = self.target.repo(c)?;
        let oid = self.target.oid.as_str();
        let current = Plan::read(&repo.path, oid, self.kind, &self.input)
            .context("Repository changed; review again")?;
        ensure!(current == self.plan, "Branch or HEAD changed; review again");

        let mut cmd = command(&repo.path);
        cmd.args([
            "-c",
            "submodule.recurse=false",
            "-c",
            "merge.directoryRenames=false",
        ]);
        let mut message_file = None;
        let mut todo_file = None;
        if self.kind == Kind::Edit {
            let file = write_temp(format!("{}\n", self.input).as_bytes())?;
            cmd.args([
                "-c",
                "commit.cleanup=verbatim",
                "-c",
                "notes.rewrite.amend=false",
                "-c",
                "i18n.commitEncoding=UTF-8",
            ]);
            cmd.env("IGIT_LOG_MESSAGE", file.path());
            message_file = Some(file);
        }
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
                    oid,
                ]);
            }
            Plan::Undo { parent, .. } => {
                cmd.args(["reset", "--soft", parent]);
            }
            Plan::Revert(_) => {
                cmd.args(["revert", "--no-edit", "--no-rerere-autoupdate", "--", oid]);
            }
            Plan::Amend(_) => {
                let file = message_file.as_ref().context("Message file missing")?;
                cmd.args(["commit", "--amend", "--only", "--allow-empty", "-F"])
                    .arg(file.path());
            }
            Plan::Replay { base, commits, .. } => {
                let mut todo = format!(
                    "{} {oid}\n",
                    if self.kind == Kind::Drop {
                        "drop"
                    } else {
                        "reword"
                    },
                );
                for commit in &commits[1..] {
                    todo.push_str(&format!("pick {commit}\n"));
                }
                let file = write_temp(todo.as_bytes())?;
                configure_rebase(&mut cmd, base.as_deref(), &file, message_file.is_some());
                todo_file = Some(file);
            }
        }
        let result = checked(cmd.output().context("Cannot start Git")?);
        drop((message_file, todo_file));
        result.with_context(|| format!("{} failed in {}", self.kind.title(), repo.name))?;
        let detail = if matches!(self.kind, Kind::Branch | Kind::Tag) {
            &self.input
        } else {
            self.target.short()
        };
        Ok(format!(
            "{} completed in {}: {detail}",
            self.kind.title(),
            repo.name
        ))
    }
}

fn write_temp(data: &[u8]) -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(data)?;
    Ok(file)
}

fn configure_rebase(cmd: &mut Command, base: Option<&str>, todo: &NamedTempFile, edit: bool) {
    cmd.args([
        "-c",
        "rebase.updateRefs=false",
        "-c",
        "notes.rewrite.rebase=false",
        "rebase",
        "--interactive",
        "--no-autosquash",
        "--no-autostash",
        "--no-rebase-merges",
        "--keep-empty",
        "--empty=keep",
        "--reapply-cherry-picks",
        "--no-rerere-autoupdate",
        base.unwrap_or("--root"),
    ])
    // Git appends its editor's file argument to these fixed shell commands.
    .env("GIT_SEQUENCE_EDITOR", "cat \"$IGIT_LOG_TODO\" >")
    .env("IGIT_LOG_TODO", todo.path());
    if edit {
        cmd.env("GIT_EDITOR", "cat \"$IGIT_LOG_MESSAGE\" >");
    }
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
