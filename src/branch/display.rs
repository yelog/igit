//! Display options rearrange cached branch rows without querying Git.
use super::*;
use crate::ui::idea::{checkbox, TAG};
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

impl BranchPanel {
    pub(super) fn open_view_menu(&mut self) {
        self.cancel_key_sequence();
        self.focus_pane(PaneId::Branches);
        self.view_menu = true;
        self.view_menu_cursor = 0;
    }

    pub(crate) fn dismiss_view_menu(&mut self) {
        self.compare_menu = None;
        if let Some(comparison) = &mut self.comparison {
            comparison.active_mut().dismiss_filter_dropdown();
        }
        if std::mem::take(&mut self.view_menu) {
            self.hits
                .retain(|(_, hit)| !matches!(hit, Hit::GroupBy | Hit::Sort(_) | Hit::MenuSurface));
        }
    }

    pub(super) fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, hit)| *hit)
    }

    fn apply_view_option(&mut self) {
        let repo = self.repository_index().map(|index| &self.repos[index]);
        match self.view_menu_cursor {
            0 => self.tree.toggle_grouping(repo),
            1 => self.tree.set_sort(Sort::Name, repo),
            _ => self.tree.set_sort(Sort::RecentlyUsed, repo),
        }
        self.branch_search.invalidate();
        self.keep_branch_visible();
        self.hits.retain(|(_, hit)| !matches!(hit, Hit::Row(_)));
    }

    pub(super) fn view_menu_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(k) if k.modifiers.is_empty() => match k.code {
                K::Enter | K::Char(' ') => self.apply_view_option(),
                K::Down | K::Char('j') => self.move_view_option(1),
                K::Up | K::Char('k') => self.move_view_option(-1),
                K::Esc | K::Char('o' | 'h') | K::Left => self.dismiss_view_menu(),
                _ => (),
            },
            Event::Mouse(m) if matches!(m.kind, M::Down(_)) => match self.hit_at(m.column, m.row) {
                Some(hit @ (Hit::GroupBy | Hit::Sort(_)))
                    if m.kind == M::Down(MouseButton::Left) =>
                {
                    self.view_menu_cursor = option_index(hit).unwrap();
                    self.apply_view_option();
                }
                Some(Hit::GroupBy | Hit::Sort(_) | Hit::MenuSurface) => (),
                Some(Hit::ViewOptions) => self.dismiss_view_menu(),
                _ => {
                    self.dismiss_view_menu();
                    return false;
                }
            },
            Event::Mouse(m) => match m.kind {
                M::ScrollDown => self.move_view_option(1),
                M::ScrollUp => self.move_view_option(-1),
                M::Moved => {
                    if let Some(index) = self.hit_at(m.column, m.row).and_then(option_index) {
                        self.view_menu_cursor = index;
                    }
                }
                _ => (),
            },
            Event::FocusLost => self.dismiss_view_menu(),
            _ => (),
        }
        true
    }

    fn move_view_option(&mut self, delta: isize) {
        self.view_menu_cursor = (self.view_menu_cursor as isize + delta).rem_euclid(3) as usize;
    }

    pub(super) fn draw_view_menu(&mut self, f: &mut Frame, area: Rect) {
        if !self.view_menu || area.height < 3 || area.width < 3 {
            return;
        }
        let Some(anchor) = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::ViewOptions).then_some(*r))
        else {
            return;
        };
        let options = [
            ("Branch Prefix (/)", self.tree.grouped(), Hit::GroupBy),
            (
                "Name",
                self.tree.sort() == Sort::Name,
                Hit::Sort(Sort::Name),
            ),
            (
                "Recently Used",
                self.tree.sort() == Sort::RecentlyUsed,
                Hit::Sort(Sort::RecentlyUsed),
            ),
        ];
        let width = (checkbox(self.icons, true).width() + options[0].0.width() + 4)
            .min(usize::from(area.width)) as u16;
        let height = 6.min(area.height);
        let popup = Rect::new(
            anchor.right().saturating_sub(width).max(area.x),
            anchor.bottom().min(area.bottom() - height),
            width,
            height,
        );
        f.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(LINE))
            .style(Style::new().fg(FG).bg(TAG))
            .title(" View Options ");
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        self.hits.push((popup, Hit::MenuSurface));
        let selected = [0usize, 2, 3][self.view_menu_cursor];
        let top = (selected + 1).saturating_sub(usize::from(inner.height));
        for position in top..4.min(top + usize::from(inner.height)) {
            let row = Rect::new(inner.x, inner.y + (position - top) as u16, inner.width, 1);
            if position == 1 {
                f.render_widget(
                    Paragraph::new(" Sort By").style(Style::new().fg(MUTED)),
                    row,
                );
                continue;
            }
            let index = if position == 0 { 0 } else { position - 1 };
            let (label, checked, hit) = options[index];
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    checkbox(self.icons, checked),
                    Span::raw(format!(" {label}")),
                ]))
                .style(Style::new().fg(FG).bg(
                    if index == self.view_menu_cursor {
                        SEL
                    } else {
                        TAG
                    },
                )),
                row,
            );
            self.hits.push((row, hit));
        }
    }
}

fn option_index(hit: Hit) -> Option<usize> {
    match hit {
        Hit::GroupBy => Some(0),
        Hit::Sort(Sort::Name) => Some(1),
        Hit::Sort(Sort::RecentlyUsed) => Some(2),
        _ => None,
    }
}
