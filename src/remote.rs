mod editor;
mod model;
mod operations;
mod view;

use crate::{
    core::Context,
    ui::{
        help::{HelpContent, HelpSection},
        icons::IconMode,
        key_sequence::KeySequence,
        panes::PaneLayout,
        *,
    },
};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEventKind as M,
};
use model::{Remote, Repository};
use operations::{Operation, Review};
use ratatui::{layout::Rect, Frame};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub(crate) const REVIEW_TOKEN: &str = "remote-operation";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Repository(usize),
    Remote {
        root: usize,
        remote: usize,
    },
    Url {
        root: usize,
        remote: usize,
        push: bool,
        url: usize,
    },
    Message(usize),
}

impl Row {
    fn root(self) -> usize {
        match self {
            Self::Repository(root)
            | Self::Remote { root, .. }
            | Self::Url { root, .. }
            | Self::Message(root) => root,
        }
    }

    fn remote(self) -> Option<usize> {
        match self {
            Self::Remote { remote, .. } | Self::Url { remote, .. } => Some(remote),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Hit {
    Add,
    Edit,
    Remove,
    Row(usize),
}

#[derive(Default)]
pub struct RemotePanel {
    repos: Vec<Repository>,
    rows: Vec<Row>,
    folded: BTreeSet<usize>,
    cursor: usize,
    goto_pending: bool,
    top: usize,
    viewport: Rect,
    hits: Vec<(Rect, Hit)>,
    last_click: Option<(usize, Instant)>,
    editor: Option<editor::Editor>,
    review: Option<Review>,
    next_selection: Option<(usize, Option<String>)>,
    writes_blocked: bool,
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
}

impl RemotePanel {
    fn repository(&self, root: usize) -> Option<&Repository> {
        self.repos.iter().find(|repo| repo.root == root)
    }

    fn selection(&self) -> Option<(usize, &Remote)> {
        let row = *self.rows.get(self.cursor)?;
        Some((
            row.root(),
            self.repository(row.root())?.remotes.get(row.remote()?)?,
        ))
    }

    fn identity(&self) -> Option<(usize, Option<String>)> {
        let row = *self.rows.get(self.cursor)?;
        Some((
            row.root(),
            self.selection().map(|(_, remote)| remote.name.clone()),
        ))
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        self.hits.clear();
        self.last_click = None;
        for repo in &self.repos {
            self.rows.push(Row::Repository(repo.root));
            if self.folded.contains(&repo.root) {
                continue;
            }
            if repo.remotes.is_empty() {
                self.rows.push(Row::Message(repo.root));
            }
            for (remote, entry) in repo.remotes.iter().enumerate() {
                self.rows.push(Row::Remote {
                    root: repo.root,
                    remote,
                });
                self.rows.extend((1..entry.urls.len()).map(|url| Row::Url {
                    root: repo.root,
                    remote,
                    push: false,
                    url,
                }));
                self.rows
                    .extend((0..entry.push_urls.len()).map(|url| Row::Url {
                        root: repo.root,
                        remote,
                        push: true,
                        url,
                    }));
            }
        }
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
    }

    fn restore(&mut self, root: usize, name: Option<&str>) {
        self.cursor = name
            .and_then(|name| {
                let remote = self
                    .repository(root)?
                    .remotes
                    .iter()
                    .position(|remote| remote.name == name)?;
                self.rows
                    .iter()
                    .position(|row| *row == Row::Remote { root, remote })
            })
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|row| *row == Row::Repository(root))
            })
            .unwrap_or_else(|| self.cursor.min(self.rows.len().saturating_sub(1)));
    }

    fn toggle(&mut self) {
        let Some(Row::Repository(root)) = self.rows.get(self.cursor).copied() else {
            return;
        };
        if !self.folded.remove(&root) {
            self.folded.insert(root);
        }
        self.rebuild();
        self.restore(root, None);
    }

    fn horizontal(&mut self, left: bool) {
        let Some(row) = self.rows.get(self.cursor).copied() else {
            return;
        };
        let root = row.root();
        if matches!(row, Row::Repository(_)) {
            if self.folded.contains(&root) != left {
                self.toggle();
            } else if !left {
                self.cursor = (self.cursor + 1).min(self.rows.len().saturating_sub(1));
            }
        } else if left {
            self.restore(root, None);
        }
    }

    pub(crate) fn block_writes(&mut self, blocked: bool) {
        self.writes_blocked = blocked;
    }

