//! Keep repository discovery and initial panel reads off the terminal thread.
use super::*;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Layout},
};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

pub(super) const DISCOVERING: &str = "Discovering repositories...";

enum Message {
    Stage(&'static str),
    Ready(Result<Box<Shell>>),
}

pub(super) struct WorkspaceLoad {
    receiver: Receiver<Message>,
    pub(super) stage: &'static str,
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
        })
    }

    pub(super) fn poll(&mut self) -> Result<Option<Shell>> {
        loop {
            match self.receiver.try_recv() {
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
        .send(Message::Stage("Loading changes..."))
        .map_err(|_| anyhow::anyhow!("Workspace loading cancelled"))?;
    let mut app = Shell::unloaded(ctx, config);
    if let Some(file) = &launch.file_history {
        app.log.set_file_history(&app.ctx, file)?;
    }
    // Discovery already supplied current branch labels. Log is deferred until opened.
    if let Err(error) = app.commit.refresh(&app.ctx) {
        app.notice(format!("Commit: {error:#}"));
    }
    app.commit.enable_background_execution();
    sender
        .send(Message::Stage("Checking conflicts..."))
        .map_err(|_| anyhow::anyhow!("Workspace loading cancelled"))?;
    app.refresh_conflicts();
    // Enter through the regular activation path so each panel starts its own reads.
    // In particular, entering Log starts a separate worker and never waits for history.
    app.go(launch.tab);
    Ok(app)
}

pub(super) fn cancelled(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release
        && ((key.modifiers.is_empty() && matches!(key.code, KeyCode::Esc | KeyCode::Char('q')))
            || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))))
}

pub(super) fn draw_loading(f: &mut Frame, path: &Path, stage: &str) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), area);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(" IGit").style(button_style(true)), header);
    f.render_widget(
        Paragraph::new(format!("{stage}\n\n{}", path.display()))
            .wrap(Wrap { trim: false })
            .block(pane("Workspace", true)),
        body,
    );
    f.render_widget(
        Paragraph::new(" Esc / q Quit").style(Style::new().fg(MUTED)),
        footer,
    );
}

pub(super) fn open<B: Backend>(
    terminal: &mut Terminal<B>,
    launch: cli::Launch,
    config: AppConfig,
) -> Result<Option<Shell>> {
    // This draw must precede even discovery: filesystem traversal can also be slow.
    terminal.draw(|f| draw_loading(f, &launch.workspace, DISCOVERING))?;
    let path = launch.workspace.clone();
    let mut loading = WorkspaceLoad::start(launch, config)?;
    loop {
        let mut dirty = false;
        if event::poll(Duration::from_millis(16))? {
            let event = event::read()?;
            if cancelled(&event) {
                return Ok(None);
            }
            dirty = matches!(event, Event::Resize(_, _));
        }
        let previous = loading.stage;
        if let Some(app) = loading.poll()? {
            return Ok(Some(app));
        }
        if dirty || loading.stage != previous {
            terminal.draw(|f| draw_loading(f, &path, loading.stage))?;
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
                self.notice(format!("Log loader: {error}"));
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
                    self.notice(format!("Log: {error:#}"));
                }
            }
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                self.log_load = LogLoad::Ready;
                self.notice("Log loader stopped unexpectedly; press F5 to retry".into());
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
        },
    )
}
