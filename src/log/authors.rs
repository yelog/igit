use super::*;
use crossterm::event::KeyEvent;
use ratatui::widgets::BorderType;

const SURFACE: Color = Color::Rgb(37, 39, 46);
const BORDER: Color = Color::Rgb(64, 68, 79);
const SEARCH: Color = Color::Rgb(30, 32, 39);

enum Row {
    Heading(&'static str, usize),
    Choice(usize),
}

fn line(area: Rect, offset: u16) -> Rect {
    Rect::new(area.x, area.y + offset, area.width, 1)
}

fn padded(area: Rect) -> Rect {
    Rect::new(
        area.x + u16::from(area.width > 0),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    )
}

impl LogPanel {
    pub(super) fn author_applied(&self, value: &str) -> bool {
        value == self.filter.author
            || (value == "me" && self.filter.author.eq_ignore_ascii_case("me"))
    }

    pub(super) fn author_navigation(&mut self, key: &KeyEvent) -> bool {
        let count = self.suggestions.len();
        if count == 0 {
            return false;
        }
        let page = self
            .hits
            .iter()
            .filter(|(_, hit)| matches!(hit, Hit::Dropdown(DropdownHit::Choice(_))))
            .count()
            .max(1);
        let current = self.choice.unwrap_or(0);
        self.choice = Some(match key.code {
            K::Home => 0,
            K::End => count - 1,
            K::PageUp => current.saturating_sub(page),
            K::PageDown => current.saturating_add(page).min(count - 1),
            K::Tab => (current + 1).min(count - 1),
            K::BackTab => current.saturating_sub(1),
            _ => return false,
        });
        true
    }

    pub(super) fn hover_author_at(&mut self, x: u16, y: u16) -> bool {
        if !matches!(self.editor, Some((Field::Author, _))) {
            return false;
        }
        if let Some(Hit::Dropdown(DropdownHit::Choice(index))) = self
            .hits
            .iter()
            .rev()
            .find(|(area, _)| inside(*area, x, y))
            .map(|(_, hit)| hit)
        {
            let changed = self.choice != Some(*index);
            self.choice = Some(*index);
            return changed;
        }
        false
    }

    fn author_rows(&self, query: &str, compact: bool) -> Vec<Row> {
        let mut rows = Vec::new();
        if query.is_empty() && !compact {
            rows.push(Row::Heading("Quick filters", 0));
        } else if !query.is_empty() {
            rows.push(Row::Heading("Matches", self.suggestions.len()));
        }
        for index in 0..self.suggestions.len() {
            if query.is_empty() && index == 2 {
                rows.push(Row::Heading(
                    "Authors",
                    self.suggestions.len().saturating_sub(2),
                ));
            }
            rows.push(Row::Choice(index));
        }
        rows
    }

