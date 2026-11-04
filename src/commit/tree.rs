use super::{display_path, keypart, CommitPanel, Context, DEF, NEW};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(PartialEq)]
pub(super) enum Kind {
    Group,
    Repo(usize),
    Directory(PathBuf),
    File(usize),
}

pub(super) struct Row {
    pub id: String,
    pub label: String,
    pub depth: u16,
    pub files: Vec<usize>,
    pub kind: Kind,
}

impl Row {
    pub fn file(&self) -> Option<usize> {
        match self.kind {
            Kind::File(i) => Some(i),
            _ => None,
        }
    }

    pub fn foldable(&self) -> bool {
        self.file().is_none() && !self.files.is_empty()
    }

    pub fn indent(&self, width: u16) -> u16 {
        self.depth.saturating_mul(2).min(width.saturating_sub(18))
    }
}

fn repo_id(root: usize, group: &str) -> String {
    format!("r:{root}:{group}")
}

fn directory_id(repo_id: &str, path: &Path) -> String {
    format!("d:{}:{repo_id}", keypart(path))
}

// Keep native path components for identity; only convert them to text for display.
#[derive(Default)]
struct Directory<'a> {
    children: BTreeMap<&'a OsStr, Directory<'a>>,
    leaves: Vec<usize>,
    members: Vec<usize>,
}

impl Directory<'_> {
    fn append(
        self,
        panel: &CommitPanel,
        folds: &BTreeSet<String>,
        rows: &mut Vec<Row>,
        repo_id: &str,
        parent: &Path,
        depth: u16,
    ) {
        for (name, mut dir) in self.children {
            let mut label = PathBuf::from(name);
            while dir.leaves.is_empty() && dir.children.len() == 1 {
                let (child, next) = dir.children.pop_first().unwrap();
                label.push(child);
                dir = next;
            }
            let path = parent.join(&label);
            let id = directory_id(repo_id, &path);
            let open = !folds.contains(&id);
            rows.push(Row {
                id,
                label: display_path(&label),
                depth,
                files: std::mem::take(&mut dir.members),
                kind: Kind::Directory(path.clone()),
            });
            if open {
                dir.append(panel, folds, rows, repo_id, &path, depth + 1);
            }
        }
        rows.extend(self.leaves.into_iter().map(|i| panel.file_row(i, depth)));
    }
}

impl CommitPanel {
    fn file_row(&self, i: usize, depth: u16) -> Row {
        let file = &self.files[i];
        Row {
            id: file.key.clone(),
            label: display_path(Path::new(file.change.path.file_name().unwrap_or_default())),
            depth,
            files: vec![i],
            kind: Kind::File(i),
        }
    }

    pub(super) fn set_directory_grouping(&mut self, enabled: bool, c: &Context) {
        if self.group_by_directory == enabled {
            return;
        }
        if let Some(row) = self.active() {
            match &row.kind {
                Kind::File(i) if enabled => {
                    let file = &self.files[*i];
                    let repo = repo_id(file.root, &file.group);
                    for parent in file.change.path.ancestors().skip(1) {
                        self.fold.remove(&directory_id(&repo, parent));
                    }
                }
                Kind::Directory(_) if !enabled => {
                    // The flat list has no directory row; keep the same repository and changelist.
                    if let Some(i) = self.view[..self.cursor]
                        .iter()
                        .rposition(|r| matches!(r.kind, Kind::Repo(_)))
                    {
                        self.cursor = i;
                    }
                }
                _ => {}
            }
        }
        self.group_by_directory = enabled;
        self.sync(c);
    }

    pub(super) fn set_all_expanded(&mut self, expanded: bool, c: &Context) {
        if !expanded && self.active().is_some() {
            if let Some(i) = self.view[..=self.cursor]
                .iter()
                .rposition(|r| r.kind == Kind::Group)
            {
                self.cursor = i;
            }
        }
        // Include children behind closed nodes, but leave filtered roots and the other
        // grouping mode's directory folds alone.
        for row in self.rows_with_folds(c, &BTreeSet::new()) {
            if row.foldable() {
                if expanded {
                    self.fold.remove(&row.id);
                } else {
                    self.fold.insert(row.id);
                }
            }
        }
        self.sync(c);
    }

    pub(super) fn render_rows(&self, c: &Context) -> Vec<Row> {
        self.rows_with_folds(c, &self.fold)
    }

