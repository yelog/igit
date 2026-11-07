//! Present the reviewed stash and destination from the snapshot used for confirmation.
use super::{Operation, Review};
use crate::ui::{
    icons::{Icon, IconMode},
    idea::{button_style, CONFLICT, RED, TAG},
    selection::{TextLayout, Wrap},
    ACCENT, BG, FG, LINE, MUTED,
};
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

const LABEL_WIDTH: u16 = 13;

impl Review {
    pub(in crate::stash) fn draw(
        &self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        let available = area.inner(Margin::new(
            if area.width >= 80 { 2 } else { 1 },
            u16::from(area.height >= 18),
        ));
        let width = available.width.min(78);
        let text_width = width.saturating_sub(4);
        if text_width <= LABEL_WIDTH || available.height < 10 {
            return;
        }
        let (tone, action) = match self.operation {
            Operation::Apply => (ACCENT, " Apply "),
            Operation::Pop => (CONFLICT, " Pop "),
            Operation::Drop => (RED, " Drop "),
        };
        let padding = u16::from(area.height >= 24);
        let [effect, explanation] = self.operation.summary();
        let mut summary = TextLayout::default();
        summary.push(
            Line::styled(effect, Style::new().fg(tone).bold()),
            text_width - 2,
            Wrap::Word,
        );
        summary.push(
            Line::styled(
                explanation,
                if self.operation == Operation::Drop {
                    Style::new().fg(RED).bold()
                } else {
                    Style::new().fg(FG)
                },
            ),
            text_width - 2,
            Wrap::Word,
        );
        let summary_height = summary.rows.len() as u16;
        let details = self.details(text_width, padding != 0);
        let height = (usize::from(4 + 2 * padding + summary_height) + details.rows.len())
            .min(usize::from(available.height)) as u16;
        let rect = Rect::new(
            available.x + (available.width - width) / 2,
            available.y + (available.height - height) / 2,
            width,
            height,
        );

        f.buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        f.render_widget(Clear, rect);
        let frame = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(LINE))
            .style(Style::new().fg(FG).bg(BG))
            .title(Span::styled(
                format!(" {} ", self.operation.title()),
                Style::new().fg(tone).bold(),
            ));
        let inner = frame.inner(rect);
        f.render_widget(frame, rect);
        let close = format!(" {} ", icons.glyph(Icon::Close));
        let close_width = close.width() as u16;
        let close_area = Rect::new(inner.right() - close_width, rect.y, close_width, 1);
        f.render_widget(
            Paragraph::new(close).style(Style::new().fg(MUTED)),
            close_area,
        );
        hits.push((close_area, usize::MAX));

        let banner = Rect::new(inner.x + 1, inner.y + padding, text_width, summary_height);
        f.render_widget(Block::default().style(Style::new().bg(TAG)), banner);
        f.render_widget(
            Paragraph::new(
                summary
                    .rows
                    .into_iter()
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
            ),
            banner.inner(Margin::new(1, 0)),
        );