    pub(super) fn draw_author_editor(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        anchor: Rect,
        query: &str,
    ) {
        let below = area.bottom().saturating_sub(anchor.bottom());
        let available_height = if below >= 6 { below } else { area.height };
        let compact = available_height < 12;
        let rows = self.author_rows(query, compact);
        let width = self
            .suggestions
            .iter()
            .map(|(_, label)| cells(label).saturating_add(8))
            .max()
            .unwrap_or(40)
            .clamp(40, 48)
            .min(area.width);
        let height = (rows.len().max(3) + if compact { 4 } else { 5 }).min(17) as u16;
        let height = height.min(available_height);
        let popup = Rect::new(
            anchor.x.clamp(area.x, area.right() - width),
            anchor.bottom().min(area.bottom() - height).max(area.y),
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let border = Block::default()
            .borders(Borders::ALL)
            .border_type(if self.icons == IconMode::Ascii {
                BorderType::Plain
            } else {
                BorderType::Rounded
            })
            .border_style(fg(BORDER))
            .style(fg(INK).bg(SURFACE))
            .title(Span::styled(" User ", fg(INK).bold()));
        let inner = border.inner(popup);
        frame.render_widget(border, popup);
        self.hit(popup, Hit::Dropdown(DropdownHit::Surface));
        if inner.is_empty() {
            return;
        }
        self.draw_author_search(frame, padded(line(inner, 0)), query);
        let footer = inner.height >= 4;
        let list = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(if !footer {
                1
            } else if compact {
                2
            } else {
                3
            }),
        );
        if !list.is_empty() {
            if self.suggestions.is_empty() {
                for (offset, message) in [
                    "No matching authors",
                    "Enter applies the typed author.",
                    "Ctrl-U clears search.",
                ]
                .into_iter()
                .enumerate()
                .take(usize::from(list.height))
                {
                    frame.render_widget(
                        Paragraph::new(clipped(message, list.width.saturating_sub(2).into()))
                            .style(fg(if offset == 0 { INK } else { DIM })),
                        padded(line(list, offset as u16)),
                    );
                }
            } else {
                let count = rows.len();
                let visible = usize::from(list.height);
                let cursor = self.choice.and_then(|choice| {
                    rows.iter()
                        .position(|row| matches!(row, Row::Choice(index) if *index == choice))
                });
                self.author_top = self.author_top.min(count.saturating_sub(visible));
                if let Some(cursor) = cursor {
                    self.author_top = self
                        .author_top
                        .clamp(cursor.saturating_sub(visible - 1), cursor);
                }
                let scroll = count > visible;
                for (offset, row) in rows.iter().skip(self.author_top).take(visible).enumerate() {
                    let mut row_area = line(list, offset as u16);
                    row_area.width = row_area.width.saturating_sub(u16::from(scroll));
                    match row {
                        Row::Heading(label, count) => {
                            let text = if *label == "Quick filters" {
                                (*label).to_owned()
                            } else {
                                format!("{label} · {count}")
                            };
                            let body = padded(row_area);
                            let title = clipped(&text, body.width.into());
                            let mut spans = vec![Span::styled(title.clone(), fg(DIM))];
                            let remaining = body.width.saturating_sub(cells(&title) + 1);
                            if *label == "Authors" && remaining > 0 {
                                spans.push(Span::styled(
                                    format!(
                                        " {}",
                                        if self.icons == IconMode::Ascii {
                                            "-"
                                        } else {
                                            "─"
                                        }
                                        .repeat(remaining.into())
                                    ),
                                    fg(BORDER),
                                ));
                            }
                            frame.render_widget(Paragraph::new(Line::from(spans)), body);
                        }
                        Row::Choice(index) => self.draw_author_choice(frame, row_area, *index),
                    }
                }
                if scroll {
                    self.draw_author_scrollbar(frame, list, &rows);
                }
            }
        }
        if footer {
            if compact {
                frame.render_widget(
                    Block::default().style(fg(INK).bg(SEARCH)),
                    line(inner, inner.height - 1),
                );
            } else {
                let separator = line(inner, inner.height - 2);
                frame.render_widget(
                    Paragraph::new(
                        if self.icons == IconMode::Ascii {
                            "-"
                        } else {
                            "─"
                        }
                        .repeat(separator.width.into()),
                    )
                    .style(fg(BORDER)),
                    separator,
                );
            }
            self.draw_author_footer(frame, padded(line(inner, inner.height - 1)));
        }
    }

