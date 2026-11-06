use super::{
    display, display_path, keypart, CommitPanel, Context, File, IconMode, DEF, IGNORED, NEW,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(PartialEq)]
pub(super) enum Kind {
    Group,
    Conflicts,
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

pub(super) struct Tree<'a> {
    pub files: &'a [File],
    // A missing check leaves a visible file read-only.
    pub checks: &'a BTreeMap<String, bool>,
    pub fold: &'a BTreeSet<String>,
    pub groups: &'a [String],
    pub options: &'a display::Options,
    pub icons: IconMode,
    pub show_empty_groups: bool,
    pub merge_conflicts: bool,
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

pub(super) fn directory_root(c: &Context) -> &Path {
    c.repos
        .iter()
        .map(|repo| repo.path.as_path())
        .filter(|path| c.workspace.starts_with(path))
        .min_by_key(|path| path.components().count())
        .unwrap_or(&c.workspace)
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
        tree: &Tree<'_>,
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
                dir.append(tree, folds, rows, repo_id, &path, depth + 1);
            }
        }
        rows.extend(self.leaves.into_iter().map(|i| tree.file_row(i, depth)));
    }
}

impl CommitPanel {
    pub(super) fn changes_tree(&self) -> Tree<'_> {
        Tree {
            files: &self.files,
            checks: &self.checks,
            fold: &self.fold,
            groups: &self.saved.groups,
            options: &self.options,
            icons: self.icons,
            show_empty_groups: true,
            merge_conflicts: true,
        }
    }

    pub(super) fn set_directory_grouping(&mut self, enabled: bool, c: &Context) {
        self.set_grouping(enabled, self.options.modules, c);
    }

    pub(super) fn set_module_grouping(&mut self, enabled: bool, c: &Context) {
        self.set_grouping(self.options.directories, enabled, c);
    }

    fn set_grouping(&mut self, directories: bool, modules: bool, c: &Context) {
        if self.options.directories == directories && self.options.modules == modules {
            return;
        }
        // Find the closest surviving row, then reveal only its ancestors in the
        // new layout. Other directories and repositories retain their folds.
        let mut candidates = vec![];
        if let Some(row) = self.active() {
            candidates.push((self.cursor, row.id.clone()));
            let mut depth = row.depth;
            for (i, row) in self.view[..self.cursor].iter().enumerate().rev() {
                if row.depth < depth {
                    candidates.push((i, row.id.clone()));
                    depth = row.depth;
                }
            }
        }
        self.options.directories = directories;
        self.options.modules = modules;
        let all = self.rows_with_folds(c, &BTreeSet::new());
        for (old, id) in candidates {
            if let Some(i) = all.iter().position(|r| r.id == id) {
                self.cursor = old;
                let mut depth = all[i].depth;
                for row in all[..i].iter().rev() {
                    if row.depth < depth {
                        self.fold.remove(&row.id);
                        depth = row.depth;
                    }
                }
                break;
            }
        }
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
        self.changes_tree().rows_with_folds(c, folds)
    }

    pub(super) fn check_counts(&self, row: &Row) -> (usize, usize) {
        self.changes_tree().check_counts(row)
    }
}

