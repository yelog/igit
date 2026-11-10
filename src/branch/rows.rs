//! Branch rows align metadata by terminal cells, independent of tree depth.
use super::*;
use crate::ui::{
    icons::Icon,
    idea::{clipped, repo_tint, tracking_spans, CONFLICT, GOLD, LOCAL_REF, REMOTE_REF, TAG},
};
use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

const INDENT: usize = 4;
const COLUMN_GAP: u16 = 2;
const MIN_BRANCH_WIDTH: u16 = 28;
const LAST_COMMIT_WIDTH: u16 = 17; // YYYY/MM/DD, HH:MM

#[derive(Clone, Copy)]
pub(super) struct BranchColumns {
    pub upstream: Option<u16>,
    pub last_commit: Option<u16>,
}

impl BranchColumns {
    fn body_width(self, width: u16) -> u16 {
        self.last_commit.map_or(width, |x| x - COLUMN_GAP)
    }
}

fn status(branch: &Branch) -> &'static str {
    if branch.oid.is_empty() {
        " [no commits]"
    } else if !branch.current && !branch.worktree.is_empty() {
        " [worktree]"
    } else if branch.current {
        " (current)"
    } else {
        ""
    }
}

pub(super) fn measure_upstream(
    snapshot: &Snapshot,
    metadata: &model::Metadata,
) -> Option<(usize, usize)> {
    // Include folded local refs so navigation never moves the upstream column.
    let mut has_upstream = false;
    let (mut grouped, mut flat) = (0, 0);
    for branch in snapshot.branches.values().filter(|b| !b.remote) {
        has_upstream |= !branch.upstream.is_empty();
        let depth = 1 + branch.name.matches('/').count();
        let counts = metadata.tracking_counts(branch);
        let tracking_width = [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii]
            .into_iter()
            .map(|icons| {
                tracking_spans(icons, counts)
                    .iter()
                    .map(Span::width)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or_default();
        let suffix_width = tracking_width + status(branch).width();
        grouped = grouped.max(
            depth * INDENT + branch.name.rsplit('/').next().unwrap_or("").width() + suffix_width,
        );
        flat = flat.max(INDENT + branch.name.width() + suffix_width);
    }
    has_upstream.then_some((grouped, flat))
}

impl BranchPanel {
    pub(super) fn tracking_spans(&self, branch: &Branch) -> Vec<Span<'static>> {
        let counts = self
            .repository()
            .map(|repo| repo.metadata.tracking_counts(branch))
            .unwrap_or_default();
        tracking_spans(self.icons, counts)
    }

    fn disclosure_width(&self) -> usize {
        self.icons
            .disclosure(false)
            .width()
            .max(self.icons.disclosure(true).width())
            + 1
    }

    fn current_marker(&self) -> Span<'static> {
        Span::styled(
            format!("{} ", self.icons.glyph(Icon::Tag)),
            Style::new().fg(GOLD),
        )
    }

    pub(super) fn branch_columns(&self, width: u16) -> BranchColumns {
        let mut columns = BranchColumns {
            upstream: None,
            last_commit: width
                .checked_sub(LAST_COMMIT_WIDTH + 1)
                .filter(|&x| x >= MIN_BRANCH_WIDTH + COLUMN_GAP),
        };
        if width >= 64 {
            if let Some((grouped, flat)) = self.repository().and_then(|repo| repo.upstream_widths) {
                let marker_width = self
                    .icons
                    .glyph(Icon::Branch)
                    .width()
                    .max(self.icons.glyph(Icon::Tag).width());
                let natural = if self.tree.grouped() { grouped } else { flat }
                    + self.disclosure_width()
                    + marker_width
                    + 3;
                let min = usize::from(MIN_BRANCH_WIDTH);
                let max = (usize::from(columns.body_width(width)) * 3 / 5).max(min);
                columns.upstream = Some(natural.clamp(min, max) as u16);
            }
        }
        columns
    }

    pub(super) fn draw_repository_row(&self, f: &mut Frame, rect: Rect, index: usize) {
        let repo = &self.repos[index];
        let selected = Some(repo.root) == self.repo_root;
        let focused = self.focused_pane() == PaneId::BranchRepos;
        let bg = selection_bg(selected, focused);
        let icon = format!(" {} ", self.icons.repo(repo.submodule));
        let (head, counts, on_branch) = match &repo.state {
            Ok(snapshot) => {
                let counts = snapshot
                    .head
                    .as_ref()
                    .and_then(|head| snapshot.branches.get(head))
                    .map(|branch| repo.metadata.tracking_counts(branch))
                    .unwrap_or_default();
                (snapshot.head_label(), counts, snapshot.head.is_some())
            }
            Err(_) => (String::new(), (0, 0), false),
        };
        let available = usize::from(rect.width).saturating_sub(icon.width());
        // Leave room to identify the repository, even when its branch name is long.
        let head_budget = available.saturating_sub(1 + repo.name.width().min(available / 4));
        let mut tracking = tracking_spans(self.icons, counts);
        let mut count_width: usize = tracking.iter().map(Span::width).sum();
        if count_width >= head_budget {
            tracking.clear();
            count_width = 0;
        }
        let head = self.repo_search.field(&repo.root, 1).clipped(
            &head,
            head_budget.saturating_sub(count_width),
            Style::new().fg(if on_branch { FG } else { MUTED }),
        );
        let head_width = head.iter().map(Span::width).sum::<usize>() + count_width;
        let name = self.repo_search.field(&repo.root, 0).clipped(
            &repo.name,
            available.saturating_sub(head_width + usize::from(head_width > 0)),
            Style::new().fg(FG).bold(),
        );
        // Optional markers use spare cells, preserving both labels and whole counters.
        let gap =
            available.saturating_sub(name.iter().map(Span::width).sum::<usize>() + head_width);
        let marker_budget = gap.saturating_sub(1);
        let tag = (on_branch && head_width > count_width)
            .then(|| self.current_marker())
            .filter(|tag| tag.width() <= marker_budget);
        let tag_width = tag.as_ref().map_or(0, Span::width);
        let arrow = Span::styled(
            format!(" {}", self.icons.glyph(Icon::Next)),
            Style::new().fg(MUTED),
        );
        let arrow = (head_width > count_width && arrow.width() <= marker_budget - tag_width)
            .then_some(arrow);
        let padding = gap - tag_width - arrow.as_ref().map_or(0, Span::width);
        let mut spans = vec![Span::styled(icon, Style::new().fg(repo_tint(repo.root)))];
        spans.extend(name);
        spans.push(Span::raw(" ".repeat(padding)));
        spans.extend(tag);
        spans.extend(head);
        spans.extend(tracking);
        spans.extend(arrow);
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
            rect,
        );
    }

    pub(super) fn draw_row(&self, f: &mut Frame, rect: Rect, index: usize, columns: BranchColumns) {
        let row = &self.tree.rows[index];
        let highlight = self.branch_search.field(&row.id, 0);
        let selected = index == self.tree.cursor;
        let focused = self.focused_pane() == PaneId::Branches;
        let bg = selection_bg(selected, focused);
        f.render_widget(Paragraph::new("").style(Style::new().bg(bg)), rect);
        let body_width = columns.body_width(rect.width);
        let width = match &row.kind {
            Kind::Branch(b) if !b.remote => columns.upstream.map_or(body_width, |x| x - COLUMN_GAP),
            Kind::Message => rect.width,
            _ => body_width,
        };
        let step = if rect.width < 40 { 3 } else { INDENT };
        let indent = (row.depth * step).min(usize::from(width.saturating_sub(18)));
        let mut guides = String::new();
        for column in 0..indent {
            if column % step == 0 {
                guides.push_str(self.icons.glyph(Icon::Divider));
            } else {
                guides.push(' ');
            }
        }
        let mut spans = vec![Span::styled(guides, Style::new().fg(LINE))];
        let slot = self.disclosure_width();
        match &row.kind {
            Kind::Folder { open, count } => {
                let color = if row.depth == 0 {
                    if row.id.ends_with("refs/remotes/") {
                        REMOTE_REF
                    } else {
                        LOCAL_REF
                    }
                } else {
                    FG
                };
                let secondary = if selected && focused { FG } else { MUTED };
                let disclosure = self.icons.disclosure(*open);
                spans.push(Span::styled(disclosure, Style::new().fg(secondary)));
                spans.push(Span::raw(" ".repeat(slot - disclosure.width())));
                spans.push(Span::styled(
                    format!("{} ", self.icons.folder(*open)),
                    Style::new().fg(if row.depth == 0 { color } else { secondary }),
                ));
                let used: usize = spans.iter().map(Span::width).sum();
                let available = usize::from(width).saturating_sub(used);
                let tail = clipped(
                    &format!("  {count}"),
                    available.saturating_sub(row.label.width()),
                );
                spans.extend(highlight.clipped(
                    &row.label,
                    available.saturating_sub(tail.width()),
                    Style::new().fg(color).bold(),
                ));
                spans.push(Span::styled(tail, Style::new().fg(secondary)));
            }
            Kind::Branch(branch) => {
                let color = if branch.remote { REMOTE_REF } else { LOCAL_REF };
                spans.push(Span::raw(" ".repeat(slot)));
                spans.push(if branch.current {
                    self.current_marker()
                } else {
                    Span::styled(
                        format!("{} ", self.icons.glyph(Icon::Branch)),
                        Style::new().fg(color),
                    )
                });
                let used: usize = spans.iter().map(Span::width).sum();
                let available = usize::from(width).saturating_sub(used);
                let tracking = self.tracking_spans(branch);
                let tracking_width: usize = tracking.iter().map(Span::width).sum();
                let tail = status(branch);
                let tail = clipped(tail, available.saturating_sub(tracking_width + 4));
                spans.extend(highlight.clipped(
                    &row.label,
                    available.saturating_sub(tracking_width + tail.width()),
                    if branch.current {
                        Style::new().fg(FG).bold()
                    } else {
                        Style::new().fg(FG)
                    },
                ));
                spans.extend(tracking);
                spans.push(Span::styled(
                    tail,
                    Style::new().fg(if !branch.current && !branch.worktree.is_empty() {
                        CONFLICT
                    } else {
                        MUTED
                    }),
                ));
                if let Some(x) = columns.upstream.filter(|_| !branch.remote) {
                    self.draw_column_divider(f, rect, x, bg);
                    let area = Rect::new(rect.x + x, rect.y, body_width - x, 1);
                    self.draw_upstream(f, area, branch, bg);
                }
                if let Some(x) = columns.last_commit {
                    if let Some(date) = self
                        .repository()
                        .and_then(|repo| repo.metadata.last_commits.get(&branch.reference))
                    {
                        self.draw_column_divider(f, rect, x, bg);
                        f.render_widget(
                            Paragraph::new(date.as_str()).style(Style::new().fg(MUTED).bg(bg)),
                            Rect::new(rect.x + x, rect.y, LAST_COMMIT_WIDTH, 1),
                        );
                    }
                }
            }
            Kind::Message => spans.extend(highlight.clipped(
                &row.label,
                usize::from(width).saturating_sub(indent),
                Style::new().fg(MUTED),
            )),
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
            Rect { width, ..rect },
        );
    }

    fn draw_column_divider(&self, f: &mut Frame, row: Rect, x: u16, bg: Color) {
        f.render_widget(
            Paragraph::new(self.icons.glyph(Icon::Divider)).style(Style::new().fg(LINE).bg(bg)),
            Rect::new(row.x + x - COLUMN_GAP, row.y, 1, 1),
        );
    }

    fn draw_upstream(&self, f: &mut Frame, area: Rect, branch: &Branch, bg: Color) {
        if branch.upstream.is_empty() {
            f.render_widget(
                Paragraph::new("No upstream").style(Style::new().fg(MUTED).bg(bg)),
                area,
            );
            return;
        }
        let prefix = format!("{} ", self.icons.glyph(Icon::Right));
        let icon = format!("{} ", self.icons.glyph(Icon::Branch));
        let tail = if branch.gone { " [gone]" } else { "" };
        let width =
            usize::from(area.width).saturating_sub(prefix.width() + icon.width() + tail.width());
        let spans = vec![
            Span::styled(prefix, Style::new().fg(MUTED)),
            Span::styled(icon, Style::new().fg(REMOTE_REF)),
            Span::styled(
                clipped(model::short_ref(&branch.upstream), width),
                Style::new().fg(if branch.gone { CONFLICT } else { MUTED }),
            ),
            Span::styled(tail, Style::new().fg(CONFLICT)),
        ];
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
            area,
        );
    }
}

fn selection_bg(selected: bool, focused: bool) -> Color {
    match (selected, focused) {
        (true, true) => SEL,
        (true, false) => TAG,
        _ => BG,
    }
}