    fn draw_author_search(&mut self, frame: &mut Frame, area: Rect, query: &str) {
        frame.render_widget(Block::default().style(fg(INK).bg(SEARCH)), area);
        self.hit(area, Hit::Dropdown(DropdownHit::AuthorInput));
        let mut body = padded(area);
        let icon = take(&mut body, cells(self.icons.glyph(Icon::Search)) + 1);
        frame.render_widget(
            Paragraph::new(self.icons.glyph(Icon::Search)).style(fg(BLUE)),
            icon,
        );
        if !query.is_empty() && body.width > 4 {
            let width = cells(self.icons.glyph(Icon::Close)) + 1;
            body.width = body.width.saturating_sub(width);
            let clear = Rect::new(body.right(), body.y, width, 1);
            frame.render_widget(
                Paragraph::new(self.icons.glyph(Icon::Close)).style(fg(DIM)),
                clear,
            );
            self.hit(clear, Hit::Dropdown(DropdownHit::AuthorClear));
        }
        let cursor = self.icons.glyph(Icon::Cursor);
        let text = if query.is_empty() {
            Line::from(vec![
                Span::styled(cursor, fg(BLUE)),
                Span::styled("Search authors...", fg(DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled(query, fg(INK)),
                Span::styled(cursor, fg(BLUE)),
            ])
        };
        let shift = text.width().saturating_sub(usize::from(body.width));
        frame.render_widget(
            Paragraph::new(text).scroll((0, shift.min(usize::from(u16::MAX)) as u16)),
            body,
        );
    }

    fn draw_author_choice(&mut self, frame: &mut Frame, area: Rect, index: usize) {
        let (value, label) = &self.suggestions[index];
        let focused = self.choice == Some(index);
        let applied = self.author_applied(value);
        let background = if focused { SELECT } else { SURFACE };
        frame.render_widget(Block::default().style(fg(INK).bg(background)), area);
        let mut body = padded(area);
        let mark_width = cells(self.icons.glyph(Icon::Success)).saturating_add(1);
        let mark_width = mark_width.min(body.width);
        body.width -= mark_width;
        let mark = Rect::new(body.right(), body.y, mark_width, 1);
        if applied {
            frame.render_widget(
                Paragraph::new(format!(" {}", self.icons.glyph(Icon::Success)))
                    .style(fg(BLUE).bold()),
                mark,
            );
        }
        let (name, detail) = match value.as_str() {
            "" => ("All users", "Any author"),
            "me" => ("me", "Git identity"),
            _ => (label.as_str(), ""),
        };
        let detail_width = cells(detail);
        if !detail.is_empty() && body.width >= cells(name) + detail_width + 3 {
            body.width -= detail_width;
            frame.render_widget(
                Paragraph::new(detail).style(fg(if focused { INK } else { DIM })),
                Rect::new(body.right(), body.y, detail_width, 1),
            );
            body.width = body.width.saturating_sub(2);
        }
        frame.render_widget(
            Paragraph::new(clipped(name, body.width.into())).style(if focused || applied {
                fg(INK).bold()
            } else {
                fg(INK)
            }),
            body,
        );
        self.hit(area, Hit::Dropdown(DropdownHit::Choice(index)));
    }

    fn draw_author_scrollbar(&mut self, frame: &mut Frame, area: Rect, rows: &[Row]) {
        let height = usize::from(area.height);
        let count = rows.len();
        let thumb = (height * height / count).max(1);
        let top = self.author_top * (height - thumb) / (count - height);
        for offset in 0..height {
            let track = Rect::new(area.right() - 1, area.y + offset as u16, 1, 1);
            let active = (top..top + thumb).contains(&offset);
            frame.render_widget(
                Paragraph::new(if self.icons == IconMode::Ascii {
                    "|"
                } else if active {
                    "┃"
                } else {
                    "│"
                })
                .style(fg(if active { DIM } else { BORDER })),
                track,
            );
            let position = offset * (count - 1) / height.saturating_sub(1).max(1);
            let choice = rows[position..].iter().find_map(|row| match row {
                Row::Choice(index) => Some(*index),
                Row::Heading(_, _) => None,
            });
            if let Some(choice) = choice {
                self.hit(track, Hit::Dropdown(DropdownHit::AuthorScroll(choice)));
            }
        }
    }

    fn draw_author_footer(&mut self, frame: &mut Frame, mut area: Rect) {
        let close_width = 9.min(area.width);
        area.width -= close_width;
        let close = Rect::new(area.right(), area.y, close_width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Esc", fg(INK)),
                Span::styled(" Close", fg(DIM)),
            ])),
            close,
        );
        self.hit(close, Hit::Dropdown(DropdownHit::Cancel));
        let label = if self.choice.is_some() {
            "Select"
        } else {
            "Apply text"
        };
        let enter = self.icons.glyph(Icon::Enter);
        let action_width = cells(enter) + cells(label) + 2;
        let navigation = format!(
            "{}{} Move  ",
            self.icons.glyph(Icon::Up),
            self.icons.glyph(Icon::Down)
        );
        if !self.suggestions.is_empty() && area.width > cells(&navigation) + action_width {
            let hint = take(&mut area, cells(&navigation));
            frame.render_widget(Paragraph::new(navigation).style(fg(DIM)), hint);
        }
        let apply = take(&mut area, action_width);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{enter} "), fg(BLUE)),
                Span::styled(label, fg(INK)),
            ])),
            apply,
        );
        self.hit(apply, Hit::Dropdown(DropdownHit::Apply));
    }
}
