//! Contextual shortcut palette using IGit's shared window palette.
use super::{
    icons::IconMode,
    idea::{clipped, BLUE, DIM, GOLD, INK, PANEL, SELECT, TAG},
    inside,
};
use crate::config::keys::{self, CommandAction};
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

const SEARCH_MATCH: Style = Style::new()
    .fg(GOLD)
    .add_modifier(Modifier::BOLD)
    .add_modifier(Modifier::UNDERLINED);

/// Each sequence is executable and represents exactly one action.
pub(crate) type Binding = (&'static str, &'static str);

pub(crate) const PREVIEW_SCROLL_BINDINGS: &[Binding] = &[
    ("J", "Scroll file preview down"),
    ("K", "Scroll file preview up"),
];

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
                "Text Selection",
                if read_only {
                    &[
                        ("Ctrl-c", "Copy selected text"),
                        ("Ctrl-Insert", "Copy selected text"),
                        ("y", "Copy selected text"),
                    ]
                } else {
                    &[
                        ("Ctrl-c", "Copy selected text"),
                        ("Ctrl-Insert", "Copy selected text"),
                    ]
                },
            ));
        }
        self
    }
}

pub(crate) fn global_section(show_conflicts: bool, show_scope_actions: bool) -> HelpSection {
    const BINDINGS: &[Binding] = &[
        ("r", "Open Roots repo scope"),
        ("p", "Review Pull for all repositories in Roots"),
        ("1", "Switch to Commit tab"),
        ("2", "Switch to Log tab"),
        ("3", "Switch to Push tab"),
        ("4", "Switch to Branch tab"),
        ("5", "Switch to Stash tab"),
        ("6", "Switch to Remote tab"),
        ("f", "Fetch repositories in Roots"),
        (":", "Open Actions"),
        ("F5", "Refresh workspace"),
        ("F9", "Open Update Center"),
        ("g n", "Open notification history"),
        ("F1", "Open help for current pane"),
        ("?", "Open help for current pane"),
        ("q", "Quit IGit"),
        ("7", "Switch to Conflicts tab"),
    ];
    let count = BINDINGS.len() - usize::from(!show_conflicts);
    HelpSection::new(
        "Global Actions",
        &BINDINGS[2 * usize::from(!show_scope_actions)..count],
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpCommand {
    pub action: CommandAction,
}

impl HelpCommand {
    /// Only static catalogue sequences are parsed, never the search query.
    pub fn events(self) -> Option<Vec<Event>> {
        let sequence = self.action.keys()?;
        if let Some((keys, _)) = super::panes::WINDOW_KEYS
            .iter()
            .find(|(keys, _)| *keys == sequence)
        {
            return Some(vec![
                Event::Key(KeyEvent::new(K::Char('w'), M::CONTROL)),
                Event::Key(KeyEvent::new(K::Char(keys.chars().last()?), M::NONE)),
            ]);
        }
        if sequence.contains(char::is_whitespace) {
            if sequence == "g n" {
                return Some(
                    keys::catalogue_sequence(sequence)
                        .into_iter()
                        .map(Event::Key)
                        .collect(),
                );
            }
            return keys::COMMANDS
                .iter()
                .any(|c| c.action == self.action)
                .then(|| {
                    keys::catalogue_sequence(sequence)
                        .into_iter()
                        .map(Event::Key)
                        .collect()
                });
        }
        let mut events = vec![];
        for token in sequence.split_whitespace() {
            if matches!(
                token,
                "gg" | "yy" | "yf" | "yn" | "yr" | "ym" | "ya" | "[c" | "]c"
            ) {
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
    labels: std::collections::BTreeMap<&'static str, Vec<String>>,
    direct_actions: std::collections::BTreeMap<&'static str, CommandAction>,
    native_commands: Vec<(String, &'static str, CommandAction)>,
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
            labels: Default::default(),
            direct_actions: Default::default(),
            native_commands: vec![],
        }
    }

    pub fn configured(content: HelpContent, config: &crate::config::AppConfig) -> Self {
        let context = crate::config::keys::context(content.context());
        let labels = content
            .sections
            .iter()
            .filter(|section| section.title != "Text Selection")
            .flat_map(|section| section.bindings)
            .filter_map(|(key, _)| {
                config
                    .help_labels(context, key)
                    .map(|labels| (*key, labels))
            })
            .collect();
        let direct_actions = content
            .sections
            .iter()
            .filter(|section| section.title != "Text Selection")
            .flat_map(|section| section.bindings)
            .filter_map(|(key, _)| {
                let target = keys::catalogue_sequence(key);
                config
                    .commands
                    .iter()
                    .find(|command| command.defaults.contains(&target))
                    .map(|command| command.command.action)
                    .filter(|action| action.is_direct())
                    .map(|action| (*key, action))
            })
            .collect();
        Self {
            labels,
            direct_actions,
            native_commands: config
                .active(context)
                .filter(|c| c.command.action.is_smart_pane() && !c.sequences.is_empty())
                .map(|c| {
                    (
                        c.sequences
                            .iter()
                            .map(|s| keys::label(s))
                            .collect::<Vec<_>>()
                            .join(" / "),
                        c.command.action.description(),
                        c.command.action,
                    )
                })
                .collect(),
            ..Self::new(content)
        }
    }

    fn search_tokens(&self) -> Vec<String> {
        self.query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect()
    }

    fn entries(&self) -> Vec<(String, &'static str, CommandAction)> {
        let tokens = self.search_tokens();
        let mut ranked: Vec<(SearchRank, usize, (String, &'static str, CommandAction))> = vec![];
        let mut seen = std::collections::BTreeSet::new();
        let mut displayed = std::collections::BTreeSet::new();
        for section in &self.content.sections {
            for binding in section.bindings {
                if !seen.insert(binding.0) {
                    continue;
                }
                let keys = match self.labels.get(binding.0) {
                    Some(labels) if labels.is_empty() => continue,
                    Some(labels) => labels.join(" / "),
                    None => binding.0.to_string(),
                };
                if !displayed.insert(keys.clone()) {
                    continue;
                }
                if let Some(rank) = search_rank(&[section.title, &keys, binding.1], &tokens) {
                    let order = ranked.len();
                    let action = self
                        .direct_actions
                        .get(binding.0)
                        .copied()
                        .unwrap_or(CommandAction::Keys(binding.0));
                    ranked.push((rank, order, (keys, binding.1, action)));
                }
            }
        }
        for (keys, description, action) in &self.native_commands {
            if let Some(rank) = search_rank(&["Panes", keys, description], &tokens) {
                let order = ranked.len();
                ranked.push((rank, order, (keys.clone(), description, *action)));
            }
        }
        ranked.sort_by_key(|(rank, order, _)| (*rank, *order));
        ranked.into_iter().map(|(_, _, entry)| entry).collect()
    }

    fn command(&self) -> Option<HelpCommand> {
        let entries = self.entries();
        let binding = entries.get(self.selected)?;
        Some(HelpCommand { action: binding.2 })
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
                            let start = super::selection::previous_word(&self.query, self.caret);
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
        // Avoid color changes that reset modifiers when Crossterm suppresses colors.
        let monochrome = crossterm::style::Colored::ansi_color_disabled_memoized();
        let [surface, title_bg, ink, muted, accent, action, selected] =
            [PANEL, TAG, INK, DIM, BLUE, BLUE, SELECT].map(|color| {
                if monochrome {
                    Color::Reset
                } else {
                    color
                }
            });
        f.render_widget(Clear, popup);
        let close = " Esc close ";
        let title = clipped(
            &format!(" KEYMAP // {} ", self.content.context().to_uppercase()),
            width.saturating_sub(close.len() as u16 + 3) as usize,
        );
        let block = Block::default()
            .title(title)
            .title_top(
                Line::from(Span::styled(close, Style::new().fg(muted).bg(title_bg)))
                    .alignment(Alignment::Right),
            )
            .title_style(
                Style::new()
                    .fg(accent)
                    .bg(title_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(accent))
            .style(Style::new().bg(surface).fg(ink));
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
            .style(Style::new().fg(accent)),
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
        // Keep the normal column spacing, but give English descriptions the
        // spare padding on narrow surfaces. Use the whole catalogue so typing
        // in Search does not shift the description column.
        let key_width = if self.viewport.width < 60 {
            self.content
                .sections
                .iter()
                .flat_map(|section| section.bindings)
                .map(|(keys, _)| {
                    self.labels
                        .get(keys)
                        .map(|labels| labels.join(" / ").width())
                        .unwrap_or_else(|| keys.width())
                        + 2
                })
                .max()
                .unwrap_or(2)
                .min(18)
        } else {
            18
        };
        let description_width = usize::from(self.viewport.width).saturating_sub(key_width + 2);
        let tokens = self.search_tokens();
        let match_style = if monochrome {
            Style::new()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            SEARCH_MATCH
        };
        let lines: Vec<_> = entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.viewport.height as usize)
            .map(|(i, (keys, description, _))| {
                let marker = if i == self.selected { "> " } else { "  " };
                let background = if i == self.selected {
                    selected
                } else {
                    surface
                };
                let key_style = Style::new()
                    .fg(action)
                    .bg(background)
                    .add_modifier(Modifier::BOLD);
                let description_style = Style::new().fg(ink).bg(background);
                let key = clipped(keys, key_width);
                let padding = key_width.saturating_sub(key.width());
                let description_text = clipped(description, description_width);
                let fill = description_width.saturating_sub(description_text.width());
                let mut spans = vec![Span::styled(marker, key_style)];
                spans.extend(search_spans(keys, key, &tokens, key_style, match_style));
                spans.push(Span::styled(" ".repeat(padding), key_style));
                spans.extend(search_spans(
                    description,
                    description_text,
                    &tokens,
                    description_style,
                    match_style,
                ));
                if fill > 0 {
                    spans.push(Span::styled(" ".repeat(fill), description_style));
                }
                Line::from(spans)
            })
            .collect();
        let lines = if lines.is_empty() {
            vec![Line::styled(
                "No matching shortcuts",
                Style::new().fg(muted),
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
                Style::new().fg(muted),
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
                    Style::new().fg(if active { accent } else { muted }),
                ));
            }
            lines.push(Line::styled(
                if ascii { "v" } else { "▼" },
                Style::new().fg(muted),
            ));
            f.render_widget(Paragraph::new(lines), self.scrollbar);
        }
        let footer = [
            "Up/Down select   Enter run   Esc close   Ctrl-W/U/A/E edit",
            "Up/Down select  Enter run  Esc close",
            "Enter run  Esc close",
            "Esc close",
        ]
        .into_iter()
        .find(|hint| hint.width() <= usize::from(inner.width))
        .unwrap_or("");
        f.render_widget(
            Paragraph::new(footer).style(Style::new().fg(muted)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum MatchKind {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SearchRank {
    fuzzies: usize,
    substrings: usize,
    prefixes: usize,
    leftmost: usize,
}

fn search_rank(fields: &[&str], tokens: &[String]) -> Option<SearchRank> {
    let fields: Vec<_> = fields.iter().map(|text| text.to_lowercase()).collect();
    let mut rank = SearchRank {
        fuzzies: 0,
        substrings: 0,
        prefixes: 0,
        leftmost: usize::MAX,
    };
    for token in tokens {
        let (kind, start) = fields
            .iter()
            .filter_map(|text| {
                let (kind, hits) = search_hits(text, token)?;
                Some((kind, hits.first()?.start))
            })
            .min()?;
        rank.leftmost = rank.leftmost.min(start);
        match kind {
            MatchKind::Exact => {}
            MatchKind::Prefix => rank.prefixes += 1,
            MatchKind::Substring => rank.substrings += 1,
            MatchKind::Fuzzy => rank.fuzzies += 1,
        }
    }
    Some(rank)
}

/// Both inputs are lowercase; returned ranges index bytes in `text`.
fn search_hits(text: &str, token: &str) -> Option<(MatchKind, Vec<std::ops::Range<usize>>)> {
    // Preserve all contiguous occurrences before falling back to a subsequence.
    let mut hits: Vec<_> = text
        .char_indices()
        .filter_map(|(start, _)| {
            text[start..]
                .starts_with(token)
                .then_some(start..start + token.len())
        })
        .collect();
    if let Some(first) = hits.first() {
        let kind = match first.start {
            0 if text.len() == token.len() => MatchKind::Exact,
            0 => MatchKind::Prefix,
            _ => MatchKind::Substring,
        };
        return Some((kind, hits));
    }

    let mut chars = text.char_indices();
    for wanted in token.chars() {
        let (start, ch) = chars.find(|(_, ch)| *ch == wanted)?;
        hits.push(start..start + ch.len_utf8());
    }
    Some((MatchKind::Fuzzy, hits))
}

fn search_spans(
    text: &str,
    display: String,
    tokens: &[String],
    style: Style,
    highlight: Style,
) -> Vec<Span<'static>> {
    if tokens.is_empty() || display.is_empty() {
        return vec![Span::styled(display, style)];
    }

    let lower = text.to_lowercase();
    let mut hits = vec![false; lower.len()];
    for token in tokens {
        if let Some((_, ranges)) = search_hits(&lower, token) {
            for range in ranges {
                hits[range].fill(true);
            }
        }
    }

    // Match against the full label so a token can extend past the visible prefix.
    let truncated = display != text;
    let visible = if truncated {
        display
            .strip_suffix('…')
            .expect("clipped label ends with an ellipsis")
    } else {
        &display
    };
    let highlight = style.patch(highlight);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut offset = 0;
    for grapheme in Span::raw(visible).styled_graphemes(style) {
        // Lowercase can change the byte length; highlight whole display graphemes.
        let end = offset
            + grapheme
                .symbol
                .chars()
                .flat_map(char::to_lowercase)
                .map(char::len_utf8)
                .sum::<usize>();
        let style = if hits[offset..end].contains(&true) {
            highlight
        } else {
            style
        };
        if let Some(span) = spans.last_mut().filter(|span| span.style == style) {
            span.content.to_mut().push_str(grapheme.symbol);
        } else {
            spans.push(Span::styled(grapheme.symbol.to_owned(), style));
        }
        offset = end;
    }
    if truncated {
        spans.push(Span::styled("…", style));
    }
    spans
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
                "Log / File Tree",
                &[
                    ("Enter", "View file diff"),
                    ("z", "Collapse all folders"),
                    ("Z", "Expand all folders"),
                ],
            ),
            vec![global_section(false, true)],
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
    fn help_opens_with_search_cursor_and_uses_the_shared_window_palette() {
        let mut help = help();
        let text = draw(&mut help, 120, 36);
        assert!(text.contains("KEYMAP // LOG / FILE TREE"));
        assert!(text.contains("Search "));
        assert!(text.contains("> Enter"));
        assert!(text.contains("Up/Down select   Enter run   Esc close"));
        assert!(!text.contains("Current pane ·"));
        assert!(!text.contains("Global Actions"));
        assert!(text.contains('╭') && text.contains('╰'));
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal
            .draw(|f| help.draw(f, f.area(), IconMode::Unicode))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let popup = Rect::new(23, 1, 74, 34);
        if !crossterm::style::Colored::ansi_color_disabled_memoized() {
            assert_eq!(buffer[(popup.x, popup.y)].fg, BLUE);
            assert_eq!(buffer[(popup.x + 1, popup.y)].fg, BLUE);
            assert_eq!(buffer[(popup.x + 1, popup.y)].bg, TAG);
            assert_eq!(
                buffer[(help.viewport.x + 2, help.viewport.y)].fg,
                BLUE,
                "shortcut keys use the shared blue action color"
            );
            assert_eq!(
                buffer[(help.viewport.x + 2, help.viewport.y)].bg,
                SELECT,
                "the selected row uses the shared selection background"
            );
            assert_eq!(
                buffer[(help.viewport.x + 20, help.viewport.y)].fg,
                INK,
                "descriptions use the shared body text color"
            );
        }
        help.event(&key(K::Char('j')));
        assert_eq!(help.query, "j", "letters edit Search instead of navigating");
        assert_eq!(help.event(&key(K::Enter)), HelpEvent::Keep);
    }

    #[test]
    fn global_help_keeps_remote_before_the_conditional_conflicts_tab() {
        let available = global_section(true, true).bindings;
        assert!(available.contains(&("6", "Switch to Remote tab")));
        assert!(available.contains(&("7", "Switch to Conflicts tab")));

        let clean = global_section(false, true).bindings;
        assert!(clean.contains(&("6", "Switch to Remote tab")));
        assert!(!clean.iter().any(|(key, _)| *key == "7"));
    }

    #[test]
    fn help_narrow_layout_keeps_long_keys_and_english_actions_readable() {
        const BINDINGS: &[Binding] = &[
            ("Shift-Right", "Extend selection right"),
            ("Ctrl-Insert", "Copy selected text"),
            ("Ctrl-w h", "Focus left pane"),
        ];
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let mut help = HelpState::new(HelpContent::new(
                HelpSection::new("Commit / Text Selection", BINDINGS),
                vec![global_section(false, true)],
            ));
            let text = draw(&mut help, width, height);
            for (keys, description) in BINDINGS {
                assert!(
                    text.lines()
                        .any(|line| line.contains(keys) && line.contains(description)),
                    "{width}x{height}: {keys} and its action should fit together\n{text}"
                );
            }
            let footer = text
                .lines()
                .find(|line| line.contains("Up/Down select"))
                .unwrap();
            assert!(footer.contains("Enter run") && footer.contains("Esc close"));
            if width >= 80 {
                assert!(footer.contains("Ctrl-W/U/A/E edit"));
            }
        }
    }

    #[test]
    fn help_search_and_unicode_cursor_editing_reset_selection() {
        let mut help = help();
        help.event(&Event::Paste("file tree enter".into()));
        assert_eq!(help.entries().len(), 1);
        help.event(&ctrl('u'));
        help.event(&key(K::Down));
        assert_eq!(help.selected, 1);
        help.event(&Event::Paste("folders".into()));
        assert_eq!(help.selected, 0);
        assert_eq!(help.entries().len(), 2);
        help.event(&ctrl('u'));
        // Fullwidth letters preserve multi-byte, two-cell cursor coverage.
        help.event(&Event::Paste("ＡＢ".into()));
        draw(&mut help, 44, 12);
        help.event(&key(K::Left));
        help.event(&key(K::Char('Ｃ')));
        assert_eq!(help.query, "ＡＣＢ");
        help.event(&key(K::Backspace));
        help.event(&key(K::Delete));
        assert_eq!(help.query, "Ａ");
        help.event(&ctrl('a'));
        help.event(&Event::Paste("hello world ".into()));
        help.event(&ctrl('w'));
        assert_eq!(help.query, "hello Ａ");
        help.event(&ctrl('e'));
        assert_eq!(help.caret, help.query.len());
        help.event(&ctrl('u'));
        assert!(help.query.is_empty());
        help.content.sections.push(HelpSection::new(
            "General",
            &[("Enter", "Must not replace the current pane description")],
        ));
        help.event(&Event::Paste("must not replace".into()));
        assert!(
            help.entries().is_empty(),
            "filtering must preserve the current region's key precedence"
        );
    }

    #[test]
    fn help_search_fuzzy_matches_fields_and_runs_the_original_command() {
        let mut help = HelpState::new(HelpContent::new(
            HelpSection::new("File Tree", &[("s", "Save current file")]),
            vec![],
        ));
        help.labels.insert("s", vec!["Ctrl-Shift-s".into()]);
        help.native_commands.push((
            "Alt-h".into(),
            "Focus left pane",
            CommandAction::Keys("Ctrl-w h"),
        ));

        for (query, action) in [
            ("CSs SCF", "s"),
            ("fltr scf", "s"),
            ("pns alh flp", "Ctrl-w h"),
        ] {
            help.event(&ctrl('u'));
            help.event(&Event::Paste(query.into()));
            assert_eq!(help.entries().len(), 1, "{query}");
            assert_eq!(
                help.event(&key(K::Enter)),
                HelpEvent::Run(HelpCommand {
                    action: CommandAction::Keys(action),
                }),
                "{query}"
            );
        }

        // A token must match in order within one field, never across hidden context.
        for query in ["fsc", "scff", "treecs", "scf missing"] {
            help.event(&ctrl('u'));
            help.event(&Event::Paste(query.into()));
            assert!(help.entries().is_empty(), "{query}");
        }
    }

    #[test]
    fn help_search_orders_match_quality_before_position() {
        let mut help = HelpState::new(HelpContent::new(
            HelpSection::new(
                "Actions",
                &[
                    ("z", "Find in log entries"),
                    ("q", "Save current file"),
                    ("file", "Exact key"),
                    ("files", "Prefix key"),
                    ("m", "File menu"),
                    ("n", "Open file now"),
                ],
            ),
            vec![],
        ));
        help.event(&Event::Paste("file".into()));
        let entries = help.entries();
        let labels: Vec<_> = entries.iter().map(|entry| entry.0.as_str()).collect();
        assert_eq!(labels, ["file", "files", "m", "n", "q", "z"]);
    }

    #[test]
    fn help_search_ranks_multiple_terms_by_the_weakest_match() {
        let mut help = HelpState::new(HelpContent::new(
            HelpSection::new(
                "Actions",
                &[
                    ("a", "Save frequent items in log entries"),
                    ("b", "File save"),
                    ("c", "Save prompt"),
                ],
            ),
            vec![],
        ));
        help.event(&Event::Paste("save file".into()));
        let entries = help.entries();
        let labels: Vec<_> = entries.iter().map(|entry| entry.0.as_str()).collect();
        assert_eq!(labels, ["b", "a"]);
    }

    #[test]
    fn help_search_renders_fuzzy_hits_in_keys_and_descriptions() {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let mut help = HelpState::new(HelpContent::new(
                HelpSection::new("Actions", &[("s", "Save current file")]),
                vec![],
            ));
            help.labels.insert("s", vec!["Ctrl-Shift-s".into()]);
            help.event(&Event::Paste("CSs SCF".into()));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| help.draw(f, f.area(), IconMode::Unicode))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let y = help.viewport.y;
            let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            for (label, hits, base) in [
                ("Ctrl-Shift-s", [0, 5, 11], BLUE),
                ("Save current file", [0, 5, 13], INK),
            ] {
                let start = row[..row.find(label).expect("matching label is visible")].width();
                for offset in 0..label.len() {
                    let cell = &buffer[((start + offset) as u16, y)];
                    let highlighted = cell.modifier.contains(Modifier::UNDERLINED);
                    if !crossterm::style::Colored::ansi_color_disabled_memoized() {
                        let color = if hits.contains(&offset) { GOLD } else { base };
                        assert_eq!(cell.fg, color, "{width}x{height}: {label} at {offset}");
                    }
                    assert_eq!(
                        highlighted,
                        hits.contains(&offset),
                        "{width}x{height}: {label} at {offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn help_search_highlights_fuzzy_characters_and_preserves_exact_matches() {
        let base = Style::new().fg(INK);
        for (text, query, expected) in [
            ("Save current file", "scf", vec!["S", "c", "f"]),
            (
                "Save current file",
                "scf current",
                vec!["S", "current", "f"],
            ),
            ("Select next stash", "stash", vec!["stash"]),
            ("Stash stash", "stash", vec!["Stash", "stash"]),
            ("aaaa", "aaa", vec!["aaaa"]),
            ("Save current file", "scff", vec![]),
            ("Save current file", "", vec![]),
            ("İx界e\u{301}Z", "i界z", vec!["İ", "界", "Z"]),
            ("Xe\u{301}界y", "e界", vec!["e\u{301}界"]),
        ] {
            let tokens: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
            let spans = search_spans(text, text.into(), &tokens, base, SEARCH_MATCH);
            assert_eq!(
                spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
                text
            );
            assert_eq!(
                spans
                    .iter()
                    .filter(|s| s.style == base.patch(SEARCH_MATCH))
                    .map(|s| s.content.as_ref())
                    .collect::<Vec<_>>(),
                expected,
                "{query:?} in {text:?}"
            );
        }
    }

    #[test]
    fn help_search_fuzzy_highlights_survive_clipping_and_monochrome() {
        let base = Style::new().fg(BLUE).add_modifier(Modifier::BOLD);
        let monochrome = Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        for highlight in [SEARCH_MATCH, monochrome] {
            let spans = search_spans(
                "İx界e\u{301}Z",
                clipped("İx界e\u{301}Z", 5),
                &["i界z".into()],
                base,
                highlight,
            );
            assert_eq!(
                spans,
                vec![
                    Span::styled("İ", base.patch(highlight)),
                    Span::styled("x", base),
                    Span::styled("界", base.patch(highlight)),
                    Span::styled("…", base),
                ]
            );
        }
    }

    #[test]
    fn help_arrows_wrap_choose_and_enter_returns_the_exact_sequence() {
        let mut help = help();
        draw(&mut help, 80, 24);
        help.event(&key(K::Down));
        assert_eq!(
            help.event(&key(K::Enter)),
            HelpEvent::Run(HelpCommand {
                action: CommandAction::Keys("z")
            })
        );
        help.event(&key(K::Up));
        help.event(&key(K::Up));
        assert_eq!(help.selected, help.entries().len() - 1);
        assert_eq!(help.command().unwrap().action, CommandAction::Keys("q"));
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
            assert!(text.contains("Quit IGit"), "{text}");
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
        for sequence in ["gg", "yy", "ym", "ya", "[c", "]c", "Space b"] {
            let events = HelpCommand {
                action: CommandAction::Keys(sequence),
            }
            .events()
            .unwrap();
            assert_eq!(events.len(), 2);
        }
        assert_eq!(
            HelpCommand {
                action: CommandAction::Keys("Ctrl-f")
            }
            .events()
            .unwrap(),
            vec![Event::Key(KeyEvent::new(K::Char('f'), M::CONTROL))]
        );
        assert_eq!(
            HelpCommand {
                action: CommandAction::Keys("Shift-Tab")
            }
            .events()
            .unwrap(),
            vec![Event::Key(KeyEvent::new(K::BackTab, M::SHIFT))]
        );
        for sequence in ["Input text", "j / k", "F99", "Run-arbitrary"] {
            assert!(HelpCommand {
                action: CommandAction::Keys(sequence)
            }
            .events()
            .is_none());
        }
    }
}
