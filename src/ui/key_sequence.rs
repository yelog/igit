//! Transient chooser for the suffixes of an already pending panel shortcut.
use super::{idea::clipped, inside, pane, ACCENT, BG, FG, SEL};
use crossterm::event::{Event, KeyCode as K, KeyModifiers as M, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

const HINT_DELAY: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowPrefix {
    pub mask: u16,
    pub count: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeySequence {
    LogGoto,
    LogYank,
    MergePrevious,
    MergeNext,
    Window(WindowPrefix),
}

impl KeySequence {
    fn prefix(self) -> &'static str {
        match self {
            Self::LogGoto => "g",
            Self::LogYank => "y",
            Self::MergePrevious => "[",
            Self::MergeNext => "]",
            Self::Window(_) => "Ctrl-W",
        }
    }

    fn shortcuts(self) -> Vec<(char, &'static str)> {
        let entries: &[(char, &str)] = match self {
            Self::LogGoto => &[('g', "Go to first commit")],
            Self::LogYank => &[
                ('y', "Copy full commit ID"),
                ('m', "Copy full commit message"),
                ('a', "Copy author name and email"),
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

#[derive(Default)]
pub(crate) struct KeySequencePicker {
    show_at: Option<Instant>,
    selected: usize,
    columns: usize,
    area: Rect,
    hits: Vec<(Rect, char)>,
}

impl KeySequencePicker {
    pub fn pending(now: Instant) -> Self {
        Self {
            show_at: Some(now + HINT_DELAY),
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

    pub fn event(&mut self, event: &Event, sequence: KeySequence) -> SequenceEvent {
        match event {
            Event::Mouse(m) if self.contains(m.column, m.row) => {
                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    if let Some((_, suffix)) = self
                        .hits
                        .iter()
                        .find(|(rect, _)| inside(*rect, m.column, m.row))
                    {
                        return SequenceEvent::Run(*suffix);
                    }
                }
                // Borders, padding, wheel events and releases never reach content below.
                SequenceEvent::Handled
            }
            Event::Key(k) => {
                if k.code == K::Esc || k.code == K::Char('c') && k.modifiers == M::CONTROL {
                    return SequenceEvent::Cancel;
                }
                if self.show_at.is_some() || !k.modifiers.is_empty() {
                    return SequenceEvent::Pass;
                }
                let shortcuts = sequence.shortcuts();
                if shortcuts.is_empty() {
                    return SequenceEvent::Pass;
                }
                self.selected = self.selected.min(shortcuts.len() - 1);
                match k.code {
                    K::Enter => SequenceEvent::Run(shortcuts[self.selected].0),
                    K::Up | K::Down | K::Left | K::Right => {
                        let rows = shortcuts.len().div_ceil(self.columns.max(1));
                        let column = self.selected / rows;
                        let row = self.selected % rows;
                        let start = column * rows;
                        let count = rows.min(shortcuts.len() - start);
                        self.selected = match k.code {
                            K::Up => start + (row + count - 1) % count,
                            K::Down => start + (row + 1) % count,
                            K::Left if column > 0 => start - rows + row,
                            K::Right if start + rows < shortcuts.len() => {
                                (start + rows + row).min(shortcuts.len() - 1)
                            }
                            _ => self.selected,
                        };
                        SequenceEvent::Handled
                    }
                    _ => SequenceEvent::Pass,
                }
            }
            _ => SequenceEvent::Pass,
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, sequence: KeySequence) {
        self.clear_geometry();
        if self.show_at.is_some() || area.width < 16 || area.height < 3 {
            return;
        }
        let shortcuts = sequence.shortcuts();
        if shortcuts.is_empty() {
            return;
        }
        let inner_width = usize::from(area.width.saturating_sub(4));
        let cell_width = shortcuts
            .iter()
            .map(|(_, description)| description.width() + 5)
            .max()
            .unwrap_or(1)
            .max(26);
        let columns = (inner_width / cell_width).clamp(1, 3).min(shortcuts.len());
        let rows = shortcuts.len().div_ceil(columns);
        self.columns = shortcuts.len().div_ceil(rows);
        self.selected = self.selected.min(shortcuts.len() - 1);
        let height = (rows as u16 + 2).min(area.height);
        self.area = Rect::new(area.x + 1, area.bottom() - height, area.width - 2, height);
        let prefix = match sequence {
            KeySequence::Window(WindowPrefix { count: Some(n), .. }) => format!("Ctrl-W {n}"),
            _ => sequence.prefix().into(),
        };
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
                        format!(" {suffix}  "),
                        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(clipped(description, width.saturating_sub(5))),
                ]))
                .style(Style::new().fg(FG).bg(background)),
                rect,
            );
            self.hits.push((rect, *suffix));
        }
    }
}
