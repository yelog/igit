//! Each list's motions use its usable height, excluding controls and details.
use super::*;
use crate::ui::key_sequence::KeySequence;
use crossterm::event::KeyEvent;
use std::ops::Range;

#[derive(Default)]
pub(super) struct Viewport {
    pub area: Rect,
    pub top: usize,
}

impl Viewport {
    pub fn reveal(&mut self, cursor: usize, len: usize) {
        let height = usize::from(self.area.height);
        if height > 0 {
            self.top = self
                .top
                .min(cursor)
                .min(len.saturating_sub(height))
                .max((cursor + 1).saturating_sub(height));
        }
    }

    pub fn visible(&self, len: usize) -> Range<usize> {
        self.top..len.min(self.top + usize::from(self.area.height))
    }

    fn move_cursor(&mut self, cursor: usize, len: usize, motion: Motion) -> usize {
        let height = usize::from(self.area.height);
        let next = match motion {
            Motion::Move(delta) => cursor.saturating_add_signed(delta),
            Motion::Page { down, half } if height > 0 && len > 0 => {
                let rows = if half { (height / 2).max(1) } else { height } as isize;
                let delta = if down { rows } else { -rows };
                self.top = self
                    .top
                    .saturating_add_signed(delta)
                    .min(len.saturating_sub(height));
                cursor.saturating_add_signed(delta)
            }
            Motion::Screen(position) if !self.visible(len).is_empty() => {
                let count = self.visible(len).len();
                self.top
                    + match position {
                        'H' => 0,
                        'M' => (count - 1) / 2,
                        _ => count - 1,
                    }
            }
            _ => cursor,
        }
        .min(len.saturating_sub(1));
        self.reveal(next, len);
        next
    }
}

pub(super) enum Motion {
    Move(isize),
    Page { down: bool, half: bool },
    Screen(char),
}

impl BranchPanel {
    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        (self.goto_pending && self.input.is_none()).then_some(KeySequence::BranchGoto)
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.goto_pending = false;
    }

    pub(super) fn keep_branch_visible(&mut self) {
        self.viewport.reveal(self.tree.cursor, self.tree.rows.len());
    }

    pub(super) fn navigate(&mut self, motion: Motion) {
        if self.focused_pane() == PaneId::BranchRepos {
            if let Some(cursor) = self.repository_index() {
                let next = self
                    .repo_viewport
                    .move_cursor(cursor, self.repos.len(), motion);
                self.select_repository(next);
            }
        } else {
            self.tree.cursor =
                self.viewport
                    .move_cursor(self.tree.cursor, self.tree.rows.len(), motion);
        }
    }

    pub(super) fn navigation_key(&mut self, key: &KeyEvent) -> bool {
        let pending = std::mem::take(&mut self.goto_pending);
        let motion = if key.modifiers == Mods::CONTROL {
            match key.code {
                K::Char(ch @ ('f' | 'b' | 'd' | 'u')) => Motion::Page {
                    down: matches!(ch, 'f' | 'd'),
                    half: matches!(ch, 'd' | 'u'),
                },
                _ => return false,
            }
        } else if key.modifiers.is_empty() || key.modifiers == Mods::SHIFT {
            match key.code {
                K::Char('g') if pending => Motion::Move(isize::MIN),
                K::Char('g') => {
                    self.goto_pending = true;
                    return true;
                }
                K::Home => Motion::Move(isize::MIN),
                K::Char('G') | K::End => Motion::Move(isize::MAX),
                K::Char('j') | K::Down => Motion::Move(1),
                K::Char('k') | K::Up => Motion::Move(-1),
                K::Char(ch @ ('H' | 'M' | 'L')) => Motion::Screen(ch),
                K::PageDown | K::PageUp => Motion::Page {
                    down: key.code == K::PageDown,
                    half: false,
                },
                _ => return false,
            }
        } else {
            return false;
        };
        self.navigate(motion);
        true
    }
}
