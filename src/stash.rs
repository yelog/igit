//! Repository-grouped stash browsing and reviewed operations.
mod model;
mod operations;
mod preview;
mod view;

use crate::{
    core::Context,
    log::snapshot::SnapshotPreview,
    ui::{
        help::{HelpContent, HelpSection},
        icons::IconMode,
        panes::{PaneId, PaneLayout},
        *,
    },
};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEventKind as M,
};
use model::{Entry, Repository};
use operations::{Operation, Review};
use ratatui::{layout::Rect, Frame};
use std::{collections::BTreeSet, time::Duration};

pub(crate) const REVIEW_TOKEN: &str = "stash-operation";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Row {
    root: usize,
    entry: Option<usize>,
}

pub struct StashPanel {
    repos: Vec<Repository>,
    rows: Vec<Row>,
    folded: BTreeSet<usize>,
    cursor: usize,
    top: usize,
    focus: PaneId,
    viewport: Rect,
    hits: Vec<(Rect, usize)>,
    preview: preview::Preview,
    files: SnapshotPreview,
    review: Option<Review>,
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
}

impl Default for StashPanel {
    fn default() -> Self {
        Self {
            repos: vec![],
            rows: vec![],
            folded: BTreeSet::new(),
            cursor: 0,
            top: 0,
            focus: PaneId::Stashes,
            viewport: Rect::default(),
            hits: vec![],
            preview: Default::default(),
            files: Default::default(),
            review: None,
            icons: Default::default(),
            panes: Default::default(),
        }
    }
}

impl StashPanel {
    fn repository(&self, root: usize) -> Option<&Repository> {
        self.repos.iter().find(|r| r.root == root)
    }

    fn selection(&self) -> Option<(usize, &Entry)> {
        let row = self.rows.get(self.cursor)?;
        Some((
            row.root,
            self.repository(row.root)?.entries.get(row.entry?)?,
        ))
    }

    fn identity(&self) -> Option<(usize, String)> {
        self.selection()
            .map(|(root, entry)| (root, entry.oid.clone()))
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        for repo in &self.repos {
            self.rows.push(Row {
                root: repo.root,
                entry: None,
            });
            if !self.folded.contains(&repo.root) {
                self.rows.extend((0..repo.entries.len()).map(|entry| Row {
                    root: repo.root,
                    entry: Some(entry),
                }));
            }
        }
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
    }

    fn select(&mut self, row: usize) {
        let row = row.min(self.rows.len().saturating_sub(1));
        if self.cursor != row {
            self.cursor = row;
            self.request_preview(preview::NAV_SETTLE);
        }
    }

    fn toggle(&mut self) {
        let Some(row) = self.rows.get(self.cursor).copied() else {
            return;
        };
        if row.entry.is_some() {
            return;
        }
        if !self.folded.remove(&row.root) {
            self.folded.insert(row.root);
        }
        self.rebuild();
        self.cursor = self.rows.iter().position(|r| *r == row).unwrap_or(0);
        self.request_preview(Duration::ZERO);
    }

    fn horizontal(&mut self, left: bool) {
        let Some(row) = self.rows.get(self.cursor).copied() else {
            return;
        };
        if row.entry.is_some() {
            if left {
                if let Some(parent) = self
                    .rows
                    .iter()
                    .position(|r| r.root == row.root && r.entry.is_none())
                {
                    self.select(parent);
                }
            } else {
                self.focus = PaneId::Files;
            }
        } else if self.folded.contains(&row.root) != left {
            self.toggle();
        } else if !left
            && self
                .rows
                .get(self.cursor + 1)
                .is_some_and(|r| r.root == row.root)
        {
            self.select(self.cursor + 1);
        }
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        self.focus
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        if matches!(id, PaneId::Stashes | PaneId::Files) {
            self.focus = id;
        }
    }

    fn review_operation(&mut self, c: &Context, operation: Operation) -> Result<Action> {
        self.review = None;
        let (root, source) = self
            .selection()
            .ok_or_else(|| anyhow::anyhow!("Select a stash entry first"))?;
        let entries = &self.repository(root).unwrap().entries;
        let review = Review::prepare(c, root, source, entries, operation)?;
        let action = Action::Confirm {
            title: operation.title().into(),
            body: review.body(),
            token: REVIEW_TOKEN.into(),
        };
        self.review = Some(review);
        Ok(action)
    }

