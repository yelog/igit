//! Visible tree rows retain exact indexes into the byte paths read from Git.
use super::label;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(super) struct Change {
    pub path: PathBuf,
    pub previous: Option<PathBuf>,
    pub status: u8,
}

fn path(raw: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(raw.to_vec()).into()
    }
    #[cfg(not(unix))]
    {
        String::from_utf8_lossy(raw).into_owned().into()
    }
}

pub(super) fn added_paths(raw: Vec<u8>) -> Vec<Change> {
    raw.split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| Change {
            path: path(p),
            previous: None,
            status: b'A',
        })
        .collect()
}

pub(super) fn name_status(raw: Vec<u8>) -> Vec<Change> {
    let mut fields = raw.split(|&b| b == 0);
    let mut files = Vec::new();
    while let Some(status) = fields.next().and_then(|s| s.first()).copied() {
        let Some(first) = fields.next().filter(|p| !p.is_empty()) else {
            break;
        };
        let (path, previous) = if matches!(status, b'R' | b'C') {
            let Some(next) = fields.next().filter(|p| !p.is_empty()) else {
                break;
            };
            (path(next), Some(path(first)))
        } else {
            (path(first), None)
        };
        files.push(Change {
            path,
            previous,
            status,
        });
    }
    files
}

#[derive(Clone)]
pub(super) enum Kind {
    Repository,
    Module,
    Directory { path: PathBuf, name: String },
    File(usize),
}

#[derive(Clone)]
pub(super) struct Node {
    pub kind: Kind,
    pub depth: usize,
    pub count: usize,
    pub end: usize,
}

#[derive(Default)]
struct Directory {
    dirs: BTreeMap<OsString, Directory>,
    leaves: Vec<usize>,
    count: usize,
}

