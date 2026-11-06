//! A file-only adapter to Log's inspector and historical Diff. It never loads history.
use super::*;
use crate::core::git;
use crate::ui::help::HelpContent;
use anyhow::{ensure, Context as _};
use std::path::Path;

pub(crate) struct Snapshot {
    revision: model::Rev,
    files: Vec<model::File>,
    sources: Vec<model::Rev>,
}

impl Snapshot {
    pub(crate) fn stash(path: &Path, root: usize, oid: &str) -> Result<Self> {
        let parents = git(
            path,
            &[
                "show",
                "-s",
                "--no-show-signature",
                "--no-notes",
                "--format=%P",
                oid,
                "--",
            ],
        )?;
        let parents: Vec<_> = parents.split_whitespace().collect();
        ensure!(
            (2..=3).contains(&parents.len()),
            "Unsupported stash snapshot: expected two or three parents"
        );
        let revision = model::Rev {
            root,
            oid: oid.into(),
            // A stash is a merge object, but its worktree comparison is base -> saved tree.
            parents: vec![parents[0].into()],
            ..Default::default()
        };
        let (mut files, _) = model::merge_diff::changed_files(path, &revision)?;
        let mut sources = vec![revision.clone(); files.len()];
        if let Some(untracked) = parents.get(2) {
            let extra = model::Rev {
                root,
                oid: (*untracked).into(),
                ..Default::default()
            };
            let (saved, _) = model::merge_diff::changed_files(path, &extra)?;
            sources.extend(vec![extra; saved.len()]);
            files.extend(saved);
        }
        Ok(Self {
            revision,
            files,
            sources,
        })
    }
}

#[derive(Default)]
pub(crate) struct SnapshotPreview {
    panel: LogPanel,
    sources: Vec<model::Rev>,
}

impl SnapshotPreview {
    pub(crate) fn set(&mut self, snapshot: Snapshot) {
        self.panel = LogPanel::default();
        self.panel.hist.revs = vec![snapshot.revision];
        self.panel.visible = vec![0];
        self.panel.details.files = snapshot.files;
        self.panel.inspector = inspector::Inspector::new(&self.panel.details);
        self.sources = snapshot.sources;
    }

    pub(crate) fn draw_files(
        &mut self,
        f: &mut Frame,
        area: Rect,
        c: &Context,
        icons: IconMode,
        focused: bool,
    ) {
        self.panel.icons = icons;
        self.panel.focus = if focused { Pane::Files } else { Pane::Graph };
        self.panel.hits.clear();
        self.panel.draw_files(f, area, c);
        if self.panel.rev().is_some() && self.panel.details.files.is_empty() && area.height > 1 {
            f.render_widget(
                Paragraph::new("No saved file changes").style(fg(DIM)),
                Rect::new(area.x, area.y + 1, area.width, area.height - 1),
            );
        }
    }

    pub(crate) fn draw_overlay(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.panel.draw_overlays(f, area, c);
    }

    pub(crate) fn editing(&self) -> bool {
        self.panel.diff.is_some()
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.panel.selected_text()
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        let mut content = self.panel.keymap();
        if let Some(section) = content.sections.first_mut() {
            section.title = if self
                .panel
                .diff
                .as_ref()
                .is_some_and(|d| d.comparison.is_some())
            {
                "Stash / Diff (read-only)"
            } else {
                "Stash / Patch (read-only)"
            };
        }
        content
    }

    fn open(&mut self, c: &Context, file: usize) -> Result<()> {
        let source = self
            .sources
            .get(file)
            .context("Select a saved file first")?
            .clone();
        // Untracked files belong to the third parent's tree, not the saved worktree.
        // Restore the displayed stash identity after reading the selected file.
        let original = std::mem::replace(&mut self.panel.hist.revs[0], source);
        let result = self.panel.open_diff_file(c, file);
        self.panel.hist.revs[0] = original;
        result
    }

    fn next_file(&mut self, c: &Context, delta: isize) -> Result<()> {
        if let Some(current) = self.panel.diff.as_ref().and_then(|d| d.file) {
            let next = current
                .saturating_add_signed(delta)
                .min(self.sources.len().saturating_sub(1));
            if next != current {
                self.open(c, next)?;
            }
        }
        Ok(())
    }

