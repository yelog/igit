//! Motions use the usable tree height, excluding controls and details.
use super::*;
use crate::ui::key_sequence::KeySequence;
use crossterm::event::KeyEvent;

impl BranchPanel {
    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        (self.goto_pending && self.input.is_none()).then_some(KeySequence::BranchGoto)
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.goto_pending = false;
    }

    pub(super) fn keep_cursor_visible(&mut self) {
        let height = usize::from(self.viewport.height);
        if height == 0 {
            return;
        }
        self.top = self
            .top
            .min(self.tree.cursor)
            .min(self.tree.rows.len().saturating_sub(height))
            .max((self.tree.cursor + 1).saturating_sub(height));
    }

    fn page(&mut self, down: bool, half: bool) {
        let height = usize::from(self.viewport.height);
        if height == 0 || self.tree.rows.is_empty() {
            return;
        }
        let rows = if half { (height / 2).max(1) } else { height } as isize;
        let delta = if down { rows } else { -rows };
        self.top = self
            .top
            .saturating_add_signed(delta)
            .min(self.tree.rows.len().saturating_sub(height));
        self.tree.move_by(delta);
    }

    fn screen_row(&mut self, key: char) {
        let count =
            usize::from(self.viewport.height).min(self.tree.rows.len().saturating_sub(self.top));
        if count > 0 {
            self.tree.cursor = self.top
                + match key {
                    'H' => 0,
                    'M' => (count - 1) / 2,
                    _ => count - 1,
                };
        }
    }

    pub(super) fn navigation_key(&mut self, key: &KeyEvent) -> bool {
        let pending = std::mem::take(&mut self.goto_pending);
        if key.modifiers == Mods::CONTROL {
            match key.code {
                K::Char(ch @ ('f' | 'b' | 'd' | 'u')) => {
                    self.page(matches!(ch, 'f' | 'd'), matches!(ch, 'd' | 'u'));
                }
                _ => return false,
            }
        } else if key.modifiers.is_empty() || key.modifiers == Mods::SHIFT {
            match key.code {
                K::Char('g') if pending => self.tree.move_by(isize::MIN),
                K::Char('g') => self.goto_pending = true,
                K::Home => self.tree.move_by(isize::MIN),
                K::Char('G') | K::End => self.tree.move_by(isize::MAX),
                K::Char('j') | K::Down => self.tree.move_by(1),
                K::Char('k') | K::Up => self.tree.move_by(-1),
                K::Char(ch @ ('H' | 'M' | 'L')) => self.screen_row(ch),
                K::PageDown => self.page(true, false),
                K::PageUp => self.page(false, false),
                _ => return false,
            }
        } else {
            return false;
        }
        self.keep_cursor_visible();
        true
    }
}
