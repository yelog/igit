//! Review and run a workspace update without routing through the Push dialog.
use crate::{
    core::{command, git, redact_remote_error, revpath, Context},
    ui::{
        help::{HelpContent, HelpSection},
        icons::IconMode,
        idea::button_style,
        pane,
        selection::{TextLayout, Wrap},
        ACCENT, MUTED,
    },
};
use anyhow::{bail, ensure, Context as _, Result};
use crossterm::event::{Event, KeyCode as K, KeyModifiers as M, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Paragraph},
    Frame,
};
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
    fn label(self) -> &'static str {
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

    fn label(&self) -> String {
        match self {
            Self::Pull {
                branch,
                upstream,
                remote,
                ..
            } => format!(
                "{branch} <- {}:{}",
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
    name: String,
    path: PathBuf,
    plan: Result<Plan, String>,
}

struct Report {
    summary: String,
    body: String,
}

fn execute(kind: Kind, strategy: Strategy, targets: Vec<Target>) -> Report {
    let (mut succeeded, mut failed, mut skipped) = (0, 0, 0);
    let mut rows = Vec::with_capacity(targets.len());
    for Target { name, path, plan } in targets {
        let result = match plan {
            Ok(plan) => match plan.execute(&path, kind, strategy) {
                Ok(()) => {
                    succeeded += 1;
                    "OK".into()
                }
                Err(error) => {
                    failed += 1;
                    format!("Failed\n  {error}")
                }
            },
            Err(reason) => {
                skipped += 1;
                format!("Skipped · {reason}")
            }
        };
        rows.push(format!("{name}: {result}"));
    }
    let counts = format!("{succeeded} succeeded · {failed} failed · {skipped} skipped");
    Report {
        body: format!("{counts}\n\n{}", rows.join("\n")),
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

pub(super) struct Dialog {
    kind: Kind,
    strategy: Strategy,
    stage: Stage,
    ready: usize,
    body: String,
    scroll: usize,
    max_scroll: usize,
    confirm_focused: bool,
}

impl Dialog {
    pub(super) fn new(kind: Kind, ctx: &Context) -> Self {
        let targets: Vec<_> = ctx
            .scope
            .iter()
            .map(|&index| {
                let repo = &ctx.repos[index];
                Target {
                    name: repo.name.clone(),
                    path: repo.path.clone(),
                    plan: Plan::read(&repo.path, kind).map_err(|error| error.to_string()),
                }
            })
            .collect();
        let ready = targets.iter().filter(|target| target.plan.is_ok()).count();
        let rows = targets
            .iter()
            .map(|target| {
                let details = match &target.plan {
                    Ok(plan) => plan.label(),
                    Err(reason) => format!("Skip · {reason}"),
                };
                format!("{}: {details}", target.name)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let body = format!("{} in Roots · {ready} ready\n{rows}", targets.len());
        Self {
            kind,
            strategy: Strategy::Merge,
            stage: Stage::Review(targets),
            ready,
            body,
            scroll: 0,
            max_scroll: 0,
            confirm_focused: false,
        }
    }

    pub(super) fn poll(&mut self) -> Option<(Kind, String)> {
        let Stage::Running(job) = &self.stage else {
            return None;
        };
        let report = match job.try_recv() {
            Ok(report) => report,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Report {
                summary: format!("{} worker stopped before reporting results", self.kind.label()),
                body: "Update worker stopped before reporting results. Inspect the repositories before retrying.".into(),
            },
        };
        self.stage = Stage::Results;
        self.body = report.body;
        self.scroll = 0;
        self.max_scroll = 0;
        Some((self.kind, report.summary))
    }

    fn pull_review(&self) -> bool {
        self.kind == Kind::Pull && matches!(self.stage, Stage::Review(_))
    }

    pub(super) fn event(&mut self, event: &Event, hit: Option<usize>) -> bool {
        let key = match event {
            Event::Key(k)
                if k.modifiers.is_empty() || k.modifiers == M::SHIFT && k.code == K::BackTab =>
            {
                Some(k.code)
            }
            _ => None,
        };
        match event {
            Event::Mouse(m) if m.kind == MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_sub(3)
            }
            Event::Mouse(m) if m.kind == MouseEventKind::ScrollDown => {
                self.scroll = (self.scroll + 3).min(self.max_scroll)
            }
            _ => match key {
                Some(K::Up | K::Char('k')) => self.scroll = self.scroll.saturating_sub(1),
                Some(K::Down | K::Char('j')) => {
                    self.scroll = (self.scroll + 1).min(self.max_scroll)
                }
                _ => (),
            },
        }
        match &mut self.stage {
            Stage::Running(_) => return false,
            Stage::Results => return hit == Some(CANCEL) || matches!(key, Some(K::Esc | K::Enter)),
            Stage::Review(_) => (),
        }
        if hit == Some(CANCEL) || matches!(key, Some(K::Esc | K::Char('n'))) {
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
        if self.ready > 0 && matches!(key, Some(K::Tab | K::BackTab | K::Left | K::Right)) {
            self.confirm_focused = !self.confirm_focused;
        }
        if key == Some(K::Enter) && !self.confirm_focused {
            return true;
        }
        if self.ready > 0 && (hit == Some(CONFIRM) || matches!(key, Some(K::Enter | K::Char('y'))))
        {
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
                    ("j", "Scroll repositories down"),
                    ("k", "Scroll repositories up"),
                    ("Tab", "Switch focused button"),
                    ("Enter", "Activate selected button"),
                    ("y", "Confirm workspace update"),
                    ("Esc", "Cancel workspace update"),
                ],
            ),
            Stage::Running(_) => ("Update / Running", &[]),
            Stage::Results => (
                "Update / Results",
                &[
                    ("j", "Scroll results down"),
                    ("k", "Scroll results up"),
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

    pub(super) fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        let width = (area.width - 4).min(84);
        let layout = TextLayout::plain(&self.body, width - 4, Wrap::Word);
        let choices = if self.pull_review() { 3 } else { 0 };
        let height =
            (layout.rows.len() + choices + 3).clamp(7, usize::from(area.height - 2)) as u16;
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let title = match self.stage {
            Stage::Review(_) if self.kind == Kind::Pull => "Update Project".into(),
            Stage::Review(_) => "Fetch Repositories".into(),
            Stage::Running(_) => format!("{} · Running", self.kind.label()),
            Stage::Results => format!("{} Results", self.kind.label()),
        };
        f.buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        f.render_widget(Clear, rect);
        let block = pane(title, true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if choices > 0 {
            for (offset, strategy, label, id) in [
                (0, Strategy::Merge, "m  Merge incoming changes", MERGE),
                (1, Strategy::Rebase, "r  Rebase current branches", REBASE),
            ] {
                let selected = self.strategy == strategy;
                let radio = match (icons, selected) {
                    (IconMode::Ascii, true) => "(o)",
                    (IconMode::Ascii, false) => "( )",
                    (_, true) => "◉",
                    (_, false) => "○",
                };
                let row = Rect::new(inner.x + 1, inner.y + offset, inner.width - 2, 1);
                let style = if selected {
                    Style::new().fg(ACCENT)
                } else {
                    Style::default()
                };
                f.render_widget(
                    Paragraph::new(format!("{radio}  {label}")).style(style),
                    row,
                );
                hits.push((row, id));
            }
        }
        let body = Rect::new(
            inner.x + 1,
            inner.y + choices as u16,
            inner.width - 2,
            inner.height - choices as u16 - 1,
        );
        self.max_scroll = layout.rows.len().saturating_sub(usize::from(body.height));
        self.scroll = self.scroll.min(self.max_scroll);
        let lines = layout
            .rows
            .into_iter()
            .skip(self.scroll)
            .take(usize::from(body.height))
            .map(|row| row.line)
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), body);

        let mut x = inner.right() - 1;
        let mut button = |label: &str, id, focused, enabled| {
            let width = label.len() as u16;
            x -= width;
            let row = Rect::new(x, inner.bottom() - 1, width, 1);
            let style = if enabled {
                button_style(focused)
            } else {
                Style::new().fg(MUTED)
            };
            f.render_widget(Paragraph::new(label).style(style), row);
            if enabled {
                hits.push((row, id));
            }
            x -= 1;
        };
        match self.stage {
            Stage::Review(_) => {
                button(
                    &format!(" y {} {} ", self.kind.label(), self.ready),
                    CONFIRM,
                    self.confirm_focused,
                    self.ready > 0,
                );
                button(" n Cancel ", CANCEL, !self.confirm_focused, true);
            }
            Stage::Running(_) => f.render_widget(
                Paragraph::new(format!(" Running Git for {} repositories…", self.ready))
                    .style(Style::new().fg(ACCENT)),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            ),
            Stage::Results => button(" Close / Esc ", CANCEL, true, true),
        }
    }
}