    fn rows_with_folds(&self, c: &Context, folds: &BTreeSet<String>) -> Vec<Row> {
        let mut rows = vec![];
        for group in std::iter::once(DEF)
            .chain(self.saved.groups.iter().map(String::as_str))
            .chain(std::iter::once(NEW))
        {
            let mut repos: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (i, file) in self.files.iter().enumerate() {
                if file.group == group && c.scope.contains(&file.root) {
                    repos.entry(file.root).or_default().push(i);
                }
            }
            if group == NEW && repos.is_empty() {
                continue;
            }
            if !self.group_by_directory {
                for members in repos.values_mut() {
                    members.sort_by_cached_key(|&i| {
                        let path = &self.files[i].change.path;
                        (
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_lowercase(),
                            path.parent(),
                            path.as_os_str(),
                        )
                    });
                }
            }
            let id = format!("g:{group}");
            let open = !folds.contains(&id);
            rows.push(Row {
                id,
                label: group.to_owned(),
                depth: 0,
                files: repos.values().flatten().copied().collect(),
                kind: Kind::Group,
            });
            if !open {
                continue;
            }
            for (r, members) in repos {
                let id = repo_id(r, group);
                let open = !folds.contains(&id);
                rows.push(Row {
                    id: id.clone(),
                    label: c.repos[r].name.clone(),
                    depth: 1,
                    files: members.clone(),
                    kind: Kind::Repo(r),
                });
                if !open {
                    continue;
                }
                if !self.group_by_directory {
                    rows.extend(members.into_iter().map(|i| self.file_row(i, 2)));
                    continue;
                }
                let mut root = Directory::default();
                for i in members {
                    let mut dir = &mut root;
                    if let Some(parent) = self.files[i].change.path.parent() {
                        for name in parent {
                            dir = dir.children.entry(name).or_default();
                            dir.members.push(i);
                        }
                    }
                    dir.leaves.push(i);
                }
                root.append(self, folds, &mut rows, &id, Path::new(""), 2);
            }
        }
        rows
    }

