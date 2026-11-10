use super::*;
use crate::ui::{
    icons::Icon,
    idea::{button_style, clipped, clipped_path, CONFLICT, TAG},
    panes::{pane_frame, Axis, SplitId},
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
        self.repo_viewport.area = Rect::default();
        self.viewport.area = Rect::default();
        let focus = self.focused_pane();
        let [repos, branches] = if !self.has_repository_pane() {
            [Rect::default(), area]
        } else if self.panes.maximized {
            if focus == PaneId::BranchRepos {
                [area, Rect::default()]
            } else {
                [Rect::default(), area]
            }
        } else {
            self.panes.divide(
                SplitId::BranchRepos,
                area,
                Axis::Width,
                (area.width / 4).clamp(15, 36),
                [15, 26],
            )
        };
        if self.has_repository_pane() {
            self.panes.add(PaneId::BranchRepos, repos);
        }
        self.panes.add(PaneId::Branches, branches);
        if !repos.is_empty() {
            let inner = pane_frame(f, repos, self.panes.active && focus == PaneId::BranchRepos);
            self.draw_repositories(f, inner);
        }
        if !branches.is_empty() {
            let inner = pane_frame(f, branches, self.panes.active && focus == PaneId::Branches);
            self.draw_branches(f, inner);
            self.draw_view_menu(f, inner);
        }
        if self.input.is_some() {
            self.draw_input(f, area);
        }
    }

    fn draw_repositories(&mut self, f: &mut Frame, inner: Rect) {
        let [heading, list] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
        f.render_widget(
            Paragraph::new(" Repositories").style(Style::new().fg(MUTED)),
            heading,
        );
        self.repo_viewport.area = list;
        let len = self.repos.len();
        if let Some(cursor) = self.repository_index() {
            self.repo_viewport.reveal(cursor, len);
        }
        for index in self.repo_viewport.visible(len) {
            let rect = row_rect(&self.repo_viewport, len, index);
            self.draw_repository_row(f, rect, index);
            self.hits.push((rect, Hit::Repository(index)));
        }
        draw_scrollbar(f, &self.repo_viewport, len);
    }

    fn draw_branches(&mut self, f: &mut Frame, inner: Rect) {
        let compact = inner.width < 44;
        let eye_label = format!(" {} ", self.icons.glyph(Icon::Preview));
        let eye_width = (eye_label.width() as u16).min(inner.width);
        let buttons = [
            ("a", "New", "New", Operation::Create),
            ("d", "Delete", "Del", Operation::Delete),
            ("Enter", "Switch", "Go", Operation::Checkout),
            ("m", "Merge", "Merge", Operation::Merge),
            ("r", "Rename", "Rename", Operation::Rename),
            ("U", "Pull", "Pull", Operation::Pull),
            ("P", "Push", "Push", Operation::Push),
        ]
        .map(|(key, full, short, op)| {
            let key = self.shortcut_labels.get(key).map_or(key, String::as_str);
            let label = if compact {
                format!("{key} {short}")
            } else {
                format!(" {key} {full} ")
            };
            (label, op)
        });
        let total: usize =
            buttons.iter().map(|(s, _)| s.width()).sum::<usize>() + buttons.len() - 1;
        let lead = u16::from(compact);
        let bar_height = if total + usize::from(eye_width + lead) > usize::from(inner.width) {
            2
        } else {
            1
        };
        let narrow = inner.width < 64;
        let detached = self
            .repository()
            .and_then(|repo| repo.state.as_ref().ok())
            .filter(|state| state.head.is_none())
            .map(Snapshot::head_label);
        let show_path = self.has_repository_pane();
        let len = self.tree.rows.len();
        let show_status = len > 0 && self.read_status().is_some();
        let detail_height = if len > 0 {
            1 + u16::from(narrow) + u16::from(show_path || detached.is_some())
        } else {
            0
        };
        let [bar, status_line, heading, list, details] = Layout::vertical([
            Constraint::Length(bar_height),
            Constraint::Length(u16::from(show_status)),
            Constraint::Length(u16::from(inner.height >= 10)),
            Constraint::Fill(1),
            Constraint::Length(detail_height),
        ])
        .areas(inner);
        let eye = Rect::new(bar.right() - eye_width, bar.y, eye_width, bar.height.min(1));
        if !eye.is_empty() {
            f.render_widget(
                Paragraph::new(eye_label).style(button_style(self.view_menu)),
                eye,
            );
            self.hits.push((eye, Hit::ViewOptions));
        }
        let row_start = bar.x + lead.min(bar.width);
        let (mut x, mut y, mut right) = (row_start, bar.y, eye.x);
        for (label, op) in buttons {
            let width = label.width() as u16;
            if x > row_start && x.saturating_add(width) > right {
                x = row_start;
                y += 1;
                right = bar.right();
            }
            if y >= bar.bottom() || x >= right {
                break;
            }
            let width = width.min(right - x);
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
            x = x.saturating_add(width + 1);
        }
        self.viewport.area = list;
        self.keep_branch_visible();
        let columns = self.branch_columns(row_width(list, len));
        for (label, offset) in [
            (" Branch", Some(0)),
            ("Upstream", columns.upstream),
            ("Last Commit", columns.last_commit),
        ] {
            if let Some(x) = offset {
                f.render_widget(
                    Paragraph::new(label).style(Style::new().fg(MUTED)),
                    Rect::new(heading.x + x, heading.y, heading.width - x, heading.height),
                );
            }
        }
        for i in self.viewport.visible(len) {
            let rect = row_rect(&self.viewport, len, i);
            self.draw_row(f, rect, i, columns);
            self.hits.push((rect, Hit::Row(i)));
        }
        if len == 0 {
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
        draw_scrollbar(f, &self.viewport, len);
        self.draw_details(f, details, narrow, show_path, detached.as_deref());
    }

    fn draw_details(
        &self,
        f: &mut Frame,
        area: Rect,
        narrow: bool,
        show_path: bool,
        detached: Option<&str>,
    ) {
        let Some(row) = self.tree.rows.get(self.tree.cursor) else {
            return;
        };
        let width = usize::from(area.width.saturating_sub(2));
        let (label, metadata, tracked) = match &row.kind {
            Kind::Branch(b) => (
                format!("{}{}", b.name, if b.current { " (current)" } else { "" }),
                if b.upstream.is_empty() {
                    if b.remote {
                        "Remote branch".into()
                    } else {
                        "No upstream".into()
                    }
                } else {
                    let prefix = if narrow { "Upstream: " } else { "" };
                    let tail = if b.gone { " [gone]" } else { "" };
                    let name = model::short_ref(&b.upstream);
                    let name = if narrow {
                        clipped(name, width.saturating_sub(prefix.width() + tail.width()))
                    } else {
                        name.into()
                    };
                    format!("{prefix}{name}{tail}")
                },
                !b.upstream.is_empty(),
            ),
            Kind::Folder { count, .. } => (row.label.clone(), format!("{count} branches"), false),
            Kind::Message => (row.label.clone(), String::new(), false),
        };
        let tracking = match &row.kind {
            Kind::Branch(branch) => self.tracking_spans(branch),
            _ => Vec::new(),
        };
        let tracking_width: usize = tracking.iter().map(Span::width).sum();
        let mut label = vec![Span::styled(
            format!(" {}", clipped(&label, width.saturating_sub(tracking_width))),
            Style::new().fg(FG),
        )];
        label.extend(tracking);
        let mut lines = if narrow && matches!(row.kind, Kind::Branch(_)) {
            vec![Line::from(label), Line::from(format!(" {metadata}"))]
        } else {
            let separator = if tracked { " -> " } else { "  " };
            label.push(Span::raw(format!("{separator}{metadata}")));
            vec![Line::from(label)]
        };
        if show_path || detached.is_some() {
            let mut context = detached.unwrap_or_default().to_owned();
            if show_path {
                if let Some(repo) = self.repository() {
                    if !context.is_empty() {
                        context.push_str("  ");
                    }
                    context.push_str(&clipped_path(
                        repo.path.display().to_string(),
                        width.saturating_sub(context.width()),
                    ));
                }
            }
            lines.push(Line::from(format!(" {context}")));
        }
        f.render_widget(
            Paragraph::new(lines).style(Style::new().fg(MUTED).bg(TAG)),
            area,
        );
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
        let submit = if input.operation == Operation::Track {
            " Enter Switch "
        } else {
            " Enter Review "
        };
        let mut x = inner.x;
        for (label, hit) in [(submit, Hit::Submit), (" Esc Cancel ", Hit::Cancel)] {
            let rect = Rect::new(x, inner.bottom().saturating_sub(1), label.width() as u16, 1);
            if rect.right() <= inner.right() {
                f.render_widget(Paragraph::new(label).style(button_style(false)), rect);
                self.hits.push((rect, hit));
                x = rect.right() + 1;
            }
        }
    }
}

fn row_width(list: Rect, count: usize) -> u16 {
    list.width
        .saturating_sub(u16::from(count > usize::from(list.height)))
}

fn row_rect(viewport: &Viewport, count: usize, index: usize) -> Rect {
    let list = viewport.area;
    Rect::new(
        list.x,
        list.y + (index - viewport.top) as u16,
        row_width(list, count),
        1,
    )
}

fn draw_scrollbar(f: &mut Frame, viewport: &Viewport, count: usize) {
    let list = viewport.area;
    let visible = usize::from(list.height);
    if !list.is_empty() && count > visible {
        let mut state = ScrollbarState::new(count - visible + 1).position(viewport.top);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(None),
            list,
            &mut state,
        );
    }
}
