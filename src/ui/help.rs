//! Contextual shortcut palette, matching LazyDB's KEYMAP panel.
use super::{icons::IconMode, idea::clipped, inside};
use crossterm::event::{
    Event, KeyCode as K, KeyEvent, KeyModifiers as M, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

// LazyDB's deep_space palette, scoped to this explicitly matching panel.
const SURFACE: Color = Color::Rgb(17, 28, 43);
const TITLE_BG: Color = Color::Rgb(12, 19, 30);
const TEXT: Color = Color::Rgb(215, 226, 237);
const MUTED: Color = Color::Rgb(105, 126, 146);
const ACCENT: Color = Color::Rgb(99, 230, 216);
const ACTION: Color = Color::Rgb(101, 167, 255);

/// Each sequence is executable and represents exactly one action.
pub(crate) type Binding = (&'static str, &'static str);

#[derive(Clone, Debug)]
pub(crate) struct HelpSection {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

impl HelpSection {
    pub const fn new(title: &'static str, bindings: &'static [Binding]) -> Self {
        Self { title, bindings }
    }
}

#[derive(Debug)]
pub(crate) struct HelpContent {
    pub sections: Vec<HelpSection>,
}

impl HelpContent {
    pub fn new(current: HelpSection, sections: Vec<HelpSection>) -> Self {
        let title = current.title;
        let mut ordered = vec![current];
        ordered.extend(sections.into_iter().filter(|s| s.title != title));
        Self { sections: ordered }
    }

    pub fn context(&self) -> &'static str {
        self.sections.first().map_or("IGit", |s| s.title)
    }

    pub fn with_selection(mut self, selected: bool, read_only: bool) -> Self {
        if selected {
            self.sections.push(HelpSection::new(
                "文本选区",
                if read_only {
                    &[
                        ("Ctrl-c", "复制选中文本"),
                        ("Ctrl-Insert", "复制选中文本"),
                        ("y", "复制选中文本"),
                    ]
                } else {
                    &[("Ctrl-c", "复制选中文本"), ("Ctrl-Insert", "复制选中文本")]
                },
            ));
        }
        self
    }
}

pub(crate) fn global_section() -> HelpSection {
    HelpSection::new(
        "全局操作",
        &[
            ("1", "聚焦下半区 Git Log"),
            ("2", "聚焦上半区 Commit"),
            ("3", "打开 Push 弹窗"),
            ("4", "打开 Merge 弹窗"),
            ("r", "打开 Roots 仓库范围"),
            (":", "打开动作列表"),
            ("F5", "刷新分屏和当前操作窗口"),
            ("F1", "打开当前区域的帮助"),
            ("q", "审查并退出 IGit"),
        ],
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpCommand {
    pub sequence: &'static str,
}

impl HelpCommand {
    /// Only static catalogue sequences are parsed, never the search query.
    pub fn events(self) -> Option<Vec<Event>> {
        if let Some((keys, _)) = super::panes::WINDOW_KEYS
            .iter()
            .find(|(keys, _)| *keys == self.sequence)
        {
            return Some(vec![
                Event::Key(KeyEvent::new(K::Char('w'), M::CONTROL)),
                Event::Key(KeyEvent::new(K::Char(keys.chars().last()?), M::NONE)),
            ]);
        }
        if self.sequence.contains(char::is_whitespace) {
            return None;
        }
        let mut events = vec![];
        for token in self.sequence.split_whitespace() {
            if matches!(token, "gg" | "yy" | "ym" | "ya" | "[c" | "]c") {
                events.extend(
                    token
                        .chars()
                        .map(|ch| Event::Key(KeyEvent::new(K::Char(ch), M::NONE))),
                );
                continue;
            }
            let mut modifiers = M::NONE;
            let mut key = token;
            while let Some((modifier, rest)) = key.split_once('-') {
                modifiers |= match modifier.to_ascii_lowercase().as_str() {
                    "ctrl" => M::CONTROL,
                    "alt" => M::ALT,
                    "shift" => M::SHIFT,
                    _ => return None,
                };
                key = rest;
            }
            let code = match key {
                "Enter" => K::Enter,
                "Esc" => K::Esc,
                "Space" => K::Char(' '),
                "Tab" if modifiers.contains(M::SHIFT) => K::BackTab,
                "Tab" => K::Tab,
                "Up" => K::Up,
                "Down" => K::Down,
                "Left" => K::Left,
                "Right" => K::Right,
                "Home" => K::Home,
                "End" => K::End,
                "PgUp" => K::PageUp,
                "PgDn" => K::PageDown,
                "Backspace" => K::Backspace,
                "Delete" => K::Delete,
                "Insert" => K::Insert,
                s if s.starts_with('F') && s.len() > 1 => {
                    let number = s[1..].parse::<u8>().ok().filter(|n| (1..=12).contains(n))?;
                    K::F(number)
                }
                s if s.chars().count() == 1 => K::Char(s.chars().next()?),
                _ => return None,
            };
            events.push(Event::Key(KeyEvent::new(code, modifiers)));
        }
        (!events.is_empty()).then_some(events)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum HelpEvent {
    Keep,
    Close,
    Run(HelpCommand),
}

#[derive(Debug)]
pub(crate) struct HelpState {
    pub content: HelpContent,
    query: String,
    caret: usize,
    selected: usize,
    scroll: usize,
    viewport: Rect,
    search: Rect,
    close: Rect,
    scrollbar: Rect,
}

impl HelpState {
    pub fn new(content: HelpContent) -> Self {
        Self {
            content,
            query: String::new(),
            caret: 0,
            selected: 0,
            scroll: 0,
            viewport: Rect::default(),
            search: Rect::default(),
            close: Rect::default(),
            scrollbar: Rect::default(),
        }
    }

    fn entries(&self) -> Vec<Binding> {
        let tokens: Vec<_> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let mut entries = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for section in &self.content.sections {
            for binding in section.bindings {
                if !seen.insert(binding.0) {
                    continue;
                }
                let text = format!("{} {} {}", section.title, binding.0, binding.1).to_lowercase();
                if tokens.iter().all(|token| text.contains(token)) {
                    entries.push(*binding);
                }
            }
        }
        entries
    }

    fn command(&self) -> Option<HelpCommand> {
        let binding = *self.entries().get(self.selected)?;
        Some(HelpCommand {
            sequence: binding.0,
        })
    }

    fn normalize(&mut self) {
        let count = self.entries().len();
        self.selected = self.selected.min(count.saturating_sub(1));
        let height = self.viewport.height.max(1) as usize;
        self.scroll = self.scroll.min(count.saturating_sub(height));
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.entries().len();
        if count > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
        }
        self.normalize();
    }

    fn set_scroll(&mut self, offset: usize) {
        let count = self.entries().len();
        let height = self.viewport.height.max(1) as usize;
        self.scroll = offset.min(count.saturating_sub(height));
        self.selected = self.selected.clamp(
            self.scroll,
            (self.scroll + height - 1).min(count.saturating_sub(1)),
        );
    }

    fn edit(&mut self, text: &str) {
        self.query.insert_str(self.caret, text);
        self.caret += text.len();
        self.filtered();
    }

    fn filtered(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    fn previous(&self) -> usize {
        self.query[..self.caret]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.query[self.caret..]
            .chars()
            .next()
            .map_or(self.caret, |ch| self.caret + ch.len_utf8())
    }

    pub fn event(&mut self, event: &Event) -> HelpEvent {
        match event {
            Event::Key(k) => {
                if k.code == K::Esc
                    || k.code == K::F(1)
                    || k.code == K::Char('c') && k.modifiers == M::CONTROL
                {
                    return HelpEvent::Close;
                }
                match k.code {
                    K::Enter if k.modifiers.is_empty() => {
                        return self.command().map_or(HelpEvent::Keep, HelpEvent::Run)
                    }
                    K::Up if k.modifiers.is_empty() => self.move_selection(-1),
                    K::Down if k.modifiers.is_empty() => self.move_selection(1),
                    K::PageUp => self.move_selection(-(self.viewport.height.max(1) as isize)),
                    K::PageDown => self.move_selection(self.viewport.height.max(1) as isize),
                    K::Left => self.caret = self.previous(),
                    K::Right => self.caret = self.next(),
                    K::Home => self.caret = 0,
                    K::End => self.caret = self.query.len(),
                    K::Backspace => {
                        let previous = self.previous();
                        self.query.replace_range(previous..self.caret, "");
                        self.caret = previous;
                        self.filtered();
                    }
                    K::Delete => {
                        self.query.replace_range(self.caret..self.next(), "");
                        self.filtered();
                    }
                    K::Char(ch) if k.modifiers == M::CONTROL => match ch {
                        'a' => self.caret = 0,
                        'e' => self.caret = self.query.len(),
                        'u' => {
                            self.query.clear();
                            self.caret = 0;
                            self.filtered();
                        }
                        'w' => {
                            let prefix = &self.query[..self.caret];
                            let end = prefix.trim_end().len();
                            let start = prefix[..end]
                                .char_indices()
                                .rev()
                                .find(|(_, ch)| ch.is_whitespace())
                                .map_or(0, |(i, ch)| i + ch.len_utf8());
                            self.query.replace_range(start..self.caret, "");
                            self.caret = start;
                            self.filtered();
                        }
                        _ => {}
                    },
                    K::Char(ch) if !k.modifiers.intersects(M::CONTROL | M::ALT | M::SUPER) => {
                        self.edit(&ch.to_string())
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => {
                let text: String = text
                    .chars()
                    .map(|ch| if ch.is_control() { ' ' } else { ch })
                    .collect();
                self.edit(&text);
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) if inside(self.close, m.column, m.row) => {
                    return HelpEvent::Close
                }
                MouseEventKind::Down(MouseButton::Left) if inside(self.search, m.column, m.row) => {
                    let column = m.column.saturating_sub(self.search.x + 7) as usize;
                    let start = self.input_start(self.search.width.saturating_sub(7) as usize);
                    let mut cells = 0;
                    self.caret = self.query.len();
                    for (i, ch) in self.query[start..].char_indices() {
                        if cells >= column {
                            self.caret = start + i;
                            break;
                        }
                        cells += ch.to_string().width();
                    }
                }
                MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                    if inside(self.scrollbar, m.column, m.row) =>
                {
                    let max = self
                        .entries()
                        .len()
                        .saturating_sub(self.viewport.height as usize);
                    if m.row == self.scrollbar.y {
                        self.set_scroll(self.scroll.saturating_sub(1));
                    } else if m.row + 1 == self.scrollbar.bottom() {
                        self.set_scroll(self.scroll.saturating_add(1));
                    } else {
                        let track = self.scrollbar.height.saturating_sub(3).max(1) as usize;
                        self.set_scroll((m.row - self.scrollbar.y - 1) as usize * max / track);
                    }
                }
                MouseEventKind::Down(MouseButton::Left)
                    if inside(self.viewport, m.column, m.row) =>
                {
                    let index = self.scroll + (m.row - self.viewport.y) as usize;
                    if index < self.entries().len() {
                        self.selected = index;
                    }
                }
                MouseEventKind::ScrollDown
                    if inside(self.viewport, m.column, m.row)
                        || inside(self.scrollbar, m.column, m.row) =>
                {
                    self.set_scroll(self.scroll.saturating_add(3))
                }
                MouseEventKind::ScrollUp
                    if inside(self.viewport, m.column, m.row)
                        || inside(self.scrollbar, m.column, m.row) =>
                {
                    self.set_scroll(self.scroll.saturating_sub(3))
                }
                _ => {}
            },
            _ => {}
        }
        HelpEvent::Keep
    }

    fn input_start(&self, width: usize) -> usize {
        let mut start = 0;
        while self.query[start..self.caret].width() >= width.max(1) && start < self.caret {
            start += self.query[start..].chars().next().unwrap().len_utf8();
        }
        start
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, icons: IconMode) {
        let width = area.width.saturating_sub(4).min(74);
        let height = area.height.saturating_sub(2).min(34);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        self.viewport = Rect::default();
        self.search = Rect::default();
        self.close = Rect::default();
        self.scrollbar = Rect::default();
        if width < 12 || height < 6 {
            return;
        }
        f.render_widget(Clear, popup);
        let close = " Esc close ";
        let title = clipped(
            &format!(" KEYMAP // {} ", self.content.context().to_uppercase()),
            width.saturating_sub(close.len() as u16 + 3) as usize,
        );
        let block = Block::default()
            .title(title)
            .title_top(
                Line::from(Span::styled(close, Style::new().fg(MUTED).bg(TITLE_BG)))
                    .alignment(Alignment::Right),
            )
            .title_style(
                Style::new()
                    .fg(ACCENT)
                    .bg(TITLE_BG)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(ACCENT))
            .style(Style::new().bg(SURFACE).fg(TEXT));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        self.close = Rect::new(
            popup.right() - close.len() as u16 - 1,
            popup.y,
            close.len() as u16,
            1,
        );
        self.search = Rect::new(inner.x, inner.y, inner.width, 1);
        let input_width = inner.width.saturating_sub(7) as usize;
        let start = self.input_start(input_width);
        f.render_widget(
            Paragraph::new(format!(
                "Search {}",
                clipped(&self.query[start..], input_width)
            ))
            .style(Style::new().fg(ACCENT)),
            self.search,
        );
        f.set_cursor_position((
            inner.x + 7 + self.query[start..self.caret].width() as u16,
            inner.y,
        ));
        self.viewport = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 3);
        self.normalize();
        let entries = self.entries();
        if entries.len() > self.viewport.height as usize {
            self.scrollbar = Rect::new(
                self.viewport.right() - 1,
                self.viewport.y,
                1,
                self.viewport.height,
            );
            self.viewport.width = self.viewport.width.saturating_sub(1);
        }
        let lines: Vec<_> = entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.viewport.height as usize)
            .map(|(i, (keys, description))| {
                let marker = if i == self.selected { ">" } else { " " };
                let key = clipped(keys, 18);
                Line::from(vec![
                    Span::styled(
                        format!(
                            "{marker} {key}{}",
                            " ".repeat(18usize.saturating_sub(key.width()))
                        ),
                        Style::new().fg(ACTION).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        clipped(description, self.viewport.width.saturating_sub(20) as usize),
                        Style::new().fg(TEXT),
                    ),
                ])
            })
            .collect();
        let lines = if lines.is_empty() {
            vec![Line::styled(
                "No matching shortcuts",
                Style::new().fg(MUTED),
            )]
        } else {
            lines
        };
        f.render_widget(Paragraph::new(lines), self.viewport);
        if self.scrollbar.height > 0 {
            let rail = self.scrollbar.height.saturating_sub(2) as usize;
            let count = entries.len();
            let visible = self.viewport.height as usize;
            let thumb = (rail * visible / count).max(1).min(rail);
            let top = self.scroll * (rail - thumb) / count.saturating_sub(visible).max(1);
            let ascii = icons == IconMode::Ascii;
            let mut lines = vec![Line::styled(
                if ascii { "^" } else { "▲" },
                Style::new().fg(MUTED),
            )];
            for y in 0..rail {
                let active = (top..top + thumb).contains(&y);
                let symbol = match (ascii, active) {
                    (true, true) => "#",
                    (true, false) => "|",
                    (false, true) => "┃",
                    (false, false) => "│",
                };
                lines.push(Line::styled(
                    symbol,
                    Style::new().fg(if active { ACCENT } else { MUTED }),
                ));
            }
            lines.push(Line::styled(
                if ascii { "v" } else { "▼" },
                Style::new().fg(MUTED),
            ));
            f.render_widget(Paragraph::new(lines), self.scrollbar);
        }
        f.render_widget(
            Paragraph::new("Up/Down select   Enter run   Esc close   Ctrl-W/U/A/E edit")
                .style(Style::new().fg(MUTED)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        backend::{Backend, TestBackend},
        Terminal,
    };

    fn help() -> HelpState {
        HelpState::new(HelpContent::new(
            HelpSection::new(
                "Log / 文件树",
                &[
                    ("Enter", "查看文件 Diff"),
                    ("z", "收起全部目录"),
                    ("Z", "展开全部目录"),
                ],
            ),
            vec![global_section()],
        ))
    }
    fn key(code: K) -> Event {
        Event::Key(KeyEvent::new(code, M::NONE))
    }
    fn ctrl(ch: char) -> Event {
        Event::Key(KeyEvent::new(K::Char(ch), M::CONTROL))
    }
    fn draw(help: &mut HelpState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| help.draw(f, f.area(), IconMode::Unicode))
            .unwrap();
        let mut expected = terminal.backend().clone();
        expected.show_cursor().unwrap();
        assert_eq!(
            terminal.backend(),
            &expected,
            "Search is focused immediately"
        );
        let cursor = terminal.get_cursor_position().unwrap();
        assert!(inside(help.search, cursor.x, cursor.y));
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut line = String::new();
                let mut x = 0;
                while x < width {
                    let symbol = buffer[(x, y)].symbol();
                    line.push_str(symbol);
                    x += symbol.width().max(1) as u16;
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn help_opens_with_search_cursor_and_uses_lazydb_flat_keymap_style() {
        let mut help = help();
        let text = draw(&mut help, 120, 36);
        assert!(text.contains("KEYMAP // LOG / 文件树"));
        assert!(text.contains("Search "));
        assert!(text.contains("> Enter"));
        assert!(text.contains("Up/Down select   Enter run   Esc close"));
        assert!(!text.contains("当前区域 ·"));
        assert!(!text.contains("全局操作"));
        assert!(text.contains('╭') && text.contains('╰'));
        help.event(&key(K::Char('j')));
        assert_eq!(help.query, "j", "letters edit Search instead of navigating");
        assert_eq!(help.event(&key(K::Enter)), HelpEvent::Keep);
    }

    #[test]
    fn help_search_and_unicode_cursor_editing_reset_selection() {
        let mut help = help();
        help.event(&Event::Paste("文件树 enter".into()));
        assert_eq!(help.entries().len(), 1);
        help.event(&ctrl('u'));
        help.event(&key(K::Down));
        assert_eq!(help.selected, 1);
        help.event(&Event::Paste("目录".into()));
        assert_eq!(help.selected, 0);
        assert_eq!(help.entries().len(), 2);
        help.event(&key(K::Left));
        help.event(&key(K::Char('新')));
        assert_eq!(help.query, "目新录");
        help.event(&key(K::Backspace));
        help.event(&key(K::Delete));
        assert_eq!(help.query, "目");
        help.event(&ctrl('a'));
        help.event(&Event::Paste("hello world ".into()));
        help.event(&ctrl('w'));
        assert_eq!(help.query, "hello 目");
        help.event(&ctrl('e'));
        assert_eq!(help.caret, help.query.len());
        help.event(&ctrl('u'));
        assert!(help.query.is_empty());
        help.content.sections.push(HelpSection::new(
            "面板通用",
            &[("Enter", "不应替代当前区域的描述")],
        ));
        help.event(&Event::Paste("不应替代".into()));
        assert!(
            help.entries().is_empty(),
            "filtering must preserve the current region's key precedence"
        );
    }

    #[test]
    fn help_arrows_wrap_choose_and_enter_returns_the_exact_sequence() {
        let mut help = help();
        draw(&mut help, 80, 24);
        help.event(&key(K::Down));
        assert_eq!(
            help.event(&key(K::Enter)),
            HelpEvent::Run(HelpCommand { sequence: "z" })
        );
        help.event(&key(K::Up));
        help.event(&key(K::Up));
        assert_eq!(help.selected, help.entries().len() - 1);
        assert_eq!(help.command().unwrap().sequence, "q");
        help.event(&Event::Paste("no such shortcut".into()));
        assert_eq!(help.event(&key(K::Enter)), HelpEvent::Keep);
        assert!(draw(&mut help, 44, 12).contains("No matching shortcuts"));
        assert_eq!(help.event(&key(K::Esc)), HelpEvent::Close);
    }

    #[test]
    fn help_selection_stays_visible_after_scroll_filter_and_resize() {
        let mut help = help();
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            help.event(&ctrl('u'));
            draw(&mut help, width, height);
            for _ in 0..help.entries().len() {
                help.event(&key(K::Down));
            }
            assert_eq!(help.selected, 0);
            help.event(&key(K::Up));
            let text = draw(&mut help, width, height);
            assert!(text.contains("> q"), "{text}");
            assert!(
                help.selected >= help.scroll
                    && help.selected < help.scroll + help.viewport.height as usize
            );
        }
        help.event(&ctrl('u'));
        help.event(&Event::Paste("Diff".into()));
        draw(&mut help, 44, 12);
        assert_eq!((help.selected, help.scroll), (0, 0));
    }

    #[test]
    fn help_mouse_selects_rows_and_scrolls_without_executing() {
        use crossterm::event::MouseEvent;
        let mut help = help();
        draw(&mut help, 44, 12);
        let mouse = |kind, x, y| {
            Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: M::NONE,
            })
        };
        assert_eq!(
            help.event(&mouse(
                MouseEventKind::Down(MouseButton::Left),
                help.viewport.x,
                help.viewport.y + 1
            )),
            HelpEvent::Keep
        );
        assert_eq!(help.selected, 1);
        help.event(&mouse(
            MouseEventKind::ScrollDown,
            help.viewport.x,
            help.viewport.y,
        ));
        assert!(help.scroll > 0);
        assert!(help.selected >= help.scroll);
        help.event(&mouse(
            MouseEventKind::Down(MouseButton::Left),
            help.scrollbar.x,
            help.scrollbar.bottom() - 1,
        ));
        assert!(help.selected >= help.scroll);
        assert_eq!(
            help.event(&mouse(
                MouseEventKind::Down(MouseButton::Left),
                help.close.x,
                help.close.y
            )),
            HelpEvent::Close
        );
    }

    #[test]
    fn help_commands_decode_modifiers_sequences_and_reject_non_shortcut_labels() {
        for sequence in ["gg", "yy", "ym", "ya", "[c", "]c"] {
            let events = HelpCommand { sequence }.events().unwrap();
            assert_eq!(events.len(), 2);
        }
        assert_eq!(
            HelpCommand { sequence: "Ctrl-f" }.events().unwrap(),
            vec![Event::Key(KeyEvent::new(K::Char('f'), M::CONTROL))]
        );
        assert_eq!(
            HelpCommand {
                sequence: "Shift-Tab"
            }
            .events()
            .unwrap(),
            vec![Event::Key(KeyEvent::new(K::BackTab, M::SHIFT))]
        );
        for sequence in ["输入文本", "j / k", "F99", "Run-arbitrary"] {
            assert!(HelpCommand { sequence }.events().is_none());
        }
    }
}
