//! Review and run a workspace update without routing through the Push dialog.
mod view;
#[cfg(test)]
mod view_tests;

use crate::{
    core::{command, git, redact_remote_error, revpath, Context},
    ui::{
        help::{HelpContent, HelpSection},
        inside, NoticeLevel,
    },
};
use anyhow::{bail, ensure, Context as _, Result};
use crossterm::event::{Event, KeyCode as K, KeyModifiers as M, MouseEventKind};
use ratatui::layout::Rect;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Pull,
    Fetch,
}

impl Kind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Pull => "Pull",
            Self::Fetch => "Fetch",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Merge,
    Rebase,
}

#[derive(PartialEq, Eq)]
struct Remote {
    name: String,
    url: String,
}

impl Remote {
    fn read(repo: &Path, name: &str) -> Result<Self> {
        let url = if name == "." {
            String::new()
        } else {
            git(repo, &["remote", "get-url", "--", name]).context("Remote URL unavailable")?
        };
        Ok(Self {
            name: name.to_owned(),
            url,
        })
    }
}

#[derive(PartialEq, Eq)]
enum Plan {
    Pull {
        branch: String,
        head: String,
        upstream: String,
        remote: Remote,
    },
    Fetch(Vec<Remote>),
}

impl Plan {
    fn read(repo: &Path, kind: Kind) -> Result<Self> {
        ensure!(
            revpath(repo, "--show-toplevel").as_deref() == Some(repo),
            "Worktree unavailable; rescan the workspace"
        );
        if kind == Kind::Fetch {
            let names = git(repo, &["remote"]).context("Cannot read configured remotes")?;
            ensure!(!names.is_empty(), "No remotes configured");
            return Ok(Self::Fetch(
                names
                    .lines()
                    .map(|name| Remote::read(repo, name))
                    .collect::<Result<_>>()?,
            ));
        }

        let dir = revpath(repo, "--absolute-git-dir").context("Git directory unavailable")?;
        ensure!(
            ![
                "MERGE_HEAD",
                "rebase-merge",
                "rebase-apply",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
                "sequencer",
            ]
            .iter()
            .any(|name| dir.join(name).exists()),
            "Finish the current Git operation first"
        );
        let reference = git(repo, &["symbolic-ref", "-q", "HEAD"]).context("Detached HEAD")?;
        let branch = reference.strip_prefix("refs/heads/").context("No branch")?;
        let head = git(repo, &["rev-parse", "--verify", "HEAD"]).context("No commits yet")?;
        let upstream = git(
            repo,
            &["config", "--get-all", &format!("branch.{branch}.merge")],
        )
        .context("No upstream configured")?;
        ensure!(!upstream.contains('\n'), "Multiple upstream branches");
        git(repo, &["check-ref-format", &upstream]).context("Invalid upstream branch")?;
        let name = git(
            repo,
            &["config", "--get", &format!("branch.{branch}.remote")],
        )
        .context("No upstream remote configured")?;
        Ok(Self::Pull {
            branch: branch.to_owned(),
            head,
            upstream,
            remote: Remote::read(repo, &name)?,
        })
    }

