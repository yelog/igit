use super::*;
use crate::ui::{
    idea::repo_tint,
    panes::{pane_frame, Axis, SplitId},
};
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

impl StashPanel {
    pub(super) fn draw_content(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.hits.clear();
        self.panes.begin();
        let [list, files] = if self.panes.maximized {
            if self.focus == PaneId::Stashes {
                [area, Rect::default()]
            } else {
                [Rect::default(), area]
            }
        } else {
            self.panes.divide(
                SplitId::StashFiles,
                area,
                Axis::Width,
                area.width * 55 / 100,
                [23, 18],
            )
        };
        self.panes.add(PaneId::Stashes, list);
        self.panes.add(PaneId::Files, files);
        let list = pane_frame(f, list, self.panes.active && self.focus == PaneId::Stashes);
        let files = pane_frame(f, files, self.panes.active && self.focus == PaneId::Files);
        self.draw_list(f, list, c);
        self.files.draw_files(
            f,
            files,
            c,
            self.icons,
            self.panes.active && self.focus == PaneId::Files,
        );
        let message = self.preview.message().or_else(|| {
            self.selection()
                .is_none()
                .then_some("Select a stash to view saved files")
        });
        if let Some(message) = message {
            f.render_widget(
                Paragraph::new(message)
                    .style(Style::new().fg(MUTED))
                    .wrap(Wrap { trim: false }),
                files,
            );
        }
        self.files.draw_overlay(f, area, c);
    }

    fn draw_list(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.viewport = Rect::default();
        if area.is_empty() {
            return;
        }
        let count: usize = self.repos.iter().map(|r| r.entries.len()).sum();
        f.render_widget(
            Paragraph::new(format!(
                "Stashes · {} repos · {count} saved",
                self.repos.len()
            ))
            .style(Style::new().fg(FG)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let list = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        self.viewport = list;
        if list.is_empty() {
            return;
        }
        if self.repos.is_empty() {
            f.render_widget(
                Paragraph::new("No repositories in Roots").style(Style::new().fg(MUTED)),
                list,
            );
            return;
        }
        let mut lines = Vec::new();
        let mut selected_start = 0;
        let mut selected_end = 0;
        for (index, row) in self.rows.iter().enumerate() {
            let repo = self.repository(row.root).unwrap();
            if index == self.cursor {
                selected_start = lines.len();
            }
            if let Some(entry) = row.entry.and_then(|i| repo.entries.get(i)) {
                lines.push((
                    index,
                    Line::from(vec![
                        Span::styled(format!("  {} ", entry.selector), Style::new().fg(ACCENT)),
                        Span::raw(entry.subject.clone()),
                    ]),
                ));
                lines.push((
                    index,
                    Line::styled(format!("  {}", entry.saved), Style::new().fg(MUTED)),
                ));
            } else {
                let open = !self.folded.contains(&row.root);
                let name = model::label(&c.repos[row.root].name);
                lines.push((
                    index,
                    Line::styled(
                        format!(
                            "{} {} {} ({})",
                            self.icons.disclosure(open),
                            self.icons.repo(c.repos[row.root].submodule),
                            name,
                            repo.entries.len()
                        ),
                        Style::new().fg(repo_tint(row.root)),
                    ),
                ));
                if open {
                    if let Some(error) = &repo.error {
                        lines.push((
                            index,
                            Line::styled(
                                format!("  Error: {}", model::label(error)),
                                Style::new().fg(crate::ui::idea::CONFLICT),
                            ),
                        ));
                    } else if repo.entries.is_empty() {
                        lines.push((index, Line::styled("  No stashes", Style::new().fg(MUTED))));
                    }
                }
            }
            if index == self.cursor {
                selected_end = lines.len();
            }
        }
        let height = usize::from(list.height);
        self.top = self
            .top
            .min(lines.len().saturating_sub(height))
            .min(selected_start)
            .max(selected_end.saturating_sub(height));
        let overflow = lines.len() > height;
        for (offset, (index, line)) in lines.iter().enumerate().skip(self.top).take(height) {
            let row = Rect::new(
                list.x,
                list.y + (offset - self.top) as u16,
                list.width.saturating_sub(u16::from(overflow)),
                1,
            );
            let style = Style::new()
                .fg(FG)
                .bg(if *index == self.cursor { SEL } else { BG });
            // Paragraph clips at terminal cells; controls and Unicode stay in their own row.
            f.render_widget(Paragraph::new(line.clone()).style(style), row);
            self.hits.push((row, *index));
        }
        if overflow {
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                list,
                &mut ScrollbarState::new(lines.len().saturating_sub(height)).position(self.top),
            );
        }
    }
}