    fn handle(&mut self, e: &Event, c: &Context) -> Result<Action> {
        if matches!(e, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return Ok(Action::None);
        }
        if self.editing() {
            return self.files.event(e, c);
        }
        if let Event::Mouse(m) = e {
            if self.panes.mouse(m) {
                return Ok(Action::None);
            }
            let pane = self.panes.at(m.column, m.row);
            if m.kind == M::Down(MouseButton::Left) {
                if let Some(pane) = pane {
                    self.focus_pane(pane);
                }
            }
            if pane == Some(PaneId::Files) {
                return self.files.event(e, c);
            }
            if pane == Some(PaneId::Stashes) {
                match m.kind {
                    M::Down(MouseButton::Left) => {
                        if let Some(row) = self
                            .hits
                            .iter()
                            .find(|(rect, _)| inside(*rect, m.column, m.row))
                            .map(|(_, row)| *row)
                        {
                            self.select(row);
                            self.toggle();
                        }
                    }
                    M::ScrollDown => self.select(self.cursor.saturating_add(3)),
                    M::ScrollUp => self.select(self.cursor.saturating_sub(3)),
                    _ => (),
                }
            }
            return Ok(Action::None);
        }
        if matches!(e, Event::FocusLost | Event::Resize(_, _)) {
            self.panes.cancel_drag();
            return self.files.event(e, c);
        }
        let Event::Key(k) = e else {
            return Ok(Action::None);
        };
        if k.modifiers
            .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER)
        {
            return Ok(Action::None);
        }
        match k.code {
            K::Char('a') => return self.review_operation(c, Operation::Apply),
            K::Char('P') => return self.review_operation(c, Operation::Pop),
            K::Char('d') => return self.review_operation(c, Operation::Drop),
            _ => (),
        }
        if matches!(k.code, K::Tab | K::BackTab | K::F(6)) {
            self.focus = if self.focus == PaneId::Stashes {
                PaneId::Files
            } else {
                PaneId::Stashes
            };
        } else if k.code == K::Esc {
            self.focus = PaneId::Stashes;
        } else if self.focus == PaneId::Files {
            return self.files.event(e, c);
        } else {
            match k.code {
                K::Char('j') | K::Down => self.select(self.cursor.saturating_add(1)),
                K::Char('k') | K::Up => self.select(self.cursor.saturating_sub(1)),
                K::Char('g') | K::Home => self.select(0),
                K::Char('G') | K::End => self.select(self.rows.len().saturating_sub(1)),
                K::PageDown => self.select(
                    self.cursor
                        .saturating_add(usize::from(self.viewport.height / 2).max(1)),
                ),
                K::PageUp => self.select(
                    self.cursor
                        .saturating_sub(usize::from(self.viewport.height / 2).max(1)),
                ),
                K::Char('h') | K::Left => self.horizontal(true),
                K::Char('l') | K::Right => self.horizontal(false),
                K::Char(' ') => self.toggle(),
                K::Enter if self.selection().is_some() => self.focus = PaneId::Files,
                K::Enter => self.toggle(),
                _ => (),
            }
        }
        Ok(Action::None)
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        if self.editing() {
            return self.files.keymap();
        }
        let list = HelpSection::new(
            "Stash / Repositories",
            &[
                ("j", "Select next stash or repository"),
                ("k", "Select previous stash or repository"),
                ("h", "Collapse repository or move to parent"),
                ("l", "Expand repository or focus files"),
                ("Space", "Toggle repository"),
                ("Enter", "Focus saved files or toggle repository"),
                ("g", "Select first row"),
                ("G", "Select last row"),
                ("PgDn", "Move down one page"),
                ("PgUp", "Move up one page"),
            ],
        );
        let files = HelpSection::new(
            "Stash / File Tree",
            &[
                ("j", "Select next file or directory"),
                ("k", "Select previous file or directory"),
                ("h", "Collapse directory"),
                ("l", "Expand directory"),
                ("Space", "Toggle directory"),
                ("Enter", "Open file comparison or toggle directory"),
                ("g", "Select first row"),
                ("G", "Select last row"),
                ("PgDn", "Move down one page"),
                ("PgUp", "Move up one page"),
                ("z", "Collapse all directories"),
                ("Z", "Expand all directories"),
                ("Esc", "Focus stash list"),
            ],
        );
        let actions = HelpSection::new(
            "Stash / Selected Entry",
            &[
                ("a", "Review Apply; keep the stash"),
                ("P", "Review Pop; remove after successful apply"),
                ("d", "Review Drop; remove without applying"),
            ],
        );
        if self.focus == PaneId::Stashes {
            HelpContent::new(list, vec![actions, files])
        } else {
            HelpContent::new(files, vec![actions, list])
        }
    }
}

impl Panel for StashPanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        let keep = self.identity();
        let previous = self.rows.get(self.cursor).copied();
        self.repos = model::load(c);
        self.rebuild();
        self.cursor = keep
            .and_then(|(root, oid)| {
                self.rows.iter().position(|r| {
                    r.root == root
                        && r.entry
                            .and_then(|i| self.repository(root)?.entries.get(i))
                            .is_some_and(|e| e.oid == oid)
                })
            })
            .or_else(|| {
                let previous = previous?;
                let repo = self.repository(previous.root)?;
                let entry = previous.entry.and_then(|index| {
                    (!repo.entries.is_empty()).then(|| index.min(repo.entries.len() - 1))
                });
                self.rows
                    .iter()
                    .position(|r| r.root == previous.root && r.entry == entry)
            })
            .or_else(|| self.rows.iter().position(|r| r.entry.is_some()))
            .unwrap_or(0);
        self.hits.clear();
        self.request_preview(Duration::ZERO);
        Ok(())
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.draw_content(f, area, c);
    }
    fn event(&mut self, e: &Event, c: &Context) -> Action {
        self.handle(e, c)
            .unwrap_or_else(|e| Action::Notice(format!("Stash: {e:#}")))
    }
    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token != REVIEW_TOKEN {
            return Action::None;
        }
        let Some(review) = self.review.take() else {
            return Action::None;
        };
        let result = review
            .execute(c)
            .unwrap_or_else(|error| format!("Stash: {error:#}"));
        // Native apply can leave conflicts or partial changes even on failure.
        // Shell refreshes Stash, Commit, and Conflicts for every attempted write.
        Action::Refresh(result)
    }
    fn editing(&self) -> bool {
        self.files.editing()
    }
    fn selected_text(&self) -> Option<&str> {
        self.files.selected_text()
    }
    fn help(&self) -> &'static str {
        "a Apply   P Pop   d Drop   Tab Panes   Enter Diff   F1 Help"
    }
}

#[cfg(test)]
mod tests;
