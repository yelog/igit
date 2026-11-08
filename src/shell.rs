mod cli;
mod configuration;
mod editor;
mod kitty;
mod panes;
mod software_update;
mod startup;
mod update;
mod workspace;

use self::Tab::{Branch, Commit, Conflicts, Log, Push, Stash};
use crate::{
    branch::BranchPanel,
    commit::{CommitPanel, Rollback, RollbackEvent},
    config::{keys::CommandAction, AppConfig},
    core::{Context, Repo},
    log::LogPanel,
    merge::MergePanel,
    push::PushPanel,
    stash::{StashPanel, REVIEW_TOKEN},
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
use std::path::PathBuf;
use std::{
    collections::BTreeSet,
    env,
    io::{self, IsTerminal},
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Commit,
    Log,
    Push,
    Branch,
    Stash,
    Conflicts,
}
impl Tab {
    const DISPLAY_ORDER: [Self; 6] = [Commit, Log, Push, Branch, Stash, Conflicts];

    fn name(self) -> &'static str {
        match self {
            Commit => "Commit",
            Log => "Log",
            Push => "Push",
            Branch => "Branch",
            Stash => "Stash",
            Conflicts => "Conflicts",
        }
    }

    fn binding(self) -> &'static str {
        match self {
            Commit => "focus-commit",
            Log => "focus-log",
            Push => "open-push",
            Branch => "focus-branch",
            Stash => "focus-stash",
            Conflicts => "open-merge",
        }
    }
}
#[derive(Clone, Copy)]
enum MenuAction {
    Switch(Tab),
    Roots,
    Update(update::Kind),
    SoftwareUpdate,
    Refresh,
    Open,
    Console,
    Quit,
}
const HEADER: &str = concat!(
    "IGit ",
    env!("CARGO_PKG_VERSION"),
    " — terminal Git workbench"
);
const HELP: &str = "Usage: igit [OPTIONS] [DIRECTORY]  (alias: ig)
       igit [OPTIONS] <commit|log|push|branch|stash> [DIRECTORY]
       igit [OPTIONS] log [FILE]

  commit / log / push / branch / stash
                             open the selected panel (default: Commit)
  log FILE                   open a file-history tab in its owning worktree
  update [OPTIONS]           check or update a native installation
  --icons MODE               nerd-font (default), unicode or ascii
  --mouse MODE               on or off (overrides settings)
  --config PATH              use an explicit settings.toml
  --check-config             validate settings without opening the TUI
  --print-default-config     print the complete built-in TOML settings
  --list-roots [DIRECTORY]   print detected Git worktrees
  -h, --help                 show this help
  -V, --version              print version
  --                         treat remaining arguments as literal paths

Options may appear before or after the panel. Use igit -- log or igit ./log
to open a directory named log; use igit log -- -file for an option-like filename.

Tabs: 1 Commit / 2 Log / 3 Push / 4 Branch / 5 Stash / 6 Conflicts (when available)   p Pull / f Fetch
r repositories (Branch: rename; use Actions for Roots)   F9 updates   ? / F1 help   q quit
Runs Git on your local working trees; mutations require a reviewed confirmation.";
enum Popup {
    Rollback(Box<Rollback>),
    Update(update::Dialog),
    SoftwareUpdate,
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
        owner: Tab,
        scroll: u16,
        yes: bool,
    },
    Journal,
    Open {
        path: String,
    },
}

