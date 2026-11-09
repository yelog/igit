use super::*;
use crate::ui::search::{Input, Prompt};

impl StashPanel {
    pub(crate) fn focused_search(&self) -> Option<&Prompt> {
        if self.files.editing() {
            return None;
        }
        Some(if self.focus == PaneId::Files {
            self.files.search_prompt()
        } else {
            self.stash_search.prompt()
        })
    }

    pub(super) fn empty_group_label(&self) -> String {
        let count = self.repos.iter().filter(|repo| repo.is_empty()).count();
        let noun = if count == 1 { "repo" } else { "repos" };
        if count == self.repos.len() {
            format!("No stashes · {count} {noun}")
        } else {
            format!("{count} {noun} without stashes")
        }
    }

    pub(super) fn stash_search_event(&mut self, e: &Event, c: &Context) -> bool {
        let direction = match self.stash_search.event(e) {
            Input::Pass => return false,
            Input::Handled => return true,
            Input::Jump(direction) => direction,
        };
        let rows = self.rows_with_folds(true);
        let empty_label = self.empty_group_label();
        let repos = &self.repos;
        let hit = self.stash_search.jump(
            direction,
            self.rows.get(self.cursor),
            rows.into_iter().map(|row| {
                let label = match row {
                    Row::Stash { root, entry } => std::borrow::Cow::Borrowed(
                        repos.iter().find(|r| r.root == root).unwrap().entries[entry].message(),
                    ),
                    Row::Repository(root) => {
                        std::borrow::Cow::Owned(model::label(&c.repos[root].name))
                    }
                    Row::EmptyGroup => std::borrow::Cow::Borrowed(empty_label.as_str()),
                };
                (row, [label])
            }),
        );
        if let Some(hit) = hit {
            if let Some(i) = self.row_index(hit.row) {
                self.select(i);
            } else {
                match hit.row {
                    Row::Stash { root, .. } => {
                        self.folded.remove(&root);
                    }
                    Row::Repository(_) => self.empty_expanded = true,
                    Row::EmptyGroup => (),
                }
                self.rebuild();
                if let Some(i) = self.row_index(hit.row) {
                    self.cursor = i;
                    self.request_preview(preview::NAV_SETTLE);
                }
            }
        }
        true
    }
}
