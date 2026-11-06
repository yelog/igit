//! Push-style repository review; layout and hit areas share terminal-cell widths.
use super::*;
use crate::ui::{
    icons::{Icon, IconMode},
    idea::{button_style, clipped, repo_tint, CONFLICT, GREEN, RED, TAG},
    pane,
    selection::{TextLayout, Wrap},
    ACCENT, BG, FG, LINE, MUTED, SEL,
};
use ratatui::{
    layout::{Constraint as C, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Default)]
struct Column {
    x: usize,
    width: usize,
}

struct Columns {
    name: Column,
    branch: Column,
    target: Column,
    status: Column,
    module: Column,
    prefix: usize,
}

impl Columns {
    fn new(dialog: &Dialog, width: u16, icons: IconMode) -> Self {
        let width = usize::from(width);
        let prefix = 2 + icons.repo(false).width().max(icons.repo(true).width());
        let targets = width >= 32;
        let branches = dialog.kind == Kind::Pull && width >= 70;
        let modules = width >= 100 && dialog.rows.iter().any(|row| row.submodule);
        let max = |values: Vec<usize>, min| values.into_iter().max().unwrap_or(min).max(min);
        let mut sizes = [
            prefix + max(dialog.rows.iter().map(|row| row.name.width()).collect(), 8),
            if branches {
                max(
                    dialog.rows.iter().map(|row| row.branch.width()).collect(),
                    6,
                ) + icons.glyph(Icon::Left).width()
                    + 1
            } else {
                0
            },
            if targets {
                max(
                    dialog
                        .rows
                        .iter()
                        .map(|row| match &row.outcome {
                            Outcome::Skipped(reason) => reason.width().min(48),
                            _ => dialog.target(row, icons, branches).width(),
                        })
                        .collect(),
                    10,
                )
            } else {
                0
            },
            if width >= 16 { 7 } else { 0 },
            if modules { 9 } else { 0 },
        ];
        let minimum = [
            prefix + 6,
            if branches { 8 } else { 0 },
            if targets { 8 } else { 0 },
            sizes[3],
            sizes[4],
        ];
        let gaps = sizes
            .iter()
            .filter(|&&size| size > 0)
            .count()
            .saturating_sub(1)
            * 2;
        while sizes.iter().sum::<usize>() + gaps > width {
            let Some(index) = (0..sizes.len())
                .filter(|&i| sizes[i] > minimum[i])
                .max_by_key(|&i| sizes[i] - minimum[i])
            else {
                break;
            };
            sizes[index] -= 1;
        }
        // Keep status/type near the right edge while giving long targets room.
        let spare = width.saturating_sub(sizes.iter().sum::<usize>() + gaps + 1);
        let name_extra = if targets { (spare / 3).min(16) } else { spare };
        sizes[0] += name_extra;
        if targets {
            sizes[2] += spare - name_extra;
        }
        let mut x: usize = 0;
        let mut column = |size: usize| {
            if size == 0 {
                return Column::default();
            }
            let column = Column {
                x,
                width: size.min(width.saturating_sub(x)),
            };
            x += size + 2;
            column
        };
        Self {
            name: column(sizes[0]),
            branch: column(sizes[1]),
            target: column(sizes[2]),
            status: column(sizes[3]),
            module: column(sizes[4]),
            prefix,
        }
    }
}

fn put(line: &mut Line<'static>, column: Column, text: &str, style: Style) {
    if column.width > 0 {
        line.push_span(" ".repeat(column.x.saturating_sub(line.width())));
        line.push_span(Span::styled(clipped(text, column.width), style));
    }
}

impl Dialog {
    fn status(&self, row: &RepositoryRow) -> (&'static str, Color) {
        match row.outcome {
            Outcome::Ready if matches!(self.stage, Stage::Running(_)) => ("Pending", ACCENT),
            Outcome::Ready => ("Ready", MUTED),
            Outcome::Succeeded => ("OK", GREEN),
            Outcome::Failed(_) => ("Failed", RED),
            Outcome::Skipped(_) => ("Skipped", CONFLICT),
        }
    }

    fn target(&self, row: &RepositoryRow, icons: IconMode, separate_branch: bool) -> String {
        if row.destination.is_empty() {
            return String::new();
        }
        if self.kind == Kind::Pull && !separate_branch {
            format!(
                "{} {} {}",
                row.branch,
                icons.glyph(Icon::Left),
                row.destination
            )
        } else {
            row.destination.clone()
        }
    }

