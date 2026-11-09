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
    }

    pub(crate) fn dismiss_view_menu(&mut self) {
        if std::mem::take(&mut self.view_menu) {
            self.hits
                .retain(|(_, hit)| !matches!(hit, Hit::GroupBy | Hit::MenuSurface));
        }
    }

    pub(super) fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, hit)| *hit)
    }

    fn toggle_grouping(&mut self) {
        let repo = self.repository_index().map(|index| &self.repos[index]);
        self.tree.toggle_grouping(repo);
        self.branch_search.invalidate();
        self.keep_branch_visible();
        self.hits
            .retain(|(_, hit)| !matches!(hit, Hit::Row(_) | Hit::Operation(_)));
    }

    pub(super) fn view_menu_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(k) if k.modifiers.is_empty() => match k.code {
                K::Enter | K::Char(' ') => self.toggle_grouping(),
                K::Esc | K::Char('o' | 'h') | K::Left => self.dismiss_view_menu(),
                _ => (),
            },
            Event::Mouse(m) if matches!(m.kind, M::Down(_)) => match self.hit_at(m.column, m.row) {
                Some(Hit::GroupBy) if m.kind == M::Down(MouseButton::Left) => {
                    self.toggle_grouping()
                }
                Some(Hit::GroupBy | Hit::MenuSurface) => (),
                Some(Hit::ViewOptions) => self.dismiss_view_menu(),
                _ => {
                    self.dismiss_view_menu();
                    return false;
                }
            },
            Event::FocusLost => self.dismiss_view_menu(),
            _ => (),
        }
        true
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
        let mark = checkbox(self.icons, self.tree.grouped());
        let label = "Branch Prefix (/)";
        let width = (mark.width() + label.width() + 4).min(usize::from(area.width)) as u16;
        let popup = Rect::new(
            anchor.right().saturating_sub(width).max(area.x),
            anchor.bottom().min(area.bottom() - 3),
            width,
            3,
        );
        f.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(LINE))
            .style(Style::new().fg(FG).bg(TAG))
            .title(" Group By ");
        let row = block.inner(popup);
        f.render_widget(block, popup);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                mark,
                Span::raw(format!(" {label}")),
            ]))
            .style(Style::new().fg(FG).bg(SEL)),
            row,
        );
        self.hits.push((popup, Hit::MenuSurface));
        self.hits.push((row, Hit::GroupBy));
    }
}