impl Tree<'_> {
    fn file_row(&self, i: usize, depth: u16) -> Row {
        let file = &self.files[i];
        let mut label = display_path(Path::new(file.change.path.file_name().unwrap_or_default()));
        if file.change.directory() {
            label.push('/');
        }
        Row {
            id: file.key.clone(),
            label,
            depth,
            files: vec![i],
            kind: Kind::File(i),
        }
    }

    pub(super) fn rows_with_folds(&self, c: &Context, folds: &BTreeSet<String>) -> Vec<Row> {
        let mut rows = vec![];
        let conflicts: Vec<_> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| self.merge_conflicts && f.change.conflict && c.scope.contains(&f.root))
            .map(|(i, _)| i)
            .collect();
        for group in std::iter::once(DEF)
            .chain(self.groups.iter().map(String::as_str))
            .chain(std::iter::once(NEW))
            .chain(self.options.ignored.then_some(IGNORED))
        {
            let mut repos: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (i, file) in self.files.iter().enumerate() {
                if file.group == group
                    && !(self.merge_conflicts && file.change.conflict)
                    && c.scope.contains(&file.root)
                {
                    repos.entry(file.root).or_default().push(i);
                }
            }
            let unmerged = if group == DEF { &conflicts[..] } else { &[] };
            if (!self.show_empty_groups || group == NEW || group == IGNORED)
                && repos.is_empty()
                && unmerged.is_empty()
            {
                continue;
            }
            let id = format!("g:{group}");
            let open = !folds.contains(&id);
            rows.push(Row {
                id,
                label: group.to_owned(),
                depth: 0,
                files: repos.values().flatten().chain(unmerged).copied().collect(),
                kind: Kind::Group,
            });
            if !open {
                continue;
            }
            if !unmerged.is_empty() {
                let id = "merge-conflicts".to_owned();
                let open = !folds.contains(&id);
                rows.push(Row {
                    id,
                    label: "Merge Conflicts".to_owned(),
                    depth: 1,
                    files: unmerged.to_vec(),
                    kind: Kind::Conflicts,
                });
                if open {
                    self.append_flat_files(c, unmerged.to_vec(), 2, &mut rows);
                }
            }
            if !self.options.modules {
                self.append_files(
                    c,
                    repos.into_values().flatten().collect(),
                    &format!("w:{group}"),
                    1,
                    folds,
                    &mut rows,
                );
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
                self.append_files(c, members, &id, 2, folds, &mut rows);
            }
        }
        rows
    }

    fn append_files(
        &self,
        c: &Context,
        members: Vec<usize>,
        id: &str,
        depth: u16,
        folds: &BTreeSet<String>,
        rows: &mut Vec<Row>,
    ) {
        if !self.options.directories {
            self.append_flat_files(c, members, depth, rows);
            return;
        }
        let paths: Vec<_> = members
            .into_iter()
            .map(|i| {
                let file = &self.files[i];
                let repo = &c.repos[file.root].path;
                let prefix = if self.options.modules {
                    Path::new("")
                } else {
                    repo.strip_prefix(directory_root(c)).unwrap_or(repo)
                };
                (i, prefix.join(&file.change.path))
            })
            .collect();
        let mut root = Directory::default();
        for (i, path) in &paths {
            let mut dir = &mut root;
            if let Some(parent) = path.parent() {
                for name in parent {
                    dir = dir.children.entry(name).or_default();
                    dir.members.push(*i);
                }
            }
            dir.leaves.push(*i);
        }
        root.append(self, folds, rows, id, Path::new(""), depth);
    }

    fn append_flat_files(
        &self,
        c: &Context,
        mut members: Vec<usize>,
        depth: u16,
        rows: &mut Vec<Row>,
    ) {
        members.sort_by_cached_key(|&i| {
            let file = &self.files[i];
            let path = &file.change.path;
            (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
                path.parent(),
                &c.repos[file.root].path,
                path.as_os_str(),
            )
        });
        rows.extend(members.into_iter().map(|i| self.file_row(i, depth)));
    }

    pub(super) fn check_counts(&self, row: &Row) -> (usize, usize) {
        let mut checked = 0;
        let mut eligible = 0;
        for &i in &row.files {
            let file = &self.files[i];
            if let Some(&check) = self
                .checks
                .get(&file.key)
                .filter(|_| file.change.selectable())
            {
                eligible += 1;
                checked += usize::from(check);
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
                    "Ｗ/shared.rs",
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
    fn merge_conflicts_are_scoped_once_and_keep_changelist_membership() {
        let (_d, mut c, mut panel) = fixture();
        let app = panel.view[file_row(&panel, 0, "a/shared.rs")]
            .file()
            .unwrap();
        let lib = panel.view[file_row(&panel, 1, "a/shared.rs")]
            .file()
            .unwrap();
        panel.saved.groups.push("Later".into());
        panel.files[app].group = "Later".into();
        for i in [app, lib] {
            panel.files[i].change.conflict = true;
            panel.files[i].change.xy = *b"UU";
        }
        panel.sync(&c);
        for (directories, modules) in [(true, false), (false, true), (false, false), (true, true)] {
            panel.set_grouping(directories, modules, &c);
            let rows = &panel.view;
            assert_eq!(rows[0].label, DEF);
            assert!(rows[0].files.contains(&app) && rows[0].files.contains(&lib));
            assert_eq!(rows[1].label, "Merge Conflicts");
            assert_eq!(rows[1].depth, 1);
            assert_eq!(panel.check_counts(&rows[1]), (0, 0));
            assert_eq!(rows[2].file(), Some(app));
            assert_eq!(rows[3].file(), Some(lib));
            assert_eq!(rows[2].depth, 2);
            assert_eq!(rows.iter().filter(|r| r.file() == Some(app)).count(), 1);
        }
        panel.fold(1, Some(true), &c);
        assert!(panel.view.iter().all(|row| row.file() != Some(app)));
        panel.fold(1, Some(false), &c);
        c.scope.remove(&1);
        panel.sync(&c);
        assert_eq!(panel.view[1].files, [app]);
        assert!(panel.view.iter().all(|row| row.file() != Some(lib)));

        panel.files[app].change.conflict = false;
        panel.files[app].change.xy = *b"M ";
        panel.sync(&c);
        assert!(panel.view.iter().all(|row| row.kind != Kind::Conflicts));
        assert_eq!(
            panel
                .view
                .iter()
                .find(|r| r.label == "Later")
                .unwrap()
                .files,
            [app]
        );
    }

    #[test]
    fn flat_rows_sort_by_filename_and_select_the_exact_git_path() {
        let (_d, c, mut panel) = fixture();
        panel.set_grouping(false, true, &c);
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
                "Ｗ/shared.rs",
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
        for (root, path) in [(0, "Ｗ/shared.rs"), (1, "a/shared.rs")] {
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
        panel.set_module_grouping(true, &c);
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
        assert_eq!(
            panel.view[dir_row(&panel, 1, "pkg/src")].label,
            Path::new("pkg").join("src").display().to_string()
        );
        panel.set_directory_grouping(false, &c);
        assert_eq!(panel.active().unwrap().id, file_key);
        assert_eq!(panel.checks, checks);
    }

    #[test]
    fn module_and_directory_options_preserve_selection_identity_and_scope() {
        let (_d, mut c, mut panel) = fixture();
        panel.cursor = file_row(&panel, 1, "pkg/src/view/shared.rs");
        panel.toggle(panel.cursor);
        let active = panel.active().unwrap().id.clone();
        let checks = panel.checks.clone();
        for modules in [false, true] {
            panel.set_module_grouping(modules, &c);
            for directories in [true, false] {
                panel.set_directory_grouping(directories, &c);
                assert_eq!(panel.active().unwrap().id, active);
                assert_eq!(panel.checks, checks);
                assert_eq!(
                    panel.view.iter().any(|r| matches!(r.kind, Kind::Repo(_))),
                    modules
                );
                assert_eq!(
                    panel
                        .view
                        .iter()
                        .any(|r| matches!(r.kind, Kind::Directory(_))),
                    directories
                );
                let leaves: BTreeSet<_> = panel.view.iter().filter_map(Row::file).collect();
                assert_eq!(leaves.len(), panel.files.len());
                if directories && !modules {
                    assert!(panel.view.iter().any(|r| matches!(
                        &r.kind, Kind::Directory(p) if p == Path::new("lib/pkg/src/model")
                    )));
                }
            }
        }
        c.scope.remove(&1);
        panel.set_module_grouping(false, &c);
        panel.set_directory_grouping(true, &c);
        assert!(panel
            .view
            .iter()
            .flat_map(|r| &r.files)
            .all(|&i| panel.files[i].root == 0));
        assert!(panel.selection(&c).iter().all(|f| f.root == 0));
        c.scope.insert(1);
        panel.refresh(&c).unwrap();
        assert_eq!(panel.checks, checks);
        assert_eq!(
            panel.check_counts(&panel.view[file_row(&panel, 1, "pkg/src/view/shared.rs")]),
            (0, 1)
        );
    }

    #[test]
    fn bulk_folds_preserve_the_active_group_checks_and_filtered_root_folds() {
        let (_d, mut c, mut panel) = fixture();
        panel.set_module_grouping(true, &c);
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
        panel.cursor = file_row(&panel, 0, "Ｗ/shared.rs");
        let key = panel.active().unwrap().id.clone();
        panel.set_all_expanded(true, &c);
        assert_eq!(panel.active().unwrap().id, key);
        c.scope.insert(1);
        panel.refresh(&c).unwrap();
        assert!(panel.view.iter().any(|r| r.id == was_closed));
        assert_eq!(panel.checks, checks);
    }
}
