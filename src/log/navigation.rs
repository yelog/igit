//! Commit-list motion uses the last rendered height, including toolbar/error rows.
use super::*;
use crate::ui::key_sequence::KeySequence;
use crossterm::event::KeyEvent;

impl LogPanel {
    pub(crate) fn begin_filter_prefix_on_space(&mut self) -> bool {
        if self.key_prefix.is_some()
            || self.editing()
            || self.workspace_comparison.is_some()
            || self.branch_menu.is_some()
            || self.path_dialog.is_some()
            || self.date_range.is_some()
            || self.history_diff.is_some()
            || self.pick_mode && self.focus == Pane::Graph
        {
            return false;
        }
        self.key_prefix = Some(' ');
        true
    }

    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        if self.editing() {
            return None;
        }
        if let Some(comparison) = &self.workspace_comparison {
            return comparison.panel.key_sequence();
        }
        if let Some(view) = self.diff.as_ref().and_then(|diff| diff.comparison.as_ref()) {
            if let Some(sequence) = view.key_sequence() {
                return Some(sequence);
            }
        }
        if let Some(view) = self
            .history_diff
            .as_ref()
            .and_then(|diff| diff.comparison.as_ref())
        {
            if let Some(sequence) = view.key_sequence() {
                return Some(sequence);
            }
        }
        match self.key_prefix {
            Some(' ') => Some(KeySequence::LogFilters),
            Some('g') if self.focus == Pane::Graph => Some(KeySequence::LogGoto),
            Some('g') => Some(KeySequence::FirstItem),
            Some('y') if self.focus == Pane::Graph => Some(KeySequence::CommitCopy),
            _ => None,
        }
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.key_prefix = None;
        if let Some(comparison) = &mut self.workspace_comparison {
            comparison.panel.cancel_key_sequence();
        }
        if let Some(view) = self.diff.as_mut().and_then(|diff| diff.comparison.as_mut()) {
            view.cancel_key_sequence();
        }
        if let Some(view) = self
            .history_diff
            .as_mut()
            .and_then(|diff| diff.comparison.as_mut())
        {
            view.cancel_key_sequence();
        }
    }

    pub(super) fn filter_key(&mut self, key: &KeyEvent, c: &Context) -> Result<bool> {
        if self.key_prefix == Some(' ') {
            self.cancel_key_sequence();
            let field = match (key.code, key.modifiers) {
                (K::Char('b'), Mods::NONE) => Some(Field::Branch),
                (K::Char('u'), Mods::NONE) => Some(Field::Author),
                (K::Char('d'), Mods::NONE) => Some(Field::Date),
                (K::Char('p'), Mods::NONE) => Some(Field::Path),
                _ => None,
            };
            if let Some(field) = field {
                self.begin(field, c)?;
                return Ok(true);
            }
        }
        if key.code == K::Char(' ') && key.modifiers.is_empty() {
            self.key_prefix = Some(' ');
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn keep_graph_cursor_visible(&mut self) {
        if self.graph_height == 0 {
            return;
        }
        self.graph_top = self
            .graph_top
            .min(self.cursor)
            .min(self.visible.len().saturating_sub(self.graph_height))
            .max((self.cursor + 1).saturating_sub(self.graph_height));
    }

    pub(super) fn first_item_key(&mut self, key: &KeyEvent) -> bool {
        if !key.modifiers.is_empty() && key.modifiers != Mods::SHIFT {
            self.cancel_key_sequence();
            return false;
        }
        if key.code == K::Char('g') {
            if self.key_prefix == Some('g') {
                self.key_prefix = None;
                self.mv(-1_000_000);
            } else {
                self.key_prefix = Some('g');
            }
            return true;
        }
        if self.key_prefix == Some('g') {
            self.key_prefix = None;
        }
        false
    }

    fn page_graph(&mut self, down: bool, half: bool) {
        if self.graph_height == 0 || self.visible.is_empty() {
            return;
        }
        let rows = if half {
            (self.graph_height / 2).max(1)
        } else {
            self.graph_height
        } as isize;
        let delta = if down { rows } else { -rows };
        self.graph_top = self
            .graph_top
            .saturating_add_signed(delta)
            .min(self.visible.len().saturating_sub(self.graph_height));
        self.select_row(self.cursor.saturating_add_signed(delta));
    }

    fn graph_screen_row(&mut self, key: char) {
        let count = self
            .graph_height
            .min(self.visible.len().saturating_sub(self.graph_top));
        if count == 0 {
            return;
        }
        self.select_row(
            self.graph_top
                + match key {
                    'H' => 0,
                    'M' => (count - 1) / 2,
                    _ => count - 1,
                },
        );
    }

    pub(super) fn graph_key(&mut self, k: &KeyEvent, c: &Context) -> Result<Option<Action>> {
        if self.focus != Pane::Graph {
            self.cancel_key_sequence();
            return Ok(None);
        }
        if k.modifiers == Mods::CONTROL {
            self.cancel_key_sequence();
            match k.code {
                K::Char('f' | 'b' | 'd' | 'u') => {
                    self.page_graph(
                        matches!(k.code, K::Char('f' | 'd')),
                        matches!(k.code, K::Char('d' | 'u')),
                    );
                    return Ok(Some(Action::None));
                }
                _ => return Ok(None),
            }
        }
        if !k.modifiers.is_empty() && k.modifiers != Mods::SHIFT {
            self.cancel_key_sequence();
            return Ok(None);
        }
        match (self.key_prefix.take(), k.code) {
            (Some('g'), K::Char('g')) => {
                self.select_row(0);
                return Ok(Some(Action::None));
            }
            (Some('y'), K::Char(ch @ ('y' | 'm' | 'a'))) => {
                return self.yank_commit(ch, c).map(Some);
            }
            _ => {}
        }
        match k.code {
            K::Char(ch @ ('g' | 'y')) => self.key_prefix = Some(ch),
            K::Char('G') => self.select_row(self.visible.len().saturating_sub(1)),
            K::Char(ch @ ('H' | 'M' | 'L')) => self.graph_screen_row(ch),
            K::PageDown => self.page_graph(true, false),
            K::PageUp => self.page_graph(false, false),
            _ => return Ok(None),
        }
        Ok(Some(Action::None))
    }

    fn yank_commit(&self, key: char, c: &Context) -> Result<Action> {
        let Some(rev) = self.rev() else {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Log: no commit selected".into(),
            ));
        };
        crate::ui::commit_copy::copy(
            &c.repos[rev.root].path,
            &rev.oid,
            Some((&rev.author, &rev.email)),
            key,
        )
    }
}
