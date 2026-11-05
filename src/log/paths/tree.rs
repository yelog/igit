//! Visible nodes only: expanding a repository or directory reads one level of its worktree.
use super::*;
use crate::ui::idea::checkbox;
use anyhow::{ensure, Context as _};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::log) enum TreeHit {
    Row(usize),
    Fold(usize),
    Check(usize),
}

struct Row {
    root: usize,
    path: PathBuf, // Empty only on a repository row.
    directory: bool,
    open: bool,
    enabled: bool,
}

type Selection = BTreeSet<(usize, PathBuf)>;

pub(in crate::log) struct PathTree {
    rows: Vec<Row>,
    selected: Selection,
    initial: Selection,
    cursor: usize,
    height: usize,
}

impl PathTree {
    pub(super) fn new(filter: &Filter, c: &Context) -> Self {
        let rows = c
            .scope
            .iter()
            .map(|&root| Row {
                root,
                path: PathBuf::new(),
                directory: true,
                open: false,
                enabled: filter.includes_root(root),
            })
            .collect();
        let mut selected = Selection::new();
        for (root, path) in &filter.paths.0 {
            if let Some(id) = root {
                selected.insert((*id, path.clone()));
            } else {
                selected.extend((0..c.repos.len()).map(|id| (id, path.clone())));
            }
        }
        // Keep only the highest selected ancestor within each root.
        let initial = selected
            .iter()
            .filter(|(id, path)| {
                !path
                    .ancestors()
                    .skip(1)
                    .any(|a| selected.contains(&(*id, a.into())))
            })
            .cloned()
            .collect::<Selection>();
        Self {
            rows,
            selected: initial.clone(),
            initial,
            cursor: 0,
            height: 1,
        }
    }

    pub(super) fn can_apply(&self) -> bool {
        !self.selected.is_empty() || !self.initial.is_empty()
    }

    pub(super) fn result(&self, original: &PathFilter) -> PathFilter {
        if self.selected == self.initial {
            return original.clone();
        }
        PathFilter(
            self.selected
                .iter()
                .map(|(id, p)| (Some(*id), p.clone()))
                .collect(),
        )
    }

    pub(super) fn move_by(&mut self, delta: isize) {
        self.cursor = self
            .cursor
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
    }