    fn counts(&self) -> Line<'static> {
        let skipped = self
            .rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Skipped(_)))
            .count();
        let mut line = Line::default();
        if matches!(self.stage, Stage::Results) {
            let succeeded = self
                .rows
                .iter()
                .filter(|row| matches!(row.outcome, Outcome::Succeeded))
                .count();
            let failed = self
                .rows
                .iter()
                .filter(|row| matches!(row.outcome, Outcome::Failed(_)))
                .count();
            line.push_span(Span::styled(
                format!("{succeeded} succeeded"),
                Style::new().fg(GREEN),
            ));
            line.push_span(Span::styled(" · ", Style::new().fg(MUTED)));
            line.push_span(Span::styled(
                format!("{failed} failed"),
                Style::new().fg(if failed > 0 { RED } else { MUTED }),
            ));
        } else {
            line.push_span(Span::styled(
                format!("{} ready", self.ready),
                Style::new().fg(ACCENT),
            ));
        }
        if skipped > 0 || matches!(self.stage, Stage::Results) {
            line.push_span(Span::styled(" · ", Style::new().fg(MUTED)));
            line.push_span(Span::styled(
                format!("{skipped} skipped"),
                Style::new().fg(if skipped > 0 { CONFLICT } else { MUTED }),
            ));
        }
        line
    }

    pub(in crate::shell) fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        self.list_area = Rect::default();
        self.detail_area = Rect::default();
        if area.is_empty() {
            return;
        }
        let available = area.inner(Margin::new(
            u16::from(area.width >= 80) * 2,
            u16::from(area.height >= 18),
        ));
        let width = available.width.min(132);
        let height = available
            .height
            .min((self.rows.len().saturating_add(12)).clamp(16, 32) as u16);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        f.render_widget(Clear, rect);
        f.render_widget(Block::default().style(Style::new().fg(FG).bg(BG)), rect);
        let [title, content, footer] =
            Layout::vertical([C::Length(1), C::Min(0), C::Length(2)]).areas(rect);
        let name = match (self.kind, &self.stage) {
            (Kind::Fetch, Stage::Results) => "Fetch Results",
            (Kind::Pull, Stage::Results) => "Pull Results",
            (Kind::Fetch, _) => "Fetch Repositories",
            (Kind::Pull, _) => "Update Project",
        };
        let suffix = if matches!(self.stage, Stage::Running(_)) {
            "Running".into()
        } else {
            format!(
                "{} {}",
                self.rows.len(),
                if self.rows.len() == 1 {
                    "repository"
                } else {
                    "repositories"
                }
            )
        };
        f.render_widget(
            Paragraph::new(clipped(
                &format!(" {name} · {suffix}"),
                usize::from(title.width),
            ))
            .style(Style::new().fg(FG).bg(TAG).add_modifier(Modifier::BOLD)),
            title,
        );
        let heading = if self.kind == Kind::Pull {
            "Incoming changes"
        } else {
            "Remote references"
        };
        let block = pane(heading, true).borders(Borders::TOP | Borders::LEFT | Borders::RIGHT);
        let inner = block.inner(content);
        f.render_widget(block, content);
        let details_height = if inner.height >= 11 {
            5
        } else if inner.height >= 6 {
            3
        } else {
            0
        };
        let [toolbar, summary, header, list, details] = Layout::vertical([
            C::Length(1),
            C::Length(1),
            C::Length(u16::from(inner.height >= 8)),
            C::Min(1),
            C::Length(details_height),
        ])
        .areas(inner);
        self.draw_toolbar(f, toolbar, icons, hits);
        let mut counts = Line::from(" ");
        counts.spans.extend(self.counts().spans);
        f.render_widget(Paragraph::new(counts), summary);
        let columns = Columns::new(self, list.width, icons);
        if header.height > 0 {
            let mut line = Line::default();
            let muted = Style::new().fg(MUTED);
            put(
                &mut line,
                Column {
                    x: columns.prefix,
                    width: columns.name.width.saturating_sub(columns.prefix),
                },
                if columns.name.width >= columns.prefix + 10 {
                    "Repository"
                } else {
                    "Repo"
                },
                muted,
            );
            put(&mut line, columns.branch, "Branch", muted);
            put(
                &mut line,
                columns.target,
                if self.kind == Kind::Pull {
                    "Upstream"
                } else {
                    "Remotes"
                },
                muted,
            );
            put(&mut line, columns.status, "Status", muted);
            put(&mut line, columns.module, "Type", muted);
            f.render_widget(Paragraph::new(line), header);
        }
        self.list_area = list;
        self.draw_rows(f, list, icons, &columns, hits);
        self.draw_details(f, details, icons);
        self.draw_footer(f, footer, hits);
    }

    fn draw_toolbar(
        &self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        if self.pull_review() {
            let full = area.width >= 66;
            let mut x = area.x + u16::from(area.width > 0);
            for (strategy, id, key, label) in [
                (
                    Strategy::Merge,
                    MERGE,
                    "m",
                    if full {
                        "Merge incoming changes"
                    } else {
                        "Merge"
                    },
                ),
                (
                    Strategy::Rebase,
                    REBASE,
                    "r",
                    if full {
                        "Rebase current branches"
                    } else {
                        "Rebase"
                    },
                ),
            ] {
                let selected = self.strategy == strategy;
                let radio = match (icons, selected) {
                    (IconMode::Ascii, true) => "(o)",
                    (IconMode::Ascii, false) => "( )",
                    (_, true) => "◉",
                    (_, false) => "○",
                };
                let text = format!(" {radio} {key} {label} ");
                let width = (text.width() as u16).min(area.right().saturating_sub(x));
                let row = Rect::new(x, area.y, width, area.height.min(1));
                if row.is_empty() {
                    break;
                }
                f.render_widget(
                    Paragraph::new(text).style(if selected {
                        button_style(false).fg(ACCENT)
                    } else {
                        Style::new().fg(MUTED)
                    }),
                    row,
                );
                hits.push((row, id));
                x = row.right().saturating_add(1);
            }
        } else {
            let label = match (&self.stage, self.kind, self.strategy) {
                (Stage::Running(_), _, _) => "Running Git for reviewed repositories…",
                (Stage::Results, _, _) => "Select a repository to inspect its result",
                (_, Kind::Fetch, _) => "All remotes · remote refs only",
                (_, Kind::Pull, Strategy::Merge) => "Merge incoming changes",
                (_, Kind::Pull, Strategy::Rebase) => "Rebase current branches",
            };
            f.render_widget(
                Paragraph::new(format!(" {} {label}", icons.glyph(Icon::Refresh)))
                    .style(Style::new().fg(MUTED)),
                area,
            );
        }
    }

    fn draw_rows(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        columns: &Columns,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        let height = usize::from(area.height);
        self.offset = self.offset.min(self.rows.len().saturating_sub(height));
        if height > 0 {
            self.offset = self
                .offset
                .min(self.pos)
                .max((self.pos + 1).saturating_sub(height));
        }
        for (index, repo) in self.rows.iter().enumerate().skip(self.offset).take(height) {
            let row = Rect::new(area.x, area.y + (index - self.offset) as u16, area.width, 1);
            let muted = Style::new().fg(MUTED);
            let skipped = matches!(repo.outcome, Outcome::Skipped(_));
            let mut line = Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    icons.repo(repo.submodule),
                    Style::new().fg(repo_tint(repo.root)),
                ),
            ]);
            put(
                &mut line,
                Column {
                    x: columns.prefix,
                    width: columns.name.width.saturating_sub(columns.prefix),
                },
                &repo.name,
                Style::new()
                    .fg(if skipped { MUTED } else { FG })
                    .add_modifier(Modifier::BOLD),
            );
            if columns.branch.width > 0 {
                let arrow = icons.glyph(Icon::Left);
                let branch = clipped(
                    &repo.branch,
                    columns.branch.width.saturating_sub(arrow.width() + 1),
                );
                put(
                    &mut line,
                    columns.branch,
                    &format!("{branch} {arrow}"),
                    muted,
                );
            }
            match &repo.outcome {
                Outcome::Skipped(reason) => {
                    put(&mut line, columns.target, reason, Style::new().fg(CONFLICT))
                }
                _ => put(
                    &mut line,
                    columns.target,
                    &self.target(repo, icons, columns.branch.width > 0),
                    Style::new().fg(ACCENT),
                ),
            }
            let (status, color) = self.status(repo);
            put(&mut line, columns.status, status, Style::new().fg(color));
            if repo.submodule {
                put(&mut line, columns.module, "submodule", muted);
            }
            f.render_widget(
                Paragraph::new(line).style(Style::new().fg(FG).bg(if index == self.pos {
                    SEL
                } else {
                    BG
                })),
                row,
            );
            hits.push((row, ROW + index));
        }
        if self.rows.is_empty() {
            f.render_widget(
                Paragraph::new(" No repositories in Roots").style(Style::new().fg(MUTED)),
                area,
            );
        }
    }

    fn draw_details(&mut self, f: &mut Frame, area: Rect, icons: IconMode) {
        self.detail_max_scroll = 0;
        if area.height < 2 {
            return;
        }
        let title = if self.rows.is_empty() {
            "Details".into()
        } else {
            format!("Details · {}/{}", self.pos + 1, self.rows.len())
        };
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(LINE))
            .title(Span::styled(format!(" {title} "), Style::new().fg(MUTED)));
        let inner = block.inner(area).inner(Margin::new(1, 0));
        let mut layout = TextLayout::default();
        if let Some(row) = self.rows.get(self.pos) {
            let (status, color) = self.status(row);
            layout.push(
                Line::from(vec![
                    Span::styled(
                        row.name.clone(),
                        Style::new().fg(FG).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(": {status}"), Style::new().fg(color)),
                ]),
                inner.width,
                Wrap::Word,
            );
            if !row.destination.is_empty() {
                layout.push(
                    Line::styled(self.target(row, icons, false), Style::new().fg(ACCENT)),
                    inner.width,
                    Wrap::Word,
                );
            }
            let detail = match &row.outcome {
                Outcome::Failed(reason) | Outcome::Skipped(reason) => reason.as_str(),
                Outcome::Succeeded if self.kind == Kind::Fetch => {
                    "Remote refs refreshed. Working tree unchanged."
                }
                Outcome::Succeeded if self.strategy == Strategy::Merge => {
                    "Pull completed using Merge."
                }
                Outcome::Succeeded => "Pull completed using Rebase.",
                _ if self.kind == Kind::Fetch => {
                    "Refresh remote refs; keep working trees unchanged."
                }
                _ if self.strategy == Strategy::Merge => {
                    "Fast-forward when possible; otherwise create a merge commit."
                }
                _ => "Replay local commits on the upstream branch.",
            };
            for line in detail.lines() {
                layout.push(
                    Line::styled(
                        line,
                        Style::new().fg(
                            if matches!(row.outcome, Outcome::Failed(_) | Outcome::Skipped(_)) {
                                color
                            } else {
                                MUTED
                            },
                        ),
                    ),
                    inner.width,
                    Wrap::Word,
                );
            }
        } else {
            layout.push(
                Line::styled(
                    "Close this review and select repositories in Roots (r).",
                    Style::new().fg(MUTED),
                ),
                inner.width,
                Wrap::Word,
            );
        }
        self.detail_max_scroll = layout.rows.len().saturating_sub(usize::from(inner.height));
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll);
        let block = if self.detail_max_scroll > 0 && area.width >= 30 {
            block.title_top(Line::styled(" J/K scroll ", Style::new().fg(MUTED)).right_aligned())
        } else {
            block
        };
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(
                layout
                    .rows
                    .into_iter()
                    .skip(self.detail_scroll)
                    .take(usize::from(inner.height))
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
            ),
            inner,
        );
        self.detail_area = area;
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, hits: &mut Vec<(Rect, usize)>) {
        if area.height < 2 {
            return;
        }
        f.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(LINE)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let row = Rect::new(area.x, area.y + 1, area.width, 1);
        if matches!(self.stage, Stage::Running(_)) {
            f.render_widget(
                Paragraph::new(format!(" Updating {} repositories…", self.ready))
                    .style(Style::new().fg(ACCENT)),
                row,
            );
            return;
        }
        let mut right = row.right().saturating_sub(1).max(row.x);
        let actions = if matches!(self.stage, Stage::Results) {
            vec![(" Close / Esc ".into(), CANCEL, true, true)]
        } else {
            vec![
                (
                    format!(" y {} {} ", self.kind.label(), self.ready),
                    CONFIRM,
                    self.confirm_focused,
                    self.ready > 0,
                ),
                (" n Cancel ".into(), CANCEL, !self.confirm_focused, true),
            ]
        };
        for (label, id, focused, enabled) in actions {
            let width = (label.width() as u16).min(right.saturating_sub(row.x));
            let rect = Rect::new(right - width, row.y, width, 1);
            let mut style = button_style(id == CONFIRM && enabled);
            if !enabled {
                style = style.fg(MUTED);
            }
            if focused {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            f.render_widget(Paragraph::new(label).style(style), rect);
            if enabled && !rect.is_empty() {
                hits.push((rect, id));
            }
            right = rect.x.saturating_sub(1).max(row.x);
        }
        let hint = format!(" {} in Roots", self.rows.len());
        if hint.width() < usize::from(right.saturating_sub(row.x)) {
            f.render_widget(
                Paragraph::new(hint).style(Style::new().fg(MUTED)),
                Rect::new(row.x, row.y, right - row.x, 1),
            );
        }
    }
}
