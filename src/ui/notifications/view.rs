use super::{clock, timestamp, Center, History, LiveHit, Notification};
use crate::ui::{
    icons::{Icon, IconMode},
    idea::{self, BLUE, DIM, EDGE, GOLD, INK, PANEL, SELECT, TAG},
    selection::{self, TextLayout, Wrap},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

impl Center {
    pub(crate) fn draw_live(&mut self, frame: &mut Frame<'_>, area: Rect, icons: IconMode) {
        self.clear_hits();
        let area = area.intersection(frame.area());
        if area.width < 24 || area.height < 5 {
            return;
        }
        let entries: Vec<_> = self
            .live()
            .iter()
            .filter_map(|live| self.get(live.id))
            .collect();
        let wanted = entries
            .iter()
            .map(|entry| {
                entry.source.width().max(
                    entry
                        .body
                        .lines()
                        .map(UnicodeWidthStr::width)
                        .max()
                        .unwrap_or(0),
                )
            })
            .max()
            .unwrap_or(0)
            .saturating_add(8);
        let width = wanted.clamp(24, 56).min(usize::from(area.width)) as u16;
        let mut top = area.y + 1;
        let mut hits = Vec::new();
        for entry in entries {
            let remaining = area.bottom().saturating_sub(top);
            if remaining < 4 {
                break;
            }
            let rows = wrapped(&entry.body, width - 2, "");
            let height = (rows.len().saturating_add(4)).clamp(4, 8) as u16;
            let card = Rect::new(area.right() - width, top, width, height.min(remaining));
            let close = draw_card(frame, card, entry, rows, icons);
            hits.push(LiveHit {
                id: entry.id,
                area: card,
                close,
            });
            top = card.bottom().saturating_add(1);
        }
        self.hits = hits;
    }

    pub(crate) fn draw_history(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        icons: IconMode,
        state: &mut History,
    ) {
        self.clear_hits();
        state.clear_hits();
        let area = area.intersection(frame.area());
        let narrow = area.width < 72;
        let popup = history_area(area, narrow);
        let block = Block::default()
            .title(if state.detail.is_some() {
                " NOTIFICATION DETAIL "
            } else {
                " NOTIFICATION HISTORY "
            })
            .borders(Borders::ALL)
            .border_style(Style::new().fg(EDGE))
            .style(Style::new().fg(INK).bg(TAG));
        let inner = block.inner(popup);
        frame.render_widget(Clear, popup);
        frame.render_widget(block, popup);
        if popup.width >= 5 && popup.height > 0 {
            let glyph = icons.glyph(if state.detail.is_some() {
                Icon::Back
            } else {
                Icon::Close
            });
            let width = (glyph.width() as u16 + 2).min(popup.width - 2);
            state.close_area = Rect::new(popup.right() - width - 1, popup.y, width, 1);
            frame.render_widget(
                Paragraph::new(format!(" {glyph} ")).style(Style::new().fg(DIM).bg(TAG)),
                state.close_area,
            );
        }
        if inner.is_empty() {
            state.page_size = 0;
            return;
        }
        state.reconcile(self);
        if let Some(detail) = &mut state.detail {
            let content = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(1),
            );
            let rows = detail_lines(&detail.entry, content.width, icons, &state.query);
            detail.page_size = usize::from(content.height);
            detail.max_scroll = rows.len().saturating_sub(detail.page_size);
            detail.scroll = detail.scroll.min(detail.max_scroll);
            frame.render_widget(
                Paragraph::new(
                    rows.into_iter()
                        .skip(detail.scroll)
                        .take(detail.page_size)
                        .collect::<Vec<_>>(),
                ),
                content,
            );
            footer(
                frame,
                inner,
                &[
                    "j/k scroll  y copy  Esc/q back",
                    "y copy  Esc/q back",
                    "Esc back",
                ],
                DIM,
            );
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(inner);
        let body = if narrow {
            vec![chunks[1]]
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
                .split(chunks[1])
                .to_vec()
        };
        state.list_area = body[0];
        state.page_size = usize::from(body[0].height);
        state.reconcile(self);
        draw_search(frame, chunks[0], state, self);
        if self.history.is_empty() {
            frame.render_widget(
                Paragraph::new("No notifications").style(Style::new().fg(DIM)),
                body[0],
            );
        }
        for (index, entry) in self
            .history()
            .enumerate()
            .skip(state.offset)
            .take(state.page_size)
        {
            let row = Rect::new(
                body[0].x,
                body[0].y + (index - state.offset) as u16,
                body[0].width,
                1,
            );
            draw_history_row(frame, row, entry, state, icons);
            state.rows.push((row, entry.id));
        }
        if !narrow {
            if let Some(entry) = state.selected_id.and_then(|id| self.get(id)) {
                let preview = body[1];
                let block = Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::new().fg(EDGE));
                let text = block.inner(preview);
                frame.render_widget(block, preview);
                frame.render_widget(
                    Paragraph::new(detail_lines(entry, text.width, icons, state.query())),
                    text,
                );
            }
        }
        if state.clear_confirm {
            footer(
                frame,
                inner,
                &[
                    "Clear all notifications? y confirm  n/Esc cancel",
                    "Clear all? y / n / Esc",
                    "Clear? y/n",
                ],
                GOLD,
            );
        } else if state.input.is_some() {
            footer(
                frame,
                inner,
                &[
                    "Enter find  Esc cancel  Ctrl-U clear  Ctrl-Z undo  Ctrl-Y redo",
                    "Enter find  Esc cancel",
                    "Esc cancel",
                ],
                DIM,
            );
        } else {
            footer(
                frame,
                inner,
                &[
                    "j/k select  / search  n/N match  Enter detail  y copy  c clear  Esc/q close",
                    "/ search  Enter detail  y copy  c clear  Esc/q close",
                    "Enter detail  Esc/q close",
                    "Esc/q close",
                ],
                DIM,
            );
        }
    }
}