        let footer = Rect::new(inner.x, inner.bottom() - 2, inner.width, 2);
        let y = banner.bottom() + padding;
        let body = Rect::new(banner.x, y, text_width, footer.y.saturating_sub(y));
        let visible = usize::from(body.height);
        let max_scroll = details.rows.len().saturating_sub(visible);
        *scroll = usize::from(*scroll).min(max_scroll) as u16;
        f.render_widget(
            Paragraph::new(
                details
                    .rows
                    .into_iter()
                    .skip(usize::from(*scroll))
                    .take(visible)
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
            ),
            body,
        );
        if max_scroll > 0 {
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_symbol(Some("│"))
                    .track_style(Style::new().fg(LINE))
                    .thumb_symbol("┃")
                    .thumb_style(Style::new().fg(MUTED)),
                Rect::new(body.right(), body.y, 1, body.height),
                &mut ScrollbarState::new(max_scroll + 1).position(usize::from(*scroll)),
            );
        }
        draw_footer(
            f,
            footer,
            action,
            tone,
            max_scroll > 0,
            confirm_focused,
            hits,
        );
    }

    fn details(&self, width: u16, spaced: bool) -> TextLayout {
        let mut rows = TextLayout::default();
        let muted = Style::new().fg(MUTED);
        let ref_style = Style::new().fg(ACCENT).bold();
        let mut put = |label: &str, value: &str, style| {
            field(
                &mut rows,
                width,
                Span::styled(label, muted),
                value,
                style,
                Wrap::Word,
            )
        };
        put("Repository", &self.repo_name, Style::new().bold());
        put("Stash", &self.source.selector, ref_style);
        if let Some(target) = &self.destination {
            put("Apply to", &target.branch_label(), ref_style);
            put("Staging", target.staging_label(), Style::new());
        }
        put("Message", &self.source.subject, Style::new());
        if spaced {
            rows.decoration(Line::default());
        }
        let note = if self.operation == Operation::Drop {
            Span::styled("Note", muted)
        } else {
            Span::styled("Caution", Style::new().fg(CONFLICT).bold())
        };
        field(
            &mut rows,
            width,
            note,
            self.operation.note(),
            Style::new(),
            Wrap::Word,
        );
        if spaced {
            rows.decoration(Line::default());
        }
        rows.decoration(Line::styled("Details", Style::new().fg(FG).bold()));
        for (label, value) in [
            ("Saved", self.source.saved.as_str()),
            ("Path", &self.path_label()),
            ("Stash object", &self.source.oid),
        ]
        .into_iter()
        .chain(
            self.destination
                .iter()
                .map(|target| ("Current HEAD", target.head.as_str())),
        ) {
            field(
                &mut rows,
                width,
                Span::styled(label, muted),
                value,
                muted,
                Wrap::Character,
            );
        }
        rows
    }
}

fn field(
    rows: &mut TextLayout,
    width: u16,
    label: Span<'_>,
    value: &str,
    style: Style,
    wrap: Wrap,
) {
    let range = rows.push(
        Line::from(Span::styled(value, style)),
        width - LABEL_WIDTH,
        wrap,
    );
    for (i, row) in rows.rows[range].iter_mut().enumerate() {
        let text = if i == 0 { label.content.as_ref() } else { "" };
        row.line.spans.insert(
            0,
            Span::styled(
                format!("{text:<width$}", width = usize::from(LABEL_WIDTH)),
                label.style,
            ),
        );
    }
}

fn draw_footer(
    f: &mut Frame,
    area: Rect,
    action: &str,
    tone: Color,
    overflow: bool,
    confirm_focused: bool,
    hits: &mut Vec<(Rect, usize)>,
) {
    let divider = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(LINE));
    let row = divider.inner(area).inner(Margin::new(1, 0));
    f.render_widget(divider, area);
    let mut right = row.right();
    let primary = Style::new().fg(BG).bg(tone).bold();
    for (label, id, style, focused) in [
        (action, 1, primary, confirm_focused),
        (" Cancel ", 0, button_style(false), !confirm_focused),
    ] {
        let width = label.width() as u16;
        let button = Rect::new(right - width, row.y, width, 1);
        f.render_widget(
            Paragraph::new(label).style(if focused { style.underlined() } else { style }),
            button,
        );
        hits.push((button, id));
        right = button.x - 1;
    }
    let hints = if overflow {
        ["j/k Scroll · Tab Switch · Esc Cancel", "j/k Scroll · Esc"]
    } else {
        ["Tab Switch · Esc Cancel", "Esc Cancel"]
    };
    let room = right - row.x;
    if let Some(hint) = hints
        .into_iter()
        .find(|hint| hint.width() <= usize::from(room))
    {
        f.render_widget(
            Paragraph::new(hint).style(Style::new().fg(MUTED)),
            Rect::new(row.x, row.y, room, 1),
        );
    }
}
