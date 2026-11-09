use super::*;
use crate::ui::{
    idea::{clipped, CONFLICT, TAG},
    panes::{pane_frame, Axis, SplitId},
    selection::{TextLayout, Wrap as TextWrap},
};
use ratatui::{
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr;

fn take_row(area: &mut Rect) -> Rect {
    let height = area.height.min(1);
    let row = Rect { height, ..*area };
    area.y += height;
    area.height -= height;
    row
}

impl StashPanel {
    pub(crate) fn draw_review(
        &self,
        f: &mut Frame,
        area: Rect,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) -> bool {
        let Some(review) = &self.review else {
            return false;
        };
        review.draw(f, area, self.icons, scroll, confirm_focused, hits);
        true
    }

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
        self.draw_diff(f, files, c);
        self.files.draw_overlay(f, area, c);
    }

    fn draw_diff(&mut self, f: &mut Frame, mut area: Rect, c: &Context) {
        if let Some((root, entry)) = self.selection().filter(|_| !area.is_empty()) {
            draw_diff_context(f, &mut area, &model::label(&c.repos[root].name), entry);
        }
        self.files.draw_files(
            f,
            area,
            c,
            self.icons,
            self.panes.active && self.focus == PaneId::Files,
        );
        if let Some(Row::Repository(root)) = self.rows.get(self.cursor) {
            if let Some(error) = self.repository(*root).and_then(|repo| repo.error.as_ref()) {
                f.render_widget(
                    Paragraph::new(format!("Error: {}", model::label(error)))
                        .style(Style::new().fg(CONFLICT))
                        .wrap(Wrap { trim: false }),
                    area,
                );
                return;
            }
        }
        if let Some(message) = self.preview.message().or_else(|| {
            self.selection()
                .is_none()
                .then_some("Select a stash to view saved files")
        }) {
            f.render_widget(
                Paragraph::new(message)
                    .style(Style::new().fg(MUTED))
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
    }

    fn draw_list(&mut self, f: &mut Frame, mut list: Rect, c: &Context) {
        self.viewport = Rect::default();
        if list.is_empty() {
            return;
        }
        let count: usize = self.repos.iter().map(|r| r.entries.len()).sum();
        let populated = self.repos.iter().filter(|r| !r.entries.is_empty()).count();
        let full = format!("  {count} saved · {populated}/{} repos", self.repos.len());
        let room = usize::from(list.width).saturating_sub("Stashes".len());
        let summary = if full.width() <= room {
            full
        } else {
            clipped(&format!("  {count} saved"), room)
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Stashes", Style::new().fg(FG).bold()),
                Span::styled(summary, Style::new().fg(MUTED)),
            ])),
            take_row(&mut list),
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
        let height = usize::from(list.height);
        let max_top = self.rows.len().saturating_sub(height);
        self.top = self
            .top
            .min(max_top)
            .min(self.cursor)
            .max((self.cursor + 1).saturating_sub(height));
        for (offset, &item) in self.rows.iter().skip(self.top).take(height).enumerate() {
            let index = self.top + offset;
            let row = Rect::new(
                list.x,
                list.y + offset as u16,
                list.width.saturating_sub(u16::from(max_top > 0)),
                1,
            );
            let selected = index == self.cursor;
            let bg = if !selected {
                BG
            } else if self.panes.active && self.focus == PaneId::Stashes {
                SEL
            } else {
                TAG
            };
            let line = self.list_line(c, item, usize::from(row.width), selected);
            f.render_widget(Paragraph::new(line).style(Style::new().fg(FG).bg(bg)), row);
            self.hits.push((row, index));
        }
        if max_top > 0 {
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                list,
                &mut ScrollbarState::new(max_top).position(self.top),
            );
        }
    }

    fn list_line(&self, c: &Context, row: Row, width: usize, selected: bool) -> Line<'static> {
        let muted = Style::new().fg(if selected { FG } else { MUTED });
        match row {
            Row::Stash { root, entry } => stash_line(
                &self.repository(root).unwrap().entries[entry],
                width,
                selected,
            ),
            Row::Repository(root) => {
                let repo = self.repository(root).unwrap();
                let name = model::label(&c.repos[root].name);
                let icon = self.icons.repo(c.repos[root].submodule);
                if repo.is_empty() {
                    return Line::styled(clipped(&format!("  {icon} {name}"), width), muted);
                }
                let (prefix, badge, badge_style) = if repo.error.is_some() {
                    (
                        format!("  {icon} "),
                        "  Error".into(),
                        Style::new().fg(CONFLICT),
                    )
                } else {
                    let disclosure = self.icons.disclosure(!self.folded.contains(&root));
                    (
                        format!("{disclosure} {icon} "),
                        format!("  {}", repo.entries.len()),
                        muted,
                    )
                };
                let label = clipped(&name, width.saturating_sub(prefix.width() + badge.width()));
                Line::from(vec![
                    Span::styled(prefix, muted),
                    Span::styled(label, Style::new().fg(FG).bold()),
                    Span::styled(badge, badge_style),
                ])
            }
            Row::EmptyGroup => {
                let count = self.repos.iter().filter(|repo| repo.is_empty()).count();
                let noun = if count == 1 { "repo" } else { "repos" };
                let all_empty = count == self.repos.len();
                let title = if all_empty {
                    format!("No stashes · {count} {noun}")
                } else {
                    format!("{count} {noun} without stashes")
                };
                let disclosure = self.icons.disclosure(self.empty_expanded);
                let room = width.saturating_sub(disclosure.width() + 1);
                let title = if title.width() <= room {
                    title
                } else if all_empty {
                    clipped(&format!("No stashes ({count})"), room)
                } else {
                    clipped(&format!("{count} without stashes"), room)
                };
                Line::styled(format!("{disclosure} {title}"), muted)
            }
        }
    }
}

