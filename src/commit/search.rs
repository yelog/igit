use super::*;
use crate::ui::search::{Input, Jump, Prompt};

impl CommitPanel {
    pub(crate) fn focused_search(&self) -> Option<&Prompt> {
        if self.edit.is_some() || self.view_menu.is_some() || self.staging.is_some() {
            return None;
        }
        match self.focus {
            Focus::Tree => Some(self.changes_search.prompt()),
            Focus::Diff
                if self.preview_open
                    && (self.diff_search.prompt().editing()
                        || self.active().and_then(Row::file).is_some()) =>
            {
                Some(self.diff_search.prompt())
            }
            _ => None,
        }
    }

    pub(super) fn search_event(&mut self, event: &Event, c: &Context) -> bool {
        if self.focused_search().is_none() {
            return false;
        }
        let input = match self.focus {
            Focus::Tree => self.changes_search.event(event),
            _ => self.diff_search.event(event),
        };
        match input {
            Input::Pass => false,
            Input::Handled => true,
            Input::Jump(direction) => {
                if self.focus == Focus::Tree {
                    self.find_change(direction, c);
                } else {
                    let current = self.active().is_some_and(|row| row.id == self.diff_key);
                    self.diff_match = self.diff_search.jump(
                        direction,
                        self.diff_anchor.or(self.diff_line_at_top).as_ref(),
                        self.diff_text
                            .split('\n')
                            .enumerate()
                            .filter(|_| current)
                            .map(|(i, line)| (i, [line])),
                    );
                    self.diff_anchor = self.diff_match.as_ref().map(|hit| hit.row);
                }
                true
            }
        }
    }

    fn find_change(&mut self, direction: Jump, c: &Context) {
        let unfolded = (!self.fold.is_empty()).then(|| self.rows_with_folds(c, &BTreeSet::new()));
        let rows = unfolded.as_deref().unwrap_or(&self.view);
        let hit = self.changes_search.jump(
            direction,
            self.view.get(self.cursor).map(|row| &row.id),
            rows.iter().map(|row| (&row.id, [row.search_label()])),
        );
        let Some(hit) = hit else { return };
        if let Some(i) = self.view.iter().position(|r| r.id == hit.row) {
            self.cursor = i;
        } else if let Some(i) = rows.iter().position(|r| r.id == hit.row) {
            let mut depth = rows[i].depth;
            for parent in rows[..i].iter().rev() {
                if parent.depth < depth {
                    self.fold.remove(&parent.id);
                    depth = parent.depth;
                }
            }
            self.sync(c);
            if let Some(i) = self.view.iter().position(|row| row.id == hit.row) {
                self.cursor = i;
            }
        }
    }

    pub(super) fn invalidate_diff_search(&mut self) {
        self.diff_search.invalidate();
        self.diff_match = None;
        self.diff_line_at_top = None;
        self.diff_anchor = None;
    }
}