fn confirm_uses_letter_keys(owner: Tab, token: &str) -> bool {
    !matches!(
        (owner, token),
        (Push, "push") | (Stash, REVIEW_TOKEN) | (Log, crate::log::REVIEW_TOKEN)
    )
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
    log_load: startup::LogLoad,
    commit: CommitPanel,
    push: PushPanel,
    merge: MergePanel,
    branch: BranchPanel,
    stash: StashPanel,
    tab: Tab,
    main_tab: Tab,
    remote_refs_changed: bool,
    pop: Option<Popup>,
    msg: String,
    journal: Vec<String>,
    quit: bool,
    restart: Option<PathBuf>,
    pending_edit: Option<editor::Request>,
    software_update: software_update::SoftwareUpdate,
    regions: workspace::Regions,
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
        let mut s = Self::unloaded(ctx, config);
        s.log_load = startup::LogLoad::Ready;
        s.refresh();
        s
    }
    fn unloaded(ctx: Context, config: AppConfig) -> Self {
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
            log_load: startup::LogLoad::Dormant,
            commit: Default::default(),
            push: Default::default(),
            merge: Default::default(),
            branch: Default::default(),
            stash: Default::default(),
            tab: Commit,
            main_tab: Commit,
            remote_refs_changed: false,
            pop: None,
            msg: "".into(),
            journal: vec![],
            quit: false,
            restart: None,
            pending_edit: None,
            software_update: Default::default(),
            regions: Default::default(),
            pop_hits: vec![],
            key_picker: KeySequencePicker::default(),
            window_prefix: None,
        };
        s.log.icons = icons;
        s.commit.icons = icons;
        s.commit.ai_config = s.config.ai.clone();
        s.push.icons = icons;
        s.merge.icons = icons;
        s.branch.icons = icons;
        s.stash.icons = icons;
        s.branch.shortcut_labels = s.config.shortcut_labels("branch");
        s.merge.shortcut_labels = s.config.shortcut_labels("merge");
        s.push.shortcut_labels = s.config.shortcut_labels("push");
        s
    }
    fn tab_available(&self, tab: Tab) -> bool {
        tab != Conflicts || self.merge.has_pending_work()
    }
    fn panel(&self) -> &dyn Panel {
        match self.tab {
            Log => &self.log,
            Commit => &self.commit,
            Push => &self.push,
            Conflicts => &self.merge,
            Branch => &self.branch,
            Stash => &self.stash,
        }
    }
    fn key_sequence(&self) -> Option<KeySequence> {
        if self.pop.is_some() || self.ctx.repos.is_empty() {
            return None;
        }
        if let Some(prefix) = self.window_prefix {
            return Some(KeySequence::Window(prefix));
        }
        match self.tab {
            Log => self.log.key_sequence(),
            Branch => self.branch.key_sequence(),
            Conflicts => self.merge.key_sequence(),
            _ => None,
        }
    }
    fn cancel_key_sequences(&mut self) {
        self.pending_keys = None;
        self.window_prefix = None;
        self.log.cancel_key_sequence();
        self.branch.cancel_key_sequence();
        self.merge.cancel_key_sequence();
        self.key_picker = KeySequencePicker::default();
    }
    fn panel_event(&mut self, ev: &Event) {
        if self.commit.add_running()
            && self.tab == Push
            && matches!(ev, Event::Key(key) if key.code == KeyCode::Char('F'))
        {
            self.notice("Adding files to Git; wait before fetching".into());
            return;
        }
        if self.push.busy()
            && self.tab == Commit
            && matches!(ev, Event::Key(key) if matches!(key.code, KeyCode::Char('a') | KeyCode::Insert) && key.modifiers.is_empty())
        {
            self.notice("Wait for the remote operation before adding files".into());
            return;
        }
        self.push
            .block_writes(self.commit.execution_running() || self.commit.add_running());
        if self.tab == Log && !self.log_load.ready() {
            return;
        }
        let before = self.key_sequence();
        let action = match self.tab {
            Log => self.log.event(ev, &self.ctx),
            Commit => self.commit.event(ev, &self.ctx),
            Push => self.push.event(ev, &self.ctx),
            Conflicts => self.merge.event(ev, &self.ctx),
            Branch => self.branch.event(ev, &self.ctx),
            Stash => self.stash.event(ev, &self.ctx),
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
        self.branch_refs_changed();
        for &root in &self.ctx.scope {
            self.ctx.repos[root].refresh_branch();
        }
        if let Err(e) = self.commit.refresh(&self.ctx) {
            self.notice(format!("Commit: {e:#}"));
        }
        self.refresh_log();
        self.refresh_conflicts();
        if self.tab == Push {
            self.refresh_push();
        }
        if self.tab == Stash {
            self.refresh_stash();
        }
    }
    fn refresh_stash(&mut self) {
        if let Err(error) = self.stash.refresh(&self.ctx) {
            self.notice(format!("Stash: {error:#}"));
        }
    }
    fn refresh_branch(&mut self) {
        if let Err(error) = self.branch.refresh(&self.ctx) {
            self.notice(format!("Branch: {error:#}"));
        }
    }
    fn branch_refs_changed(&mut self) {
        if self.tab == Branch {
            self.refresh_branch();
        } else {
            self.branch.invalidate_reads(&self.ctx);
        }
    }
    fn refresh_log(&mut self) {
        self.remote_refs_changed = false;
        if !self.log_load.ready() {
            // Dropping the receiver prevents an old scope/ref snapshot from being installed.
            self.log_load = startup::LogLoad::Dormant;
            if self.tab == Log {
                self.load_log();
            }
            return;
        }
        if let Err(e) = self.log.refresh(&self.ctx) {
            self.notice(format!("Log: {e:#}"));
        }
    }
    fn refresh_push(&mut self) {
        if let Err(e) = self.push.refresh(&self.ctx) {
            self.notice(format!("{e:#}"))
        }
    }
    fn refresh_conflicts(&mut self) {
        let result = self.merge.refresh(&self.ctx);
        if self.tab == Conflicts && !self.tab_available(Conflicts) {
            self.go(self.main_tab);
        }
        if let Err(e) = result {
            self.notice(format!("Conflicts: {e:#}"));
        }
    }
    fn notice(&mut self, s: String) {
        self.msg = s.replace(['\n', '\r'], " · ");
        self.journal.push(s);
        if self.journal.len() > 100 {
            self.journal.remove(0);
        }
    }
    fn go(&mut self, tab: Tab) {
        self.cancel_key_sequences();
        if self.tab == tab {
            return;
        }
        self.msg.clear();
        // Commit can discover external conflicts before a full workspace refresh.
        if tab == Conflicts {
            self.refresh_conflicts();
        }
        if !self.tab_available(tab) {
            self.notice("No conflicts or operations to continue in Roots".into());
            return;
        }
        if tab == Push && self.ctx.repos.is_empty() {
            self.notice("No Git repositories in this workspace".into());
            return;
        }
        if !self.can_leave_panel() {
            if self.msg.is_empty() {
                self.notice("Finish or cancel the current input first (Esc)".into());
            }
            return;
        }
        self.cancel_drags();
        self.panel_event(&Event::FocusLost);
        self.layout_mut(self.tab).begin();
        self.tab = tab;
        match tab {
            Commit => {
                self.main_tab = tab;
                self.commit.resume_refresh();
            }
            Log => {
                self.main_tab = tab;
                self.load_log();
            }
            Push => self.refresh_push(),
            Conflicts => (),
            Branch => {
                self.main_tab = tab;
                self.branch.enter(&self.ctx);
            }
            Stash => {
                self.main_tab = tab;
                self.refresh_stash();
            }
        }
    }
    fn action(&mut self, a: Action) {
        if self.commit.add_running()
            && matches!(&a, Action::Confirm { .. } | Action::ReviewRollback)
        {
            self.notice("Adding files to Git; wait before starting another Git operation".into());
            return;
        }
        match a {
            Action::None => (),
            Action::Notice(s) => self.notice(s),
            Action::EditFile { repo, path } => {
                if self.commit.execution_running() || self.commit.add_running() {
                    self.notice("Wait for the Git operation before opening an editor".into());
                    return;
                }
                self.cancel_key_sequences();
                self.pending_edit = Some(editor::Request { repo, path });
            }
            Action::Copy { text, label } => match clipboard::copy(&text) {
                Ok(()) => self.notice(format!("Copied {label}")),
                Err(e) => self.notice(format!("Copy failed: {e:#}")),
            },
            Action::Refresh(s) => {
                self.refresh();
                self.notice(s)
            }
            Action::OpenConflicts => self.go(Conflicts),
            Action::CloseDialog => {
                if matches!(self.tab, Push | Conflicts) {
                    self.go(self.main_tab);
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
                    yes: !confirm_uses_letter_keys(self.tab, &token)
                        && (self.tab, token.as_str()) != (Log, crate::log::REVIEW_TOKEN),
                    token,
                    owner: self.tab,
                    scroll: 0,
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
                    self.go(Push)
                }
                self.notice(message)
            }
        }
    }
    fn confirm(&mut self, owner: Tab, token: &str) {
        self.push
            .block_writes(self.commit.execution_running() || self.commit.add_running());
        if token == "__quit" {
            self.quit = true;
            return;
        }
        if self.git_busy() {
            self.notice("Wait for the Git operation to finish".into());
            return;
        }
        let a = match owner {
            Log => self.log.confirm(token, &self.ctx),
            Commit => self.commit.confirm(token, &self.ctx),
            Push => self.push.confirm(token, &self.ctx),
            Conflicts => self.merge.confirm(token, &self.ctx),
            Branch => self.branch.confirm(token, &self.ctx),
            Stash => self.stash.confirm(token, &self.ctx),
        };
        self.action(a)
    }
    fn quit_box(&mut self) {
        self.cancel_key_sequences();
        if self.software_update.installing() {
            self.notice("Wait for the software update to finish before quitting".into());
            return;
        }
        if self.git_busy() {
            self.notice("A Git operation is running. Wait for it to finish before quitting".into());
            return;
        }
        self.pop = Some(Popup::Confirm {
            title: "Quit IGit?".into(),
            body: "Close the workbench? Unsaved edits may be lost. Completed Git operations will be kept.".into(),
            token: "__quit".into(),
            owner: self.tab,
            scroll: 0,
            yes: false,
        })
    }
    fn review_update(&mut self, kind: update::Kind) {
        self.cancel_key_sequences();
        self.cancel_drags();
        if self.git_busy() {
            self.notice("Wait for the Git operation to finish".into());
        } else if self.ctx.scope.is_empty() {
            self.notice("No Git repositories in Roots".into());
        } else {
            self.pop = Some(Popup::Update(update::Dialog::new(kind, &self.ctx)));
        }
    }
    fn global(&mut self, n: u8) {
        self.cancel_key_sequences();
        if n == 5 && self.commit.add_running() {
            self.notice(
                "Adding files to Git; Changes will refresh automatically when finished".into(),
            );
            return;
        }
        if n == 0 && (self.commit.execution_running() || self.commit.add_running()) {
            self.notice("Wait for the Git operation before changing Roots".into());
            return;
        }
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
            Some(Popup::SoftwareUpdate) => self.software_update.keymap(),
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
            Some(Popup::Confirm { owner, token, .. }) => HelpContent::new(
                HelpSection::new(
                    "Review / Confirm action",
                    &[
                        ("j", "Scroll review down"),
                        ("k", "Scroll review up"),
                        ("Tab", "Switch focused button"),
                        ("Enter", "Activate selected button"),
                        ("Esc", "Cancel review and return"),
                    ],
                ),
                if confirm_uses_letter_keys(*owner, token) {
                    vec![HelpSection::new(
                        "Review shortcuts",
                        &[
                            ("y", "Confirm reviewed action"),
                            ("n", "Cancel review and return"),
                        ],
                    )]
                } else {
                    vec![]
                },
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
                let mut content = match self.tab {
                    Log => self.log.keymap(),
                    Commit => self.commit.keymap(),
                    Push => self.push.keymap(),
                    Conflicts => self.merge.keymap(),
                    Branch => self.branch.keymap(),
                    Stash => self.stash.keymap(),
                };
                if self.can_return_to_main_tab() {
                    content.sections.push(HelpSection::new(
                        "Workspace tabs",
                        &[("Esc", "Return to the last Commit, Log, Branch or Stash tab")],
                    ));
                }
                if !self.panel().editing() {
                    self.pane_help(&mut content);
                    content.sections.push(global_section(
                        self.tab_available(Conflicts),
                        self.tab != Branch,
                    ));
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
    fn menu_actions(&self) -> impl Iterator<Item = MenuAction> {
        let show_conflicts = self.tab_available(Conflicts);
        Tab::DISPLAY_ORDER
            .into_iter()
            .filter(move |&tab| tab != Conflicts || show_conflicts)
            .map(MenuAction::Switch)
            .chain([
                MenuAction::Roots,
                MenuAction::Update(update::Kind::Pull),
                MenuAction::Update(update::Kind::Fetch),
                MenuAction::Refresh,
                MenuAction::Open,
                MenuAction::Console,
                MenuAction::SoftwareUpdate,
                MenuAction::Quit,
            ])
    }
    fn menu_action(&mut self, row: usize) {
        match self.menu_actions().nth(row) {
            Some(MenuAction::Switch(tab)) => self.go(tab),
            Some(MenuAction::Roots) => self.global(0),
            Some(MenuAction::Update(kind)) => self.review_update(kind),
            Some(MenuAction::Refresh) => self.global(5),
            Some(MenuAction::Open) => {
                if self.git_busy() {
                    self.notice("Wait for the Git operation before opening a workspace".into());
                    return;
                }
                self.pop = Some(Popup::Open {
                    path: self.ctx.workspace.display().to_string(),
                })
            }
            Some(MenuAction::Console) => self.global(4),
            Some(MenuAction::SoftwareUpdate) => self.open_software_update(),
            Some(MenuAction::Quit) => self.global(3),
            None => (),
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
        if matches!(self.pop, Some(Popup::SoftwareUpdate)) {
            self.software_update_event(ev);
            return;
        }
        if let Some(Popup::Rollback(dialog)) = self.pop.as_mut() {
            match dialog.event(ev, &self.ctx) {
                RollbackEvent::Keep => (),
                RollbackEvent::Close => self.pop = None,
                RollbackEvent::Help => self.open_help(),
                RollbackEvent::Apply if self.git_busy() => {
                    self.notice("Wait for the Git operation to finish".into())
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
            Popup::SoftwareUpdate => unreachable!("software update events are handled above"),
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
                let letter_keys = confirm_uses_letter_keys(owner, &token);
                if hit == Some(1)
                    || (key == Some(KeyCode::Enter) && yes)
                    || (letter_keys && key == Some(KeyCode::Char('y')))
                {
                    self.confirm(owner, &token);
                    return;
                }
                if hit == Some(0)
                    || key == Some(KeyCode::Enter)
                    || (letter_keys && key == Some(KeyCode::Char('n')))
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
                let last = self.menu_actions().count() - 1;
                row = row.min(last);
                if up {
                    row = row.saturating_sub(1)
                }
                if down {
                    row = (row + 1).min(last)
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
                    if self.git_busy() {
                        self.notice("Wait for current push/fetch".into());
                        return;
                    }
                    match Context::discover(std::path::Path::new(&path)) {
                        Ok(c) => {
                            let kitty = std::mem::take(&mut self.kitty);
                            let software_update = std::mem::take(&mut self.software_update);
                            let background = self.commit.background_execution_enabled();
                            *self = Self::with_config(c, self.config.clone());
                            if background {
                                self.commit.enable_background_execution();
                                self.log.enable_background_loading();
                            }
                            self.kitty = kitty;
                            self.software_update = software_update;
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
        match self.tab {
            Log => self.log.dismiss_filter_dropdown(),
            Commit => {
                self.commit.dismiss_view_menu();
                if let Err(error) = self.commit.finish_message(&self.ctx) {
                    self.notice(format!("Commit message: {error:#}"));
                    return false;
                }
            }
            Push => self.push.dismiss_group_menu(),
            Branch => self.branch.dismiss_view_menu(),
            Conflicts | Stash => (),
        }
        !self.panel().editing()
    }
    fn can_return_to_main_tab(&self) -> bool {
        match self.tab {
            Push => self.push.at_root(),
            Conflicts => self.merge.at_root(),
            Commit | Log | Branch | Stash => false,
        }
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
        if self.smart_pane_event(ev) {
            return;
        }
        if self.tab == Log
            && self.pop.is_none()
            && self.pending_keys.is_none()
            && self.window_prefix.is_none()
            && self.key_sequence().is_none()
            && self.log.inline_diff_key(ev)
        {
            self.panel_event(ev);
            return;
        }
        if !self.configured_event(ev) {
            self.dispatch_builtin(ev);
        }
    }
    fn dispatch_builtin(&mut self, ev: &Event) {
        if matches!(ev,Event::Key(k) if k.kind==KeyEventKind::Release) {
            return;
        }
        if matches!(ev, Event::FocusGained) {
            self.commit.resume_refresh();
            if self.tab == Branch {
                if self.pop.is_none() && !self.branch.editing() {
                    self.refresh_branch();
                }
            } else if self.branch.reads_pending() {
                // Re-entry after an external edit must not reuse a read from before focus returned.
                self.branch.invalidate_reads(&self.ctx);
            }
            if self.tab == Stash && self.pop.is_none() && !self.stash.editing() {
                self.refresh_stash();
            }
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
                        self.panel_event(&Event::Key(KeyEvent::new(
                            KeyCode::Char(suffix),
                            KeyModifiers::NONE,
                        )));
                    }
                    return;
                }
                SequenceEvent::Pass => {
                    // A pending Log suffix takes priority over the standalone Pull key.
                    if sequence == KeySequence::LogFilters
                        && matches!(ev, Event::Key(key) if self.key_picker.accepts_key(key, sequence))
                    {
                        self.panel_event(ev);
                        return;
                    }
                }
            }
        }
        if matches!(self.tab, Commit | Log | Branch | Stash | Conflicts)
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
            // Help and configured actions may replay directly through this
            // route, bypassing dispatch's raw-key preview guard.
            if self.tab == Log && self.window_prefix.is_none() && self.log.inline_diff_key(ev) {
                self.panel_event(ev);
                return;
            }
            if self.tab == Commit
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
                    KeyCode::Esc if self.can_return_to_main_tab() => {
                        self.go(self.main_tab);
                        return;
                    }
                    KeyCode::Char(ch @ '1'..='6') => {
                        self.go(Tab::DISPLAY_ORDER[(ch as u8 - b'1') as usize]);
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
                    KeyCode::Char('r') if self.tab != Branch => {
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
                    KeyCode::F(9) if k.modifiers.is_empty() => {
                        self.open_software_update();
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
            self.panel_event(ev);
        }
    }
    fn poll(&mut self) -> bool {
        let mut change = self.poll_log_load();
        let action = self.commit.poll_add(&self.ctx);
        if !matches!(action, Action::None) {
            self.action(action);
            change = true;
        }
        change |= self.commit.add_running();
        let action = self.commit.poll_execution(&self.ctx);
        if !matches!(action, Action::None) {
            self.action(action);
            change = true;
        }
        change |= self.software_update.poll();
        change |= self.stash.poll_preview(&self.ctx);
        if self.tab == Commit && self.pop.is_none() {
            match self.commit.poll_refresh(&mut self.ctx) {
                Ok(redraw) => change |= redraw,
                Err(error) => {
                    self.notice(format!("Commit: {error:#}"));
                    change = true;
                }
            }
        }
        change |= self.key_picker.poll(Instant::now());
        let popup = match self.pop.as_mut() {
            Some(Popup::Help { origin, .. }) => origin.as_deref_mut(),
            popup => popup,
        };
        if let Some(Popup::Update(dialog)) = popup {
            if let Some((kind, summary)) = dialog.poll() {
                if kind == update::Kind::Pull {
                    self.refresh();
                } else {
                    self.branch_refs_changed();
                    self.refresh_log();
                    if self.tab == Push {
                        self.refresh_push();
                    }
                }
                self.notice(summary);
                change = true;
            }
        }
        if let Some(result) = self.kitty.poll() {
            self.maximize_pending = false;
            match result {
                Ok(kitty::Outcome::Maximized(Some(target))) => self.set_pane_maximized(target),
                Ok(kitty::Outcome::Maximized(None)) => self.toggle_pane_maximized(),
                Ok(kitty::Outcome::Done) => (),
                Err(error) => self.notice(error),
            }
            change = true;
        }
        match self.log.poll_history(&self.ctx) {
            Ok(redraw) => change |= redraw,
            Err(error) => {
                self.notice(format!("Log: {error:#}"));
                change = true;
            }
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
            self.branch_refs_changed();
            self.notice(msg);
            change = true
        }
        match self.push.poll_loading(&self.ctx) {
            Ok(redraw) => change |= redraw,
            Err(error) => {
                self.notice(format!("Push: {error:#}"));
                change = true;
            }
        }
        // A fetch/push can finish after leaving Push. Reload moved refs
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
            // A background AI request can open an error dialog between two
            // input events, even while a window command or drag is pending.
            if self.tab == Commit && self.commit.editing() {
                self.cancel_key_sequences();
                self.cancel_drags();
            }
            self.action(a);
            change = true
        }
        change |= self.tab == Commit && self.commit.ai_running();
        if self.branch.reads_pending() {
            match self.branch.poll_loading(&self.ctx) {
                Ok(redraw) => change |= redraw && self.tab == Branch,
                Err(error) if self.tab == Branch => {
                    self.notice(format!("Branch: {error:#}"));
                    change = true;
                }
                Err(_) => (),
            }
        }
        change
    }
    fn poll_timeout(&self) -> Duration {
        const IDLE: Duration = Duration::from_millis(150);
        let idle = if self.log_load.pending()
            || self.log.history_pending()
            || self.commit.execution_running()
            || self.push.reads_pending()
            || self.tab == Branch && self.branch.reads_pending()
        {
            Duration::from_millis(16)
        } else if self.commit.add_running() {
            Duration::from_millis(100)
        } else {
            IDLE
        };
        let timeout = self
            .log
            .preview_wakeup()
            .unwrap_or(idle)
            .min(self.stash.preview_wakeup().unwrap_or(idle))
            .min(idle);
        self.key_picker
            .wakeup(Instant::now())
            .map_or(timeout, |remaining| remaining.min(timeout))
    }

    fn git_busy(&self) -> bool {
        self.push.busy() || self.commit.execution_running() || self.commit.add_running()
    }
    fn draw_popup(&mut self, f: &mut Frame, a: Rect) {
        if matches!(self.pop, Some(Popup::SoftwareUpdate)) {
            self.software_update
                .draw(f, a, self.icons, &mut self.pop_hits);
            return;
        }
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
        if let Some(Popup::Confirm {
            owner: Stash,
            token,
            scroll,
            yes,
            ..
        }) = self.pop.as_mut()
        {
            if token == REVIEW_TOKEN
                && self
                    .stash
                    .draw_review(f, a, scroll, *yes, &mut self.pop_hits)
            {
                return;
            }
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
            Popup::SoftwareUpdate => unreachable!("software update is rendered above"),
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
            Popup::SoftwareUpdate => unreachable!("software update is rendered above"),
            Popup::Confirm {
                body: text,
                yes,
                scroll,
                owner,
                token,
                ..
            } => {
                f.render_widget(
                    Paragraph::new(text.as_str())
                        .wrap(Wrap { trim: false })
                        .scroll((*scroll, 0)),
                    body,
                );
                let (cancel, confirm) = if confirm_uses_letter_keys(*owner, token) {
                    (" n Cancel ", " y Confirm ")
                } else {
                    (" Cancel ", " Confirm ")
                };
                controls.extend([(cancel, 0, !yes), (confirm, 1, *yes)])
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
                let actions = self.menu_actions();
                let selected = (*row).min(self.menu_actions().count() - 1);
                let start = (selected + 1).saturating_sub(usize::from(inner.height));
                for (i, action) in actions.enumerate().skip(start).take(inner.height.into()) {
                    let label = |action| self.config.label("global", action);
                    let s = match action {
                        MenuAction::Switch(tab) => {
                            format!("{}  Switch to {}", label(tab.binding()), tab.name())
                        }
                        MenuAction::Roots => "Roots / Select repositories".into(),
                        MenuAction::Update(update::Kind::Pull) => {
                            format!("{}  Pull repositories…", label("pull"))
                        }
                        MenuAction::Update(update::Kind::Fetch) => {
                            format!("{}  Fetch repositories…", label("fetch"))
                        }
                        MenuAction::Refresh => "Refresh workspace".into(),
                        MenuAction::Open => "Open / rescan workspace…".into(),
                        MenuAction::Console => "Console".into(),
                        MenuAction::SoftwareUpdate => format!("{}  Update IGit…", label("update")),
                        MenuAction::Quit => "Quit".into(),
                    };
                    let r = Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1);
                    f.render_widget(
                        Paragraph::new(s).style(Style::default().bg(if i == selected {
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
        if matches!(pop, Popup::Confirm { owner: Log, token, .. } if token == crate::log::REVIEW_TOKEN)
            && inner.right().saturating_sub(x) >= 10
        {
            let r = Rect::new(x, inner.bottom() - 1, inner.right() - x, 1);
            f.render_widget(
                Paragraph::new("j/k Scroll").style(Style::default().fg(MUTED)),
                r,
            );
        }
    }
}
#[derive(Default)]
struct Tty {
    kitty_marker: bool,
    enhanced_keys: bool,
}
impl Tty {
    fn enter(&mut self, mouse: bool, marker: bool) -> Result<()> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            EnableFocusChange,
            EnableBracketedPaste
        )?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        if marker {
            self.kitty_marker = true;
            kitty::set_user_var(&mut out, true)?;
        }
        // Request directly: capability queries can block on unsupported terminals.
        self.enhanced_keys = execute!(
            out,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        let mut out = io::stdout();
        if self.kitty_marker {
            let _ = kitty::set_user_var(&mut out, false);
            self.kitty_marker = false;
        }
        if self.enhanced_keys {
            let _ = execute!(out, PopKeyboardEnhancementFlags);
            self.enhanced_keys = false;
        }
        let screen = execute!(
            out,
            DisableFocusChange,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let raw = disable_raw_mode();
        screen?;
        raw?;
        Ok(())
    }
}
impl Drop for Tty {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}
pub fn run() -> Result<()> {
    let options = match cli::parse(env::args_os().skip(1))? {
        cli::Command::Help => {
            println!("{HEADER}\n\n{HELP}");
            return Ok(());
        }
        cli::Command::Version => {
            println!("{HEADER}");
            return Ok(());
        }
        cli::Command::Open(options) => options,
    };
    if options.print_config {
        print!("{}", crate::config::DEFAULT_TOML);
        return Ok(());
    }
    let (config_path, required) = crate::config::settings_path(options.config_path.clone())?;
    let mut config = AppConfig::load(&config_path, required)?;
    if let Some(icons) = options.icons {
        config.icons = icons;
    }
    if let Some(mouse) = options.mouse {
        config.mouse = mouse;
    }
    if options.check_config {
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
    let launch = options.launch(&env::current_dir()?)?;
    if options.list {
        let ctx = Context::discover(&launch.workspace)?;
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
    let mut guard = Tty::default();
    let mut kitty = kitty::Kitty::default();
    if config.smart_panes_enabled() {
        kitty = kitty::Kitty::from_env();
    }
    guard.enter(config.mouse, kitty.available())?;
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    term.clear()?;
    let Some(mut app) = startup::open(&mut term, launch, config)? else {
        return Ok(());
    };
    app.kitty = kitty;
    let mut dirty = true;
    let mut buffered = None;
    while !app.quit {
        let scheduled_redraw = if dirty {
            term.draw(|f| app.draw(f))?;
            app.software_update
                .schedule(&app.ctx.state_dir, &app.config.updates)
        } else {
            false
        };
        dirty = scheduled_redraw;
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
                    let motion = app.tab == Log
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
        if let Some(request) = app.pending_edit.take() {
            match request.prepare(env::var(editor::NVIM_SESSION).ok().as_deref()) {
                Ok(editor::Prepared::Neovim(sequence)) => {
                    if let Err(error) = editor::send_request(&mut io::stdout(), &sequence) {
                        app.notice(format!("Edit file: {error:#}"));
                    }
                }
                Ok(editor::Prepared::External(mut command)) => {
                    guard.leave()?;
                    let result = editor::execute(&mut command);
                    // Always reacquire the terminal, including a failed editor launch.
                    guard.enter(app.config.mouse, app.kitty.available())?;
                    term.clear()?;
                    app.refresh();
                    match result {
                        Ok(()) => app.notice("Returned from editor".into()),
                        Err(error) => app.notice(format!("Edit file: {error:#}")),
                    }
                }
                Err(error) => app.notice(format!("Edit file: {error:#}")),
            }
            dirty = true;
        }
        if app.poll() {
            dirty = true
        }
    }
    if let Some(executable) = app.restart.take() {
        drop(term);
        drop(guard);
        let mut command = std::process::Command::new(executable);
        command.args(env::args_os().skip(1));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            return Err(command.exec().into());
        }
        #[cfg(windows)]
        {
            let status = command.status()?;
            if !status.success() {
                bail!("Restarted IGit exited with {status}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod key_sequence_tests;

#[cfg(test)]
mod configuration_tests;
#[cfg(test)]
mod discard_tests;
#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod log_tests;
#[cfg(test)]
mod maximize_tests;
#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod update_tests;
#[cfg(test)]
mod workspace_tests;

#[cfg(test)]
mod branch_tests;

#[cfg(test)]
mod stash_tests;

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
        assert!(screen(&mut app).contains("on main"));
        git(&repo, &["switch", "-qc", "fresh-main"]).unwrap();
        git(&repo, &["tag", "fresh-main"]).unwrap();
        key(&mut app, KeyCode::F(5));
        let refreshed = screen(&mut app);
        assert!(refreshed.contains("on fresh-main"), "{refreshed}");
        assert_eq!(refreshed.matches("fresh-main").count(), 2); // Tree badge and repository preview.
        key(&mut app, KeyCode::Char('2'));
        assert!(screen(&mut app).contains("fresh-main")); // Log refs refresh while Commit is active.

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

        send(&mut app, key(KeyCode::Char('2')));
        send(&mut app, key(KeyCode::Char(' ')));
        send(&mut app, key(KeyCode::Char('b')));
        let commit = app.regions.tab(Commit);
        send(&mut app, click(commit));
        assert_eq!(app.tab, Commit);
        assert!(!app.log.editing());
        send(&mut app, key(KeyCode::Char('2')));
        send(&mut app, key(KeyCode::Char('r')));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        assert!(!app.log.editing());
        send(&mut app, click(commit));
        assert_eq!(app.tab, Log);
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('o')));
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Char('1')));
        assert_eq!(app.tab, Log);
        send(&mut app, click(commit));
        assert_eq!(app.tab, Commit);
        assert!(!app.log.editing());
        send(&mut app, key(KeyCode::Char('2')));

        send(&mut app, key(KeyCode::Char('/')));
        send(&mut app, click(commit));
        assert_eq!(app.tab, Log);
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char(' ')));
        send(&mut app, key(KeyCode::Char('d')));
        send(&mut app, click(commit));
        assert_eq!(app.tab, Commit);
        assert!(!app.log.editing());
    }
}