    pub(super) fn check_counts(&self, row: &Row) -> (usize, usize) {
        let mut checked = 0;
        let mut eligible = 0;
        for &i in &row.files {
            let file = &self.files[i];
            if !file.change.conflict {
                eligible += 1;
                checked += usize::from(self.checks.get(&file.key) == Some(&true));
            }
        }
        (checked, eligible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::git, ui::Panel};
    use std::fs;

    fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
        let dir = tempfile::tempdir().unwrap();
        for (name, paths) in [
            (
                "app",
                &[
                    "z/alpha.rs",
                    "Zebra.rs",
                    "a/shared.rs",
                    "b/SHARED.rs",
                    "界/shared.rs",
                ][..],
            ),
            (
                "lib",
                &[
                    "a/shared.rs",
                    "pkg/src/model/shared.rs",
                    "pkg/src/view/shared.rs",
                ][..],
            ),
        ] {
            let repo = dir.path().join(name);
            fs::create_dir(&repo).unwrap();
            git(&repo, &["init", "-q", "-b", "main"]).unwrap();
            for path in paths {
                let file = repo.join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, format!("{name} {path}\n")).unwrap();
            }
            git(&repo, &["add", "--", "."]).unwrap();
            fs::write(repo.join("note.txt"), "untracked").unwrap();
        }
        let mut c = Context::discover(dir.path()).unwrap();
        c.state_dir = dir.path().join("state");
        let mut panel = CommitPanel::default();
        panel.refresh(&c).unwrap();
        (dir, c, panel)
    }

    fn file_row(p: &CommitPanel, root: usize, path: &str) -> usize {
        p.view
            .iter()
            .position(|r| {
                r.file().is_some_and(|i| {
                    p.files[i].root == root && p.files[i].change.path == Path::new(path)
                })
            })
            .unwrap()
    }

    fn dir_row(p: &CommitPanel, root: usize, path: &str) -> usize {
        p.view
            .iter()
            .position(|r| {
                matches!(&r.kind, Kind::Directory(d) if d == Path::new(path))
                    && p.files[r.files[0]].root == root
            })
            .unwrap()
    }

    #[test]
    fn flat_rows_sort_by_filename_and_select_the_exact_git_path() {
        let (_d, c, mut panel) = fixture();
        let paths: Vec<_> = panel
            .view
            .iter()
            .filter_map(|row| {
                let file = &panel.files[row.file()?];
                assert_eq!(row.depth, 2);
                (file.root == 0 && file.group == DEF).then_some(file.change.path.as_path())
            })
            .collect();
        assert_eq!(
            paths,
            [
                "z/alpha.rs",
                "a/shared.rs",
                "b/SHARED.rs",
                "界/shared.rs",
                "Zebra.rs"
            ]
            .map(Path::new)
        );
        assert!(!panel
            .view
            .iter()
            .any(|r| matches!(r.kind, Kind::Directory(_))));
        let i = file_row(&panel, 0, "a/shared.rs");
        panel.cursor = i;
        panel.toggle(i);
        assert_eq!(panel.check_counts(&panel.view[i]), (0, 1));
        for (root, path) in [(0, "界/shared.rs"), (1, "a/shared.rs")] {
            let twin = &panel.view[file_row(&panel, root, path)];
            assert_eq!(panel.view[i].label, twin.label);
            assert_ne!(panel.view[i].id, twin.id);
            assert_eq!(panel.check_counts(twin), (1, 1));
        }
        let key = panel.view[i].id.clone();
        panel.refresh(&c).unwrap();
        assert_eq!(panel.active().unwrap().id, key);
        assert_eq!(panel.check_counts(panel.active().unwrap()), (0, 1));
        let file = &panel.files[panel.active().unwrap().file().unwrap()];
        let diff = super::super::backend::diff(&c.repos[file.root].path, &file.change).unwrap();
        assert!(diff.lines().any(|line| line == "+app a/shared.rs"));
    }

    #[test]
    fn regrouping_reveals_the_active_file_and_preserves_other_directory_folds() {
        let (_d, c, mut panel) = fixture();
        let checks = panel.checks.clone();
        panel.set_directory_grouping(true, &c);
        let sibling = panel.view[dir_row(&panel, 1, "pkg/src/model")].id.clone();
        for path in ["pkg/src/model", "pkg/src/view", "pkg/src"] {
            panel.fold(dir_row(&panel, 1, path), Some(true), &c);
        }
        panel.cursor = dir_row(&panel, 1, "pkg/src");
        panel.set_directory_grouping(false, &c);
        assert!(matches!(panel.active().unwrap().kind, Kind::Repo(1)));
        // A bulk action in the flat layout leaves the directory preference intact.
        panel.set_all_expanded(true, &c);
        panel.cursor = file_row(&panel, 1, "pkg/src/view/shared.rs");
        let file_key = panel.active().unwrap().id.clone();
        panel.set_directory_grouping(true, &c);
        assert_eq!(panel.active().unwrap().id, file_key);
        assert!(panel.fold.contains(&sibling));
        assert_eq!(panel.view[dir_row(&panel, 1, "pkg/src")].label, "pkg/src");
        panel.set_directory_grouping(false, &c);
        assert_eq!(panel.active().unwrap().id, file_key);
        assert_eq!(panel.checks, checks);
    }

    #[test]
    fn bulk_folds_preserve_the_active_group_checks_and_filtered_root_folds() {
        let (_d, mut c, mut panel) = fixture();
        panel.set_directory_grouping(true, &c);
        let i = dir_row(&panel, 1, "pkg/src/model");
        let was_closed = panel.view[i].id.clone();
        let was_open = panel.view[dir_row(&panel, 1, "pkg/src/view")].id.clone();
        panel.fold(i, Some(true), &c);
        c.scope.remove(&1);
        panel.refresh(&c).unwrap();
        panel.cursor = file_row(&panel, 0, "note.txt");
        let checks = panel.checks.clone();
        panel.set_all_expanded(false, &c);
        assert!(panel.view.iter().all(|r| r.kind == Kind::Group));
        assert_eq!(panel.active().unwrap().label, NEW);
        assert!(panel.fold.contains(&was_closed) && !panel.fold.contains(&was_open));
        panel.fold(panel.cursor, Some(false), &c);
        assert!(panel.view.iter().any(|r| r.kind == Kind::Repo(0)));
        assert!(panel.view.iter().all(|r| r.file().is_none()));
        panel.set_all_expanded(true, &c);
        assert_eq!(panel.active().unwrap().label, NEW);
        assert!(panel.fold.contains(&was_closed) && !panel.fold.contains(&was_open));
        panel.cursor = file_row(&panel, 0, "界/shared.rs");
        let key = panel.active().unwrap().id.clone();
        panel.set_all_expanded(true, &c);
        assert_eq!(panel.active().unwrap().id, key);
        c.scope.insert(1);
        panel.refresh(&c).unwrap();
        assert!(panel.view.iter().any(|r| r.id == was_closed));
        assert_eq!(panel.checks, checks);
    }
}