    fn begin_editor(&mut self, c: &Context, add: bool) -> Action {
        self.review = None;
        let Some(row) = self.rows.get(self.cursor).copied() else {
            return Action::Notice(NoticeLevel::Warning, "Select a repository first".into());
        };
        let root = row.root();
        if !c.scope.contains(&root) {
            return Action::Notice(NoticeLevel::Warning, "Repository is outside Roots".into());
        }
        let source = if add {
            None
        } else if let Some((_, remote)) = self.selection() {
            Some(remote.clone())
        } else {
            return Action::Notice(NoticeLevel::Warning, "Select a remote to edit".into());
        };
        let default_name = if self
            .repository(root)
            .is_some_and(|repo| !repo.remotes.iter().any(|remote| remote.name == "origin"))
        {
            "origin"
        } else {
            ""
        };
        self.editor = Some(editor::Editor::new(root, source, default_name));
        Action::Redraw
    }

    fn prepare(&mut self, c: &Context, root: usize, operation: Operation) -> Result<Action> {
        if self.writes_blocked {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Wait for the Git operation to finish".into(),
            ));
        }
        self.review = None;
        let review = Review::prepare(c, root, operation)?;
        let action = Action::Confirm {
            title: review.title().into(),
            body: review.body(),
            token: REVIEW_TOKEN.into(),
        };
        self.review = Some(review);
        Ok(action)
    }

    fn review_editor(&mut self, c: &Context) -> Result<Action> {
        let Some(editor) = &self.editor else {
            return Ok(Action::None);
        };
        let target = editor.target();
        let operation = match &editor.source {
            Some(before) => Operation::Edit {
                before: before.clone(),
                after: target,
            },
            None => Operation::Add(target),
        };
        self.prepare(c, editor.root, operation)
    }

    fn remove(&mut self, c: &Context) -> Result<Action> {
        let Some((root, remote)) = self.selection() else {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Select a remote to remove".into(),
            ));
        };
        self.prepare(c, root, Operation::Remove(remote.clone()))
    }

    fn activate(&mut self, hit: Hit, c: &Context) -> Result<Action> {
        match hit {
            Hit::Add => Ok(self.begin_editor(c, true)),
            Hit::Edit => Ok(self.begin_editor(c, false)),
            Hit::Remove => self.remove(c),
            Hit::Row(index) => {
                self.cursor = index;
                if matches!(self.rows.get(index), Some(Row::Repository(_))) {
                    self.toggle();
                } else if self.last_click.is_some_and(|(previous, time)| {
                    previous == index && time.elapsed() <= Duration::from_millis(400)
                }) {
                    self.last_click = None;
                    return Ok(self.begin_editor(c, false));
                } else {
                    self.last_click = Some((index, Instant::now()));
                }
                Ok(Action::Redraw)
            }
        }
    }

    fn handle(&mut self, event: &Event, c: &Context) -> Result<Action> {
        if matches!(event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return Ok(Action::None);
        }
        if let Some(editor) = &mut self.editor {
            return match editor.event(event) {
                editor::Event::Keep => Ok(Action::Redraw),
                editor::Event::Cancel => {
                    self.editor = None;
                    self.review = None;
                    Ok(Action::Redraw)
                }
                editor::Event::Review => self.review_editor(c),
            };
        }
        if let Event::Key(key) = event {
            if key.code == K::Char('g') && key.modifiers.is_empty() {
                if self.goto_pending {
                    self.goto_pending = false;
                    self.cursor = 0;
                } else {
                    self.goto_pending = true;
                }
                return Ok(Action::None);
            }
            self.goto_pending = false;
        } else {
            self.goto_pending = false;
        }
        let length = self.rows.len();
        let page = usize::from(self.viewport.height).max(1);
        match event {
            Event::Key(key) => match (key.code, key.modifiers) {
                (K::Char('j') | K::Down, Mods::NONE) => {
                    self.cursor = (self.cursor + 1).min(length.saturating_sub(1));
                }
                (K::Char('k') | K::Up, Mods::NONE) => {
                    self.cursor = self.cursor.saturating_sub(1);
                }
                (K::Home, Mods::NONE) => self.cursor = 0,
                (K::Char('G') | K::End, _) => self.cursor = length.saturating_sub(1),
                (K::PageDown, _) | (K::Char('f'), Mods::CONTROL) => {
                    self.cursor = (self.cursor + page).min(length.saturating_sub(1));
                }
                (K::PageUp, _) | (K::Char('b'), Mods::CONTROL) => {
                    self.cursor = self.cursor.saturating_sub(page);
                }
                (K::Char('h') | K::Left, Mods::NONE) => self.horizontal(true),
                (K::Char('l') | K::Right, Mods::NONE) => self.horizontal(false),
                (K::Char(' '), Mods::NONE) => self.toggle(),
                (K::Enter, Mods::NONE)
                    if matches!(self.rows.get(self.cursor), Some(Row::Repository(_))) =>
                {
                    self.toggle();
                }
                (K::Char('a') | K::Char('+') | K::Insert, _) => {
                    return Ok(self.begin_editor(c, true));
                }
                (K::Char('e') | K::F(2) | K::Enter, _) => {
                    return Ok(self.begin_editor(c, false));
                }
                (K::Char('d') | K::Char('-') | K::Delete, _) => return self.remove(c),
                _ => return Ok(Action::None),
            },
            Event::Mouse(mouse) => {
                if mouse.kind == M::Down(MouseButton::Left) {
                    if let Some(hit) = self
                        .hits
                        .iter()
                        .find(|(rect, _)| inside(*rect, mouse.column, mouse.row))
                        .map(|(_, hit)| *hit)
                    {
                        return self.activate(hit, c);
                    }
                } else if inside(self.viewport, mouse.column, mouse.row) {
                    match mouse.kind {
                        M::ScrollDown => {
                            self.cursor = (self.cursor + 3).min(length.saturating_sub(1));
                        }
                        M::ScrollUp => self.cursor = self.cursor.saturating_sub(3),
                        _ => return Ok(Action::None),
                    }
                }
            }
            _ => return Ok(Action::None),
        }
        Ok(Action::Redraw)
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        if self.editor.is_some() {
            return HelpContent::new(
                HelpSection::new(
                    "Remote / Editor",
                    &[
                        ("Tab", "Focus next field or button"),
                        ("Shift-Tab", "Focus previous field or button"),
                        ("Enter", "New URL line or activate button"),
                        ("Ctrl-u", "Clear the current field"),
                        ("Ctrl-s", "Review remote changes"),
                        ("Esc", "Discard remote editor"),
                    ],
                ),
                vec![],
            );
        }
        HelpContent::new(
            HelpSection::new(
                "Remote / Remotes",
                &[
                    ("j", "Select next remote or repository"),
                    ("k", "Select previous remote or repository"),
                    ("h", "Collapse repository or move to parent"),
                    ("l", "Expand repository or move to child"),
                    ("Space", "Toggle repository"),
                    ("Enter", "Edit remote or toggle repository"),
                    ("a", "Add remote to selected repository"),
                    ("e", "Edit selected remote"),
                    ("d", "Review removal of selected remote"),
                    ("gg", "Select first row"),
                    ("G", "Select last row"),
                    ("PgDn", "Move down one page"),
                    ("PgUp", "Move up one page"),
                ],
            ),
            vec![],
        )
    }

    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        self.goto_pending.then_some(KeySequence::FirstItem)
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.goto_pending = false;
    }
}

