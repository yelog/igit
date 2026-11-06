use super::model::{Branch, Repository};
use std::collections::{BTreeMap, BTreeSet};

pub(super) enum Kind {
    Folder { open: bool },
    Branch(Branch),
    Message,
}

pub(super) struct Row {
    pub id: String,
    pub root: usize,
    pub depth: usize,
    pub label: String,
    pub kind: Kind,
}

#[derive(Default)]
pub(super) struct Tree {
    pub rows: Vec<Row>,
    pub cursor: usize,
    closed: BTreeSet<String>,
}

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    branch: Option<Branch>,
    current: bool,
}

impl Node {
    fn insert(&mut self, branch: &Branch) {
        let mut node = self;
        for part in branch.name.split('/') {
            node.current |= branch.current;
            node = node.children.entry(part.into()).or_default();
        }
        node.current |= branch.current;
        node.branch = Some(branch.clone());
    }
}

impl Tree {
    pub fn rebuild(&mut self, repos: &[Repository]) {
        let previous = self.rows.get(self.cursor).map(|r| r.id.clone());
        self.rows.clear();
        for repo in repos {
            let root = repo.root;
            let id = format!("{root}:repo");
            if !self.folder(root, &id, &repo.name, 0) {
                continue;
            }
            match &repo.state {
                Ok(state) => {
                    for remote in [false, true] {
                        let prefix = if remote {
                            "refs/remotes/"
                        } else {
                            "refs/heads/"
                        };
                        let id = format!("{root}:{prefix}");
                        let label = if remote {
                            "Remote Branches"
                        } else {
                            "Local Branches"
                        };
                        if !self.folder(root, &id, label, 1) {
                            continue;
                        }
                        let mut tree = Node::default();
                        for branch in state.branches.values().filter(|b| b.remote == remote) {
                            tree.insert(branch);
                        }
                        if tree.children.is_empty() {
                            self.rows.push(Row {
                                id: format!("{id}empty"),
                                root,
                                depth: 2,
                                label: "No branches".into(),
                                kind: Kind::Message,
                            });
                        } else {
                            self.children(root, &id, &tree, 2);
                        }
                    }
                }
                Err(error) => self.rows.push(Row {
                    id: format!("{root}:error"),
                    root,
                    depth: 1,
                    label: error.clone(),
                    kind: Kind::Message,
                }),
            }
        }
        self.cursor = previous
            .as_ref()
            .and_then(|id| self.rows.iter().position(|r| &r.id == id))
            .unwrap_or_else(|| {
                if previous.is_none() {
                    self.rows
                        .iter()
                        .position(|r| matches!(r.kind, Kind::Branch(_)))
                        .unwrap_or(0)
                } else {
                    self.cursor.min(self.rows.len().saturating_sub(1))
                }
            });
    }

    fn folder(&mut self, root: usize, id: &str, label: &str, depth: usize) -> bool {
        let open = !self.closed.contains(id);
        self.rows.push(Row {
            id: id.into(),
            root,
            depth,
            label: label.into(),
            kind: Kind::Folder { open },
        });
        open
    }

    fn children(&mut self, root: usize, prefix: &str, node: &Node, depth: usize) {
        // Pin every ancestor, so a slash-named current branch is still the first leaf.
        for current in [true, false] {
            for (name, child) in node.children.iter().filter(|(_, n)| n.current == current) {
                if let Some(branch) = &child.branch {
                    self.rows.push(Row {
                        id: format!("{root}:{}", branch.reference),
                        root,
                        depth,
                        label: name.clone(),
                        kind: Kind::Branch(branch.clone()),
                    });
                } else {
                    let id = format!("{prefix}{name}/");
                    if self.folder(root, &id, name, depth) {
                        self.children(root, &id, child, depth + 1);
                    }
                }
            }
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        self.cursor = self
            .cursor
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
    }

    pub fn toggle(&mut self, repos: &[Repository]) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if let Kind::Folder { open } = row.kind {
            if open {
                self.closed.insert(row.id.clone());
            } else {
                self.closed.remove(&row.id);
            }
            self.rebuild(repos);
        }
    }

    pub fn horizontal(&mut self, left: bool, repos: &[Repository]) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if matches!(row.kind, Kind::Folder { open } if open == left) {
            self.toggle(repos);
        } else if left {
            let depth = row.depth;
            if let Some(parent) = self.rows[..self.cursor]
                .iter()
                .rposition(|r| r.depth < depth)
            {
                self.cursor = parent;
            }
        } else if matches!(row.kind, Kind::Folder { open: true }) {
            let depth = row.depth;
            if self
                .rows
                .get(self.cursor + 1)
                .is_some_and(|r| r.depth > depth)
            {
                self.cursor += 1;
            }
        }
    }
}
