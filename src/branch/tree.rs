use super::{
    model::{Branch, Repository},
    recent::LastUsed,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

pub(super) enum Kind {
    Folder { open: bool, count: usize },
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
    flat: bool,
    sort: Sort,
    collapsed: BTreeSet<String>,
    remembered: BTreeMap<usize, (String, usize)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
    Name,
    #[default]
    RecentlyUsed,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Current,
    Default,
    #[default]
    Other,
}

impl Priority {
    fn of(branch: &Branch, default: Option<&str>) -> Self {
        if branch.current {
            Self::Current
        } else if Some(branch.reference.as_str()) == default {
            Self::Default
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Order {
    priority: Priority,
    recent: Reverse<Option<LastUsed>>,
}

impl Sort {
    fn order(self, branch: &Branch, repo: &Repository) -> Order {
        let recent = self == Self::RecentlyUsed && !branch.remote;
        Order {
            priority: Priority::of(
                branch,
                repo.metadata.default_branch.as_deref().filter(|_| !recent),
            ),
            recent: Reverse(
                recent
                    .then(|| repo.metadata.last_used.get(&branch.reference).copied())
                    .flatten(),
            ),
        }
    }
}

#[derive(Default)]
struct Node<'a> {
    children: BTreeMap<&'a str, Node<'a>>,
    branch: Option<&'a Branch>,
    order: Order,
    count: usize,
}

impl<'a> Node<'a> {
    fn insert(&mut self, branch: &'a Branch, order: Order) {
        let mut node = self;
        for part in branch.name.split('/') {
            node.order = node.order.min(order);
            node.count += 1;
            node = node.children.entry(part).or_default();
        }
        node.order = node.order.min(order);
        node.count += 1;
        node.branch = Some(branch);
    }
}

fn namespace(remote: bool) -> &'static str {
    if remote {
        "refs/remotes/"
    } else {
        "refs/heads/"
    }
}

impl Tree {
    pub fn unfolded_rows(&self, repo: Option<&Repository>) -> Option<Vec<Row>> {
        if self.collapsed.is_empty() {
            return None;
        }
        let mut tree = Self {
            flat: self.flat,
            sort: self.sort,
            ..Self::default()
        };
        tree.rebuild(repo);
        Some(tree.rows)
    }

    pub fn reveal(&mut self, id: &str, repo: Option<&Repository>) {
        if !self.rows.iter().any(|row| row.id == id) {
            self.collapsed
                .retain(|parent| id == parent || !id.starts_with(parent));
            self.rebuild(repo);
        }
        if let Some(i) = self.rows.iter().position(|row| row.id == id) {
            self.cursor = i;
        }
    }

    pub fn for_refresh(&self) -> Self {
        Self {
            flat: self.flat,
            sort: self.sort,
            collapsed: self.collapsed.clone(),
            ..Self::default()
        }
    }

    pub fn clear_rows(&mut self) {
        if let Some(row) = self.rows.get(self.cursor) {
            self.remembered
                .insert(row.root, (row.id.clone(), self.cursor));
        }
        self.rows.clear();
        self.cursor = 0;
    }

    pub fn install(&mut self, prepared: Self, repo: Option<&Repository>) {
        if self.flat != prepared.flat
            || self.sort != prepared.sort
            || self.collapsed != prepared.collapsed
            || prepared.rows.first().map(|row| row.root) != repo.map(|repo| repo.root)
        {
            // Keep any selection, grouping, or fold change made while Git was reading.
            self.rebuild(repo);
            return;
        }
        self.clear_rows();
        self.rows = prepared.rows;
        if let Some(repo) = repo {
            self.restore_cursor(repo.root);
        }
    }

    pub fn rebuild(&mut self, repo: Option<&Repository>) {
        self.clear_rows();
        let Some(repo) = repo else { return };
        let root = repo.root;
        match &repo.state {
            Ok(state) => {
                for remote in [false, true] {
                    let label = if remote {
                        "Remote Branches"
                    } else {
                        "Local Branches"
                    };
                    let id = format!("{root}:{}", namespace(remote));
                    let branches = state.branches.values().filter(|b| b.remote == remote);
                    let count = branches.clone().count();
                    if !self.folder(root, &id, label, 0, count) {
                        continue;
                    }
                    if count == 0 {
                        self.rows.push(Row {
                            id: format!("{id}empty"),
                            root,
                            depth: 1,
                            label: "No branches".into(),
                            kind: Kind::Message,
                        });
                    } else if self.flat {
                        let mut branches: Vec<_> = branches.collect();
                        branches
                            .sort_by_key(|branch| (self.sort.order(branch, repo), &branch.name));
                        for branch in branches {
                            self.branch(root, 1, &branch.name, branch);
                        }
                    } else {
                        let mut tree = Node::default();
                        for branch in branches {
                            tree.insert(branch, self.sort.order(branch, repo));
                        }
                        self.children(root, &id, &tree, 1);
                    }
                }
            }
            Err(error) => self.rows.push(Row {
                id: format!("{root}:error"),
                root,
                depth: 0,
                label: error.clone(),
                kind: Kind::Message,
            }),
        }
        self.restore_cursor(root);
    }

    fn restore_cursor(&mut self, root: usize) {
        self.cursor = if let Some((id, cursor)) = self.remembered.remove(&root) {
            self.rows
                .iter()
                .position(|row| row.id == id)
                .or_else(|| {
                    // A prefix can disappear in flat mode or hide a remembered branch.
                    self.rows.iter().rposition(|row| {
                        matches!(row.kind, Kind::Folder { .. }) && id.starts_with(&row.id)
                    })
                })
                .unwrap_or_else(|| cursor.min(self.rows.len().saturating_sub(1)))
        } else {
            self.rows
                .iter()
                .position(|row| matches!(row.kind, Kind::Branch(_)))
                .unwrap_or(0)
        };
    }

    fn folder(&mut self, root: usize, id: &str, label: &str, depth: usize, count: usize) -> bool {
        let open = !self.collapsed.contains(id);
        self.rows.push(Row {
            id: id.into(),
            root,
            depth,
            label: label.into(),
            kind: Kind::Folder { open, count },
        });
        open
    }

    fn branch(&mut self, root: usize, depth: usize, label: &str, branch: &Branch) {
        self.rows.push(Row {
            id: format!("{root}:{}", branch.reference),
            root,
            depth,
            label: label.into(),
            kind: Kind::Branch(branch.clone()),
        });
    }

    fn children(&mut self, root: usize, prefix: &str, node: &Node<'_>, depth: usize) {
        let mut children: Vec<_> = node.children.iter().collect();
        children.sort_by_key(|(name, child)| (child.order, *name));
        for (&name, child) in children {
            if let Some(branch) = child.branch {
                self.branch(root, depth, name, branch);
            } else {
                let id = format!("{prefix}{name}/");
                if self.folder(root, &id, name, depth, child.count) {
                    self.children(root, &id, child, depth + 1);
                }
            }
        }
    }

    pub fn grouped(&self) -> bool {
        !self.flat
    }

    pub fn sort(&self) -> Sort {
        self.sort
    }

    pub fn set_sort(&mut self, sort: Sort, repo: Option<&Repository>) {
        self.sort = sort;
        self.rebuild(repo);
    }

    pub fn toggle_grouping(&mut self, repo: Option<&Repository>) {
        self.flat = !self.flat;
        if self.grouped() {
            if let Some(Row {
                root,
                kind: Kind::Branch(branch),
                ..
            }) = self.rows.get(self.cursor)
            {
                let prefix = format!("{root}:{}", namespace(branch.remote));
                // Keep the selected ref visible when restoring prefix folders.
                for (slash, _) in branch.name.match_indices('/') {
                    self.collapsed
                        .remove(&format!("{prefix}{}", &branch.name[..=slash]));
                }
            }
        }
        self.rebuild(repo);
    }

    pub fn toggle(&mut self, repo: Option<&Repository>) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if let Kind::Folder { open, .. } = row.kind {
            if open {
                self.collapsed.insert(row.id.clone());
            } else {
                self.collapsed.remove(&row.id);
            }
            self.rebuild(repo);
        }
    }

    pub fn horizontal(&mut self, left: bool, repo: Option<&Repository>) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if matches!(row.kind, Kind::Folder { open, .. } if open == left) {
            self.toggle(repo);
        } else if left {
            let depth = row.depth;
            if let Some(parent) = self.rows[..self.cursor]
                .iter()
                .rposition(|r| r.depth < depth)
            {
                self.cursor = parent;
            }
        } else if matches!(row.kind, Kind::Folder { open: true, .. }) {
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
