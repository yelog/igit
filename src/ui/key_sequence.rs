//! Transient chooser for the suffixes of an already pending panel shortcut.
use super::{idea::clipped, inside, pane, ACCENT, BG, FG, SEL};
use crossterm::event::{
    Event, KeyCode as K, KeyEvent, KeyModifiers as M, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
const HINT_DELAY: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowPrefix {
    pub mask: u16,
    pub count: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeySequence {
    LogGoto,
    BranchGoto,
    PathCopy,
    CommitCopy,
    LogFilters,
    MergePrevious,
    MergeNext,
    Window(WindowPrefix),
}

impl KeySequence {
    fn prefix(self) -> &'static str {
        match self {
            Self::LogGoto | Self::BranchGoto => "g",
            Self::CommitCopy | Self::PathCopy => "y",
            Self::LogFilters => "Space",
            Self::MergePrevious => "[",
            Self::MergeNext => "]",
            Self::Window(_) => "Ctrl-W",
        }
    }

    fn shortcuts(self) -> Vec<(char, &'static str)> {
        let entries: &[(char, &str)] = match self {
            Self::LogGoto => &[('g', "Go to first commit")],
            Self::BranchGoto => &[('g', "Go to first row")],
            Self::PathCopy => &[
                ('y', "Copy absolute path"),
                ('f', "Copy name"),
                ('n', "Copy name without extension"),
                ('r', "Copy relative path"),
            ],
            Self::CommitCopy => &[
                ('y', "Copy full commit ID"),
                ('m', "Copy full commit message"),
                ('a', "Copy author name and email"),
            ],
            Self::LogFilters => &[
                ('b', "Open Branch filter"),
                ('u', "Open User filter"),
                ('d', "Open Date filter"),
                ('p', "Open Paths filter"),
            ],
            Self::MergePrevious => &[('c', "Previous unresolved conflict")],
            Self::MergeNext => &[('c', "Next unresolved conflict")],
            Self::Window(prefix) => {
                return super::panes::window_keys(prefix.mask)
                    .map(|(keys, description)| (keys.chars().last().unwrap(), *description))
                    .collect()
            }
        };
        entries.to_vec()
    }
}

pub(crate) enum SequenceEvent {
    Pass,
    Handled,
    Cancel,
    Run(char),
}

pub(crate) enum PickerEvent {
    Pass,
    Handled,
    Cancel,
    Run(usize),
}

#[derive(Default)]
pub(crate) struct KeySequencePicker {
    show_at: Option<Instant>,
    selected: usize,
    columns: usize,
    area: Rect,
    hits: Vec<(Rect, usize)>,
}

impl KeySequencePicker {
    #[cfg(test)]
    pub fn pending(now: Instant) -> Self {
        Self::pending_with_delay(now, HINT_DELAY)
    }

    pub fn pending_with_delay(now: Instant, delay: Duration) -> Self {
        Self {
            show_at: Some(now + delay),
            ..Self::default()
        }
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        if self.show_at.is_some_and(|deadline| now >= deadline) {
            self.show_at = None;
            return true;
        }
        false
    }

    pub fn wakeup(&self, now: Instant) -> Option<Duration> {
        self.show_at
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub fn clear_geometry(&mut self) {
        self.area = Rect::default();
        self.hits.clear();
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        inside(self.area, x, y)
    }

    /// Keep default suffixes and picker controls ahead of newly configured commands.
    pub fn accepts_key(&self, key: &KeyEvent, sequence: KeySequence) -> bool {
        if key.code == K::Esc || key.code == K::Char('c') && key.modifiers == M::CONTROL {
            return true;
        }
        if self.show_at.is_none()
            && key.modifiers.is_empty()
            && matches!(key.code, K::Enter | K::Up | K::Down | K::Left | K::Right)
        {
            return true;
        }
        let K::Char(ch) = key.code else {
            return false;
        };
        if matches!(sequence, KeySequence::Window(_))
            && (key.modifiers == M::CONTROL && matches!(ch, 'w' | 'h' | 'j' | 'k' | 'l')
                || key.modifiers.is_empty() && ch.is_ascii_digit())
        {
            return true;
        }
        (key.modifiers.is_empty() || key.modifiers == M::SHIFT)
            && sequence.shortcuts().iter().any(|(suffix, _)| *suffix == ch)
    }

    pub fn event(&mut self, event: &Event, sequence: KeySequence) -> SequenceEvent {
        let shortcuts = sequence.shortcuts();
        match self.event_items(event, shortcuts.len()) {
            PickerEvent::Pass => SequenceEvent::Pass,
            PickerEvent::Handled => SequenceEvent::Handled,
            PickerEvent::Cancel => SequenceEvent::Cancel,
            PickerEvent::Run(index) => SequenceEvent::Run(shortcuts[index].0),
        }
    }

    pub fn event_items(&mut self, event: &Event, count: usize) -> PickerEvent {
        match event {
            Event::Mouse(m) if self.contains(m.column, m.row) => {
                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    if let Some((_, index)) = self
                        .hits
                        .iter()
                        .find(|(rect, _)| inside(*rect, m.column, m.row))
                    {
                        return PickerEvent::Run(*index);
                    }
                }
                // Borders, padding, wheel events and releases never reach content below.
                PickerEvent::Handled
            }
            Event::Key(k) => {
                if k.code == K::Esc || k.code == K::Char('c') && k.modifiers == M::CONTROL {
                    return PickerEvent::Cancel;
                }
                if self.show_at.is_some() || !k.modifiers.is_empty() {
                    return PickerEvent::Pass;
                }
                if count == 0 {
                    return PickerEvent::Pass;
                }
                self.selected = self.selected.min(count - 1);
                match k.code {
                    K::Enter => PickerEvent::Run(self.selected),
                    K::Up | K::Down | K::Left | K::Right => {
                        let rows = count.div_ceil(self.columns.max(1));
                        let column = self.selected / rows;
                        let row = self.selected % rows;
                        let start = column * rows;
                        let column_count = rows.min(count - start);
                        self.selected = match k.code {
                            K::Up => start + (row + column_count - 1) % column_count,
                            K::Down => start + (row + 1) % column_count,
                            K::Left if column > 0 => start - rows + row,
                            K::Right if start + rows < count => (start + rows + row).min(count - 1),
                            _ => self.selected,
                        };
                        PickerEvent::Handled
                    }
                    _ => PickerEvent::Pass,
                }
            }
            _ => PickerEvent::Pass,
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, sequence: KeySequence) {
        let prefix = match sequence {
            KeySequence::Window(WindowPrefix { count: Some(n), .. }) => format!("Ctrl-W {n}"),
            _ => sequence.prefix().into(),
        };
        let shortcuts = sequence
            .shortcuts()
            .into_iter()
            .map(|(key, description)| (key.to_string(), description))
            .collect::<Vec<_>>();
        self.draw_items(f, area, &prefix, &shortcuts);
    }

    pub fn draw_items(
        &mut self,
        f: &mut Frame,
        area: Rect,
        prefix: &str,
        shortcuts: &[(String, &str)],
    ) {
        self.clear_geometry();
        if self.show_at.is_some() || area.width < 16 || area.height < 3 {
            return;
        }
        if shortcuts.is_empty() {
            return;
        }
        let inner_width = usize::from(area.width.saturating_sub(4));
        let cell_width = shortcuts
            .iter()
            .map(|(key, description)| key.width() + description.width() + 4)
            .max()
            .unwrap_or(1)
            .max(26);
        let columns = (inner_width / cell_width).clamp(1, 3).min(shortcuts.len());
        let rows = shortcuts.len().div_ceil(columns);
        self.columns = shortcuts.len().div_ceil(rows);
        self.selected = self.selected.min(shortcuts.len() - 1);
        let height = (rows as u16 + 2).min(area.height);
        self.area = Rect::new(area.x + 1, area.bottom() - height, area.width - 2, height);
        let title = if area.width >= 64 {
            format!("{prefix}  Arrows select  Enter run  Esc/Ctrl-C cancel")
        } else {
            format!("{prefix}  Arrows  Enter run  Esc/Ctrl-C")
        };
        f.render_widget(Clear, self.area);
        let block = pane(title, true);
        let inner = block.inner(self.area);
        f.render_widget(block, self.area);
        let width = usize::from(inner.width) / self.columns;
        let first_row = (self.selected % rows + 1).saturating_sub(usize::from(inner.height));
        for (i, (suffix, description)) in shortcuts.iter().enumerate() {
            let row = i % rows;
            if row < first_row || row - first_row >= usize::from(inner.height) {
                continue;
            }
            let column = i / rows;
            let x = inner.x + (column * width) as u16;
            let width = if column + 1 == self.columns {
                usize::from(inner.right() - x)
            } else {
                width
            };
            let rect = Rect::new(x, inner.y + (row - first_row) as u16, width as u16, 1);
            let background = if i == self.selected { SEL } else { BG };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!(" {}  ", clipped(suffix, width.saturating_sub(3))),
                        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(clipped(
                        description,
                        width.saturating_sub(suffix.width() + 4),
                    )),
                ]))
                .style(Style::new().fg(FG).bg(background)),
                rect,
            );
            self.hits.push((rect, i));
        }
    }
}