    pub(crate) fn event(&mut self, e: &Event, c: &Context) -> Result<Action> {
        if let Some(diff) = self.panel.diff.as_mut().and_then(|d| d.comparison.as_mut()) {
            use crate::merge::RevisionEvent;
            match diff.event(e) {
                RevisionEvent::None => (),
                RevisionEvent::Close => self.panel.diff = None,
                RevisionEvent::File(delta) => self.next_file(c, delta)?,
                RevisionEvent::Copy(text) => {
                    return Ok(Action::Copy {
                        text,
                        label: "stash revision",
                    })
                }
            }
            return Ok(Action::None);
        }
        if self.editing() {
            if let Event::Key(k) = e {
                if matches!(k.code, K::Char('[' | ']')) {
                    self.next_file(c, if k.code == K::Char('[') { -1 } else { 1 })?;
                    return Ok(Action::None);
                }
            }
            return Ok(self.panel.event(e, c));
        }
        // Only inspector controls can enter this adapter; Log search/filter/history
        // handlers must never replace a stash snapshot with normal commit history.
        match e {
            Event::Key(k)
                if !k
                    .modifiers
                    .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER) =>
            {
                match k.code {
                    K::Char('j') | K::Down => self.panel.file_move(1),
                    K::Char('k') | K::Up => self.panel.file_move(-1),
                    K::Char('g') | K::Home => self.panel.file_move(isize::MIN),
                    K::Char('G') | K::End => self.panel.file_move(isize::MAX),
                    K::PageDown => self.panel.file_move(12),
                    K::PageUp => self.panel.file_move(-12),
                    K::Char('h') | K::Left => {
                        self.panel.file_horizontal(true);
                    }
                    K::Char('l') | K::Right => {
                        self.panel.file_horizontal(false);
                    }
                    K::Char('z' | 'Z') => self.panel.file_fold_all(k.code == K::Char('z')),
                    K::Char(' ') => self.panel.file_activate(self.panel.file_cursor(), true),
                    K::Enter => {
                        if let Some(file) = self.panel.file_index() {
                            self.open(c, file)?;
                        } else {
                            self.panel.file_activate(self.panel.file_cursor(), true);
                        }
                    }
                    _ => (),
                }
            }
            Event::Mouse(m) => match m.kind {
                M::ScrollDown => self.panel.file_move(3),
                M::ScrollUp => self.panel.file_move(-3),
                M::Down(MouseButton::Left) => {
                    let hit = self
                        .panel
                        .hits
                        .iter()
                        .rev()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                        .map(|(_, hit)| hit.clone());
                    match hit {
                        Some(Hit::File(row)) => {
                            let double = self.panel.double(Pane::Files, row);
                            self.panel.file_activate(row, true);
                            if double {
                                if let Some(file) = self.panel.file_index() {
                                    self.open(c, file)?;
                                }
                            }
                        }
                        Some(Hit::Diff) => {
                            if let Some(file) = self.panel.file_index() {
                                self.open(c, file)?;
                            }
                        }
                        Some(Hit::FileFold(collapse)) => self.panel.file_fold_all(collapse),
                        _ => (),
                    }
                }
                _ => (),
            },
            Event::FocusLost => self.panel.last = None,
            _ => (),
        }
        Ok(Action::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::tests::{git, init};
    use std::fs;

    fn saved_fixture() -> (tempfile::TempDir, Context, String) {
        let dir = init();
        let path = dir.path();
        git(path, &["config", "commit.gpgsign", "false"]);
        git(path, &["config", "core.hooksPath", "/dev/null"]);
        for (name, text) in [
            ("mixed.txt", "base\n"),
            ("deleted.txt", "delete me\n"),
            ("other.txt", "old\n"),
        ] {
            fs::write(path.join(name), text).unwrap();
        }
        git(path, &["add", "."]);
        git(path, &["commit", "-qm", "base"]);
        fs::write(path.join("mixed.txt"), "staged\n").unwrap();
        fs::write(path.join("other.txt"), "other staged\n").unwrap();
        git(path, &["add", "."]);
        fs::write(path.join("mixed.txt"), "saved working\n").unwrap();
        fs::remove_file(path.join("deleted.txt")).unwrap();
        let literal = if cfg!(windows) {
            "[literal].txt"
        } else {
            "[literal]*.txt"
        };
        fs::write(path.join(literal), "literal saved\n").unwrap();
        fs::create_dir(path.join("目录")).unwrap();
        fs::write(path.join("目录/新增.txt"), "新增内容\n").unwrap();
        fs::write(path.join("binary.bin"), b"\0\x01binary").unwrap();
        fs::write(path.join("large.txt"), vec![b'x'; 512 * 1024 + 1]).unwrap();
        git(path, &["stash", "push", "-u", "-m", "mixed snapshot"]);
        let oid = git(path, &["rev-parse", "refs/stash"]);
        let context = Context::discover(path).unwrap();
        (dir, context, oid)
    }

    #[test]
    fn stash_snapshot_combines_worktree_changes_and_saved_untracked_files() {
        let (_dir, c, oid) = saved_fixture();
        let path = &c.repos[0].path;
        let saved = Snapshot::stash(path, 0, &oid).unwrap();
        let names: Vec<_> = saved.files.iter().map(|f| model::plain(&f.raw)).collect();
        for name in [
            "mixed.txt",
            "deleted.txt",
            "other.txt",
            "目录/新增.txt",
            "binary.bin",
            "large.txt",
        ] {
            assert!(names.iter().any(|n| n == name), "{names:?}");
        }
        let read = |name: &str| {
            let i = names.iter().position(|n| n == name).unwrap();
            model::merge_diff::pair(path, &saved.sources[i], &saved.files[i])
                .unwrap()
                .map(|s| s.text)
        };
        assert_eq!(read("mixed.txt"), ["base\n", "saved working\n"]);
        assert_eq!(read("other.txt"), ["old\n", "other staged\n"]);
        assert_eq!(read("deleted.txt"), ["delete me\n", ""]);
        assert_eq!(read("目录/新增.txt"), ["", "新增内容\n"]);
        let literal = names.iter().find(|n| n.starts_with("[literal]")).unwrap();
        assert_eq!(read(literal), ["", "literal saved\n"]);
        assert_eq!(
            saved.revision.parents.len(),
            1,
            "Stash must open Before/After, not a merge comparison"
        );
    }

    #[test]
    fn stash_snapshot_binary_large_files_and_navigation_keep_the_correct_tree() {
        let (_dir, c, oid) = saved_fixture();
        let path = &c.repos[0].path;
        let mut preview = SnapshotPreview::default();
        preview.set(Snapshot::stash(path, 0, &oid).unwrap());
        let before = git(path, &["ls-files", "--stage"]);
        for (name, reason) in [("binary.bin", "Binary"), ("large.txt", "512 KiB")] {
            let i = preview
                .panel
                .details
                .files
                .iter()
                .position(|f| f.raw == name.as_bytes())
                .unwrap();
            preview.open(&c, i).unwrap();
            let diff = preview.panel.diff.as_ref().unwrap();
            assert!(diff.comparison.is_none());
            assert!(diff.lines.iter().any(|line| line.contains(reason)));
            assert!(diff.lines.iter().any(|line| line.contains(name)));
            assert_eq!(preview.panel.hist.revs[0].oid, oid);
        }
        for i in 0..preview.sources.len() {
            preview.open(&c, i).unwrap();
            if let Some(diff) = &preview.panel.diff.as_ref().unwrap().comparison {
                assert!(!diff.is_three_way());
            }
        }
        assert_eq!(git(path, &["ls-files", "--stage"]), before);
    }

    #[cfg(unix)]
    #[test]
    fn stash_snapshot_preserves_non_utf8_and_pathspec_sensitive_paths() {
        use std::{io::Write, process::Stdio};
        let dir = init();
        let path = dir.path();
        git(
            path,
            &[
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--allow-empty",
                "-qm",
                "base",
            ],
        );
        let bytes = b"bad-\xff\t\n.txt";
        // macOS filesystems reject non-UTF-8 names. Build the saved Git tree
        // directly so byte-path handling is still exercised on every Unix host.
        let object = |args: &[&str], input: &[u8]| {
            let mut child = crate::core::command(path)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(result.status.success());
            String::from_utf8(result.stdout).unwrap().trim().to_owned()
        };
        let blob = object(&["hash-object", "-w", "--stdin"], b"exact bytes\n");
        let mut record = format!("100644 blob {blob}\t").into_bytes();
        record.extend_from_slice(bytes);
        record.push(0);
        let tree = object(&["mktree", "-z"], &record);
        let untracked = git(path, &["commit-tree", &tree, "-m", "untracked"]);
        let base = git(path, &["rev-parse", "HEAD"]);
        let base_tree = git(path, &["rev-parse", "HEAD^{tree}"]);
        let index = git(
            path,
            &["commit-tree", &base_tree, "-p", &base, "-m", "index"],
        );
        let oid = git(
            path,
            &[
                "commit-tree",
                &base_tree,
                "-p",
                &base,
                "-p",
                &index,
                "-p",
                &untracked,
                "-m",
                "stash",
            ],
        );
        let saved = Snapshot::stash(path, 0, &oid).unwrap();
        assert_eq!(saved.files.len(), 1);
        assert_eq!(saved.files[0].raw, bytes);
        let text = model::merge_diff::pair(path, &saved.sources[0], &saved.files[0]).unwrap();
        assert_eq!(text[1].text, "exact bytes\n");
    }
}