    fn expand(&mut self, c: &Context) -> Result<()> {
        let Some(row) = self.rows.get(self.cursor) else {
            return Ok(());
        };
        if !row.enabled || !row.directory || row.open {
            return Ok(());
        }
        let dir = c.repos[row.root].path.join(&row.path);
        ensure!(
            dir.canonicalize()
                .with_context(|| dir.display().to_string())?
                == dir,
            "Directory became a symlink. Reopen the path tree"
        );
        let mut children = Vec::new();
        for entry in
            fs::read_dir(&dir).with_context(|| format!("Cannot expand {}", dir.display()))?
        {
            let e = entry?;
            if e.file_name() == ".git" {
                continue;
            }
            // Treat a nested repository as a leaf (its gitlink in the parent).
            // Its own files belong to its separate top-level repository row.
            let directory = e.file_type()?.is_dir() && !c.repos.iter().any(|r| r.path == e.path());
            children.push(Row {
                root: row.root,
                path: row.path.join(e.file_name()),
                directory,
                open: false,
                enabled: row.enabled,
            });
        }
        children.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.path.cmp(&b.path))
        });
        self.rows[self.cursor].open = true;
        self.rows.splice(self.cursor + 1..self.cursor + 1, children);
        Ok(())
    }

    fn collapse(&mut self) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if row.open {
            let children = self.rows[self.cursor + 1..]
                .iter()
                .take_while(|r| r.root == row.root && r.path.starts_with(&row.path))
                .count();
            self.rows.drain(self.cursor + 1..self.cursor + 1 + children);
            self.rows[self.cursor].open = false;
        } else if let Some(parent) = row.path.parent() {
            if let Some(i) = self.rows[..self.cursor]
                .iter()
                .rposition(|r| r.root == row.root && r.path == parent)
            {
                self.cursor = i;
            }
        }
    }

    fn covering(&self, root: usize, path: &Path) -> Option<PathBuf> {
        path.ancestors()
            .find(|a| self.selected.contains(&(root, (*a).into())))
            .map(Path::to_owned)
    }

    fn toggle(&mut self) {
        let Some(row) = self.rows.get(self.cursor).filter(|r| r.enabled) else {
            return;
        };
        let (id, path) = (row.root, &row.path);
        if let Some(cover) = self.covering(id, path) {
            self.selected.remove(&(id, cover.clone()));
            if cover != *path {
                // To reach this checked child, each ancestor on its route was
                // expanded. Keep its already listed siblings without a recursive scan.
                for r in &self.rows {
                    if r.root == id
                        && r.path.starts_with(&cover)
                        && !path.starts_with(&r.path)
                        && r.path.parent().is_some_and(|p| path.starts_with(p))
                    {
                        self.selected.insert((id, r.path.clone()));
                    }
                }
            }
        } else {
            self.selected
                .retain(|(root, p)| *root != id || !p.starts_with(path));
            self.selected.insert((id, path.clone()));
        }
    }

    pub(in crate::log) fn click(&mut self, hit: TreeHit, c: &Context) -> Result<()> {
        let (TreeHit::Row(i) | TreeHit::Fold(i) | TreeHit::Check(i)) = hit;
        if i >= self.rows.len() {
            return Ok(());
        }
        self.cursor = i;
        match hit {
            TreeHit::Row(_) => (),
            TreeHit::Check(_) => self.toggle(),
            TreeHit::Fold(_) if self.rows[i].open => self.collapse(),
            TreeHit::Fold(_) => self.expand(c)?,
        }
        Ok(())
    }

    pub(super) fn key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        match k.code {
            K::Up | K::Char('k') => self.move_by(-1),
            K::Down | K::Char('j') => self.move_by(1),
            K::PageUp => self.move_by(-(self.height as isize)),
            K::PageDown => self.move_by(self.height as isize),
            K::Home | K::Char('g') => self.cursor = 0,
            K::End | K::Char('G') => self.cursor = self.rows.len().saturating_sub(1),
            K::Char(' ') => self.toggle(),
            K::Left | K::Char('h') => self.collapse(),
            K::Right | K::Char('l') => {
                if let Some(row) = self.rows.get(self.cursor) {
                    if row.open {
                        if self.rows.get(self.cursor + 1).is_some_and(|r| {
                            r.root == row.root && r.path.parent() == Some(&row.path)
                        }) {
                            self.move_by(1);
                        }
                    } else {
                        self.expand(c)?;
                    }
                }
            }
            _ => (),
        }
        Ok(())
    }

    pub(super) fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        c: &Context,
        hits: &mut Vec<(Rect, Hit)>,
    ) {
        if area.is_empty() {
            return;
        }
        let h = area.height.saturating_sub(1);
        self.height = usize::from(h).max(1);
        let top = window(self.cursor, h.into(), self.rows.len());
        for (dy, row) in self.rows.iter().skip(top).take(h.into()).enumerate() {
            let i = top + dy;
            let r = Rect::new(area.x, area.y + dy as u16, area.width, 1);
            let style = fg(if row.enabled { INK } else { DIM }).bg(if i == self.cursor {
                SELECT
            } else {
                PANEL
            });
            f.render_widget(Paragraph::new("").style(style), r);
            hits.push((r, Hit::Dropdown(DropdownHit::PathTree(TreeHit::Row(i)))));
            let mut main = r;
            let indent = (row.path.components().count() * 2)
                .min(usize::from(r.width.saturating_sub(14))) as u16;
            take(&mut main, indent);
            let fold = fold_slot(icons, row.directory.then_some(row.open));
            let fold_rect = take(&mut main, cells(&fold));
            f.render_widget(Paragraph::new(fold).style(style), fold_rect);
            if row.directory && row.enabled {
                hits.push((
                    fold_rect,
                    Hit::Dropdown(DropdownHit::PathTree(TreeHit::Fold(i))),
                ));
            }
            let full = self.covering(row.root, &row.path).is_some();
            let partial = !full
                && self
                    .selected
                    .iter()
                    .any(|(id, p)| *id == row.root && p.starts_with(&row.path));
            let mark = if partial {
                Span::styled(icons.glyph(Icon::Partial), fg(BLUE))
            } else {
                checkbox(icons, full)
            };
            let check_rect = take(&mut main, mark.width() as u16 + 1);
            f.render_widget(Paragraph::new(mark).style(style), check_rect);
            if row.enabled {
                hits.push((
                    check_rect,
                    Hit::Dropdown(DropdownHit::PathTree(TreeHit::Check(i))),
                ));
            }
            let repo = &c.repos[row.root];
            let (glyph, color, label) = if let Some(name) = row.path.file_name() {
                let (glyph, color) = if row.directory {
                    (
                        icons.glyph(if row.open {
                            Icon::FolderOpen
                        } else {
                            Icon::Folder
                        }),
                        DIM,
                    )
                } else {
                    icons.file(&row.path)
                };
                (glyph, color, model::plain(name.as_encoded_bytes()))
            } else {
                (
                    icons.repo(repo.submodule),
                    repo_tint(row.root),
                    repo.name.clone(),
                )
            };
            let mut spans = vec![
                Span::styled(format!("{glyph} "), fg(color)),
                Span::raw(label),
            ];
            if row.path.as_os_str().is_empty() {
                if !row.enabled {
                    spans.push(Span::styled(" [off in Roots]", fg(DIM)));
                }
                spans.push(Span::styled(format!(" ({})", repo.path.display()), fg(DIM)));
            }
            f.render_widget(Paragraph::new(Line::from(spans)).style(style), main);
        }
        if self.rows.is_empty() && h > 0 {
            f.render_widget(Paragraph::new("No roots in scope").style(fg(DIM)), area);
        }
        f.render_widget(
            Paragraph::new("Space: check   h/l: fold/open   ↑/↓: move").style(fg(DIM)),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    }
}
