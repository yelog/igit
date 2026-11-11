use super::*;
use crate::ui::search::{Input, Prompt};
use std::borrow::Cow;

impl BranchPanel {
    pub(crate) fn focused_search(&self) -> Option<&Prompt> {
        if let Some(comparison) = &self.comparison {
            return comparison.active().focused_search();
        }
        if self.input.is_some() || self.view_menu || self.compare_menu.is_some() {
            return None;
        }
        Some(if self.searching_repositories() {
            self.repo_search.prompt()
        } else {
            self.branch_search.prompt()
        })
    }

    fn searching_repositories(&self) -> bool {
        self.repo_search.prompt().editing() || self.focused_pane() == PaneId::BranchRepos
    }

    pub(super) fn search_event(&mut self, event: &Event) -> bool {
        if self.focused_search().is_none() {
            return false;
        }
        if self.searching_repositories() {
            match self.repo_search.event(event) {
                Input::Pass => return false,
                Input::Handled => (),
                Input::Jump(direction) => {
                    let hit = self.repo_search.jump(
                        direction,
                        self.repo_root.as_ref(),
                        self.repos.iter().map(|repo| {
                            let head = repo
                                .state
                                .as_ref()
                                .map_or_else(|_| String::new(), Snapshot::head_label);
                            (
                                repo.root,
                                [Cow::Borrowed(repo.name.as_str()), Cow::Owned(head)],
                            )
                        }),
                    );
                    if let Some(index) =
                        hit.and_then(|hit| self.repos.iter().position(|repo| repo.root == hit.row))
                    {
                        self.select_repository(index);
                    }
                }
            }
        } else {
            match self.branch_search.event(event) {
                Input::Pass => return false,
                Input::Handled => (),
                Input::Jump(direction) => {
                    let index = self.repository_index();
                    let repo = index.map(|i| &self.repos[i]);
                    let unfolded = self.tree.unfolded_rows(repo);
                    let hit = self.branch_search.jump(
                        direction,
                        self.tree.rows.get(self.tree.cursor).map(|row| &row.id),
                        unfolded
                            .as_deref()
                            .unwrap_or(&self.tree.rows)
                            .iter()
                            .map(|row| (&row.id, [&row.label])),
                    );
                    if let Some(hit) = hit {
                        self.tree.reveal(&hit.row, repo);
                        self.keep_branch_visible();
                    }
                }
            }
        }
        true
    }
}