fn stash_line(entry: &Entry, width: usize, selected: bool) -> Line<'static> {
    let indent = if width >= 8 { "    " } else { "" };
    let room = width.saturating_sub(indent.len());
    // Keep a readable message column. When width runs out, drop the number,
    // then the time, then the date. The full identity is in the Diff pane.
    let minute = entry.saved.get(..16);
    let date = entry.saved.get(..10);
    let full = entry
        .number()
        .zip(minute)
        .map(|(n, time)| format!("#{n}  {time}"));
    let meta = [full.as_deref(), minute, date]
        .into_iter()
        .flatten()
        .find(|value| value.width() + 2 + 16 <= room)
        .unwrap_or("");
    let gap = if meta.is_empty() { 0 } else { 2 };
    let message = clipped(entry.message(), room.saturating_sub(meta.width() + gap));
    let pad = room.saturating_sub(message.width() + meta.width());
    Line::from(vec![
        Span::raw(indent),
        Span::raw(message),
        Span::styled(
            format!("{}{meta}", " ".repeat(pad)),
            Style::new().fg(if selected { FG } else { MUTED }),
        ),
    ])
}

fn draw_diff_context(f: &mut Frame, area: &mut Rect, name: &str, entry: &Entry) {
    let width = usize::from(area.width);
    let style = Style::new().fg(FG).bold();
    let identity = format!("{name} / {}", entry.selector);
    if identity.width() <= width {
        f.render_widget(Paragraph::new(identity).style(style), take_row(area));
    } else if area.height >= 5 {
        for part in [name, &entry.selector] {
            f.render_widget(
                Paragraph::new(clipped(part, width)).style(style),
                take_row(area),
            );
        }
    } else {
        let name = clipped(name, width.saturating_sub(entry.selector.width() + 1));
        let identity = if name.is_empty() {
            clipped(&entry.selector, width)
        } else {
            format!("{name} {}", entry.selector)
        };
        f.render_widget(Paragraph::new(identity).style(style), take_row(area));
    }

    // The identity is essential at every size. Additional details should not
    // take space from the toolbar and the first few file rows in a small pane.
    let detail_rows = usize::from(area.height.saturating_sub(6).min(4));
    if detail_rows >= 2 {
        let show_time = entry.saved.width() <= width;
        let subject = TextLayout::plain(&entry.subject, area.width, TextWrap::Word);
        if subject.rows.len() + usize::from(show_time) <= detail_rows {
            for row in subject.rows {
                f.render_widget(Paragraph::new(row.line), take_row(area));
            }
        } else {
            f.render_widget(
                Paragraph::new(clipped(&entry.subject, width)),
                take_row(area),
            );
        }
        if show_time {
            f.render_widget(
                Paragraph::new(entry.saved.as_str()).style(Style::new().fg(MUTED)),
                take_row(area),
            );
        }
    }
}