    fn destination(&self) -> String {
        match self {
            Self::Pull {
                upstream, remote, ..
            } => format!(
                "{}:{}",
                remote.name,
                upstream.strip_prefix("refs/heads/").unwrap_or(upstream)
            ),
            Self::Fetch(remotes) => remotes
                .iter()
                .map(|remote| remote.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    fn execute(&self, repo: &Path, kind: Kind, strategy: Strategy) -> Result<()> {
        let current = Self::read(repo, kind)
            .map_err(|error| anyhow::anyhow!("Repository changed; review again ({error})"))?;
        ensure!(
            current == *self,
            "Branch, HEAD or remotes changed; review again"
        );
        let (args, remotes): (Vec<&str>, &[_]) = match self {
            Self::Pull {
                upstream, remote, ..
            } => (
                vec![
                    "pull",
                    "--no-recurse-submodules",
                    "--no-autostash",
                    "--ff",
                    if strategy == Strategy::Merge {
                        "--no-rebase"
                    } else {
                        "--rebase"
                    },
                    "--",
                    &remote.name,
                    upstream,
                ],
                std::slice::from_ref(remote),
            ),
            Self::Fetch(remotes) => (vec!["fetch", "--all", "--no-recurse-submodules"], remotes),
        };
        let output = command(repo)
            .args(args)
            .output()
            .context("Cannot launch Git")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let details = format!("{}\n{}", stderr.trim(), stdout.trim());
            let safe = remotes.iter().fold(details, |msg, remote| {
                redact_remote_error(&msg, &remote.url)
            });
            ensure!(!safe.trim().is_empty(), "Git exited with {}", output.status);
            bail!("{}", safe.trim());
        }
        Ok(())
    }
}

struct Target {
    path: PathBuf,
    plan: Result<Plan, String>,
}

enum Outcome {
    Ready,
    Succeeded,
    Failed(String),
    Skipped(String),
}

struct Comparison {
    branch: String,
    upstream: String,
    tracking: Option<(usize, usize)>,
}

impl Comparison {
    fn read(repo: &Path) -> Self {
        let branch = crate::core::branch(repo);
        let upstream =
            git(repo, &["rev-parse", "--symbolic-full-name", "@{upstream}"]).unwrap_or_default();
        let tracking = if upstream.is_empty() {
            None
        } else {
            git(
                repo,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("HEAD...{upstream}"),
                    "--",
                ],
            )
            .ok()
            .and_then(|output| {
                let mut counts = output.split_whitespace();
                Some((counts.next()?.parse().ok()?, counts.next()?.parse().ok()?))
            })
        };
        let upstream = upstream
            .strip_prefix("refs/remotes/")
            .or_else(|| upstream.strip_prefix("refs/heads/"))
            .unwrap_or(&upstream)
            .to_owned();
        Self {
            branch,
            upstream,
            tracking,
        }
    }
}

struct RepositoryRow {
    root: usize,
    name: String,
    submodule: bool,
    comparison: Comparison,
    destination: String,
    outcome: Outcome,
}

struct Report {
    summary: String,
    outcomes: Vec<Outcome>,
    comparisons: Vec<Comparison>,
}

fn execute(kind: Kind, strategy: Strategy, targets: Vec<Target>) -> Report {
    let (mut succeeded, mut failed, mut skipped) = (0, 0, 0);
    let mut outcomes = Vec::with_capacity(targets.len());
    let mut comparisons = Vec::with_capacity(targets.len());
    for Target { path, plan } in targets {
        let result = match plan {
            Ok(plan) => match plan.execute(&path, kind, strategy) {
                Ok(()) => {
                    succeeded += 1;
                    Outcome::Succeeded
                }
                Err(error) => {
                    failed += 1;
                    Outcome::Failed(error.to_string())
                }
            },
            Err(reason) => {
                skipped += 1;
                Outcome::Skipped(reason)
            }
        };
        outcomes.push(result);
        comparisons.push(Comparison::read(&path));
    }
    let counts = format!("{succeeded} succeeded · {failed} failed · {skipped} skipped");
    Report {
        outcomes,
        comparisons,
        summary: format!("{}: {counts}", kind.label()),
    }
}

enum Stage {
    Review(Vec<Target>),
    Running(Receiver<Report>),
    Results,
}

const CANCEL: usize = 0;
const CONFIRM: usize = 1;
const MERGE: usize = 2;
const REBASE: usize = 3;
const ROW: usize = 4;

pub(super) struct Dialog {
    kind: Kind,
    strategy: Strategy,
    stage: Stage,
    ready: usize,
    rows: Vec<RepositoryRow>,
    pos: usize,
    offset: usize,
    list_area: Rect,
    detail_area: Rect,
    detail_scroll: usize,
    detail_max_scroll: usize,
    confirm_focused: bool,
}

impl Dialog {
    pub(super) fn new(kind: Kind, ctx: &Context) -> Self {
        let mut rows = Vec::with_capacity(ctx.scope.len());
        let targets: Vec<_> = ctx
            .scope
            .iter()
            .map(|&index| {
                let repo = &ctx.repos[index];
                let plan = Plan::read(&repo.path, kind).map_err(|error| error.to_string());
                rows.push(RepositoryRow {
                    root: index,
                    name: repo.name.clone(),
                    submodule: repo.submodule,
                    comparison: Comparison::read(&repo.path),
                    destination: plan.as_ref().map(Plan::destination).unwrap_or_default(),
                    outcome: match &plan {
                        Ok(_) => Outcome::Ready,
                        Err(reason) => Outcome::Skipped(reason.clone()),
                    },
                });
                Target {
                    path: repo.path.clone(),
                    plan,
                }
            })
            .collect();
        let ready = targets.iter().filter(|target| target.plan.is_ok()).count();
        Self {
            kind,
            strategy: Strategy::Merge,
            stage: Stage::Review(targets),
            ready,
            rows,
            pos: 0,
            offset: 0,
            list_area: Rect::default(),
            detail_area: Rect::default(),
            detail_scroll: 0,
            detail_max_scroll: 0,
            confirm_focused: ready > 0,
        }
    }

