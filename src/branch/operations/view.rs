use super::{Operation, Review};
use crate::{
    branch::BranchPanel,
    ui::{
        icons::{Icon, IconMode},
        idea::{button_style, repo_tint, LOCAL_REF, REMOTE_REF},
        selection::{TextLayout, Wrap},
        ACCENT, BG, FG, LINE, MUTED,
    },
};
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

impl BranchPanel {
    pub(crate) fn draw_merge_review(
        &self,
        frame: &mut Frame,
        area: Rect,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) -> bool {
        let Some(review) = self
            .review
            .as_ref()
            .filter(|review| review.operation == Operation::Merge)
        else {
            return false;
        };
        review.draw(frame, area, self.icons, scroll, confirm_focused, hits);
        true
    }
}

impl Review {
    fn content(&self, width: u16) -> TextLayout {
        let mut content = TextLayout::default();
        let muted = Style::new().fg(MUTED);
        let mut put = |line| {
            content.push(line, width, Wrap::Word);
        };
        put(Line::from(vec![
            Span::styled("From  ", muted),
            Span::styled(
                &self.source.name,
                Style::new()
                    .fg(if self.source.remote {
                        REMOTE_REF
                    } else {
                        LOCAL_REF
                    })
                    .bold(),
            ),
        ]));
        put(Line::styled(
            format!(
                "      {}",
                &self.source.oid[..self.source.oid.len().min(12)]
            ),
            muted,
        ));
        put(Line::from(vec![
            Span::styled("Into  ", muted),
            Span::styled(self.snapshot.head_label(), Style::new().fg(ACCENT).bold()),
            Span::styled(" (current)", muted),
        ]));
        let current = self.snapshot.oid.as_deref().unwrap_or("(no commits)");
        put(Line::styled(
            format!("      {}", &current[..current.len().min(12)]),
            muted,
        ));
        put(Line::default());
        put(Line::from(vec![
            Span::styled("Repository  ", muted),
            Span::styled(
                &self.repository,
                Style::new().fg(repo_tint(self.root)).bold(),
            ),
        ]));
        put(Line::styled(self.path.display().to_string(), muted));
        put(Line::default());
        put(Line::styled("What happens", Style::new().fg(FG).bold()));
        put(Line::from("Git may fast-forward or create a merge commit."));
        put(Line::styled(
            "Resolve any conflicts in the Conflicts tab.",
            muted,
        ));
        content
    }

    fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        icons: IconMode,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        let available = area.inner(Margin::new(1, u16::from(area.height >= 18)));
        let width = available.width.min(78);
        let text_width = width.saturating_sub(4);
        if text_width < 20 || available.height < 6 {
            return;
        }
        let content = self.content(text_width);
        let padding = u16::from(available.height >= 18);
        let height = (content.rows.len() + usize::from(4 + padding * 2))
            .min(usize::from(available.height)) as u16;
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
                " Merge branch ",
                Style::new().fg(ACCENT).bold(),
            ));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let close = format!(" {} ", icons.glyph(Icon::Close));
        let close_width = close.width() as u16;
        let close_area = Rect::new(inner.right() - close_width, popup.y, close_width, 1);
        frame.render_widget(
            Paragraph::new(close).style(Style::new().fg(MUTED)),
            close_area,
        );
        hits.push((close_area, usize::MAX));

        let footer = Rect::new(inner.x, inner.bottom() - 2, inner.width, 2);
        let body = Rect::new(
            inner.x + 1,
            inner.y + padding,
            text_width,
            footer.y.saturating_sub(inner.y + padding * 2),
        );
        let max_scroll = content.rows.len().saturating_sub(usize::from(body.height));
        *scroll = usize::from(*scroll).min(max_scroll) as u16;
        frame.render_widget(
            Paragraph::new(
                content
                    .rows
                    .into_iter()
                    .skip(usize::from(*scroll))
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
                &mut ScrollbarState::new(max_scroll + 1).position(usize::from(*scroll)),
            );
        }
        frame.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(LINE)),
            footer,
        );
        let row = footer.inner(Margin::new(1, 0));
        let mut right = row.right();
        for (label, id, focused) in [
            (" Confirm ", 1, confirm_focused),
            (" Cancel ", 0, !confirm_focused),
        ] {
            let button = Rect::new(
                right - label.width() as u16,
                row.y + 1,
                label.width() as u16,
                1,
            );
            frame.render_widget(Paragraph::new(label).style(button_style(focused)), button);
            hits.push((button, id));
            right = button.x - 1;
        }
        let hints = if max_scroll > 0 {
            ["j/k Scroll  Tab Switch  Enter Select", "j/k Scroll", ""]
        } else {
            ["Tab Switch  Enter Select  Esc Cancel", "Enter Select", ""]
        };
        if let Some(hint) = hints
            .into_iter()
            .find(|hint| hint.width() <= usize::from(right - row.x))
        {
            frame.render_widget(
                Paragraph::new(hint).style(Style::new().fg(MUTED)),
                Rect::new(row.x, row.y + 1, right - row.x, 1),
            );
        }
    }
}
