//! Keep repository discovery and initial panel reads off the terminal thread.
use super::*;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Layout},
};
use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

pub(super) const DISCOVERING: &str = "Discovering repositories...";

enum Message {
    Discovered(usize),
    Stage(&'static str),
    Ready(Result<Box<Shell>>),
}

pub(super) struct WorkspaceLoad {
    receiver: Receiver<Message>,
    pub(super) stage: &'static str,
    pub(super) roots: Option<usize>,
}

impl WorkspaceLoad {
    pub(super) fn start(launch: cli::Launch, config: AppConfig) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("igit-startup".into())
            .spawn(move || {
                let result = load_workspace(&launch, config, &sender);
                let _ = sender.send(Message::Ready(result.map(Box::new)));
            })?;
        Ok(Self {
            receiver,
            stage: DISCOVERING,
            roots: None,
        })
    }

    pub(super) fn poll(&mut self) -> Result<Option<Shell>> {
        loop {
            match self.receiver.try_recv() {
                Ok(Message::Discovered(roots)) => self.roots = Some(roots),
                Ok(Message::Stage(stage)) => self.stage = stage,
                Ok(Message::Ready(result)) => return result.map(|app| Some(*app)),
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => bail!("Workspace loader stopped unexpectedly"),
            }
        }
    }
}

fn load_workspace(
    launch: &cli::Launch,
    config: AppConfig,
    sender: &Sender<Message>,
) -> Result<Shell> {
    let ctx = Context::discover(&launch.workspace)?;
    sender
        .send(Message::Discovered(ctx.repos.len()))
        .map_err(|_| anyhow::anyhow!("Workspace loading cancelled"))?;
    sender
        .send(Message::Stage("Loading changes..."))
        .map_err(|_| anyhow::anyhow!("Workspace loading cancelled"))?;
    let mut app = Shell::unloaded(ctx, config);
    if let Some(file) = &launch.file_history {
        app.log.set_file_history(&app.ctx, file)?;
    }
    // Discovery already supplied current branch labels. Log is deferred until opened.
    if let Err(error) = app.commit.refresh(&app.ctx) {
        app.notify(NoticeLevel::Error, "Commit", format!("{error:#}"));
    }
    app.commit.enable_background_execution();
    sender
        .send(Message::Stage("Checking conflicts..."))
        .map_err(|_| anyhow::anyhow!("Workspace loading cancelled"))?;
    app.refresh_conflicts();
    // Enter through the regular activation path so each panel starts its own reads.
    // In particular, entering Log starts a separate worker and never waits for history.
    app.go(launch.tab);
    app.enable_background_loading();
    Ok(app)
}

pub(super) fn cancelled(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release
        && ((key.modifiers.is_empty() && matches!(key.code, KeyCode::Esc | KeyCode::Char('q')))
            || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))))
}

pub(super) struct LoadingView {
    pub(super) commit: CommitPanel,
    pub(super) tab: Tab,
    icons: IconMode,
    path: PathBuf,
}

impl LoadingView {
    pub(super) fn new(launch: &cli::Launch, config: &AppConfig) -> Self {
        let mut commit = CommitPanel::default();
        commit.icons = config.icons;
        Self {
            commit,
            tab: launch.tab,
            icons: config.icons,
            path: launch.workspace.clone(),
        }
    }

    pub(super) fn draw(
        &mut self,
        f: &mut Frame,
        stage: &str,
        roots: Option<usize>,
        elapsed: Duration,
    ) {
        let area = f.area();
        f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), area);
        if area.width < 44 || area.height < 12 {
            self.commit.panes.begin();
            f.render_widget(
                Paragraph::new(
                    "IGit · Terminal too small\nMinimum 44 × 12; recommended 120 × 36.\nEsc / q Quit.",
                )
                .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let context = workspace::Regions::default()
            .draw_header(f, header, self.tab, self.icons, false, false);
        let title = if context.width >= 18 {
            " Opening workspace"
        } else if context.width >= 8 {
            " Loading"
        } else {
            ""
        };
        f.render_widget(Paragraph::new(title).style(Style::new().fg(MUTED)), context);
        let frames: &[&str] = if self.icons == IconMode::Ascii {
            &["|", "/", "-", "\\"]
        } else {
            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        };
        let spinner = frames[(elapsed.as_millis() / 120 % frames.len() as u128) as usize];
        let status = format!("{spinner} {stage}");
        let detail = match roots {
            Some(count) => format!("{count} root{} found", if count == 1 { "" } else { "s" }),
            None => "Looking for Git worktrees".into(),
        };
        let detail = if elapsed.as_secs() >= 5 {
            format!("{detail} · {}s elapsed", elapsed.as_secs())
        } else {
            detail
        };
        if self.tab == Commit {
            self.commit
                .draw_loading(f, body, &self.path, &status, &detail);
        } else {
            f.render_widget(
                Paragraph::new(format!("{status}\n\n{}\n{detail}", self.path.display()))
                    .wrap(Wrap { trim: false })
                    .block(pane(self.tab.name(), true)),
                body,
            );
        }
        let [brand, version] = workspace::footer_identity();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                brand,
                version,
                Span::styled("  Esc / q Quit · Ctrl-C Cancel", Style::new().fg(MUTED)),
            ]))
            .style(Style::new().bg(TAG)),
            footer,
        );
    }
}

