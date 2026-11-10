use super::{
    help::{HelpContent, HelpSection},
    icons::IconMode,
    idea::{button_style, clipped},
    inside, pane, ACCENT, BG, FG, MUTED, SEL,
};
use crate::core::strategy::Strategy;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Pending,
    Cancel,
    Selected(Strategy),
}

pub(crate) struct Selector {
    selected: Strategy,
    hits: Vec<(Rect, Choice)>,
}

impl Selector {
    pub(crate) fn new(current: Strategy) -> Self {
        Self {
            selected: current,
            hits: vec![],
        }
    }

    pub(crate) fn event(&mut self, event: &Event) -> Choice {
        match event {
            Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) =>
            {
                match key.code {
                    KeyCode::Up | KeyCode::Left | KeyCode::Char('k' | 'm') => {
                        self.selected = Strategy::Merge;
                    }
                    KeyCode::Down | KeyCode::Right | KeyCode::Char('j' | 'r') => {
                        self.selected = Strategy::Rebase;
                    }
                    KeyCode::Enter => return Choice::Selected(self.selected),
                    KeyCode::Esc => return Choice::Cancel,
                    _ => (),
                }
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                return self
                    .hits
                    .iter()
                    .find(|(rect, _)| inside(*rect, mouse.column, mouse.row))
                    .map_or(Choice::Pending, |(_, choice)| *choice);
            }
            _ => (),
        }
        Choice::Pending
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame, area: Rect, icons: IconMode) {
        self.hits.clear();
        if area.is_empty() {
            return;
        }
        let width = area.width.min(64);
        let height = area.height.min(9);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame
            .buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        frame.render_widget(Clear, rect);
        let block = pane("Default Update Strategy", true);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let [description, options, footer] = Layout::vertical([
            Constraint::Length(u16::from(inner.height >= 5)),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(" Shared by Pull and Push").style(Style::new().fg(MUTED)),
            description,
        );
        for (index, strategy, key, detail) in [
            (
                0,
                Strategy::Merge,
                "m",
                "Keep local commits; merge incoming changes",
            ),
            (
                1,
                Strategy::Rebase,
                "r",
                "Replay local commits on the incoming branch",
            ),
        ] {
            if index >= options.height {
                break;
            }
            let selected = self.selected == strategy;
            let radio = match (icons, selected) {
                (IconMode::Ascii, true) => "(o)",
                (IconMode::Ascii, false) => "( )",
                (_, true) => "◉",
                (_, false) => "○",
            };
            let label = format!(" {radio} {key} {}", strategy.label());
            let remaining = usize::from(options.width).saturating_sub(label.width() + 3);
            let mut line = Line::from(Span::styled(
                label,
                Style::new().fg(if selected { ACCENT } else { FG }),
            ));
            if remaining >= 16 {
                line.push_span(Span::styled(
                    format!(" · {}", clipped(detail, remaining)),
                    Style::new().fg(MUTED),
                ));
            }
            let row = Rect::new(options.x, options.y + index, options.width, 1);
            frame.render_widget(
                Paragraph::new(line).style(Style::new().bg(if selected { SEL } else { BG })),
                row,
            );
            if !row.is_empty() {
                self.hits.push((row, Choice::Selected(strategy)));
            }
        }
        let mut right = footer.right();
        for (label, choice, focused) in [
            (" Enter Choose ", Choice::Selected(self.selected), true),
            (" Esc Cancel ", Choice::Cancel, false),
        ] {
            let width = (label.width() as u16).min(right.saturating_sub(footer.x));
            let button = Rect::new(right - width, footer.y, width, footer.height.min(1));
            frame.render_widget(Paragraph::new(label).style(button_style(focused)), button);
            if !button.is_empty() {
                self.hits.push((button, choice));
            }
            right = button.x.saturating_sub(1).max(footer.x);
        }
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        HelpContent::new(
            HelpSection::new(
                "Update / Default strategy",
                &[
                    ("j", "Select Rebase"),
                    ("k", "Select Merge"),
                    ("Down", "Select Rebase"),
                    ("Up", "Select Merge"),
                    ("Right", "Select Rebase"),
                    ("Left", "Select Merge"),
                    ("m", "Select Merge"),
                    ("r", "Select Rebase"),
                    ("Enter", "Choose default update strategy"),
                    ("Esc", "Cancel strategy selection"),
                ],
            ),
            vec![],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{backend::TestBackend, Terminal};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn selection_waits_for_confirmation_and_cancel_returns_no_strategy() {
        let mut selector = Selector::new(Strategy::Rebase);
        assert_eq!(
            selector.event(&key(KeyCode::Enter)),
            Choice::Selected(Strategy::Rebase)
        );
        for code in [
            KeyCode::Up,
            KeyCode::Left,
            KeyCode::Char('k'),
            KeyCode::Char('m'),
        ] {
            assert_eq!(selector.event(&key(code)), Choice::Pending);
            assert_eq!(
                selector.event(&key(KeyCode::Enter)),
                Choice::Selected(Strategy::Merge)
            );
            assert_eq!(selector.event(&key(KeyCode::Char('r'))), Choice::Pending);
        }
        for code in [
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Char('j'),
            KeyCode::Char('r'),
        ] {
            selector.event(&key(KeyCode::Char('m')));
            assert_eq!(selector.event(&key(code)), Choice::Pending);
            assert_eq!(
                selector.event(&key(KeyCode::Enter)),
                Choice::Selected(Strategy::Rebase)
            );
        }
        assert_eq!(selector.event(&key(KeyCode::Esc)), Choice::Cancel);
        assert_eq!(
            selector.event(&Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::CONTROL
            ))),
            Choice::Pending
        );
        let mut released = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        released.kind = KeyEventKind::Release;
        assert_eq!(selector.event(&Event::Key(released)), Choice::Pending);
    }

    #[test]
    fn mouse_can_choose_each_strategy_or_cancel_in_all_icon_modes() {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            for (width, height) in [(44, 12), (80, 24), (120, 36), (12, 4), (1, 1)] {
                let mut selector = Selector::new(Strategy::Merge);
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| selector.draw(frame, frame.area(), icons))
                    .unwrap();
                for (index, (rect, choice)) in selector.hits.clone().into_iter().enumerate() {
                    assert!(rect.right() <= width && rect.bottom() <= height);
                    for (other, _) in selector.hits.iter().skip(index + 1) {
                        assert!(rect.intersection(*other).is_empty());
                    }
                    assert_eq!(
                        selector.event(&Event::Mouse(MouseEvent {
                            kind: MouseEventKind::Down(MouseButton::Left),
                            column: rect.x,
                            row: rect.y,
                            modifiers: KeyModifiers::NONE,
                        })),
                        choice
                    );
                }
                if width >= 44 {
                    for expected in [
                        Choice::Selected(Strategy::Merge),
                        Choice::Selected(Strategy::Rebase),
                        Choice::Cancel,
                    ] {
                        assert!(selector.hits.iter().any(|(_, choice)| *choice == expected));
                    }
                }
            }
        }
    }
}
