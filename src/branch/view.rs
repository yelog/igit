use super::*;
use crate::ui::{
    icons::Icon,
    idea::{button_style, clipped, repo_tint, CONFLICT, GREEN, TAG},
    panes::{pane_frame, PaneId},
};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
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
        let [bar, list, details] = Layout::vertical([
            Constraint::Length(bar_height),
            Constraint::Fill(1),
            Constraint::Length(2),
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
            f.render_widget(Paragraph::new(label).style(button_style(false)), rect);
            self.hits.push((rect, Hit::Operation(op)));
            x += width + 1;
        }
        self.viewport = list;
        let visible = usize::from(list.height);
        self.top = self.top.min(self.tree.rows.len().saturating_sub(visible));
        if self.tree.cursor < self.top {
            self.top = self.tree.cursor;
        }
        if self.tree.cursor >= self.top + visible {
            self.top = (self.tree.cursor + 1).saturating_sub(visible);
        }
        let overflow = self.tree.rows.len() > visible;
        for (i, row) in self
            .tree
            .rows
            .iter()
            .enumerate()
            .skip(self.top)
            .take(visible)
        {
            let rect = Rect::new(
                list.x,
                list.y + (i - self.top) as u16,
                list.width.saturating_sub(u16::from(overflow)),
                1,
            );
            let selected = i == self.tree.cursor;
            let bg = if selected { SEL } else { BG };
            let indent = (row.depth * 2).min(usize::from(rect.width.saturating_sub(18)));
            let mut spans = vec![Span::raw(" ".repeat(indent))];
            match &row.kind {
                Kind::Folder { open } => {
                    spans.push(Span::styled(
                        format!("{} ", self.icons.disclosure(*open)),
                        Style::new().fg(MUTED),
                    ));
                    spans.push(Span::styled(
                        row.label.clone(),
                        Style::new()
                            .fg(if row.depth == 0 {
                                repo_tint(row.root)
                            } else {
                                FG
                            })
                            .bold(),
                    ));
                    if row.depth == 0 {
                        if let Some(repo) = self.repos.iter().find(|r| r.root == row.root) {
                            if let Ok(state) = &repo.state {
                                spans.push(Span::styled(
                                    format!("  [{}]", state.head_label()),
                                    Style::new().fg(MUTED),
                                ));
                            }
                        }
                    }
                }
                Kind::Branch(branch) => {
                    let icon = if branch.current {
                        Icon::Current
                    } else if branch.remote {
                        Icon::Remote
                    } else {
                        Icon::Branch
                    };
                    spans.push(Span::styled(
                        format!("  {} ", self.icons.glyph(icon)),
                        Style::new().fg(if branch.current { GREEN } else { MUTED }),
                    ));
                    spans.push(Span::styled(
                        row.label.clone(),
                        if branch.current {
                            Style::new().fg(GREEN).bold()
                        } else {
                            Style::new().fg(FG)
                        },
                    ));
                    if branch.current {
                        spans.push(Span::styled(" (current)", Style::new().fg(GREEN)));
                    }
                    if !branch.upstream.is_empty() {
                        spans.push(Span::styled(
                            format!(
                                " -> {}{}",
                                model::short_ref(&branch.upstream),
                                if branch.gone { " [gone]" } else { "" }
                            ),
                            Style::new().fg(MUTED),
                        ));
                    }
                    if branch.oid.is_empty() {
                        spans.push(Span::styled(" [no commits]", Style::new().fg(MUTED)));
                    }
                    if !branch.current && !branch.worktree.is_empty() {
                        spans.push(Span::styled(" [worktree]", Style::new().fg(CONFLICT)));
                    }
                }
                Kind::Message => {
                    spans.push(Span::styled(row.label.clone(), Style::new().fg(MUTED)))
                }
            }
            f.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
                rect,
            );
            self.hits.push((rect, Hit::Row(i)));
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
            let label = match &row.kind {
                Kind::Branch(b) => format!(
                    "{}{}",
                    b.name,
                    if b.upstream.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " -> {}{}",
                            model::short_ref(&b.upstream),
                            if b.gone { " [gone]" } else { "" }
                        )
                    }
                ),
                _ => row.label.clone(),
            };
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(label),
                    Line::from(
                        repo.map(|r| r.path.display().to_string())
                            .unwrap_or_default(),
                    ),
                ])
                .style(Style::new().fg(MUTED).bg(TAG)),
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