pub(super) fn open<B: Backend>(
    terminal: &mut Terminal<B>,
    launch: cli::Launch,
    config: AppConfig,
) -> Result<Option<Shell>> {
    let started = Instant::now();
    let mut view = LoadingView::new(&launch, &config);
    terminal.draw(|f| view.draw(f, DISCOVERING, None, Duration::ZERO))?;
    let mut loading = WorkspaceLoad::start(launch, config)?;
    let mut last_draw = Instant::now();
    loop {
        let mut dirty = false;
        if event::poll(Duration::from_millis(16))? {
            let event = event::read()?;
            if cancelled(&event) {
                return Ok(None);
            }
            dirty = matches!(event, Event::Resize(_, _));
        }
        let previous = (loading.stage, loading.roots);
        if let Some(app) = loading.poll()? {
            while event::poll(Duration::ZERO)? {
                if cancelled(&event::read()?) {
                    return Ok(None);
                }
            }
            return Ok(Some(app));
        }
        if dirty
            || (loading.stage, loading.roots) != previous
            || last_draw.elapsed() >= Duration::from_millis(120)
        {
            terminal.draw(|f| view.draw(f, loading.stage, loading.roots, started.elapsed()))?;
            last_draw = Instant::now();
        }
    }
}

type LoadedLog = (LogPanel, Result<()>);

pub(super) enum LogLoad {
    Dormant,
    Pending(Receiver<LoadedLog>),
    Ready,
}

impl LogLoad {
    pub(super) fn ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub(super) fn pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }
}

impl Shell {
    pub(super) fn load_log(&mut self) {
        if !matches!(self.log_load, LogLoad::Dormant) {
            return;
        }
        self.refresh_state.clear(Log);
        let (sender, receiver) = mpsc::channel();
        let mut log = std::mem::take(&mut self.log);
        self.log.icons = self.icons;
        let ctx = self.ctx.clone();
        match thread::Builder::new()
            .name("igit-initial-log".into())
            .spawn(move || {
                let result = log.refresh(&ctx);
                log.enable_background_loading();
                let _ = sender.send((log, result));
            }) {
            Ok(_) => self.log_load = LogLoad::Pending(receiver),
            Err(error) => {
                self.log_load = LogLoad::Ready;
                self.notify(NoticeLevel::Error, "Log", format!("Log loader: {error}"));
            }
        }
    }

    pub(super) fn poll_log_load(&mut self) -> bool {
        let LogLoad::Pending(receiver) = &self.log_load else {
            return false;
        };
        match receiver.try_recv() {
            Ok((log, result)) => {
                self.log = log;
                self.log_load = LogLoad::Ready;
                if let Err(error) = result {
                    self.notify(NoticeLevel::Error, "Log", format!("{error:#}"));
                }
            }
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                self.log_load = LogLoad::Ready;
                self.notify(
                    NoticeLevel::Error,
                    "Log",
                    "Log loader stopped unexpectedly; press F5 to retry".into(),
                );
            }
        }
        true
    }
}

#[cfg(test)]
pub(super) fn pending_workspace() -> (impl FnOnce(), WorkspaceLoad) {
    let (sender, receiver) = mpsc::channel();
    (
        move || drop(sender),
        WorkspaceLoad {
            receiver,
            stage: DISCOVERING,
            roots: None,
        },
    )
}