impl Panel for RemotePanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        let keep = self.next_selection.take().or_else(|| self.identity());
        self.repos = model::load(c);
        self.rebuild();
        if let Some((root, name)) = keep {
            self.restore(root, name.as_deref());
        }
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.draw_content(f, area, c);
    }

    fn event(&mut self, event: &Event, c: &Context) -> Action {
        self.handle(event, c).unwrap_or_else(|error| {
            Action::Notice(
                NoticeLevel::Error,
                model::label(&format!("Remote: {error:#}")),
            )
        })
    }

    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token != REVIEW_TOKEN || self.writes_blocked {
            return Action::None;
        }
        let Some(review) = self.review.take() else {
            return Action::None;
        };
        let destination = review.destination();
        match review.execute(c) {
            Ok(message) => {
                self.editor = None;
                self.next_selection = Some(destination);
                Action::Refresh(NoticeLevel::Success, message)
            }
            Err(error) => Action::Refresh(
                NoticeLevel::Error,
                model::label(&format!("Remote: {error:#}")),
            ),
        }
    }

    fn editing(&self) -> bool {
        self.editor.is_some()
    }

    fn help(&self) -> &'static str {
        if self.editor.is_some() {
            "Tab Fields   Enter New line   Ctrl-S Review   Esc Cancel   F1 Help"
        } else {
            "a Add   e Edit   d Remove   Enter Edit / expand   r Roots   F1 Help"
        }
    }
}

#[cfg(test)]
mod tests;
