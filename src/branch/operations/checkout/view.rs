use super::{label, Checkout, Decision};
use crate::ui::{
    icons::IconMode,
    idea::{button_style, GREEN, RED},
    inside,
    selection::{TextLayout, Wrap},
    ACCENT, BG, FG, LINE, MUTED,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

impl Checkout {
    pub(crate) fn event(&mut self, event: &Event) -> Decision {
        let mut activate = None;
        let count = if self.force { 2 } else { 3 };
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => return Decision::Cancel,
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    self.selected = (self.selected + count - 1) % count
                }
                KeyCode::Tab | KeyCode::Right => self.selected = (self.selected + 1) % count,
                KeyCode::BackTab | KeyCode::Left => {
                    self.selected = (self.selected + count - 1) % count
                }
                KeyCode::Enter => activate = Some(self.selected),
                KeyCode::Char('j') | KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(8),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(8),
                KeyCode::Home => self.scroll = 0,
                KeyCode::End => self.scroll = usize::MAX,
                _ => (),
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(3),
                MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
                MouseEventKind::Down(MouseButton::Left) => {
                    activate = self.hits.iter().find_map(|(area, id)| {
                        inside(*area, mouse.column, mouse.row).then_some(*id)
                    });
                }
                _ => (),
            },
            _ => (),
        }
        match activate {
            Some(usize::MAX) => Decision::Cancel,
            Some(0) if !self.force => {
                self.force = true;
                self.selected = 0;
                self.scroll = 0;
                self.hits.clear();
                Decision::Keep
            }
            Some(0) if self.force => Decision::Cancel,
            Some(1) if self.force => Decision::Run { force: true },
            Some(1) => Decision::Cancel,
            Some(2) => Decision::Run { force: false },
            _ => Decision::Keep,
        }
    }

    fn content(&self, width: u16) -> TextLayout {
        let mut content = TextLayout::default();
        let muted = Style::new().fg(MUTED);
        let mut put = |line| content.push(line, width, Wrap::Word);
        if self.force {
            put(Line::styled(
                "Discard local changes and switch?",
                Style::new().fg(RED).bold(),
            ));
            put(Line::from("All tracked changes in this repository will be discarded, including staged changes. Untracked files blocking checkout will be removed."));
            put(Line::from("This action does not create a backup."));
        } else {
            put(Line::from(
                "Local changes would be overwritten by checkout.",
            ));
            put(Line::from("Smart Checkout saves this repository's changes, switches branches, then restores the changes and staging."));
            put(Line::styled(
                "If restoration conflicts, the backup stays in Stash and Conflicts opens.",
                muted,
            ));
        }
        put(Line::default());
        put(Line::from(vec![
            Span::styled("From  ", muted),
            Span::styled(
                label(&self.review.snapshot.head_label()),
                Style::new().fg(GREEN).bold(),
            ),
            Span::styled("   To  ", muted),
            Span::styled(label(self.review.target()), Style::new().fg(ACCENT).bold()),
        ]));
        put(Line::styled(
            format!(
                "Repository: {} · {}",
                label(&self.review.repository),
                label(&self.review.path.display().to_string())
            ),
            muted,
        ));
        put(Line::default());
        put(Line::styled(
            format!(
                "{} local files · staged, unstaged, and untracked",
                self.saved.files().len()
            ),
            Style::new().fg(FG).bold(),
        ));
        for file in self.saved.files() {
            let status = if file.xy == *b"??" {
                "Untracked"
            } else if file.staged() && file.xy[1] != b' ' {
                "Staged + unstaged"
            } else if file.staged() {
                "Staged"
            } else {
                "Unstaged"
            };
            put(Line::from(vec![
                Span::styled(format!("  {}  ", file.label()), Style::new().fg(FG)),
                Span::styled(status, muted),
            ]));
        }
        if !self.force {
            put(Line::default());
            put(Line::styled(
                "Git checkout problem",
                Style::new().fg(MUTED).bold(),
            ));
            for line in self.problem.lines() {
                put(Line::styled(line.to_owned(), muted));
            }
            put(Line::default());
            put(Line::styled(
                "Local changes preview",
                Style::new().fg(FG).bold(),
            ));
            for line in self.saved.preview.lines() {
                put(Line::from(label(line)));
            }
        }
        content
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame, area: Rect, icons: IconMode) {
        self.hits.clear();
        let available = area.inner(Margin::new(
            u16::from(area.width > 24),
            u16::from(area.height > 18),
        ));
        let width = available.width.min(100);
        if width < 20 || available.height < 7 {
            return;
        }
        let buttons: &[(&str, usize)] = if self.force {
            &[(" Cancel ", 0), (" Force Checkout ", 1)]
        } else {
            &[
                (" Force Checkout ", 0),
                (" Don't Checkout ", 1),
                (" Smart Checkout ", 2),
            ]
        };
        let mut button_rows = 1;
        let mut used = 0;
        for (text, _) in buttons {
            if used > 0 && used + text.width() > usize::from(width.saturating_sub(4)) {
                button_rows += 1;
                used = 0;
            }
            used += text.width() + 1;
        }
        let footer_height = button_rows + 2;
        let content = self.content(width.saturating_sub(4));
        let height =
            (content.rows.len() + footer_height + 2).min(usize::from(available.height)) as u16;
        let popup = Rect::new(
            available.x + (available.width - width) / 2,
            available.y + (available.height - height) / 2,
            width,
            height,
        );
        frame
            .buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if icons == IconMode::Ascii {
                BorderType::Plain
            } else {
                BorderType::Rounded
            })
            .border_style(Style::new().fg(LINE))
            .style(Style::new().fg(FG).bg(BG))
            .title(Span::styled(
                if self.force {
                    " Force Checkout "
                } else {
                    " Git Checkout Problem "
                },
                Style::new()
                    .fg(if self.force { RED } else { ACCENT })
                    .bold(),
            ));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let close = Rect::new(inner.right().saturating_sub(3), popup.y, 3, 1);
        frame.render_widget(Paragraph::new(" x ").style(Style::new().fg(MUTED)), close);
        self.hits.push((close, usize::MAX));
        let footer = Rect::new(
            inner.x,
            inner.bottom() - footer_height as u16,
            inner.width,
            footer_height as u16,
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(LINE)),
            footer,
        );
        let body = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            footer.y.saturating_sub(inner.y),
        );
        let max_scroll = content.rows.len().saturating_sub(usize::from(body.height));
        self.scroll = self.scroll.min(max_scroll);
        frame.render_widget(
            Paragraph::new(
                content
                    .rows
                    .into_iter()
                    .skip(self.scroll)
                    .take(usize::from(body.height))
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
            ),
            body,
        );
        if max_scroll > 0 {
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_style(Style::new().fg(LINE))
                    .thumb_style(Style::new().fg(MUTED)),
                Rect::new(body.right(), body.y, 1, body.height),
                &mut ScrollbarState::new(max_scroll + 1).position(self.scroll),
            );
        }
        let row = footer.inner(Margin::new(1, 0));
        let mut x = row.x;
        let mut y = row.y + 1;
        for &(text, id) in buttons {
            let button_width = text.width() as u16;
            if x + button_width > row.right() && x != row.x {
                x = row.x;
                y += 1;
            }
            let button = Rect::new(x, y, button_width.min(row.width), 1);
            frame.render_widget(
                Paragraph::new(text).style(button_style(self.selected == id)),
                button,
            );
            self.hits.push((button, id));
            x = button.right() + 1;
        }
        frame.render_widget(
            Paragraph::new(if row.width >= 53 {
                "j/k Scroll  Tab/Arrows Choose  Enter Select  Esc Cancel"
            } else {
                "j/k Scroll  Tab Choose  Enter Select"
            })
            .style(Style::new().fg(MUTED)),
            Rect::new(row.x, footer.bottom() - 1, row.width, 1),
        );
    }
}
