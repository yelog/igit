use crate::{
    commit::CommitPanel,
    core::{Context, Repo},
    log::LogPanel,
    merge::MergePanel,
    push::PushPanel,
    ui::{
        help::{global_section, HelpContent, HelpEvent, HelpSection, HelpState},
        icons::{Icon, IconMode},
        idea::{button_style, checkbox, repo_tint, TAG},
        key_sequence::{KeySequence, KeySequencePicker, SequenceEvent},
        *,
    },
};
use anyhow::{bail, Result};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
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
const NAMES: [&str; 4] = ["Log", "Commit", "Push", "Merge"];
const PANEL_ICONS: [Icon; 4] = [Icon::Log, Icon::Commit, Icon::Push, Icon::Merge];
const HEADER: &str = "IGit 0.1.0 — terminal Git workbench";
const HELP: &str = "Usage: igit [OPTIONS] [DIRECTORY]  (alias: ig)

  --icons MODE               nerd-font (default), unicode or ascii
  --list-roots [DIRECTORY]   print detected Git worktrees
  -h, --help                 show this help
  -V, --version              print version

1 Log   2 Commit   3 Push   4 Merge   r repositories   ? / F1 help   q quit
Runs Git on your local working trees; mutations require a reviewed confirmation.";
enum Popup {
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
        owner: usize,
        scroll: u16,
        yes: bool,
    },
    Journal,
    Open {
        path: String,
    },
}
struct Shell {
    ctx: Context,
    icons: IconMode,
    log: LogPanel,
    commit: CommitPanel,
    push: PushPanel,
    merge: MergePanel,
    tab: usize,
    pop: Option<Popup>,
    msg: String,
    journal: Vec<String>,
    quit: bool,
    tabs: [Rect; NAMES.len()],
    tools: Vec<(Rect, u8)>,
    pop_hits: Vec<(Rect, usize)>,
    mode_active: bool,
    key_picker: KeySequencePicker,
}
impl Shell {
    fn new(ctx: Context, icons: IconMode) -> Self {
        let mut s = Self {
            ctx,
            icons,
            log: Default::default(),
            commit: Default::default(),
            push: Default::default(),
            merge: Default::default(),
            tab: 0,
            pop: None,
            msg: "".into(),
            journal: vec![],
            quit: false,
            tabs: [Rect::default(); NAMES.len()],
            tools: vec![],
            pop_hits: vec![],
            mode_active: false,
            key_picker: KeySequencePicker::default(),
        };
        s.log.icons = icons;
        s.commit.icons = icons;
        s.push.icons = icons;
        s.merge.icons = icons;
        s.refresh();
        s
    }
    fn panel(&self) -> &dyn Panel {
        match self.tab {
            0 => &self.log,
            1 => &self.commit,
            2 => &self.push,
            _ => &self.merge,
        }
    }
    fn key_sequence(&self) -> Option<KeySequence> {
        if self.pop.is_some() || self.ctx.repos.is_empty() {
            return None;
        }
        match self.tab {
            0 => self.log.key_sequence(),
            3 => self.merge.key_sequence(),
            _ => None,
        }
    }
    fn cancel_key_sequences(&mut self) {
        self.log.cancel_key_sequence();
        self.merge.cancel_key_sequence();
        self.key_picker = KeySequencePicker::default();
    }
    fn panel_event(&mut self, ev: &Event) {
        let before = self.key_sequence();
        let action = match self.tab {
            0 => self.log.event(ev, &self.ctx),
            1 => self.commit.event(ev, &self.ctx),
            2 => self.push.event(ev, &self.ctx),
            _ => self.merge.event(ev, &self.ctx),
        };
        self.action(action);
        let after = self.key_sequence();
        if after != before || after.is_some() && matches!(ev, Event::Key(_)) {
            self.key_picker = if after.is_some() {
                KeySequencePicker::pending(Instant::now())
            } else {
                KeySequencePicker::default()
            };
        }
    }
    fn refresh(&mut self) {
        self.cancel_key_sequences();
        let r = match self.tab {
            0 => self.log.refresh(&self.ctx),
            1 => {
                for &root in &self.ctx.scope {
                    self.ctx.repos[root].refresh_branch();
                }
                self.commit.refresh(&self.ctx)
            }
            2 => self.push.refresh(&self.ctx),
            _ => self.merge.refresh(&self.ctx),
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
    fn go(&mut self, n: usize) {
        self.cancel_key_sequences();
        self.tab = n.min(3);
        self.msg.clear();
        self.refresh()
    }
    fn action(&mut self, a: Action) {
        match a {
            Action::None => (),
            Action::Notice(s) => self.notice(s),
            Action::Copy { text, label } => match clipboard::copy(&text) {
                Ok(()) => self.notice(format!("已复制 {label}")),
                Err(e) => self.notice(format!("复制失败：{e:#}")),
            },
            Action::Refresh(s) => {
                self.refresh();
                self.notice(s)
            }
            Action::Go(n) => self.go(n),
            Action::Confirm { title, body, token } => {
                self.cancel_key_sequences();
                self.pop = Some(Popup::Confirm {
                    title,
                    body,
                    token,
                    owner: self.tab,
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
                if push {
                    self.go(2)
                } else {
                    self.refresh()
                }
                self.notice(message)
            }
        }
    }
    fn confirm(&mut self, owner: usize, token: &str) {
        if token == "__quit" {
            self.quit = true;
            return;
        }
        if self.push.busy() {
            self.notice("请等待正在执行的远端操作".into());
            return;
        }
        let a = match owner {
            0 => self.log.confirm(token, &self.ctx),
            1 => self.commit.confirm(token, &self.ctx),
            2 => self.push.confirm(token, &self.ctx),
            _ => self.merge.confirm(token, &self.ctx),
        };
        self.action(a)
    }
    fn quit_box(&mut self) {
        self.cancel_key_sequences();
        if self.push.busy() {
            self.notice("远端操作进行中，请等待结果后退出".into());
            return;
        }
        self.pop = Some(Popup::Confirm {
            title: "退出 IGit？".into(),
            body: "关闭终端工作台。未保存的编辑可能丢失；已完成的 Git 操作会保留。".into(),
            token: "__quit".into(),
            owner: self.tab,
            scroll: 0,
            yes: false,
        })
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
                self.notice("面板已刷新 / Refreshed".into())
            }
            _ => (),
        }
    }
    fn help_content(&self) -> HelpContent {
        match self.pop.as_ref() {
            Some(Popup::Scope { .. }) => HelpContent::new(
                HelpSection::new(
                    "Roots / 仓库范围",
                    &[
                        ("j", "选择下一个仓库"),
                        ("k", "选择上一个仓库"),
                        ("Space", "勾选或取消当前仓库"),
                        ("a", "勾选全部仓库"),
                        ("F5", "刷新仓库分支信息"),
                        ("Enter", "应用仓库范围"),
                        ("Esc", "取消修改，保留原仓库范围"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Confirm { .. }) => HelpContent::new(
                HelpSection::new(
                    "审查 / 确认操作",
                    &[
                        ("j", "向下滚动审查正文"),
                        ("k", "向上滚动审查正文"),
                        ("Tab", "切换 Cancel 与 Confirm"),
                        ("Enter", "执行当前选中的按钮"),
                        ("y", "确认审查中的操作"),
                        ("n", "取消审查，返回原面板"),
                        ("Esc", "取消审查，返回原面板"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Menu { .. }) => HelpContent::new(
                HelpSection::new(
                    "Actions / 动作列表",
                    &[
                        ("j", "选择下一个动作"),
                        ("k", "选择上一个动作"),
                        ("Enter", "执行选中的动作"),
                        ("Esc", "关闭动作列表"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Open { .. }) => HelpContent::new(
                HelpSection::new(
                    "工作区 / 路径输入",
                    &[
                        ("Backspace", "删除目录输入的末尾字符"),
                        ("Ctrl-u", "清空目录输入"),
                        ("Enter", "打开或重新扫描当前输入的目录"),
                        ("Esc", "取消打开工作区"),
                    ],
                ),
                vec![],
            ),
            Some(Popup::Journal) => HelpContent::new(
                HelpSection::new("Console / 会话消息", &[("Enter", "关闭会话消息")]),
                vec![],
            ),
            Some(Popup::Help { .. }) => unreachable!("help content describes the origin"),
            None => {
                let mut content = match self.tab {
                    0 => self.log.keymap(),
                    1 => self.commit.keymap(),
                    2 => self.push.keymap(),
                    _ => self.merge.keymap(),
                };
                if !self.panel().editing() {
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
        let content = self.help_content();
        let origin = self.pop.take().map(Box::new);
        self.pop = Some(Popup::Help {
            state: HelpState::new(content),
            origin,
        });
    }
    fn menu_action(&mut self, row: usize) {
        match row {
            0..=3 => self.go(row),
            4 => self.global(0),
            5 => self.global(5),
            6 => {
                self.pop = Some(Popup::Open {
                    path: self.ctx.workspace.display().to_string(),
                })
            }
            7 => self.global(4),
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
                            self.notice("原区域的状态已变化，请重新选择快捷键".into());
                        } else if let Some(events) = command.events() {
                            for event in events {
                                self.dispatch(&event);
                            }
                        } else {
                            self.notice("无法执行此快捷键".into());
                        }
                    }
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
                    row = (row + 1).min(8)
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
                            *self = Self::new(c, self.icons);
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
    fn prepare_header_click(&mut self) -> bool {
        if self.tab == 0 {
            self.log.dismiss_filter_dropdown();
        } else if self.tab == 1 {
            self.commit.dismiss_view_menu();
        } else if self.tab == 2 {
            self.push.dismiss_group_menu();
        }
        !self.panel().editing()
    }
    fn dispatch(&mut self, ev: &Event) {
        if matches!(ev,Event::Key(k) if k.kind==KeyEventKind::Release) {
            return;
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
                    self.panel_event(&Event::Key(KeyEvent::new(
                        KeyCode::Char(suffix),
                        KeyModifiers::NONE,
                    )));
                    return;
                }
                SequenceEvent::Pass => (),
            }
        }
        if self.tab <= 1
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
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                if let Some(i) = self.tabs.iter().position(|r| inside(*r, m.column, m.row)) {
                    if self.prepare_header_click() {
                        self.go(i)
                    } else {
                        self.notice("先确认或退出当前输入 (Esc)".into())
                    }
                    return;
                }
                if let Some((_, n)) = self.tools.iter().find(|(r, _)| inside(*r, m.column, m.row)) {
                    let n = *n;
                    if n == 1 {
                        self.open_help();
                    } else if self.prepare_header_click() {
                        self.global(n)
                    }
                    return;
                }
            }
        }
        if let Event::Key(k) = ev {
            let editing = self.panel().editing();
            if !editing
                && !k
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
            {
                match k.code {
                    KeyCode::Char(ch @ '1'..='4') => {
                        self.go((ch as u8 - b'1') as usize);
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
        self.panel_event(ev)
    }
    fn poll(&mut self) -> bool {
        let mut change = self.key_picker.poll(Instant::now());
        if self.tab == 0 {
            match self.log.poll_preview(&self.ctx) {
                Ok(redraw) => change |= redraw,
                Err(e) => {
                    self.notice(format!("Log: {e:#}"));
                    change = true;
                }
            }
        }
        let seq = self.push.progress_seq;
        if let Some(msg) = self.push.poll(&self.ctx) {
            self.notice(msg);
            change = true
        }
        change |= seq != self.push.progress_seq;
        let a = self.commit.tick(&self.ctx);
        if !matches!(a, Action::None) {
            self.action(a);
            change = true
        }
        change
    }
    fn poll_timeout(&self) -> Duration {
        const IDLE: Duration = Duration::from_millis(150);
        let timeout = if self.tab == 0 {
            self.log.preview_wakeup().unwrap_or(IDLE).min(IDLE)
        } else {
            IDLE
        };
        self.key_picker
            .wakeup(Instant::now())
            .map_or(timeout, |remaining| remaining.min(timeout))
    }
    fn draw_header(&mut self, f: &mut Frame, area: Rect, log_window: bool) {
        const LOG_CONTEXT_WIDTH: u16 = 20; // Git, a Log page, and its × / + / › controls.
        f.render_widget(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(LINE)),
            area,
        );
        let mut left = Rect::new(area.x, area.y, area.width, 1);
        let roots = format!(
            " {} Roots {}/{} ",
            self.icons.glyph(Icon::Repo),
            self.ctx.scope.len(),
            self.ctx.repos.len()
        );
        let menu = format!(" {} ", self.icons.glyph(Icon::More));
        let help = format!(" {} ", self.icons.glyph(Icon::Help));
        let tools = [(help.as_str(), 1), (menu.as_str(), 2), (roots.as_str(), 0)];
        let tools_width: usize = tools.iter().map(|(label, _)| label.width()).sum();
        let tab_icons_width: usize = PANEL_ICONS
            .iter()
            .map(|&i| self.icons.glyph(i).width() + 1)
            .sum();
        let text_width: usize = NAMES.iter().map(|n| n.width() + 2).sum();
        let show_icons = self.icons != IconMode::Ascii
            && usize::from(area.width)
                >= text_width + tab_icons_width + usize::from(LOG_CONTEXT_WIDTH);
        let labels: [String; 4] = std::array::from_fn(|i| {
            if show_icons {
                format!(" {} {} ", self.icons.glyph(PANEL_ICONS[i]), NAMES[i])
            } else {
                format!(" {} ", NAMES[i])
            }
        });
        let tabs_width: usize = labels.iter().map(|label| label.width()).sum();
        // Keep room for the workspace title or the local Log tabs on the left.
        let mut tool_row = left;
        if usize::from(area.width) < tabs_width + tools_width + usize::from(LOG_CONTEXT_WIDTH) {
            tool_row.y += 1;
        }
        let mut button = |row: &mut Rect, label: &str, style| {
            let w = row.width.min(label.width() as u16);
            row.width -= w;
            let r = Rect::new(row.right(), row.y, w, 1);
            f.render_widget(Paragraph::new(label).style(style), r);
            r
        };
        let idle = Style::new().fg(MUTED);
        for (label, action) in tools {
            self.tools
                .push((button(&mut tool_row, label, idle), action));
        }
        if tool_row.y == left.y {
            left = tool_row;
        }
        for (i, label) in labels.iter().enumerate().rev() {
            let style = if self.tab == i {
                Style::new().fg(FG).bg(SEL).add_modifier(Modifier::BOLD)
            } else {
                idle
            };
            self.tabs[i] = button(&mut left, label, style);
        }
        if log_window {
            if left.width >= LOG_CONTEXT_WIDTH {
                f.render_widget(
                    Paragraph::new(" Git ").style(Style::new().fg(FG).add_modifier(Modifier::BOLD)),
                    Rect::new(left.x, left.y, 5, 1),
                );
                left.x += 6;
                left.width -= 6;
            }
            self.log.draw_tabs(f, left, &self.ctx);
        } else if self.tab == 1 && !self.ctx.repos.is_empty() {
            f.render_widget(
                Paragraph::new(" Commit ").style(Style::new().fg(FG).add_modifier(Modifier::BOLD)),
                left,
            );
        } else {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" IGit ", Style::new().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(
                            "  {} ",
                            self.ctx
                                .workspace
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        ),
                        Style::default().fg(FG),
                    ),
                    Span::styled(" / workspace", Style::default().fg(MUTED)),
                ])),
                left,
            );
            if area.width > 85 && tool_row.y == area.y {
                let console = format!(" {} Console ", self.icons.glyph(Icon::Console));
                let w = console.width() as u16;
                let r = Rect::new(area.right() - w, area.y + 1, w, 1);
                f.render_widget(Paragraph::new(console).style(idle), r);
                self.tools.push((r, 4));
            }
        }
    }
    fn draw_git_window(&mut self, f: &mut Frame, a: Rect) {
        let notice = u16::from(!self.msg.is_empty());
        let body = Rect::new(a.x, a.y + 2, a.width, a.height - 2 - notice);
        if self.tab == 0 {
            self.log.draw(f, body, &self.ctx);
        } else {
            self.commit
                .draw_with_cursor(f, body, &self.ctx, self.pop.is_none());
        }
        self.mode_active = self.panel().editing();
        if notice > 0 {
            f.render_widget(
                Paragraph::new(self.msg.as_str()).style(Style::new().fg(ACCENT)),
                Rect::new(a.x, a.bottom() - 1, a.width, 1),
            );
        }
    }
    fn draw(&mut self, f: &mut Frame) {
        let a = f.area();
        self.key_picker.clear_geometry();
        self.tabs = [Rect::default(); NAMES.len()];
        self.tools.clear();
        self.pop_hits.clear();
        f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), a);
        if a.width < 44 || a.height < 12 {
            f.render_widget(
                Paragraph::new(
                    "IGit · 窗口过小\n至少 44 列 × 12 行；推荐 120 × 36。\nq 退出，y 确认。",
                )
                .wrap(Wrap { trim: false }),
                a,
            );
            return;
        }
        let log_window = self.tab == 0 && !self.ctx.repos.is_empty();
        if self.tab <= 1 && !self.ctx.repos.is_empty() {
            self.draw_git_window(f, a);
        } else {
            self.draw_workbench(f, a);
        }
        self.draw_header(f, Rect::new(a.x, a.y, a.width, 2), log_window);
        if let Some(sequence) = self.key_sequence() {
            let footer = if self.tab <= 1 {
                u16::from(!self.msg.is_empty())
            } else {
                2
            };
            self.key_picker.draw(
                f,
                Rect::new(a.x, a.y + 2, a.width, a.height - 2 - footer),
                sequence,
            );
        }
        self.draw_popup(f, a);
    }
    fn draw_workbench(&mut self, f: &mut Frame, a: Rect) {
        let row = |y, h| Rect::new(a.x, a.y + y, a.width, h);
        let main = row(2, a.height - 4);
        if self.ctx.repos.is_empty() {
            f.render_widget(Paragraph::new("No Git working trees found.\n\nUse : Actions → Open workspace, or start igit /path/to/workspace.\nF5 refreshes the active view; reopen the workspace to rescan roots.").wrap(Wrap{trim:true}).block(pane("Workspace",true)),main)
        } else {
            match self.tab {
                2 => self.push.draw(f, main, &self.ctx),
                _ => self.merge.draw(f, main, &self.ctx),
            }
        }
        self.mode_active = self.panel().editing();
        let mode = if self.pop.is_some() {
            "DIALOG"
        } else if self.mode_active {
            "INPUT"
        } else {
            "NORMAL"
        };
        let foot = row(a.height - 2, 1);
        let note = if self.msg.is_empty() {
            self.panel().help()
        } else {
            &self.msg
        };
        f.render_widget(
            Paragraph::new(note).style(Style::default().fg(if self.msg.is_empty() {
                MUTED
            } else {
                ACCENT
            })),
            foot,
        );
        let bar = row(a.height - 1, 1);
        let bar_style = Style::new().fg(FG).bg(TAG);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {mode} "), button_style(true)),
                Span::raw(format!(" {}  ·  F5 refresh  ·  ? keys", NAMES[self.tab])),
            ]))
            .style(bar_style),
            bar,
        );
        let qr = Rect::new(a.right() - 8, bar.y, 8, 1);
        f.render_widget(Paragraph::new(" q Exit ").style(bar_style), qr);
        self.tools.push((qr, 3));
    }
    fn draw_popup(&mut self, f: &mut Frame, a: Rect) {
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
            Popup::Scope { .. } => "Workspace roots",
            Popup::Help { .. } => "IGit · 快捷键",
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
                for (i, s) in [
                    "01  Log",
                    "02  Commit",
                    "03  Push",
                    "04  Merge",
                    "Roots / 选仓库",
                    "Refresh panel",
                    "Open / rescan workspace…",
                    "Console",
                    "Quit",
                ]
                .iter()
                .enumerate()
                .take(inner.height as usize)
                {
                    let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                    f.render_widget(
                        Paragraph::new(*s).style(Style::default().bg(if i == *row {
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
struct Tty;
impl Drop for Tty {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
    }
}
pub fn run() -> Result<()> {
    let mut list = false;
    let mut icons = IconMode::default();
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
            icons = parse_icons(&value.to_string_lossy())?;
        } else if let Some(value) = a.to_str().and_then(|s| s.strip_prefix("--icons=")) {
            icons = parse_icons(value)?;
        } else if a.to_string_lossy().starts_with('-') {
            bail!("Unknown option: {}\n{HELP}", a.to_string_lossy())
        } else if path.is_none() {
            path = Some(PathBuf::from(a))
        } else {
            bail!("{HELP}")
        }
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
    let mut app = Shell::new(ctx, icons);
    let mut out = io::stdout();
    let _guard = Tty;
    enable_raw_mode()?;
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
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
                    dirty = app.pop.is_none()
                        && !app.key_picker.contains(m.column, m.row)
                        && match app.tab {
                            0 => app.log.hover_menu_at(m.column, m.row),
                            1 => app.commit.hover_at(m.column, m.row),
                            _ => false,
                        };
                }
                _ => {
                    let motion = app.tab == 0 && app.pop.is_none() && app.log.graph_motion(&e);
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

#[cfg(test)]
mod key_sequence_tests;

#[cfg(test)]
mod help_tests;

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
        key(&mut app, KeyCode::Char('j')); // Highlight the repository.
        key(&mut app, KeyCode::Char('v'));
        assert!(screen(&mut app).contains("on main"));
        git(&repo, &["switch", "-qc", "fresh-main"]).unwrap();
        git(&repo, &["tag", "fresh-main"]).unwrap();
        key(&mut app, KeyCode::F(5));
        let refreshed = screen(&mut app);
        assert!(refreshed.contains("on fresh-main"), "{refreshed}");
        assert_eq!(refreshed.matches("fresh-main").count(), 2); // Tree badge and repository preview.

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

        send(&mut app, key(KeyCode::Char('b')));
        let roots = app.tools.iter().find(|(_, n)| *n == 0).unwrap().0;
        let commit = app.tabs[1];
        send(&mut app, click(roots));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        assert!(!app.log.editing());
        send(&mut app, click(commit));
        assert_eq!(app.tab, 0);
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('o')));
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Char('2')));
        assert_eq!(app.tab, 0);
        send(&mut app, click(roots));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        assert!(!app.log.editing());
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('/')));
        send(&mut app, click(commit));
        assert_eq!(app.tab, 0);
        assert!(app.log.editing());
        send(&mut app, key(KeyCode::Esc));

        send(&mut app, key(KeyCode::Char('d')));
        send(&mut app, click(commit));
        assert_eq!(app.tab, 1);
        assert!(!app.log.editing());
    }
}
