//! Branch labels and upstreams use terminal cells, independent of tree depth.
use super::*;
use crate::ui::{
    icons::Icon,
    idea::{clipped, repo_tint, CONFLICT, LOCAL_REF, REMOTE_REF, TAG},
};
use ratatui::{
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

const INDENT: usize = 4;

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

pub(super) fn measure_upstream(snapshot: &Snapshot) -> Option<(usize, usize)> {
    // Include folded local refs so navigation never moves the upstream column.
    let mut has_upstream = false;
    let (mut grouped, mut flat) = (0, 0);
    for branch in snapshot.branches.values().filter(|b| !b.remote) {
        has_upstream |= !branch.upstream.is_empty();
        let depth = 1 + branch.name.matches('/').count();
        grouped = grouped.max(
            depth * INDENT
                + branch.name.rsplit('/').next().unwrap_or("").width()
                + status(branch).width(),
        );
        flat = flat.max(INDENT + branch.name.width() + status(branch).width());
    }
    has_upstream.then_some((grouped, flat))
}

impl BranchPanel {
    fn disclosure_width(&self) -> usize {
        self.icons
            .disclosure(false)
            .width()
            .max(self.icons.disclosure(true).width())
            + 1
    }

    pub(super) fn upstream_column(&self, width: u16) -> Option<u16> {
        if width < 64 {
            return None;
        }
        let (grouped, flat) = self.repository()?.upstream_widths?;
        let natural = if self.tree.grouped() { grouped } else { flat }
            + self.disclosure_width()
            + self.icons.glyph(Icon::Branch).width()
            + 3;
        Some(natural.clamp(28, usize::from(width) * 3 / 5) as u16)
    }

    pub(super) fn draw_repository_row(&self, f: &mut Frame, rect: Rect, index: usize) {
        let repo = &self.repos[index];
        let selected = Some(repo.root) == self.repo_root;
        let focused = self.focused_pane() == PaneId::BranchRepos;
        let bg = selection_bg(selected, focused);
        let icon = format!(" {} ", self.icons.repo(repo.submodule));
        let name = clipped(
            &repo.name,
            usize::from(rect.width).saturating_sub(icon.width()),
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(icon, Style::new().fg(repo_tint(repo.root))),
                Span::styled(name, Style::new().fg(FG).bold()),
            ]))
            .style(Style::new().bg(bg)),
            rect,
        );
    }

    pub(super) fn draw_row(&self, f: &mut Frame, rect: Rect, index: usize, upstream: Option<u16>) {
        let row = &self.tree.rows[index];
        let selected = index == self.tree.cursor;
        let focused = self.focused_pane() == PaneId::Branches;
        let bg = selection_bg(selected, focused);
        f.render_widget(Paragraph::new("").style(Style::new().bg(bg)), rect);
        let width = match &row.kind {
            Kind::Branch(b) if !b.remote => upstream.map_or(rect.width, |x| x.saturating_sub(2)),
            _ => rect.width,
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
                spans.push(Span::styled(
                    clipped(&row.label, available.saturating_sub(tail.width())),
                    Style::new().fg(color).bold(),
                ));
                spans.push(Span::styled(tail, Style::new().fg(secondary)));
            }
            Kind::Branch(branch) => {
                let color = if branch.remote { REMOTE_REF } else { LOCAL_REF };
                spans.push(Span::raw(" ".repeat(slot)));
                spans.push(Span::styled(
                    format!("{} ", self.icons.glyph(Icon::Branch)),
                    Style::new().fg(color),
                ));
                let used: usize = spans.iter().map(Span::width).sum();
                let available = usize::from(width).saturating_sub(used);
                let tail = status(branch);
                let tail = clipped(tail, available.saturating_sub(4));
                spans.push(Span::styled(
                    clipped(&row.label, available.saturating_sub(tail.width())),
                    if branch.current {
                        Style::new().fg(LOCAL_REF).bold()
                    } else {
                        Style::new().fg(FG)
                    },
                ));
                spans.push(Span::styled(
                    tail,
                    Style::new().fg(if !branch.current && !branch.worktree.is_empty() {
                        CONFLICT
                    } else if branch.current {
                        LOCAL_REF
                    } else {
                        MUTED
                    }),
                ));
                if let Some(x) = upstream.filter(|_| !branch.remote) {
                    let divider = Rect::new(rect.x + x - 2, rect.y, 1, 1);
                    f.render_widget(
                        Paragraph::new(self.icons.glyph(Icon::Divider))
                            .style(Style::new().fg(LINE).bg(bg)),
                        divider,
                    );
                    let area = Rect::new(rect.x + x, rect.y, rect.width.saturating_sub(x), 1);
                    self.draw_upstream(f, area, branch, bg);
                }
            }
            Kind::Message => spans.push(Span::styled(
                clipped(&row.label, usize::from(width).saturating_sub(indent)),
                Style::new().fg(MUTED),
            )),
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
            Rect { width, ..rect },
        );
    }

    fn draw_upstream(&self, f: &mut Frame, area: Rect, branch: &Branch, bg: ratatui::style::Color) {
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

    pub(super) fn action_enabled(&self, operation: Operation) -> bool {
        if self.read_status().is_some() {
            return false;
        }
        let Some((root, branch)) = self.selection() else {
            return false;
        };
        if branch.oid.is_empty() {
            return false;
        }
        let locked = !branch.current && !branch.worktree.is_empty();
        match operation {
            Operation::Create => true,
            Operation::Rename => !branch.remote && !locked,
            Operation::Delete => !branch.remote && !branch.current && !locked,
            Operation::Checkout => !branch.current && !locked,
            Operation::Merge => {
                !branch.current
                    && self.repos.iter().find(|r| r.root == root).is_some_and(|r| {
                        r.state
                            .as_ref()
                            .is_ok_and(|s| s.head.is_some() && s.oid.is_some())
                    })
            }
            Operation::Track => branch.remote,
        }
    }
}

fn selection_bg(selected: bool, focused: bool) -> ratatui::style::Color {
    match (selected, focused) {
        (true, true) => SEL,
        (true, false) => TAG,
        _ => BG,
    }
}
