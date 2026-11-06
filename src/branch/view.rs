use super::*;
use crate::ui::{
    icons::Icon,
    idea::{button_style, clipped, clipped_path, CONFLICT, TAG},
    panes::{pane_frame, PaneId},
};
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr;

impl BranchPanel {
    pub(super) fn draw_content(&mut self, f: &mut Frame, area: Rect) {
        self.hits.clear();
        self.panes.begin();
        self.panes.add(PaneId::Branches, area);
        let inner = pane_frame(f, area, self.panes.active);
        let buttons = [
            ("a", "New", Operation::Create),
            ("d", "Delete", Operation::Delete),
            ("Enter", "Switch", Operation::Checkout),
            ("m", "Merge", Operation::Merge),
            ("r", "Rename", Operation::Rename),
        ]
        .map(|(key, label, op)| {
            (
                format!(
                    " {} {label} ",
                    self.shortcut_labels.get(key).map_or(key, String::as_str)
                ),
                op,
            )
        });
        let total: usize = buttons.iter().map(|(s, _)| s.width() + 1).sum();
        let bar_height = if total > inner.width as usize { 2 } else { 1 };
        let narrow = inner.width < 64;
        let stacked_details = narrow && inner.height >= 12;
        let show_status = !self.tree.rows.is_empty() && self.read_status().is_some();
        let [bar, status_line, heading, list, details] = Layout::vertical([
            Constraint::Length(bar_height),
            Constraint::Length(u16::from(show_status)),
            Constraint::Length(u16::from(inner.height >= 10)),
            Constraint::Fill(1),
            Constraint::Length(if stacked_details { 3 } else { 2 }),
        ])
        .areas(inner);
        let (mut x, mut y) = (bar.x, bar.y);
        for (label, op) in buttons {
            let width = (label.width() as u16).min(bar.width);
            if x + width > bar.right() {
                x = bar.x;
                y += 1;
            }
            if y >= bar.bottom() {
                break;
            }
            let rect = Rect::new(x, y, width, 1);
            let enabled = self.action_enabled(op);
            let style = if enabled {
                button_style(false)
            } else {
                Style::new().fg(MUTED)
            };
            f.render_widget(Paragraph::new(label).style(style), rect);
            if enabled {
                self.hits.push((rect, Hit::Operation(op)));
            }
            x += width + 1;
        }
        self.viewport = list;
        let visible = usize::from(list.height);
        self.keep_cursor_visible();
        let overflow = self.tree.rows.len() > visible;
        let row_width = list.width.saturating_sub(u16::from(overflow));
        let upstream = self.upstream_column(row_width);
        f.render_widget(
            Paragraph::new(" Branch").style(Style::new().fg(MUTED)),
            heading,
        );
        if let Some(x) = upstream {
            f.render_widget(
                Paragraph::new("Upstream").style(Style::new().fg(MUTED)),
                Rect::new(
                    heading.x + x,
                    heading.y,
                    heading.width.saturating_sub(x),
                    heading.height,
                ),
            );
        }
        for i in self.top..self.tree.rows.len().min(self.top + visible) {
            let rect = Rect::new(list.x, list.y + (i - self.top) as u16, row_width, 1);
            self.draw_row(f, rect, i, upstream);
            self.hits.push((rect, Hit::Row(i)));
        }
        if self.tree.rows.is_empty() {
            f.render_widget(
                Paragraph::new(self.read_status().unwrap_or("No repositories in Roots"))
                    .style(Style::new().fg(MUTED)),
                list,
            );
        } else if let Some(status) = self.read_status() {
            f.render_widget(
                Paragraph::new(status).style(Style::new().fg(MUTED)),
                status_line,
            );
        }
        if overflow && !list.is_empty() {
            let mut state = ScrollbarState::new(self.tree.rows.len().saturating_sub(visible) + 1)
                .position(self.top);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_symbol(None),
                list,
                &mut state,
            );
        }
        if let Some(row) = self.tree.rows.get(self.tree.cursor) {
            let repo = self.repos.iter().find(|r| r.root == row.root);
            let (label, upstream_label) = match &row.kind {
                Kind::Branch(b) => (
                    format!("{}{}", b.name, if b.current { " (current)" } else { "" }),
                    if b.upstream.is_empty() {
                        if b.remote {
                            "Remote branch".into()
                        } else {
                            "No upstream".into()
                        }
                    } else {
                        format!(
                            "{}{}",
                            model::short_ref(&b.upstream),
                            if b.gone { " [gone]" } else { "" }
                        )
                    },
                ),
                Kind::Folder { count, .. } => (row.label.clone(), format!("{count} branches")),
                Kind::Message => (row.label.clone(), String::new()),
            };
            let width = usize::from(details.width.saturating_sub(2));
            let path = clipped_path(
                repo.map(|r| r.path.display().to_string())
                    .unwrap_or_default(),
                width,
            );
            let branch = match &row.kind {
                Kind::Branch(b) => Some(b),
                _ => None,
            };
            let tracked = branch.is_some_and(|b| !b.upstream.is_empty());
            let mut lines = if narrow && branch.is_some() {
                let context = if tracked {
                    format!("Upstream: {upstream_label}")
                } else {
                    upstream_label.clone()
                };
                vec![
                    Line::from(Span::styled(
                        format!(" {}", clipped(&label, width)),
                        Style::new().fg(FG),
                    )),
                    Line::from(format!(
                        " {}",
                        if tracked || stacked_details {
                            clipped(&context, width)
                        } else {
                            path.clone()
                        }
                    )),
                ]
            } else {
                let separator = if tracked { " -> " } else { "  " };
                vec![Line::from(vec![
                    Span::styled(format!(" {}", clipped(&label, width)), Style::new().fg(FG)),
                    Span::raw(format!("{separator}{upstream_label}")),
                ])]
            };
            if stacked_details && branch.is_none() {
                lines.push(Line::default());
            }
            if !narrow || stacked_details || branch.is_none() {
                lines.push(Line::from(format!(" {path}")));
            }
            f.render_widget(
                Paragraph::new(lines).style(Style::new().fg(MUTED).bg(TAG)),
                details,
            );
        }
        if self.input.is_some() {
            self.draw_input(f, area);
        }
    }

    fn draw_input(&mut self, f: &mut Frame, area: Rect) {
        self.hits.clear();
        let input = self.input.as_ref().unwrap();
        let width = 72.min(area.width);
        let height = 10.min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, rect);
        let block = pane(input.operation.title(), true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let source = Rect::new(inner.x, inner.y, inner.width, 1);
        f.render_widget(
            Paragraph::new(format!("From: {}", input.source.name)).style(Style::new().fg(MUTED)),
            source,
        );
        let field = Rect::new(inner.x, inner.y + 2, inner.width, 1);
        // Keep the insertion end visible without slicing through UTF-8 characters.
        let mut tail = input.name.as_str();
        while tail.width() + 2 > field.width as usize {
            let Some(ch) = tail.chars().next() else { break };
            tail = &tail[ch.len_utf8()..];
        }
        f.render_widget(
            Paragraph::new(format!("{tail}{}", self.icons.glyph(Icon::Cursor)))
                .style(Style::new().bg(SEL).fg(FG)),
            field,
        );
        if inner.height > 5 {
            f.render_widget(
                Paragraph::new(clipped(&input.error, inner.width as usize * 2))
                    .style(Style::new().fg(CONFLICT))
                    .wrap(Wrap { trim: false }),
                Rect::new(inner.x, inner.y + 4, inner.width, inner.height - 5),
            );
        }
        let mut x = inner.x;
        for (label, hit) in [
            (" Enter Review ", Hit::Submit),
            (" Esc Cancel ", Hit::Cancel),
        ] {
            let rect = Rect::new(x, inner.bottom().saturating_sub(1), label.width() as u16, 1);
            if rect.right() <= inner.right() {
                f.render_widget(Paragraph::new(label).style(button_style(false)), rect);
                self.hits.push((rect, hit));
                x = rect.right() + 1;
            }
        }
    }
}
