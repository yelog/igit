use super::*;
use crate::ui::search::{Input, Jump, Prompt};
use std::borrow::Cow;

impl Kind {
    pub(super) fn label<'a>(&'a self, files: &[Change], repo: &Repo) -> Cow<'a, str> {
        match self {
            Self::File(i) => {
                let path = &files[*i].path;
                Cow::Owned(label(path.file_name().unwrap_or(path.as_os_str()).as_ref()))
            }
            Self::Repository => Cow::Owned(label(Path::new(&repo.name))),
            Self::Module => Cow::Owned(label(
                repo.path
                    .file_name()
                    .unwrap_or(repo.path.as_os_str())
                    .as_ref(),
            )),
            Self::Directory { name, .. } => Cow::Borrowed(name),
        }
    }
}

impl Entry {
    pub(super) fn target_label(&self) -> String {
        if self.error.is_empty() {
            format!("{}:{}", self.t.remote, self.t.dst)
        } else {
            String::new()
        }
    }
}

impl PushPanel {
    pub(crate) fn focused_search(&self) -> Option<&Prompt> {
        if self.diff.is_some() || self.target_edit.is_some() || self.group_menu.is_some() {
            return None;
        }
        match self.focused_pane() {
            PaneId::Outgoing => Some(self.outgoing_search.prompt()),
            PaneId::PushFiles => Some(self.file_search.prompt()),
            _ => None,
        }
    }

    pub(super) fn search_event(&mut self, e: &Event, c: &Context) -> bool {
        if self.focused_search().is_none() {
            return false;
        }
        let outgoing = self.focused_pane() == PaneId::Outgoing;
        let input = if outgoing {
            self.outgoing_search.event(e)
        } else {
            self.file_search.event(e)
        };
        match input {
            Input::Pass => false,
            Input::Handled => true,
            Input::Jump(direction) => {
                if outgoing {
                    self.find_outgoing(direction, c);
                } else {
                    self.find_file(direction, c);
                }
                true
            }
        }
    }

    fn find_outgoing(&mut self, direction: Jump, c: &Context) {
        let selected = self.selected();
        let hit = self.outgoing_search.jump(
            direction,
            selected.as_ref(),
            self.entries.iter().flat_map(|e| {
                let repo = label(Path::new(&c.repos[e.idx].name));
                let source = if e.error.is_empty() {
                    e.source.as_str()
                } else {
                    ""
                };
                std::iter::once((
                    (e.idx, None),
                    [
                        Cow::Owned(repo),
                        Cow::Borrowed(source),
                        Cow::Owned(e.target_label()),
                    ],
                ))
                .chain(e.logs.iter().enumerate().map(|(i, commit)| {
                    (
                        (e.idx, Some(i)),
                        [
                            Cow::Borrowed(commit.msg.as_str()),
                            Cow::Borrowed(""),
                            Cow::Borrowed(""),
                        ],
                    )
                }))
            }),
        );
        if let Some(hit) = hit {
            let (root, commit) = hit.row;
            if commit.is_some() {
                self.expanded.insert(root);
            }
            if let Some(i) = self.rows().iter().position(|&row| row == hit.row) {
                self.pos = i;
                if selected != Some(hit.row) {
                    self.inspect(c);
                }
            }
        }
    }

    fn find_file(&mut self, direction: Jump, c: &Context) {
        let repo = self.selected().and_then(|(root, _)| c.repos.get(root));
        let tree = &mut self.preview.tree;
        let files = &self.preview.files;
        let hit = self.file_search.jump(
            direction,
            tree.visible.get(tree.cursor),
            repo.into_iter().flat_map(|repo| {
                tree.order()
                    .iter()
                    .map(|&id| (id, [tree.nodes[id].kind.label(files, repo)]))
            }),
        );
        if let Some(hit) = hit {
            tree.reveal(hit.row);
            if self.show_details {
                self.update_details();
            }
        }
    }
}
