use super::*;
use crate::ui::{
    icons::Icon,
    idea::{button_style, clipped, repo_tint, TAG},
    panes::PaneId,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

impl RemotePanel {
    pub(super) fn draw_content(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.panes.begin();
        self.panes.add(PaneId::Remotes, area);
        self.hits.clear();
        let count = self.repos.len();
        let block = pane(
            format!(
                "Remotes · {count} {}",
                if count == 1 {
                    "repository"
                } else {
                    "repositories"
                }
            ),
            true,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.viewport = Rect::default();
        if inner.height < 3 || inner.width < 4 {
            return;
        }
        let mut x = inner.x;
        for (label, hit, enabled) in [
            (" + Add ", Hit::Add, !self.rows.is_empty()),
            (" - Remove ", Hit::Remove, self.selection().is_some()),
            (" Edit ", Hit::Edit, self.selection().is_some()),
        ] {
            let width = (label.width() as u16).min(inner.right().saturating_sub(x));
            let rect = Rect::new(x, inner.y, width, 1);
            let style = if enabled && !self.writes_blocked {
                button_style(false)
            } else {
                Style::new().fg(MUTED).bg(TAG)
            };
            f.render_widget(Paragraph::new(label).style(style), rect);
            if enabled {
                self.hits.push((rect, hit));
            }
            x += width + 1;
        }
        let name_width = self
            .repos
            .iter()
            .flat_map(|repo| &repo.remotes)
            .map(|remote| model::label(&remote.name).width() + 4)
            .max()
            .unwrap_or(12)
            .clamp(12, 28)
            .min(usize::from(inner.width) / 3) as u16;
        let url_x = inner.x + name_width + 1;
        let url_width = inner.right().saturating_sub(url_x);
        f.render_widget(
            Paragraph::new(" Name").style(Style::new().fg(MUTED).bg(TAG)),
            Rect::new(inner.x, inner.y + 1, name_width, 1),
        );
        f.render_widget(
            Paragraph::new("URL").style(Style::new().fg(MUTED).bg(TAG)),
            Rect::new(url_x, inner.y + 1, url_width, 1),
        );
        self.viewport = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);
        let height = usize::from(self.viewport.height);
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + height {
            self.top = self.cursor + 1 - height;
        }
        self.top = self.top.min(self.rows.len().saturating_sub(height));
        for (offset, index) in (self.top..self.rows.len()).take(height).enumerate() {
            let row = self.rows[index];
            let rect = Rect::new(
                self.viewport.x,
                self.viewport.y + offset as u16,
                self.viewport.width,
                1,
            );
            let selected = index == self.cursor;
            let background = if selected { SEL } else { BG };
            f.render_widget(Paragraph::new("").style(Style::new().bg(background)), rect);
            let Some(repo) = self.repository(row.root()) else {
                continue;
            };
            match row {
                Row::Repository(root) => {
                    let Some(repository) = c.repos.get(root) else {
                        continue;
                    };
                    let arrow = self.icons.glyph(if self.folded.contains(&root) {
                        Icon::Collapsed
                    } else {
                        Icon::Expanded
                    });
                    let icon = self.icons.glyph(if repository.submodule {
                        Icon::Submodule
                    } else {
                        Icon::Repo
                    });
                    let name = model::label(&repository.name);
                    let label = format!(
                        "{arrow} {icon} {name}  ({} remote{})",
                        repo.remotes.len(),
                        if repo.remotes.len() == 1 { "" } else { "s" }
                    );
                    f.render_widget(
                        Paragraph::new(clipped(&label, usize::from(rect.width))).style(
                            Style::new()
                                .fg(repo_tint(root))
                                .bg(background)
                                .add_modifier(Modifier::BOLD),
                        ),
                        rect,
                    );
                }
                Row::Message(_) => {
                    let label = repo
                        .error
                        .as_deref()
                        .map(model::label)
                        .unwrap_or_else(|| "No remotes · use Add".into());
                    f.render_widget(
                        Paragraph::new(clipped(&format!("    {label}"), usize::from(rect.width)))
                            .style(Style::new().fg(MUTED).bg(background)),
                        rect,
                    );
                }
                Row::Remote { remote, .. } | Row::Url { remote, .. } => {
                    let Some(entry) = repo.remotes.get(remote) else {
                        continue;
                    };
                    let (name, url) = match row {
                        Row::Remote { .. } => (
                            format!("  {}", model::label(&entry.name)),
                            entry
                                .urls
                                .first()
                                .map(String::as_str)
                                .unwrap_or("(no fetch URL)"),
                        ),
                        Row::Url { push, url, .. } => (
                            if push { "    push" } else { "    fetch" }.into(),
                            if push { &entry.push_urls } else { &entry.urls }
                                .get(url)
                                .map(String::as_str)
                                .unwrap_or_default(),
                        ),
                        _ => unreachable!(),
                    };
                    f.render_widget(
                        Paragraph::new(Line::from(vec![Span::raw(clipped(
                            &name,
                            usize::from(name_width),
                        ))]))
                        .style(Style::new().fg(FG).bg(background)),
                        Rect::new(rect.x, rect.y, name_width, 1),
                    );
                    f.render_widget(
                        Paragraph::new(clipped(&model::label(url), usize::from(url_width)))
                            .style(Style::new().fg(FG).bg(background)),
                        Rect::new(url_x, rect.y, url_width, 1),
                    );
                }
            }
            self.hits.push((rect, Hit::Row(index)));
        }
        if let Some(editor) = &mut self.editor {
            editor.draw(f, area, c, self.panes.active);
        }
    }
}