impl Directory {
    fn append(self, nodes: &mut Vec<Node>, parent: &Path, depth: usize, files: &[Change]) {
        for (component, mut dir) in self.dirs {
            let mut path = parent.join(component);
            let mut name = label(path.file_name().unwrap().as_ref());
            while dir.leaves.is_empty() && dir.dirs.len() == 1 {
                let (next, child) = dir.dirs.pop_first().unwrap();
                name.push('/');
                name.push_str(&label(next.as_ref()));
                path.push(next);
                dir = child;
            }
            let id = nodes.len();
            nodes.push(Node {
                kind: Kind::Directory {
                    path: path.clone(),
                    name,
                },
                depth,
                count: dir.count,
                end: 0,
            });
            dir.append(nodes, &path, depth + 1, files);
            nodes[id].end = nodes.len();
        }
        let mut leaves = self.leaves;
        leaves.sort_by(|&a, &b| files[a].path.file_name().cmp(&files[b].path.file_name()));
        for file in leaves {
            nodes.push(Node {
                kind: Kind::File(file),
                depth,
                count: 1,
                end: nodes.len() + 1,
            });
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct FileTree {
    pub nodes: Vec<Node>,
    pub visible: Vec<usize>,
    pub folded: BTreeSet<usize>,
    pub cursor: usize,
    pub offset: usize,
}

impl FileTree {
    pub fn new(files: &[Change]) -> Self {
        let mut root = Directory::default();
        for (i, file) in files.iter().enumerate() {
            let mut dir = &mut root;
            if let Some(parent) = file.path.parent() {
                for part in parent.components() {
                    dir = dir.dirs.entry(part.as_os_str().into()).or_default();
                    dir.count += 1;
                }
            }
            dir.leaves.push(i);
        }
        let mut nodes = vec![Node {
            kind: Kind::Repository,
            depth: 0,
            count: files.len(),
            end: 1,
        }];
        if !files.is_empty() {
            nodes.push(Node {
                kind: Kind::Module,
                depth: 1,
                count: files.len(),
                end: 0,
            });
            root.append(&mut nodes, Path::new(""), 2, files);
            nodes[1].end = nodes.len();
            nodes[0].end = nodes.len();
        }
        Self {
            visible: (0..nodes.len()).collect(),
            nodes,
            ..Self::default()
        }
    }

    pub fn selected(&self) -> Option<&Node> {
        self.nodes.get(*self.visible.get(self.cursor)?)
    }

    pub fn file_index(&self) -> Option<usize> {
        match self.selected()?.kind {
            Kind::File(i) => Some(i),
            _ => None,
        }
    }

    fn reflow(&mut self, keep: usize) {
        self.visible.clear();
        let mut id = 0;
        while id < self.nodes.len() {
            self.visible.push(id);
            id = if self.folded.contains(&id) {
                self.nodes[id].end
            } else {
                id + 1
            };
        }
        self.cursor = self
            .visible
            .partition_point(|&n| n <= keep)
            .saturating_sub(1);
    }

    pub fn toggle(&mut self) {
        let Some(&id) = self.visible.get(self.cursor) else {
            return;
        };
        if self.nodes[id].end > id + 1 {
            if !self.folded.remove(&id) {
                self.folded.insert(id);
            }
            self.reflow(id);
        }
    }

    pub fn horizontal(&mut self, left: bool) {
        let Some(&id) = self.visible.get(self.cursor) else {
            return;
        };
        let node = &self.nodes[id];
        let open = !self.folded.contains(&id) && node.end > id + 1;
        if (left && open) || (!left && self.folded.contains(&id)) {
            self.toggle();
        } else if left {
            if let Some(parent) = self.visible[..self.cursor]
                .iter()
                .rposition(|&n| self.nodes[n].depth < node.depth)
            {
                self.cursor = parent;
            }
        } else if open {
            self.cursor = (self.cursor + 1).min(self.visible.len().saturating_sub(1));
        }
    }

    pub fn fold_all(&mut self, collapse: bool) {
        let keep = self.visible.get(self.cursor).copied().unwrap_or(0);
        self.folded.clear();
        if collapse {
            self.folded.extend(
                self.nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, n)| n.end > i + 1)
                    .map(|(i, _)| i),
            );
        }
        self.reflow(keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(paths: &[&str]) -> Vec<Change> {
        paths
            .iter()
            .map(|path| Change {
                path: path.into(),
                previous: None,
                status: b'M',
            })
            .collect()
    }

    #[test]
    fn compact_directories_count_files_and_preserve_duplicate_basenames() {
        let files = changes(&[
            "src/views/user/index.vue",
            "src/views/admin/index.vue",
            "dev/iframe-proxy/README.md",
            "dev/iframe-proxy/compose.yaml",
            "package.json",
        ]);
        let mut tree = FileTree::new(&files);
        assert_eq!(tree.nodes[0].count, 5);
        assert_eq!(tree.nodes[1].count, 5);
        let views = tree
            .nodes
            .iter()
            .position(|n| matches!(&n.kind, Kind::Directory { name, .. } if name == "src/views"))
            .unwrap();
        assert_eq!(tree.nodes[views].count, 2);
        let leaves: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|n| match n.kind {
                Kind::File(i) => Some(i),
                _ => None,
            })
            .collect();
        assert_eq!(leaves, [2, 3, 1, 0, 4]);
        for &i in &leaves {
            tree.cursor = tree
                .visible
                .iter()
                .position(|&n| matches!(tree.nodes[n].kind, Kind::File(file) if file == i))
                .unwrap();
            assert_eq!(tree.file_index(), Some(i));
        }
    }

    #[test]
    fn folding_and_parent_navigation_never_reuse_a_hidden_file_index() {
        let files = changes(&["src/a/index.vue", "src/b/index.vue", "README.md"]);
        let mut tree = FileTree::new(&files);
        let file = tree
            .visible
            .iter()
            .position(|&n| matches!(tree.nodes[n].kind, Kind::File(1)))
            .unwrap();
        tree.cursor = file;
        tree.horizontal(true);
        assert!(matches!(
            tree.selected().unwrap().kind,
            Kind::Directory { .. }
        ));
        assert_eq!(tree.file_index(), None);
        tree.toggle();
        let folded = tree.visible[tree.cursor];
        assert!(tree.folded.contains(&folded));
        assert!(!tree
            .visible
            .iter()
            .any(|&n| matches!(tree.nodes[n].kind, Kind::File(1))));
        tree.horizontal(false);
        assert!(!tree.folded.contains(&folded));
        tree.horizontal(false);
        assert_eq!(tree.file_index(), Some(1));
        tree.fold_all(true);
        assert_eq!(tree.visible, [0]);
        assert_eq!(tree.file_index(), None);
        tree.fold_all(false);
        assert_eq!(tree.visible.len(), tree.nodes.len());
        assert_eq!(tree.file_index(), None);
    }

    #[test]
    fn name_status_keeps_rename_sources_and_literal_byte_paths() {
        let raw = b"A\0src/new.ts\0M\0src/App.vue\0D\0old.conf\0R100\0old name.ts\0src/new name.ts\0C75\0base.rs\0copy.rs\0M\0bad-\xff\t\n.ts\0";
        let files = name_status(raw.to_vec());
        assert_eq!(files.len(), 6);
        assert_eq!(
            files.iter().map(|f| f.status).collect::<Vec<_>>(),
            b"AMDRCM"
        );
        assert_eq!(files[3].path, PathBuf::from("src/new name.ts"));
        assert_eq!(files[3].previous.as_deref(), Some(Path::new("old name.ts")));
        assert_eq!(files[4].previous.as_deref(), Some(Path::new("base.rs")));
        #[cfg(unix)]
        assert_eq!(
            files[5].path.as_os_str().as_encoded_bytes(),
            b"bad-\xff\t\n.ts"
        );
        let tree = FileTree::new(&files);
        assert_eq!(tree.nodes[0].count, 6);
        assert_eq!(
            added_paths(b"a\0folder/b\0".to_vec())
                .iter()
                .map(|f| f.status)
                .collect::<Vec<_>>(),
            b"AA"
        );
    }
}
