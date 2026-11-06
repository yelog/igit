mod configuration;
mod kitty;
mod panes;
mod update;
mod workspace;

use self::{
    Surface::{Dialog, Split},
    ToolWindow::{Merge, Push},
    WorkspacePane::{Commit, Log},
};
use crate::{
    commit::{CommitPanel, Rollback, RollbackEvent},
    config::{keys::CommandAction, AppConfig},
    core::{Context, Repo},
    log::LogPanel,
    merge::MergePanel,
    push::PushPanel,
    ui::{
        help::{global_section, HelpContent, HelpEvent, HelpSection, HelpState},
        icons::{Icon, IconMode},
        idea::{button_style, checkbox, repo_tint, TAG},
        key_sequence::{KeySequence, KeySequencePicker, SequenceEvent, WindowPrefix},
        *,
    },
};
use anyhow::{bail, Result};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    collections::BTreeSet,
    env,
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspacePane {
    Log,
    Commit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolWindow {
    Push,
    Merge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Split(WorkspacePane),
    Dialog(ToolWindow),
}
const SHORTCUTS: [Surface; 4] = [Split(Log), Split(Commit), Dialog(Push), Dialog(Merge)];
const HEADER: &str = "IGit 0.1.0 — terminal Git workbench";
const HELP: &str = "Usage: igit [OPTIONS] [DIRECTORY]  (alias: ig)

  --icons MODE               nerd-font (default), unicode or ascii
  --mouse MODE               on or off (overrides settings)
  --config PATH              use an explicit settings.toml
  --check-config             validate settings without opening the TUI
  --print-default-config     print the complete built-in TOML settings
  --list-roots [DIRECTORY]   print detected Git worktrees
  -h, --help                 show this help
  -V, --version              print version

Focus: 1 Git Log / 2 Commit   Dialogs: 3 Push / 4 Merge   p Pull / f Fetch
r repositories   ? / F1 help   q quit
Runs Git on your local working trees; mutations require a reviewed confirmation.";
enum Popup {
    Rollback(Box<Rollback>),
    Update(update::Dialog),
    Scope {
        picked: BTreeSet<usize>,
        row: usize,
    },
    Help {
        state: HelpState,
        origin: Option<Box<Popup>>,
    },
    Menu {
        row: usize,
    },
    Confirm {
        title: String,
        body: String,
        token: String,
        owner: Surface,
        scroll: u16,
        yes: bool,
    },
    Journal,
    Open {
        path: String,
    },
}
struct Shell {
    config: AppConfig,
    kitty: kitty::Kitty,
    pane_maximized: bool,
    maximize_pending: bool,
    pending_keys: Option<configuration::PendingKeys>,
    ctx: Context,
    icons: IconMode,
    log: LogPanel,
    commit: CommitPanel,
    push: PushPanel,
    merge: MergePanel,
    focus: WorkspacePane,
    tool: Option<ToolWindow>,
    remote_refs_changed: bool,
    pop: Option<Popup>,
    msg: String,
    journal: Vec<String>,
    quit: bool,
    regions: workspace::Regions,
    work_panes: crate::ui::panes::PaneLayout,
    tools: Vec<(Rect, u8)>,
    pop_hits: Vec<(Rect, usize)>,
    key_picker: KeySequencePicker,
    window_prefix: Option<WindowPrefix>,
}
impl Shell {
    #[cfg(test)]
    fn new(ctx: Context, icons: IconMode) -> Self {
        Self::with_config(
            ctx,
            AppConfig {
                icons,
                ..AppConfig::default()
            },
        )
    }
    fn with_config(ctx: Context, config: AppConfig) -> Self {
        let icons = config.icons;
        let mut s = Self {
            config,
            kitty: kitty::Kitty::default(),
            pane_maximized: false,
            maximize_pending: false,
            pending_keys: None,
            ctx,
            icons,
            log: Default::default(),
            commit: Default::default(),
            push: Default::default(),
            merge: Default::default(),
            focus: Log,
            tool: None,
            remote_refs_changed: false,
            pop: None,
            msg: "".into(),
            journal: vec![],
            quit: false,
            regions: Default::default(),
            work_panes: Default::default(),
            tools: vec![],
            pop_hits: vec![],
            key_picker: KeySequencePicker::default(),
            window_prefix: None,
        };
        s.log.icons = icons;
        s.commit.icons = icons;
        s.push.icons = icons;
        s.merge.icons = icons;
        s.merge.shortcut_labels = s.config.shortcut_labels("merge");
        s.push.shortcut_labels = s.config.shortcut_labels("push");
        s.refresh();
        s
    }
    fn surface(&self) -> Surface {
        self.tool.map_or(Split(self.focus), Dialog)
    }
    fn panel(&self) -> &dyn Panel {
        match self.surface() {
            Split(Log) => &self.log,
            Split(Commit) => &self.commit,
            Dialog(Push) => &self.push,
            Dialog(Merge) => &self.merge,
        }
    }
    fn key_sequence(&self) -> Option<KeySequence> {
        if self.pop.is_some() || self.ctx.repos.is_empty() {
            return None;
        }
        if let Some(prefix) = self.window_prefix {
            return Some(KeySequence::Window(prefix));
        }
        match self.surface() {
            Split(Log) => self.log.key_sequence(),
            Dialog(Merge) => self.merge.key_sequence(),
            _ => None,
        }
    }
    fn cancel_key_sequences(&mut self) {
        self.pending_keys = None;
        self.window_prefix = None;
        self.log.cancel_key_sequence();
        self.merge.cancel_key_sequence();
        self.key_picker = KeySequencePicker::default();
    }
    fn panel_event(&mut self, surface: Surface, ev: &Event) {
        let before = self.key_sequence();
        let action = match surface {
            Split(Log) => self.log.event(ev, &self.ctx),
            Split(Commit) => self.commit.event(ev, &self.ctx),
            Dialog(Push) => self.push.event(ev, &self.ctx),
            Dialog(Merge) => self.merge.event(ev, &self.ctx),
        };
        self.action(action);
        let after = self.key_sequence();
        if after != before || after.is_some() && matches!(ev, Event::Key(_)) {
            self.key_picker = if after.is_some() {
                KeySequencePicker::pending_with_delay(Instant::now(), self.config.hint_delay)
            } else {
                KeySequencePicker::default()
            };
        }
    }
    fn refresh(&mut self) {
        self.cancel_key_sequences();
        for &root in &self.ctx.scope {
            self.ctx.repos[root].refresh_branch();
        }
        if let Err(e) = self.commit.refresh(&self.ctx) {
            self.notice(format!("Commit: {e:#}"));
        }
        self.refresh_log();
        if let Some(tool) = self.tool {
            self.refresh_tool(tool);
        }
    }
    fn refresh_log(&mut self) {
        self.remote_refs_changed = false;
        if let Err(e) = self.log.refresh(&self.ctx) {
            self.notice(format!("Log: {e:#}"));
        }
    }
    fn refresh_tool(&mut self, tool: ToolWindow) {
        let r = match tool {
            Push => self.push.refresh(&self.ctx),
            Merge => self.merge.refresh(&self.ctx),
        };
        if let Err(e) = r {
            self.notice(format!("{e:#}"))
        }
    }
    fn notice(&mut self, s: String) {
        self.msg = s.replace(['\n', '\r'], " · ");
        self.journal.push(s);
        if self.journal.len() > 100 {
            self.journal.remove(0);
        }
    }
    fn go(&mut self, surface: Surface) {
        self.cancel_drags();
        self.cancel_key_sequences();
        self.msg.clear();
        match surface {
            Split(pane) => {
                self.focus = pane;
                self.tool = None;
            }
            Dialog(_) if self.ctx.repos.is_empty() => {
                self.notice("No Git repositories in this workspace".into());
            }
            Dialog(tool) if self.tool != Some(tool) => {
                self.tool = Some(tool);
                self.refresh_tool(tool);
            }
            Dialog(_) => (),
        }
    }
    fn action(&mut self, a: Action) {
        match a {
            Action::None => (),
            Action::Notice(s) => self.notice(s),
            Action::Copy { text, label } => match clipboard::copy(&text) {
                Ok(()) => self.notice(format!("Copied {label}")),
                Err(e) => self.notice(format!("Copy failed: {e:#}")),
            },
            Action::Refresh(s) => {
                self.refresh();
                self.notice(s)
            }
            Action::Go(n) => self.go(SHORTCUTS[n.min(3)]),
            Action::CloseDialog => {
                if self.tool.is_some() && self.can_leave_panel() {
                    self.go(Split(self.focus));
                }
            }
            Action::ReviewRollback => {
                self.cancel_key_sequences();
                self.cancel_drags();
                self.pop = self.commit.take_rollback().map(Popup::Rollback);
            }
            Action::Confirm { title, body, token } => {
                self.cancel_key_sequences();
                self.pop = Some(Popup::Confirm {
                    title,
                    body,
                    token,
                    owner: self.surface(),
                    scroll: 0,
                    yes: false,
                })
            }
            Action::Committed {
                repos,
                push,
                message,
            } => {
                if !repos.is_empty() {
                    self.push.reseed = Some(repos)
                }
                self.refresh();
                if push {
                    self.go(Dialog(Push))
                }
                self.notice(message)
            }
        }
    }
    fn confirm(&mut self, owner: Surface, token: &str) {
        if token == "__quit" {
            self.quit = true;
            return;
        }
        if self.push.busy() {
            self.notice("Wait for the remote operation to finish".into());
            return;
        }
        let a = match owner {
            Split(Log) => self.log.confirm(token, &self.ctx),
            Split(Commit) => self.commit.confirm(token, &self.ctx),
            Dialog(Push) => self.push.confirm(token, &self.ctx),
            Dialog(Merge) => self.merge.confirm(token, &self.ctx),
        };
        self.action(a)
    }
    fn quit_box(&mut self) {
        self.cancel_key_sequences();
        if self.push.busy() {
            self.notice(
                "A remote operation is running. Wait for it to finish before quitting".into(),
            );
            return;
        }
        self.pop = Some(Popup::Confirm {
            title: "Quit IGit?".into(),
            body: "Close the workbench? Unsaved edits may be lost. Completed Git operations will be kept.".into(),
            token: "__quit".into(),
            owner: self.surface(),
            scroll: 0,
            yes: false,
        })
    }
    fn review_update(&mut self, kind: update::Kind) {
        self.cancel_key_sequences();
        self.cancel_drags();
        if self.push.busy() {
            self.notice("Wait for the remote operation to finish".into());
        } else if self.ctx.scope.is_empty() {
            self.notice("No Git repositories in Roots".into());
        } else {
            self.pop = Some(Popup::Update(update::Dialog::new(kind, &self.ctx)));
        }
    }
    fn global(&mut self, n: u8) {
        self.cancel_key_sequences();
        match n {
            0 => {
                self.ctx.repos.iter_mut().for_each(Repo::refresh_branch);
                self.pop = Some(Popup::Scope {
                    picked: self.ctx.scope.clone(),
                    row: 0,
                })
            }
            1 => self.open_help(),
            2 => self.pop = Some(Popup::Menu { row: 0 }),
            3 => self.quit_box(),
            4 => self.pop = Some(Popup::Journal),
            5 => {
                self.refresh();
                self.notice("Workspace refreshed".into())
            }
            _ => (),
        }
    }
    fn help_content(&self) -> HelpContent {
        match self.pop.as_ref() {
            Some(Popup::Rollback(dialog)) => dialog.keymap(),
            Some(Popup::Update(dialog)) => dialog.keymap(),
            Some(Popup::Scope { .. }) => HelpContent::new(
                HelpSection::new(
                    "Roots / Repository scope",
                    &[
                        ("j", "Select next repository"),
                        ("k", "Select previous repository"),
                        ("Space", "Toggle repository selection"),
                        ("a", "Select all repositories"),
                        ("F5", "Refresh repository branches"),
                        ("Enter", "Apply repository scope"),
                        ("Esc", "Discard scope changes"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Confirm { .. }) => HelpContent::new(
                HelpSection::new(
                    "Review / Confirm action",
                    &[
                        ("j", "Scroll review down"),
                        ("k", "Scroll review up"),
                        ("Tab", "Switch focused button"),
                        ("Enter", "Activate selected button"),
                        ("y", "Confirm reviewed action"),
                        ("n", "Cancel review and return"),
                        ("Esc", "Cancel review and return"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Menu { .. }) => HelpContent::new(
                HelpSection::new(
                    "Actions / Command menu",
                    &[
                        ("j", "Select next action"),
                        ("k", "Select previous action"),
                        ("Enter", "Run selected action"),
                        ("Esc", "Close action menu"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Open { .. }) => HelpContent::new(
                HelpSection::new(
                    "Workspace / Path input",
                    &[
                        ("Backspace", "Delete last path character"),
                        ("Ctrl-u", "Clear path"),
                        ("Enter", "Open or rescan entered path"),
                        ("Esc", "Cancel opening workspace"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Journal) => HelpContent::new(
                HelpSection::new(
                    "Console / Session messages",
                    &[("Enter", "Close session messages")],
                ),
                vec![],
            ),
            Some(Popup::Help { .. }) => unreachable!("help content describes the origin"),
            None => {
                let mut content = match self.surface() {
                    Split(Log) => self.log.keymap(),
                    Split(Commit) => self.commit.keymap(),
                    Dialog(Push) => self.push.keymap(),
                    Dialog(Merge) => self.merge.keymap(),
                };
                if self.tool_at_root() {
                    content.sections.push(HelpSection::new(
                        "Tool dialog",
                        &[("Esc", "Close dialog and return")],
                    ));
                }
                if !self.panel().editing() {
                    self.pane_help(&mut content);
                    content.sections.push(global_section());
                }
                content
            }
        }
    }
    fn open_help(&mut self) {
        if matches!(self.pop, Some(Popup::Help { .. })) {
            return;
        }
        self.cancel_key_sequences();
        self.cancel_drags();
        let content = self.help_content();
        let origin = self.pop.take().map(Box::new);
        self.pop = Some(Popup::Help {
            state: HelpState::configured(content, &self.config),
            origin,
        });
    }
    fn menu_action(&mut self, row: usize) {
        match row {
            0..=3 => self.go(SHORTCUTS[row]),
            4 => self.global(0),
            5 => self.review_update(update::Kind::Pull),
            6 => self.review_update(update::Kind::Fetch),
            7 => self.global(5),
            8 => {
                self.pop = Some(Popup::Open {
                    path: self.ctx.workspace.display().to_string(),
                })
            }
            9 => self.global(4),
            _ => self.global(3),
        }
    }
    fn pop_event(&mut self, ev: &Event) {
        if matches!(self.pop, Some(Popup::Help { .. })) {
            if let Some(Popup::Help { mut state, origin }) = self.pop.take() {
                match state.event(ev) {
                    HelpEvent::Keep => self.pop = Some(Popup::Help { state, origin }),
                    HelpEvent::Close => self.pop = origin.map(|popup| *popup),
                    HelpEvent::Run(command) => {
                        self.pop = origin.map(|popup| *popup);
                        if self.help_content().context() != state.content.context() {
                            self.open_help();
                            self.notice("Context changed. Reopen help to choose a shortcut".into());
                        } else if let CommandAction::SmartFocus(direction) = command.action {
                            self.smart_focus(direction);
                        } else if let CommandAction::SmartResize(direction) = command.action {
                            self.smart_resize(direction);
                        } else if let CommandAction::SmartMaximize = command.action {
                            self.smart_maximize();
                        } else if let Some(events) = command.events() {
                            for event in events {
                                self.dispatch_builtin(&event);
                            }
                        } else {
                            self.notice("This shortcut is unavailable".into());
                        }
                    }
                }
            }
            return;
        }
        if let Some(Popup::Rollback(dialog)) = self.pop.as_mut() {
            match dialog.event(ev, &self.ctx) {
                RollbackEvent::Keep => (),
                RollbackEvent::Close => self.pop = None,
                RollbackEvent::Help => self.open_help(),
                RollbackEvent::Apply if self.push.busy() => {
                    self.notice("Wait for the remote operation to finish".into())
                }
                RollbackEvent::Apply => {
                    let Some(Popup::Rollback(dialog)) = self.pop.take() else {
                        unreachable!()
                    };
                    let action = dialog.apply(&self.ctx);
                    self.action(action);
                }
            }
            return;
        }
        let key = if let Event::Key(k) = ev {
            Some(k.code)
        } else {
            None
        };
        let hit = if let Event::Mouse(m) = ev {
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                self.pop_hits
                    .iter()
                    .rev()
                    .find(|(r, _)| inside(*r, m.column, m.row))
                    .map(|x| x.1)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(Popup::Update(dialog)) = self.pop.as_mut() {
            if dialog.event(ev, hit) {
                self.pop = None;
            }
            return;
        }
        let down = key == Some(KeyCode::Char('j'))
            || key == Some(KeyCode::Down)
            || matches!(ev,Event::Mouse(m) if m.kind==MouseEventKind::ScrollDown);
        let up = key == Some(KeyCode::Char('k'))
            || key == Some(KeyCode::Up)
            || matches!(ev,Event::Mouse(m) if m.kind==MouseEventKind::ScrollUp);
        if key == Some(KeyCode::Esc) || hit == Some(usize::MAX) {
            self.pop = None;
            return;
        }
        let popup = self.pop.take().unwrap();
        self.pop = match popup {
            Popup::Rollback(_) => unreachable!("rollback events are handled above"),
            Popup::Update(_) => unreachable!("update events are handled above"),
            Popup::Confirm {
                title,
                body,
                token,
                owner,
                mut scroll,
                mut yes,
            } => {
                if up {
                    scroll = scroll.saturating_sub(2)
                }
                if down {
                    scroll = scroll.saturating_add(2)
                }
                if matches!(
                    key,
                    Some(KeyCode::Tab | KeyCode::Left | KeyCode::Right | KeyCode::BackTab)
                ) {
                    yes = !yes
                }
                if hit == Some(1)
                    || key == Some(KeyCode::Char('y'))
                    || (key == Some(KeyCode::Enter) && yes)
                {
                    self.confirm(owner, &token);
                    return;
                }
                if hit == Some(0) || key == Some(KeyCode::Char('n')) || key == Some(KeyCode::Enter)
                {
                    return;
                }
                Some(Popup::Confirm {
                    title,
                    body,
                    token,
                    owner,
                    scroll,
                    yes,
                })
            }
            Popup::Scope {
                mut picked,
                mut row,
            } => {
                let n = self.ctx.repos.len();
                if key == Some(KeyCode::F(5)) {
                    self.ctx.repos.iter_mut().for_each(Repo::refresh_branch);
                }
                if down {
                    row = (row + 1).min(n.saturating_sub(1))
                }
                if up {
                    row = row.saturating_sub(1)
                }
                if hit == Some(n + 1) || key == Some(KeyCode::Char('a')) {
                    picked = (0..n).collect()
                }
                if let Some(i) = hit.filter(|i| *i < n) {
                    row = i;
                    if !picked.remove(&row) {
                        picked.insert(row);
                    }
                }
                if key == Some(KeyCode::Char(' ')) && n > 0 && !picked.remove(&row) {
                    picked.insert(row);
                }
                if (hit == Some(n) || key == Some(KeyCode::Enter)) && (!picked.is_empty() || n == 0)
                {
                    self.ctx.scope = picked;
                    self.refresh();
                    self.notice(format!("{} roots in scope", self.ctx.scope.len()));
                    return;
                }
                Some(Popup::Scope { picked, row })
            }
            Popup::Menu { mut row } => {
                if up {
                    row = row.saturating_sub(1)
                }
                if down {
                    row = (row + 1).min(10)
                }
                if let Some(i) = hit {
                    self.menu_action(i);
                    return;
                }
                if key == Some(KeyCode::Enter) {
                    self.menu_action(row);
                    return;
                }
                Some(Popup::Menu { row })
            }
            Popup::Open { mut path } => {
                match ev {
                    Event::Paste(s) => path.push_str(s.trim()),
                    Event::Key(k) => match k.code {
                        KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            path.clear()
                        }
                        KeyCode::Char(ch) => path.push(ch),
                        KeyCode::Backspace => {
                            path.pop();
                        }
                        _ => (),
                    },
                    _ => (),
                }
                if key == Some(KeyCode::Enter) || hit == Some(1) {
                    if self.push.busy() {
                        self.notice("Wait for current push/fetch".into());
                        return;
                    }
                    match Context::discover(std::path::Path::new(&path)) {
                        Ok(c) => {
                            let kitty = std::mem::take(&mut self.kitty);
                            *self = Self::with_config(c, self.config.clone());
                            self.kitty = kitty;
                            return;
                        }
                        Err(e) => self.notice(e.to_string()),
                    }
                }
                Some(Popup::Open { path })
            }
            Popup::Help { .. } => unreachable!("help events are handled above"),
            Popup::Journal => {
                if hit.is_some() || key == Some(KeyCode::Enter) {
                    None
                } else {
                    Some(Popup::Journal)
                }
            }
        };
    }
    fn can_leave_panel(&mut self) -> bool {
        match self.surface() {
            Split(Log) => self.log.dismiss_filter_dropdown(),
            Split(Commit) => {
                self.commit.dismiss_view_menu();
                if let Err(error) = self.commit.finish_message(&self.ctx) {
                    self.notice(format!("Commit message: {error:#}"));
                    return false;
                }
            }
            Dialog(Push) => self.push.dismiss_group_menu(),
            Dialog(Merge) => (),
        }
        !self.panel().editing()
    }
    fn dispatch(&mut self, ev: &Event) {
        // Keep the reviewed focus/input context stable while synchronizing Kitty.
        // Resize and focus notifications still run; the worker has a short deadline.
        if self.maximize_pending && matches!(ev, Event::Key(_) | Event::Mouse(_) | Event::Paste(_))
        {
            return;
        }
        if !self.config.mouse && matches!(ev, Event::Mouse(_)) {
            return;
        }
        if matches!(ev, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return;
        }
        if !self.smart_pane_event(ev) && !self.configured_event(ev) {
            self.dispatch_builtin(ev);
        }
    }
    fn dispatch_builtin(&mut self, ev: &Event) {
        if matches!(ev,Event::Key(k) if k.kind==KeyEventKind::Release) {
            return;
        }
        if matches!(ev, Event::Resize(_, _) | Event::FocusLost | Event::Paste(_)) {
            self.cancel_key_sequences();
        }
        if matches!(
            ev,
            Event::Resize(_, _) | Event::FocusLost | Event::Key(_) | Event::Paste(_)
        ) {
            self.cancel_drags();
        }
        if matches!(ev, Event::Key(k) if k.code == KeyCode::F(1))
            && !matches!(self.pop, Some(Popup::Help { .. }))
        {
            self.open_help();
            return;
        }
        if self.pop.is_some() {
            self.pop_event(ev);
            return;
        }
        if let Some(sequence) = self.key_sequence() {
            match self.key_picker.event(ev, sequence) {
                SequenceEvent::Handled => return,
                SequenceEvent::Cancel => {
                    self.cancel_key_sequences();
                    return;
                }
                SequenceEvent::Run(suffix) => {
                    if matches!(sequence, KeySequence::Window(_)) {
                        self.run_window_key(suffix);
                    } else {
                        self.panel_event(
                            self.surface(),
                            &Event::Key(KeyEvent::new(KeyCode::Char(suffix), KeyModifiers::NONE)),
                        );
                    }
                    return;
                }
                SequenceEvent::Pass => {
                    // A pending Log suffix takes priority over the standalone Pull key.
                    if sequence == KeySequence::LogFilters
                        && matches!(ev, Event::Key(key) if self.key_picker.accepts_key(key, sequence))
                    {
                        self.panel_event(self.surface(), ev);
                        return;
                    }
                }
            }
        }
        if self.tool.is_none()
            && matches!(
                ev,
                Event::Key(_)
                    | Event::Paste(_)
                    | Event::Mouse(crossterm::event::MouseEvent {
                        kind: MouseEventKind::Down(_),
                        ..
                    })
            )
        {
            self.msg.clear();
        }
        if let Event::Mouse(m) = ev {
            if self.window_prefix.is_some() && m.kind != MouseEventKind::Moved {
                self.cancel_key_sequences();
            }
            self.workspace_mouse(m);
            return;
        }
        if let Event::Key(k) = ev {
            if self.surface() == Split(Commit)
                && self.commit.message_editing()
                && matches!(k.code, KeyCode::Tab | KeyCode::BackTab | KeyCode::F(6))
                && (k.modifiers.is_empty() || k.modifiers == KeyModifiers::SHIFT)
                && !self.can_leave_panel()
            {
                return;
            }
            let editing = self.panel().editing();
            if !editing && !self.ctx.repos.is_empty() && self.pane_key(k) {
                return;
            }
            if !editing
                && !k
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
            {
                match k.code {
                    KeyCode::Esc if self.tool_at_root() => {
                        self.go(Split(self.focus));
                        return;
                    }
                    KeyCode::Char(ch @ '1'..='4') => {
                        self.go(SHORTCUTS[(ch as u8 - b'1') as usize]);
                        return;
                    }
                    KeyCode::Char(ch @ ('p' | 'f')) if k.modifiers.is_empty() => {
                        self.review_update(if ch == 'p' {
                            update::Kind::Pull
                        } else {
                            update::Kind::Fetch
                        });
                        return;
                    }
                    KeyCode::Char('r') => {
                        self.global(0);
                        return;
                    }
                    KeyCode::Char('?') => {
                        self.global(1);
                        return;
                    }
                    KeyCode::Char(':') => {
                        self.global(2);
                        return;
                    }
                    KeyCode::Char('q') => {
                        self.global(3);
                        return;
                    }
                    KeyCode::F(5) => {
                        self.global(5);
                        return;
                    }
                    _ => (),
                }
            }
            if k.modifiers.contains(KeyModifiers::CONTROL)
                && k.code == KeyCode::Char('c')
                && !editing
                && self.panel().selected_text().is_none()
            {
                self.quit_box();
                return;
            }
        }
        if !self.ctx.repos.is_empty() {
            self.panel_event(self.surface(), ev);
        }
    }
    fn poll(&mut self) -> bool {
        let mut change = self.key_picker.poll(Instant::now());
        let popup = match self.pop.as_mut() {
            Some(Popup::Help { origin, .. }) => origin.as_deref_mut(),
            popup => popup,
        };
        if let Some(Popup::Update(dialog)) = popup {
            if let Some((kind, summary)) = dialog.poll() {
                if kind == update::Kind::Pull {
                    self.refresh();
                } else {
                    self.refresh_log();
                    if self.tool == Some(Push) {
                        self.refresh_tool(Push);
                    }
                }
                self.notice(summary);
                change = true;
            }
        }
        if let Some(result) = self.kitty.poll() {
            self.maximize_pending = false;
            match result {
                Ok(kitty::Outcome::Maximized(target)) => {
                    self.set_pane_maximized(target.unwrap_or(!self.pane_maximized));
                }
                Ok(kitty::Outcome::Done) => (),
                Err(error) => self.notice(error),
            }
            change = true;
        }
        match self.log.poll_preview(&self.ctx) {
            Ok(redraw) => change |= redraw,
            Err(e) => {
                self.notice(format!("Log: {e:#}"));
                change = true;
            }
        }
        let seq = self.push.progress_seq;
        if let Some(msg) = self.push.poll(&self.ctx) {
            self.remote_refs_changed = true;
            self.notice(msg);
            change = true
        }
        // A fetch/push can finish after its window closes. Reload moved refs
        // without interrupting a Log editor, Diff, text selection or pending key.
        if self.remote_refs_changed
            && !self.log.editing()
            && self.log.selected_text().is_none()
            && self.log.key_sequence().is_none()
        {
            self.refresh_log();
            change = true;
        }
        change |= seq != self.push.progress_seq;
        let a = self.commit.tick(&self.ctx);
        if !matches!(a, Action::None) {
            // A background AI request can open the message editor between two
            // input events, even while a window command or drag is pending.
            if self.surface() == Split(Commit) && self.commit.editing() {
                self.cancel_key_sequences();
                self.cancel_drags();
            }
            self.action(a);
            change = true
        }
        change
    }
    fn poll_timeout(&self) -> Duration {
        const IDLE: Duration = Duration::from_millis(150);
        let timeout = self.log.preview_wakeup().unwrap_or(IDLE).min(IDLE);
        self.key_picker
            .wakeup(Instant::now())
            .map_or(timeout, |remaining| remaining.min(timeout))
    }
    fn draw_popup(&mut self, f: &mut Frame, a: Rect) {
        if let Some(Popup::Rollback(dialog)) = self.pop.as_mut() {
            dialog.draw(f, a, &self.ctx);
            return;
        }
        if let Some(Popup::Update(dialog)) = self.pop.as_mut() {
            dialog.draw(f, a, self.icons, &mut self.pop_hits);
            return;
        }
        if let Some(Popup::Help { state, .. }) = self.pop.as_mut() {
            state.draw(f, a, self.icons);
            return;
        }
        let Some(pop) = self.pop.as_ref() else { return };
        let h = match pop {
            Popup::Scope { .. } => self.ctx.repos.len().saturating_add(5).min(24) as u16,
            Popup::Menu { .. } => 13,
            Popup::Confirm { body, .. } => {
                ((body.len() / 55 + body.lines().count() + 6).min(32)) as u16
            }
            Popup::Open { .. } => 7,
            _ => 19,
        }
        .min(a.height - 2)
        .max(5);
        let w = if matches!(pop, Popup::Confirm { .. } | Popup::Journal) {
            90
        } else {
            72
        }
        .min(a.width - 4);
        let p = Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h);
        let title = match pop {
            Popup::Rollback(_) => unreachable!("rollback is rendered above"),
            Popup::Update(_) => unreachable!("update is rendered above"),
            Popup::Scope { .. } => "Workspace roots",
            Popup::Help { .. } => "IGit · Keyboard shortcuts",
            Popup::Confirm { title, .. } => title,
            Popup::Menu { .. } => "Actions",
            Popup::Journal => "Session console",
            Popup::Open { .. } => "Open workspace",
        };
        f.render_widget(Clear, p);
        let blk = pane(title, true);
        let inner = blk.inner(p);
        f.render_widget(blk, p);
        let close = format!(" {} ", self.icons.glyph(Icon::Close));
        let w = close.width() as u16;
        let xr = Rect::new(p.right() - w - 1, p.y, w, 1);
        f.render_widget(Paragraph::new(close).style(Style::default().fg(ACCENT)), xr);
        self.pop_hits.push((xr, usize::MAX));
        let body = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(2),
        );
        let mut controls = vec![];
        match pop {
            Popup::Rollback(_) => unreachable!("rollback is rendered above"),
            Popup::Update(_) => unreachable!("update is rendered above"),
            Popup::Confirm {
                body: text,
                yes,
                scroll,
                ..
            } => {
                f.render_widget(
                    Paragraph::new(text.as_str())
                        .wrap(Wrap { trim: false })
                        .scroll((*scroll, 0)),
                    body,
                );
                controls.extend([(" n Cancel ", 0, !yes), (" y Confirm ", 1, *yes)])
            }
            Popup::Scope { picked, row } => {
                let start = (row + 1).saturating_sub(body.height as usize);
                for (i, r) in self
                    .ctx
                    .repos
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(body.height as usize)
                {
                    let rr = Rect::new(body.x, body.y + (i - start) as u16, body.width, 1);
                    let label = Line::from(vec![
                        checkbox(self.icons, picked.contains(&i)),
                        Span::styled(
                            format!(" {} ", self.icons.repo(r.submodule)),
                            Style::new().fg(repo_tint(i)),
                        ),
                        Span::raw(format!("{}   ", r.name)),
                        Span::styled(
                            format!("{} ", self.icons.glyph(Icon::Branch)),
                            Style::new().fg(MUTED),
                        ),
                        Span::styled(
                            r.branch.as_str(),
                            Style::new().fg(if i == *row { FG } else { MUTED }),
                        ),
                        Span::styled(
                            if r.submodule { " · submodule" } else { "" },
                            Style::new().fg(MUTED),
                        ),
                    ]);
                    f.render_widget(
                        Paragraph::new(label).style(Style::default().bg(if i == *row {
                            SEL
                        } else {
                            BG
                        })),
                        rr,
                    );
                    self.pop_hits.push((rr, i))
                }
                controls.extend([
                    (" a All roots ", self.ctx.repos.len() + 1, false),
                    (" Enter Apply ", self.ctx.repos.len(), true),
                ])
            }
            Popup::Help { .. } => unreachable!("help is rendered above"),
            Popup::Journal => {
                let text = self
                    .journal
                    .iter()
                    .rev()
                    .take(25)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                f.render_widget(
                    Paragraph::new(if text.is_empty() {
                        "No messages".into()
                    } else {
                        text
                    })
                    .wrap(Wrap { trim: false }),
                    body,
                );
                controls.push((" Close / Esc ", usize::MAX, true))
            }
            Popup::Menu { row } => {
                let start = (row + 1).saturating_sub(usize::from(inner.height));
                for (i, s) in [
                    format!(
                        "{}  Focus Git Log",
                        self.config.label("global", "focus-log")
                    ),
                    format!(
                        "{}  Focus Commit",
                        self.config.label("global", "focus-commit")
                    ),
                    format!("{}  Open Push…", self.config.label("global", "open-push")),
                    format!("{}  Open Merge…", self.config.label("global", "open-merge")),
                    "Roots / Select repositories".into(),
                    format!(
                        "{}  Pull repositories…",
                        self.config.label("global", "pull")
                    ),
                    format!(
                        "{}  Fetch repositories…",
                        self.config.label("global", "fetch")
                    ),
                    "Refresh workspace".into(),
                    "Open / rescan workspace…".into(),
                    "Console".into(),
                    "Quit".into(),
                ]
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
                {
                    let r = Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1);
                    f.render_widget(
                        Paragraph::new(s.as_str()).style(Style::default().bg(if i == *row {
                            SEL
                        } else {
                            BG
                        })),
                        r,
                    );
                    self.pop_hits.push((r, i))
                }
            }
            Popup::Open { path } => {
                f.render_widget(
                    Paragraph::new(format!(
                        "Workspace path · Ctrl-U clear\n{path}{}",
                        self.icons.glyph(Icon::Cursor)
                    ))
                    .wrap(Wrap { trim: false }),
                    body,
                );
                controls.push((" Enter Open ", 1, true))
            }
        }
        let mut x = inner.x + 1;
        for (txt, n, hi) in controls {
            let w = txt.width() as u16;
            if w > inner.right().saturating_sub(x) {
                break;
            }
            let r = Rect::new(x, inner.bottom() - 1, w, 1);
            f.render_widget(Paragraph::new(txt).style(button_style(hi)), r);
            self.pop_hits.push((r, n));
            x += r.width + 2
        }
    }
}
#[derive(Default)]
struct Tty {
    kitty_marker: bool,
    enhanced_keys: bool,
}
impl Drop for Tty {
    fn drop(&mut self) {
        let mut out = io::stdout();
        if self.kitty_marker {
            let _ = kitty::set_user_var(&mut out, false);
        }
        if self.enhanced_keys {
            let _ = execute!(out, PopKeyboardEnhancementFlags);
        }
        let _ = execute!(
            out,
            DisableFocusChange,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
    }
}
pub fn run() -> Result<()> {
    let mut list = false;
    let mut icons = None;
    let mut mouse = None;
    let mut config_path = None;
    let mut check_config = false;
    let mut print_config = false;
    let mut path = None;
    let mut args = env::args_os().skip(1);
    while let Some(a) = args.next() {
        if a == "--help" || a == "-h" {
            println!("{HEADER}\n\n{HELP}");
            return Ok(());
        } else if a == "--version" || a == "-V" {
            println!("{HEADER}");
            return Ok(());
        } else if a == "--list-roots" {
            list = true
        } else if a == "--icons" {
            let value = args.next().ok_or_else(|| {
                anyhow::anyhow!("Missing --icons mode: expected nerd-font, unicode or ascii")
            })?;
            icons = Some(parse_icons(&value.to_string_lossy())?);
        } else if let Some(value) = a.to_str().and_then(|s| s.strip_prefix("--icons=")) {
            icons = Some(parse_icons(value)?);
        } else if a == "--config" {
            config_path = Some(PathBuf::from(
                args.next()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Missing --config path"))?,
            ));
        } else if let Some(value) = a.to_str().and_then(|s| s.strip_prefix("--config=")) {
            if value.is_empty() {
                bail!("Missing --config path");
            }
            config_path = Some(PathBuf::from(value));
        } else if a == "--mouse" {
            let value = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("Missing --mouse mode: expected on or off"))?;
            mouse = Some(parse_mouse(&value.to_string_lossy())?);
        } else if let Some(value) = a.to_str().and_then(|s| s.strip_prefix("--mouse=")) {
            mouse = Some(parse_mouse(value)?);
        } else if a == "--check-config" {
            check_config = true;
        } else if a == "--print-default-config" {
            print_config = true;
        } else if a.to_string_lossy().starts_with('-') {
            bail!("Unknown option: {}\n{HELP}", a.to_string_lossy())
        } else if path.is_none() {
            path = Some(PathBuf::from(a))
        } else {
            bail!("{HELP}")
        }
    }
    if print_config {
        print!("{}", crate::config::DEFAULT_TOML);
        return Ok(());
    }
    let (config_path, required) = crate::config::settings_path(config_path)?;
    let mut config = AppConfig::load(&config_path, required)?;
    if let Some(icons) = icons {
        config.icons = icons;
    }
    if let Some(mouse) = mouse {
        config.mouse = mouse;
    }
    if check_config {
        println!(
            "Configuration valid: {}{}",
            config_path.display(),
            if config_path.exists() {
                ""
            } else {
                " (built-in defaults; no user file)"
            }
        );
        return Ok(());
    }
    let p = path.unwrap_or(env::current_dir()?);
    let ctx = Context::discover(&p)?;
    if list {
        for r in &ctx.repos {
            println!(
                "{}\t{}\t{}\t{}",
                r.name,
                r.branch,
                if r.submodule { "submodule" } else { "worktree" },
                r.path.display()
            )
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("IGit needs an interactive terminal. Try igit --list-roots or igit --help.")
    }
    let mut app = Shell::with_config(ctx, config);
    let mut out = io::stdout();
    let mut guard = Tty::default();
    enable_raw_mode()?;
    execute!(
        out,
        EnterAlternateScreen,
        EnableFocusChange,
        EnableBracketedPaste
    )?;
    if app.config.mouse {
        execute!(out, EnableMouseCapture)?;
    }
    if app.config.smart_panes_enabled() {
        app.kitty = kitty::Kitty::from_env();
        if app.kitty.available() {
            guard.kitty_marker = true;
            kitty::set_user_var(&mut out, true)?;
        }
    }
    // The Kitty protocol preserves Super/Cmd and other modified chords. It is
    // negotiated for all configurations so custom non-pane commands work too.
    if crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
        guard.enhanced_keys = true;
        execute!(
            out,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
    }
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    term.clear()?;
    let mut dirty = true;
    let mut buffered = None;
    while !app.quit {
        if dirty {
            term.draw(|f| app.draw(f))?;
        }
        dirty = false;
        if buffered.is_some() || event::poll(app.poll_timeout())? {
            let e = match buffered.take() {
                Some(e) => e,
                None => event::read()?,
            };
            match &e {
                Event::Mouse(m) if m.kind == MouseEventKind::Moved => {
                    dirty = app.config.mouse && app.hover(m.column, m.row);
                }
                _ => {
                    let motion = app.surface() == Split(Log)
                        && app.pop.is_none()
                        && app.key_sequence().is_none()
                        && app.pending_keys.is_none()
                        && app.log.graph_motion(&e);
                    app.dispatch(&e);
                    dirty = true;
                    if motion {
                        // Apply repeats already in the terminal in order, then paint once.
                        // A click or a command must see the new frame and its hit targets.
                        for _ in 1..64 {
                            if !event::poll(Duration::ZERO)? {
                                break;
                            }
                            let next = event::read()?;
                            if !app.log.graph_motion(&next) {
                                buffered = Some(next);
                                break;
                            }
                            app.dispatch(&next);
                        }
                    }
                }
            }
        }
        if app.poll() {
            dirty = true
        }
    }
    Ok(())
}

fn parse_icons(value: &str) -> Result<IconMode> {
    value
        .parse()
        .map_err(|reason: &str| anyhow::anyhow!("Invalid --icons value '{value}': {reason}"))
}

fn parse_mouse(value: &str) -> Result<bool> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => bail!("Invalid --mouse value '{value}': expected on or off"),
    }
}

#[cfg(test)]
mod key_sequence_tests;

#[cfg(test)]
mod configuration_tests;
#[cfg(test)]
mod discard_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod maximize_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod update_tests;
#[cfg(test)]
mod workspace_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::backend::TestBackend;

    #[test]
    fn external_branch_switch_refreshes_commit_rows_preview_and_roots() {
        use crate::core::git;
        use std::fs;

        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("app");
        let linked = d.path().join("linked");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        fs::write(repo.join("file"), "base\n").unwrap();
        git(&repo, &["add", "--", "file"]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@invalid.test",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "base",
            ],
        )
        .unwrap();
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                linked.to_str().unwrap(),
            ],
        )
        .unwrap();
        fs::write(repo.join("file"), "pending\n").unwrap();
        let mut c = Context::discover(d.path()).unwrap();
        c.state_dir = d.path().join("state");
        let app_root = c.repos.iter().position(|r| r.name == "app").unwrap();
        c.scope = [app_root].into();
        let mut app = Shell::new(c, IconMode::Ascii);
        let key =
            |app: &mut Shell, k| app.dispatch(&Event::Key(KeyEvent::new(k, KeyModifiers::NONE)));
        let mut term = Terminal::new(TestBackend::new(160, 32)).unwrap();
        let mut screen = |app: &mut Shell| {
            term.draw(|f| app.draw(f)).unwrap();
            let buf = term.backend().buffer();
            (0..32)
                .map(|y| (0..160).map(|x| buf[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        };
        key(&mut app, KeyCode::Char('2'));
        // Enable Module so the repository row and its branch badge are visible.
        for k in [
            KeyCode::Char('o'),
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Esc,
        ] {
            key(&mut app, k);
        }
        key(&mut app, KeyCode::Char('j')); // Highlight the repository.
        key(&mut app, KeyCode::Char('v'));
        assert!(screen(&mut app).contains("on main"));
        git(&repo, &["switch", "-qc", "fresh-main"]).unwrap();
        git(&repo, &["tag", "fresh-main"]).unwrap();
        key(&mut app, KeyCode::F(5));
        let refreshed = screen(&mut app);
        assert!(refreshed.contains("on fresh-main"), "{refreshed}");
        let changes = refreshed
            .lines()
            .take(app.regions.log.y.into())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(changes.matches("fresh-main").count(), 2); // Tree badge and repository preview.
        assert!(refreshed
            .lines()
            .skip(app.regions.log.y.into())
            .any(|line| line.contains("fresh-main"))); // Log refs refresh in the same frame.

        git(&linked, &["switch", "-qc", "fresh-linked"]).unwrap();
        key(&mut app, KeyCode::Char('r'));
        let roots = screen(&mut app);
        assert!(roots
            .lines()
            .any(|l| l.contains("linked") && l.contains("fresh-linked")));
        git(&linked, &["switch", "-q", "--detach", "HEAD"]).unwrap();
        key(&mut app, KeyCode::F(5));
        assert!(screen(&mut app)
            .lines()
            .any(|l| l.contains("linked") && l.contains("detached@")));
        assert_eq!(app.ctx.scope, [app_root].into());
    }

    #[test]
    fn ascii_roots_checkbox_and_rescanned_workspace_keep_the_launch_mode() {
        let repo = tempfile::tempdir().unwrap();
        crate::core::git(repo.path(), &["init", "-q", "-b", "main"]).unwrap();
        let mut app = Shell::new(Context::discover(repo.path()).unwrap(), IconMode::Ascii);
        let mut term = Terminal::new(TestBackend::new(44, 18)).unwrap();
        app.global(0);
        term.draw(|f| app.draw(f)).unwrap();
        let row = app.pop_hits.iter().find(|(_, hit)| *hit == 0).unwrap().0;
        let cells = |term: &Terminal<TestBackend>| -> String {
            (row.x..row.x + 3)
                .map(|x| term.backend().buffer()[(x, row.y)].symbol())
                .collect()
        };
        assert_eq!(cells(&term), "[x]");
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: row.x + 2,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        }));
        term.draw(|f| app.draw(f)).unwrap();
        assert_eq!(cells(&term), "[ ]");
        assert_eq!(app.ctx.scope.len(), 1); // The open popup is still a draft.

        app.pop = Some(Popup::Open {
            path: repo.path().display().to_string(),
        });
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.pop.is_none());
        for mode in [
            app.icons,
            app.log.icons,
            app.commit.icons,
            app.push.icons,
            app.merge.icons,
        ] {
            assert_eq!(mode, IconMode::Ascii);
        }
    }

    #[test]
    fn header_clicks_dismiss_log_dropdowns_but_respect_inline_editors_and_dialogs() {
        let repo = tempfile::tempdir().unwrap();
        crate::core::git(repo.path(), &["init", "-q", "-b", "main"]).unwrap();
        let mut ctx = Context::discover(repo.path()).unwrap();
        ctx.state_dir = repo.path().join("state");
        let mut app = Shell::new(ctx, IconMode::default());
        let mut term = Terminal::new(TestBackend::new(160, 32)).unwrap();
        let mut send = |app: &mut Shell, ev| {
            app.dispatch(&ev);
            term.draw(|f| app.draw(f)).unwrap();
        };
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        let click = |r: Rect| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: r.x,
                row: r.y,
                modifiers: KeyModifiers::NONE,
            })
        };

        send(&mut app, key(KeyCode::Char(' ')));
        send(&mut app, key(KeyCode::Char('b')));
        let roots = app.tools.iter().find(|(_, n)| *n == 0).unwrap().0;
        let commit = app.regions.commit;
        send(&mut app, click(roots));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        assert!(!app.log.editing());
        send(&mut app, click(commit));
        assert_eq!(app.surface(), Split(Log));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('o')));
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Char('2')));
        assert_eq!(app.surface(), Split(Log));
        send(&mut app, click(roots));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        assert!(!app.log.editing());
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('/')));
        send(&mut app, click(commit));
        assert_eq!(app.surface(), Split(Log));
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char(' ')));
        send(&mut app, key(KeyCode::Char('d')));
        send(&mut app, click(commit));
        assert_eq!(app.surface(), Split(Commit));
        assert!(!app.log.editing());
    }
}