fn draw_card(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: &Notification,
    rows: Vec<Line<'static>>,
    icons: IconMode,
) -> Rect {
    frame.render_widget(Clear, area);
    let color = entry.level.color();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(color))
        .style(Style::new().fg(INK).bg(TAG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let close_width = (icons.glyph(Icon::Close).width() as u16 + 2).min(inner.width);
    let close = Rect::new(inner.right() - close_width, inner.y, close_width, 1);
    let header = Rect::new(inner.x, inner.y, inner.width - close_width, 1);
    let prefix = format!(
        "{} {} ",
        entry.level.icon(icons),
        entry.level.to_string().to_uppercase()
    );
    let title = idea::clipped(
        &entry.source,
        usize::from(header.width).saturating_sub(prefix.width()),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix, Style::new().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(title, Style::new().fg(INK).add_modifier(Modifier::BOLD)),
        ])),
        header,
    );
    frame.render_widget(
        Paragraph::new(format!(" {} ", icons.glyph(Icon::Close))).style(Style::new().fg(DIM)),
        close,
    );
    let body = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let truncated = rows.len() > usize::from(body.height);
    let mut visible: Vec<_> = rows.into_iter().take(usize::from(body.height)).collect();
    if truncated {
        if let Some(last) = visible.last_mut() {
            let text: String = last
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            let end = selection::at_column(&text, usize::from(body.width.saturating_sub(1)));
            *last = Line::raw(format!("{}…", &text[..end]));
        }
    }
    frame.render_widget(Paragraph::new(visible), body);
    frame.render_widget(
        Paragraph::new(clock(entry.created_at, entry.utc_offset)).style(Style::new().fg(DIM)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
    close
}

fn history_area(area: Rect, narrow: bool) -> Rect {
    let width = area
        .width
        .saturating_sub(if narrow { 2 } else { 4 })
        .min(100);
    let height = area.height.saturating_sub(2).min(18);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn footer(frame: &mut Frame<'_>, inner: Rect, candidates: &[&str], color: ratatui::style::Color) {
    let text = candidates
        .iter()
        .find(|text| text.width() <= usize::from(inner.width))
        .or_else(|| candidates.last())
        .copied()
        .unwrap_or("");
    frame.render_widget(
        Paragraph::new(text).style(Style::new().fg(color).bg(PANEL)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}

fn draw_search(frame: &mut Frame<'_>, area: Rect, state: &History, center: &Center) {
    if area.is_empty() {
        return;
    }
    let text = state.query();
    if let Some(input) = &state.input {
        let available = usize::from(area.width.saturating_sub(1));
        let caret_column = text[..input.caret()].width();
        let mut start = 0;
        while start < input.caret() && text[start..input.caret()].width() >= available.max(1) {
            start = selection::next(text, start);
        }
        let shown = &text[start..];
        let end = selection::at_column(shown, available);
        frame.render_widget(
            Paragraph::new(format!("/{}", &shown[..end])).style(Style::new().fg(BLUE)),
            area,
        );
        let column = (1 + caret_column.saturating_sub(text[..start].width()))
            .min(usize::from(area.width - 1)) as u16;
        frame.set_cursor_position((area.x + column, area.y));
    } else {
        let matches = center
            .history
            .iter()
            .filter(|entry| entry.matches(text))
            .count();
        let prompt = if text.is_empty() {
            format!("{} notifications", center.history.len())
        } else {
            format!("/{text}  {matches} matches")
        };
        frame.render_widget(
            Paragraph::new(idea::clipped(&prompt, usize::from(area.width)))
                .style(Style::new().fg(BLUE)),
            area,
        );
    }
}

fn draw_history_row(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: &Notification,
    state: &History,
    icons: IconMode,
) {
    let selected = state.selected_id == Some(entry.id);
    let background = if selected { SELECT } else { TAG };
    let base = Style::new().fg(INK).bg(background);
    let matching = !state.query().is_empty() && entry.matches(state.query());
    let marker = if selected {
        ">"
    } else if matching {
        "*"
    } else {
        " "
    };
    let level = format!("{:<7}", entry.level.to_string().to_uppercase());
    let prefix = format!("{} {level} ", entry.level.icon(icons));
    let time = clock(entry.created_at, entry.utc_offset);
    let show_time = usize::from(area.width) >= prefix.width() + time.width() + 11;
    let time = if show_time {
        format!(" {time} ")
    } else {
        String::new()
    };
    let title_width = usize::from(area.width).saturating_sub(1 + time.width() + prefix.width());
    let mut spans = vec![Span::styled(
        format!("{marker}{time}"),
        base.fg(if matching { GOLD } else { DIM }),
    )];
    spans.extend(highlight(
        &prefix,
        state.query(),
        base.fg(entry.level.color()).add_modifier(Modifier::BOLD),
    ));
    spans.extend(highlight(
        &idea::clipped(&entry.source, title_width),
        state.query(),
        if selected {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        },
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base), area);
}

fn detail_lines(
    entry: &Notification,
    width: u16,
    icons: IconMode,
    query: &str,
) -> Vec<Line<'static>> {
    let mut layout = TextLayout::default();
    let mut header = vec![Span::styled(
        format!(
            "{} {}  ",
            entry.level.icon(icons),
            entry.level.to_string().to_uppercase()
        ),
        Style::new()
            .fg(entry.level.color())
            .add_modifier(Modifier::BOLD),
    )];
    header.extend(highlight(
        &entry.source,
        query,
        Style::new().fg(INK).add_modifier(Modifier::BOLD),
    ));
    layout.push(Line::from(header), width, Wrap::Word);
    layout.push(
        Line::styled(
            timestamp(entry.created_at, entry.utc_offset),
            Style::new().fg(DIM),
        ),
        width,
        Wrap::Word,
    );
    layout.push(
        Line::from(highlight(
            &format!("source: {}", entry.source),
            query,
            Style::new().fg(DIM),
        )),
        width,
        Wrap::Word,
    );
    layout.push(Line::raw(""), width, Wrap::Word);
    for line in entry.body.split('\n') {
        layout.push(
            Line::from(highlight(line, query, Style::new().fg(INK))),
            width,
            Wrap::Word,
        );
    }
    layout.rows.into_iter().map(|row| row.line).collect()
}

fn wrapped(text: &str, width: u16, query: &str) -> Vec<Line<'static>> {
    let mut layout = TextLayout::default();
    for line in text.split('\n') {
        layout.push(
            Line::from(highlight(line, query, Style::new().fg(INK))),
            width,
            Wrap::Word,
        );
    }
    layout.rows.into_iter().map(|row| row.line).collect()
}

fn highlight(text: &str, query: &str, base: Style) -> Vec<Span<'static>> {
    if query.is_empty() {
        return vec![Span::styled(text.to_owned(), base)];
    }
    let mut folded = String::new();
    let mut offsets = Vec::new();
    for (start, character) in text.char_indices() {
        for scalar in character.to_lowercase() {
            let first = folded.len();
            folded.push(scalar);
            offsets.extend((first..folded.len()).map(|_| start..start + character.len_utf8()));
        }
    }
    let folded_query = query.to_lowercase();
    let matches: Vec<Range<usize>> = folded
        .match_indices(&folded_query)
        .map(|(start, value)| offsets[start].start..offsets[start + value.len() - 1].end)
        .collect();
    let raw = Span::raw(text);
    let mut start = 0;
    let mut spans: Vec<Span<'static>> = Vec::new();
    for glyph in raw.styled_graphemes(base) {
        let end = start + glyph.symbol.len();
        let matching = matches
            .iter()
            .any(|range| range.start < end && range.end > start);
        let style = if matching {
            base.fg(PANEL).bg(GOLD).add_modifier(Modifier::BOLD)
        } else {
            base
        };
        if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
            last.content.to_mut().push_str(glyph.symbol);
        } else {
            spans.push(Span::styled(glyph.symbol.to_owned(), style));
        }
        start = end;
    }
    spans
}
