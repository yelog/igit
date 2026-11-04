//! Commit-list motion uses the last rendered height, including toolbar/error rows.
use super::*;
use crate::ui::key_sequence::KeySequence;
use crossterm::event::KeyEvent;

impl LogPanel {
    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        if self.focus != Pane::Graph || self.editing() {
            return None;
        }
        match self.key_prefix {
            Some('g') => Some(KeySequence::LogGoto),
            Some('y') => Some(KeySequence::LogYank),
            _ => None,
        }
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.key_prefix = None;
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
            return Ok(Action::Notice("Log: 没有选中的提交".into()));
        };
        let (text, label) = match key {
            'y' => (rev.oid.clone(), "commit ID"),
            'm' => (
                crate::core::git(
                    &c.repos[rev.root].path,
                    &[
                        "show",
                        "-s",
                        "--no-show-signature",
                        "--no-notes",
                        "--format=%B",
                        &rev.oid,
                        "--",
                    ],
                )?,
                "提交信息",
            ),
            _ => (format!("{} <{}>", rev.author, rev.email), "作者信息"),
        };
        Ok(Action::Copy { text, label })
    }
}
