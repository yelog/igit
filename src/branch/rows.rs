//! Branch labels and upstreams use terminal cells, independent of tree depth.
use super::*;
use crate::ui::{
    icons::Icon,
    idea::{clipped, CONFLICT, LOCAL_REF, REMOTE_REF, TAG},
};
use ratatui::{
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

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

pub(super) fn measure_upstream(repos: &[Repository]) -> Option<usize> {
    // Include folded local refs so navigation never moves the upstream column.
    let mut has_upstream = false;
    let mut widest = 0;
    for branch in repos
        .iter()
        .filter_map(|r| r.state.as_ref().ok())
        .flat_map(|s| s.branches.values())
        .filter(|b| !b.remote)
    {
        has_upstream |= !branch.upstream.is_empty();
        let depth = 2 + branch.name.matches('/').count();
        widest = widest.max(
            depth * 2
                + 6
                + branch.name.rsplit('/').next().unwrap_or("").width()
                + status(branch).width(),
        );
    }
    has_upstream.then_some(widest)
}

impl BranchPanel {
    pub(super) fn upstream_column(&self, width: u16) -> Option<u16> {
        if width < 64 {
            return None;
        }
        let natural = self.upstream_width? + self.icons.glyph(Icon::Branch).width();
        Some(natural.clamp(28, usize::from(width) * 3 / 5) as u16)
    }

    pub(super) fn draw_row(&self, f: &mut Frame, rect: Rect, index: usize, upstream: Option<u16>) {
        let row = &self.tree.rows[index];
        let selected = index == self.tree.cursor;
        let bg = if selected {
            SEL
        } else if row.depth == 0 {
            TAG
        } else {
            BG
        };
        f.render_widget(Paragraph::new("").style(Style::new().bg(bg)), rect);
        let width = match &row.kind {
            Kind::Branch(b) if !b.remote => upstream.map_or(rect.width, |x| x.saturating_sub(2)),
            _ => rect.width,
        };
        let indent = (row.depth * 2).min(usize::from(width.saturating_sub(18)));
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        match &row.kind {
            Kind::Folder { open, count } => {
                // Repository identity is structural; reserve hue for branch type.
                let color = if row.depth == 1 {
                    if row.id.ends_with("refs/remotes/") {
                        REMOTE_REF
                    } else {
                        LOCAL_REF
                    }
                } else {
                    FG
                };
                let secondary = if selected { FG } else { MUTED };
                spans.push(Span::styled(
                    format!("{} ", self.icons.disclosure(*open)),
                    Style::new().fg(secondary),
                ));
                if row.depth == 0 {
                    spans.push(Span::styled(
                        format!("{} ", self.icons.glyph(Icon::Repo)),
                        Style::new().fg(secondary),
                    ));
                }
                let tail = if row.depth == 0 {
                    self.repos
                        .iter()
                        .find(|r| r.root == row.root)
                        .and_then(|r| r.state.as_ref().ok())
                        .map(|s| format!("  [{}]", s.head_label()))
                        .unwrap_or_default()
                } else {
                    format!("  {count}")
                };
                let used: usize = spans.iter().map(Span::width).sum();
                let available = usize::from(width).saturating_sub(used);
                // Preserve a useful repository label even for a very long HEAD.
                let tail = clipped(&tail, available.saturating_sub(row.label.width().min(16)));
                spans.push(Span::styled(
                    clipped(&row.label, available.saturating_sub(tail.width())),
                    Style::new().fg(color).bold(),
                ));
                spans.push(Span::styled(tail, Style::new().fg(secondary)));
            }
            Kind::Branch(branch) => {
                let color = if branch.remote { REMOTE_REF } else { LOCAL_REF };
                spans.push(Span::styled(
                    format!("  {} ", self.icons.glyph(Icon::Branch)),
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