    pub(super) fn poll(&mut self) -> Option<(Kind, NoticeLevel, String)> {
        let Stage::Running(job) = &self.stage else {
            return None;
        };
        let report = match job.try_recv() {
            Ok(report) => report,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Report {
                summary: format!("{} worker stopped before reporting results", self.kind.label()),
                comparisons: vec![],
                outcomes: self.rows.iter().map(|row| match &row.outcome {
                    Outcome::Skipped(reason) => Outcome::Skipped(reason.clone()),
                    _ => Outcome::Failed("Update worker stopped before reporting results. Inspect the repository before retrying.".into()),
                }).collect(),
            },
        };
        self.stage = Stage::Results;
        for (row, outcome) in self.rows.iter_mut().zip(report.outcomes) {
            row.outcome = outcome;
        }
        for (row, comparison) in self.rows.iter_mut().zip(report.comparisons) {
            row.comparison = comparison;
        }
        self.select(
            self.rows
                .iter()
                .position(|row| matches!(row.outcome, Outcome::Failed(_)))
                .unwrap_or(0),
        );
        self.detail_scroll = 0;
        let failed = self
            .rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Failed(_)))
            .count();
        let succeeded = self
            .rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Succeeded))
            .count();
        let level = if failed > 0 && succeeded == 0 {
            NoticeLevel::Error
        } else if failed > 0
            || self
                .rows
                .iter()
                .any(|row| matches!(row.outcome, Outcome::Skipped(_)))
        {
            NoticeLevel::Warning
        } else {
            NoticeLevel::Success
        };
        Some((self.kind, level, report.summary))
    }

    fn pull_review(&self) -> bool {
        self.kind == Kind::Pull && matches!(self.stage, Stage::Review(_))
    }

    fn select(&mut self, pos: usize) {
        let pos = pos.min(self.rows.len().saturating_sub(1));
        if self.pos != pos {
            self.pos = pos;
            self.detail_scroll = 0;
            self.detail_max_scroll = 0;
        }
    }

    pub(super) fn event(&mut self, event: &Event, hit: Option<usize>) -> bool {
        let key = match event {
            Event::Key(k) if k.modifiers.is_empty() || k.modifiers == M::SHIFT => Some(k.code),
            _ => None,
        };
        if let Some(index) = hit.and_then(|id| id.checked_sub(ROW)) {
            self.select(index);
        }
        match event {
            Event::Mouse(m)
                if matches!(
                    m.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) =>
            {
                let up = m.kind == MouseEventKind::ScrollUp;
                if inside(self.detail_area, m.column, m.row) {
                    self.detail_scroll = if up {
                        self.detail_scroll.saturating_sub(3)
                    } else {
                        (self.detail_scroll + 3).min(self.detail_max_scroll)
                    };
                } else if up {
                    self.select(self.pos.saturating_sub(3));
                } else {
                    self.select(self.pos.saturating_add(3));
                }
            }
            _ => match key {
                Some(K::Up | K::Char('k')) => self.select(self.pos.saturating_sub(1)),
                Some(K::Down | K::Char('j')) => self.select(self.pos.saturating_add(1)),
                Some(K::PageUp) => self.select(
                    self.pos
                        .saturating_sub(usize::from(self.list_area.height.max(1))),
                ),
                Some(K::PageDown) => self.select(
                    self.pos
                        .saturating_add(usize::from(self.list_area.height.max(1))),
                ),
                Some(K::Home) => self.select(0),
                Some(K::End | K::Char('G')) => self.select(self.rows.len().saturating_sub(1)),
                Some(K::Char('K')) => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                Some(K::Char('J')) => {
                    self.detail_scroll = (self.detail_scroll + 1).min(self.detail_max_scroll)
                }
                _ => (),
            },
        }
        match &mut self.stage {
            Stage::Running(_) => return false,
            Stage::Results => return hit == Some(CANCEL) || matches!(key, Some(K::Esc | K::Enter)),
            Stage::Review(_) => (),
        }
        if hit == Some(CANCEL) || key == Some(K::Esc) {
            return true;
        }
        if self.pull_review() {
            if hit == Some(MERGE) || key == Some(K::Char('m')) {
                self.strategy = Strategy::Merge;
            }
            if hit == Some(REBASE) || key == Some(K::Char('r')) {
                self.strategy = Strategy::Rebase;
            }
        }
        if self.ready > 0
            && matches!(
                key,
                Some(K::Tab | K::BackTab | K::Left | K::Right | K::Char('h' | 'l'))
            )
        {
            self.confirm_focused = !self.confirm_focused;
        }
        if key == Some(K::Enter) && !self.confirm_focused {
            return true;
        }
        if self.ready > 0 && (hit == Some(CONFIRM) || key == Some(K::Enter)) {
            let Stage::Review(targets) = &mut self.stage else {
                unreachable!()
            };
            let targets = std::mem::take(targets);
            let (kind, strategy) = (self.kind, self.strategy);
            let (tx, rx) = mpsc::channel();
            self.stage = Stage::Running(rx);
            std::thread::spawn(move || {
                let _ = tx.send(execute(kind, strategy, targets));
            });
        }
        false
    }

    pub(super) fn keymap(&self) -> HelpContent {
        let (title, bindings): (_, &[_]) = match self.stage {
            Stage::Review(_) => (
                "Update / Review",
                &[
                    ("j", "Next repository"),
                    ("k", "Previous repository"),
                    ("J", "Scroll details down"),
                    ("K", "Scroll details up"),
                    ("PageDown", "Next repository page"),
                    ("PageUp", "Previous repository page"),
                    ("h", "Switch focused button"),
                    ("l", "Switch focused button"),
                    ("Tab", "Switch focused button"),
                    ("Enter", "Activate selected button"),
                    ("Esc", "Cancel workspace update"),
                ],
            ),
            Stage::Running(_) => (
                "Update / Running",
                &[
                    ("j", "Next repository"),
                    ("k", "Previous repository"),
                    ("J", "Scroll details down"),
                    ("K", "Scroll details up"),
                ],
            ),
            Stage::Results => (
                "Update / Results",
                &[
                    ("j", "Next repository"),
                    ("k", "Previous repository"),
                    ("J", "Scroll details down"),
                    ("K", "Scroll details up"),
                    ("PageDown", "Next repository page"),
                    ("PageUp", "Previous repository page"),
                    ("Enter", "Close results"),
                    ("Esc", "Close results"),
                ],
            ),
        };
        let sections = if self.pull_review() {
            vec![HelpSection::new(
                "Pull strategy",
                &[
                    ("m", "Merge incoming changes"),
                    ("r", "Rebase current branches"),
                ],
            )]
        } else {
            vec![]
        };
        HelpContent::new(HelpSection::new(title, bindings), sections)
    }
}
