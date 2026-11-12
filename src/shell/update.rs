//! Review and run a workspace update without routing through the Push dialog.
mod view;
#[cfg(test)]
mod view_tests;

use crate::{
    core::{
        command, git,
        loading::{self, Load},
        redact_remote_error, revpath,
        schedule::{self, RepoLock},
        strategy::{self, Strategy},
        Context,
    },
    ui::{
        help::{HelpContent, HelpSection},
        inside,
        strategy::{Choice, Selector},
        NoticeLevel,
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
            git(repo, &["remote", "get-url", "--", name])
                .map_err(|error| anyhow::anyhow!(redact_remote_error(&error.to_string(), "")))
                .context("Remote URL unavailable")?
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
        tracking: String,
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
        let tracking = git(repo, &["for-each-ref", "--format=%(upstream)", &reference])
            .context("Cannot read upstream tracking reference")?;
        ensure!(
            (name == "." && tracking == upstream)
                || (!name.starts_with('-')
                    && git(repo, &["remote"])?.lines().any(|remote| remote == name)
                    && tracking.starts_with("refs/remotes/")),
            "Configure an upstream tracking reference before pulling"
        );
        git(repo, &["check-ref-format", &tracking])
            .context("Invalid upstream tracking reference")?;
        Ok(Self::Pull {
            branch: branch.to_owned(),
            head,
            upstream,
            tracking,
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

    fn revalidate(&self, repo: &Path, kind: Kind) -> Result<()> {
        let current = Self::read(repo, kind)
            .map_err(|error| anyhow::anyhow!("Repository changed; review again ({error})"))?;
        ensure!(
            current == *self,
            "Branch, HEAD or remotes changed; review again"
        );
        Ok(())
    }

    fn execute(&self, repo: &Path, kind: Kind, strategy: Strategy) -> Result<()> {
        self.revalidate(repo, kind)?;
        let remotes = match self {
            Self::Pull {
                upstream,
                tracking,
                remote,
                ..
            } => {
                let refspec = if remote.name == "." {
                    upstream.clone()
                } else {
                    format!("{upstream}:{tracking}")
                };
                git(
                    repo,
                    &[
                        "fetch",
                        "--no-tags",
                        "--no-prune",
                        "--no-prune-tags",
                        "--no-recurse-submodules",
                        "--no-write-fetch-head",
                        "--refmap=",
                        "--",
                        &remote.name,
                        &refspec,
                    ],
                )
                .map_err(|error| {
                    anyhow::anyhow!(redact_remote_error(&error.to_string(), &remote.url))
                })?;
                self.revalidate(repo, kind)?;
                let target = git(
                    repo,
                    &["rev-parse", "--verify", &format!("{tracking}^{{commit}}")],
                )
                .context("Fetched upstream commit unavailable")?;
                return crate::core::update::integrate(repo, &target, strategy);
            }
            Self::Fetch(remotes) => remotes,
        };
        let output = command(repo)
            .args(["fetch", "--all", "--no-recurse-submodules"])
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

#[derive(Clone)]
struct Job {
    root: usize,
    path: PathBuf,
    name: String,
    submodule: bool,
}

struct Checked {
    target: Target,
    row: RepositoryRow,
}

struct Prepared {
    targets: Vec<Target>,
    rows: Vec<RepositoryRow>,
    ready: usize,
}

enum Outcome {
    Checking,
    Ready,
    Running,
    Succeeded,
    Failed(String),
    Skipped(String),
}

#[derive(Clone)]
struct Comparison {
    branch: String,
    upstream: String,
    tracking: Option<(usize, usize)>,
}

impl Comparison {
    fn checking() -> Self {
        Self {
            branch: String::new(),
            upstream: String::new(),
            tracking: None,
        }
    }

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

impl RepositoryRow {
    fn checking(job: &Job) -> Self {
        Self {
            root: job.root,
            name: job.name.clone(),
            submodule: job.submodule,
            comparison: Comparison::checking(),
            destination: String::new(),
            outcome: Outcome::Checking,
        }
    }
}

enum Report {
    Started(usize),
    Row(usize, Outcome, Comparison),
    Done,
}

struct RunResult {
    outcome: Result<(), String>,
    comparison: Comparison,
}

fn execute(kind: Kind, strategy: Strategy, targets: Vec<Target>, sender: mpsc::Sender<Report>) {
    let mut jobs = Vec::with_capacity(targets.len());
    let mut skipped = Vec::new();
    for (row, target) in targets.into_iter().enumerate() {
        match &target.plan {
            Ok(_) => jobs.push((row, target)),
            Err(reason) => skipped.push((row, reason.clone(), target.path.clone())),
        }
    }
    let locks = jobs
        .iter()
        .map(|(_, target)| RepoLock::read(&target.path))
        .collect::<Vec<_>>();
    let worker = sender.clone();
    let completed = sender.clone();
    schedule::run(
        &jobs,
        &vec![Vec::new(); jobs.len()],
        |left, right| match kind {
            Kind::Pull => locks[left].conflicts(&locks[right]),
            Kind::Fetch => locks[left].shares_git_state(&locks[right]),
        },
        |_, (row, target)| {
            let _ = worker.send(Report::Started(*row));
            let outcome = match &target.plan {
                Ok(plan) => plan
                    .execute(&target.path, kind, strategy)
                    .map_err(|error| error.to_string()),
                Err(reason) => Err(reason.clone()),
            };
            RunResult {
                outcome,
                comparison: Comparison::read(&target.path),
            }
        },
        |_| true,
        |index, result| {
            let (row, target) = &jobs[index];
            let (outcome, comparison) = match result {
                schedule::TaskResult::Completed(run) => (
                    match &run.outcome {
                        Ok(()) => Outcome::Succeeded,
                        Err(error) => Outcome::Failed(error.clone()),
                    },
                    run.comparison.clone(),
                ),
                schedule::TaskResult::Panicked(message) => (
                    Outcome::Failed(format!("repository worker stopped unexpectedly: {message}")),
                    Comparison::read(&target.path),
                ),
                schedule::TaskResult::Blocked(blocked) => (
                    Outcome::Failed(if blocked.is_empty() {
                        "not executed: unresolved dependency".into()
                    } else {
                        "not executed: repository dependency failed".into()
                    }),
                    Comparison::read(&target.path),
                ),
            };
            let _ = completed.send(Report::Row(*row, outcome, comparison));
        },
    );
    for (row, reason, path) in skipped {
        let _ = sender.send(Report::Row(
            row,
            Outcome::Skipped(reason),
            Comparison::read(&path),
        ));
    }
    let _ = sender.send(Report::Done);
}

fn prepare(kind: Kind, jobs: Vec<Job>, version: &loading::Version) -> Prepared {
    let checked = loading::read_repositories(&jobs, version, |job| {
        let plan = Plan::read(&job.path, kind).map_err(|error| error.to_string());
        let (destination, outcome) = match &plan {
            Ok(plan) => (plan.destination(), Outcome::Ready),
            Err(reason) => (String::new(), Outcome::Skipped(reason.clone())),
        };
        Checked {
            target: Target {
                path: job.path.clone(),
                plan,
            },
            row: RepositoryRow {
                root: job.root,
                name: job.name.clone(),
                submodule: job.submodule,
                comparison: Comparison::read(&job.path),
                destination,
                outcome,
            },
        }
    });
    let ready = checked
        .iter()
        .filter(|checked| matches!(checked.row.outcome, Outcome::Ready))
        .count();
    let (targets, rows) = checked
        .into_iter()
        .map(|checked| (checked.target, checked.row))
        .unzip();
    Prepared {
        targets,
        rows,
        ready,
    }
}

enum Stage {
    Loading,
    Review(Vec<Target>),
    Running(Receiver<Report>),
    Results,
}

const CANCEL: usize = 0;
const CONFIRM: usize = 1;
const MERGE: usize = 2;
const REBASE: usize = 3;
const DEFAULT_STRATEGY: usize = 4;
const ROW: usize = 5;

pub(super) struct Dialog {
    kind: Kind,
    strategy: Strategy,
    default_strategy: Strategy,
    selector: Option<Selector>,
    state_dir: PathBuf,
    changed_strategy: Option<Strategy>,
    notice: Option<String>,
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
    jobs: Vec<Job>,
    load: Load<Prepared>,
}

impl Dialog {
    pub(super) fn new(kind: Kind, ctx: &Context) -> Self {
        let jobs: Vec<_> = ctx
            .scope
            .iter()
            .map(|&index| {
                let repo = &ctx.repos[index];
                Job {
                    root: index,
                    path: repo.path.clone(),
                    name: repo.name.clone(),
                    submodule: repo.submodule,
                }
            })
            .collect();
        let rows = jobs.iter().map(RepositoryRow::checking).collect();
        let (default_strategy, notice) = if kind == Kind::Pull {
            match strategy::load(&ctx.state_dir) {
                Ok(strategy) => (strategy, None),
                Err(_) => (
                    Strategy::default(),
                    Some(
                        "Cannot read default update strategy; using Merge for this review.".into(),
                    ),
                ),
            }
        } else {
            (Strategy::default(), None)
        };
        let mut dialog = Self {
            kind,
            strategy: default_strategy,
            default_strategy,
            selector: None,
            state_dir: ctx.state_dir.clone(),
            changed_strategy: None,
            notice,
            stage: Stage::Loading,
            ready: 0,
            rows,
            pos: 0,
            offset: 0,
            list_area: Rect::default(),
            detail_area: Rect::default(),
            detail_scroll: 0,
            detail_max_scroll: 0,
            confirm_focused: false,
            jobs,
            load: Load::default(),
        };
        dialog.load.request();
        dialog
    }

    pub(super) fn reads_pending(&self) -> bool {
        self.load.pending()
    }

    pub(super) fn label(&self) -> &'static str {
        self.kind.label()
    }

    pub(super) fn poll_loading(&mut self) -> Result<bool> {
        if self.load.needs_start() {
            let jobs = self.jobs.clone();
            let kind = self.kind;
            if let Err(error) = self.load.start("igit-update-review", move |version| {
                prepare(kind, jobs, &version)
            }) {
                return self.fail_loading(error);
            }
        }
        let Some(result) = self.load.take_result() else {
            return Ok(false);
        };
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => return self.fail_loading(error),
        };
        self.rows = prepared.rows;
        self.ready = prepared.ready;
        self.stage = Stage::Review(prepared.targets);
        self.confirm_focused = self.ready > 0;
        self.jobs.clear();
        self.select(0);
        Ok(true)
    }

    fn fail_loading(&mut self, error: anyhow::Error) -> Result<bool> {
        let reason = error.to_string();
        for row in &mut self.rows {
            row.outcome = Outcome::Failed(reason.clone());
        }
        self.ready = 0;
        self.confirm_focused = false;
        self.stage = Stage::Results;
        Err(error)
    }

    pub(super) fn strategy_changed(&mut self) -> Option<Strategy> {
        self.changed_strategy.take()
    }

    pub(super) fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub(super) fn poll(&mut self) -> Option<(Kind, NoticeLevel, String)> {
        if !matches!(self.stage, Stage::Running(_)) {
            return None;
        }
        let mut finished = false;
        let mut disconnected = false;
        if let Stage::Running(receiver) = &self.stage {
            loop {
                match receiver.try_recv() {
                    Ok(Report::Started(index)) => {
                        if let Some(row) = self.rows.get_mut(index) {
                            if matches!(row.outcome, Outcome::Ready) {
                                row.outcome = Outcome::Running;
                            }
                        }
                    }
                    Ok(Report::Row(index, outcome, comparison)) => {
                        if let Some(row) = self.rows.get_mut(index) {
                            row.outcome = outcome;
                            row.comparison = comparison;
                        }
                    }
                    Ok(Report::Done) => {
                        finished = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        finished = true;
                        break;
                    }
                }
            }
        }
        if !finished {
            return None;
        }
        if disconnected {
            for row in &mut self.rows {
                if matches!(row.outcome, Outcome::Ready | Outcome::Running) {
                    row.outcome = Outcome::Failed(
                        "Update worker stopped before reporting results. Inspect the repository before retrying."
                            .into(),
                    );
                }
            }
        }
        self.stage = Stage::Results;
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
        let skipped = self
            .rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Skipped(_)))
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
        let summary = format!(
            "{}: {succeeded} succeeded · {failed} failed · {skipped} skipped",
            self.kind.label()
        );
        Some((self.kind, level, summary))
    }

    fn pull_review(&self) -> bool {
        self.kind == Kind::Pull && matches!(self.stage, Stage::Loading | Stage::Review(_))
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
        if let Some(selector) = &mut self.selector {
            match selector.event(event) {
                Choice::Pending => (),
                Choice::Cancel => self.selector = None,
                Choice::Selected(selected) => {
                    self.selector = None;
                    match strategy::save(&self.state_dir, selected) {
                        Ok(()) => {
                            if self.default_strategy != selected {
                                self.changed_strategy = Some(selected);
                            }
                            self.default_strategy = selected;
                            self.strategy = selected;
                            self.notice = None;
                        }
                        Err(_) => {
                            self.notice = Some(
                                "Cannot save default update strategy; previous default retained."
                                    .into(),
                            );
                        }
                    }
                }
            }
            return false;
        }
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
            Stage::Loading => (),
            Stage::Running(_) => return false,
            Stage::Results => return hit == Some(CANCEL) || matches!(key, Some(K::Esc | K::Enter)),
            Stage::Review(_) => (),
        }
        if hit == Some(CANCEL) || key == Some(K::Esc) {
            return true;
        }
        if self.pull_review() {
            if hit == Some(DEFAULT_STRATEGY) || key == Some(K::Char('s')) {
                self.selector = Some(Selector::new(self.default_strategy));
                return false;
            }
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
        if matches!(self.stage, Stage::Review(_)) && key == Some(K::Enter) && !self.confirm_focused
        {
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
                execute(kind, strategy, targets, tx);
            });
        }
        false
    }

    pub(super) fn keymap(&self) -> HelpContent {
        if let Some(selector) = &self.selector {
            return selector.keymap();
        }
        let (title, bindings): (_, &[_]) = match self.stage {
            Stage::Loading => (
                "Update / Checking",
                &[
                    ("j", "Next repository"),
                    ("k", "Previous repository"),
                    ("J", "Scroll details down"),
                    ("K", "Scroll details up"),
                    ("Esc", "Cancel repository check"),
                ],
            ),
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
                    ("s", "Choose default Pull and Push strategy"),
                ],
            )]
        } else {
            vec![]
        };
        HelpContent::new(HelpSection::new(title, bindings), sections)
    }
}
