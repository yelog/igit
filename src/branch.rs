//! Repository-scoped branch browsing and reviewed local Git operations.
mod keymap;
mod loading;
mod model;
mod navigation;
mod operations;
mod rows;
mod tree;
mod view;

use crate::{
    core::Context,
    ui::{icons::IconMode, panes::PaneLayout, *},
};
use anyhow::{Context as _, Result};
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEventKind as M,
};
use model::{Branch, Repository, Snapshot};
use operations::{Operation, Review};
use ratatui::{layout::Rect, Frame};
use std::collections::BTreeMap;
use tree::{Kind, Tree};

struct NameInput {
    root: usize,
    source: Branch,
    operation: Operation,
    name: String,
    error: String,
}

#[derive(Clone, Copy)]
enum Hit {
    Row(usize),
    Operation(Operation),
    Submit,
    Cancel,
}

#[derive(Default)]
pub struct BranchPanel {
    repos: Vec<Repository>,
    tree: Tree,
    loading: loading::Loading,
    upstream_width: Option<usize>,
    top: usize,
    viewport: Rect,
    goto_pending: bool,
    hits: Vec<(Rect, Hit)>,
    input: Option<NameInput>,
    review: Option<Review>,
    pub(crate) panes: PaneLayout,
    pub(crate) icons: IconMode,
    pub(crate) shortcut_labels: BTreeMap<&'static str, String>,
}

impl BranchPanel {
    fn selection(&self) -> Option<(usize, Branch)> {
        let row = self.tree.rows.get(self.tree.cursor)?;
        if let Kind::Branch(branch) = &row.kind {
            Some((row.root, branch.clone()))
        } else {
            None
        }
    }

    fn prepare(
        &mut self,
        c: &Context,
        root: usize,
        branch: &Branch,
        operation: Operation,
        name: &str,
    ) -> Result<Action> {
        let review = Review::prepare(c, root, branch, operation, name)?;
        let action = Action::Confirm {
            title: operation.title().into(),
            body: review.body.clone(),
            token: "branch:apply".into(),
        };
        self.review = Some(review);
        Ok(action)
    }

    fn operate(&mut self, operation: Operation, c: &Context) -> Result<Action> {
        if let Some(status) = self.read_status() {
            anyhow::bail!("{status}");
        }
        self.review = None;
        let (root, mut source) = self.selection().context("Select a branch first")?;
        let mut operation = operation;
        let mut name = String::new();
        if operation == Operation::Checkout && source.remote {
            let snapshot = self
                .repos
                .iter()
                .find(|r| r.root == root)
                .and_then(|r| r.state.as_ref().ok())
                .context("Refresh branches first")?;
            name = snapshot
                .local_name(&source)
                .context("Remote is not configured; fetch or refresh first")?
                .into();
            // An existing matching tracking branch can be switched directly.
            if let Some(local) = snapshot
                .branches
                .get(&format!("refs/heads/{name}"))
                .filter(|b| b.upstream == source.reference)
            {
                source = local.clone();
                name.clear();
            } else {
                operation = Operation::Track;
            }
        }
        if operation == Operation::Rename {
            anyhow::ensure!(
                !source.remote,
                "Rename is available for local branches only"
            );
            name = source.name.clone();
        }
        if matches!(
            operation,
            Operation::Create | Operation::Rename | Operation::Track
        ) {
            self.input = Some(NameInput {
                root,
                source,
                operation,
                name,
                error: String::new(),
            });
            Ok(Action::None)
        } else {
            self.prepare(c, root, &source, operation, &name)
        }
    }

    fn submit(&mut self, c: &Context) -> Action {
        let Some(mut input) = self.input.take() else {
            return Action::None;
        };
        match self.prepare(c, input.root, &input.source, input.operation, &input.name) {
            Ok(action) => action,
            Err(error) => {
                input.error = format!("{error:#}");
                self.input = Some(input);
                Action::None
            }
        }
    }

    fn handle(&mut self, event: &Event, c: &Context) -> Result<Action> {
        if matches!(event, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return Ok(Action::None);
        }
        if self.input.is_some() {
            self.cancel_key_sequence();
            match event {
                Event::Key(k) => match k.code {
                    K::Esc => self.input = None,
                    K::Enter => return Ok(self.submit(c)),
                    K::Backspace => {
                        self.input.as_mut().unwrap().name.pop();
                    }
                    K::Char('u') if k.modifiers == Mods::CONTROL => {
                        self.input.as_mut().unwrap().name.clear()
                    }
                    K::Char(ch)
                        if !k
                            .modifiers
                            .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER)
                            && !ch.is_control() =>
                    {
                        self.input.as_mut().unwrap().name.push(ch)
                    }
                    _ => (),
                },
                Event::Paste(text) => self
                    .input
                    .as_mut()
                    .unwrap()
                    .name
                    .extend(text.chars().filter(|c| !c.is_control())),
                Event::Mouse(m) if m.kind == M::Down(MouseButton::Left) => {
                    match self
                        .hits
                        .iter()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                        .map(|(_, hit)| hit)
                    {
                        Some(Hit::Submit) => return Ok(self.submit(c)),
                        Some(Hit::Cancel) => self.input = None,
                        _ => (),
                    }
                }
                _ => (),
            }
            return Ok(Action::None);
        }
        if let Event::Key(key) = event {
            if self.navigation_key(key) {
                return Ok(Action::None);
            }
        } else if !matches!(event, Event::Mouse(m) if m.kind == M::Moved) {
            self.cancel_key_sequence();
        }
        match event {
            Event::Key(k)
                if !k
                    .modifiers
                    .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER) =>
            {
                match k.code {
                    K::Char('h') | K::Left => self.tree.horizontal(true, &self.repos),
                    K::Char('l') | K::Right => self.tree.horizontal(false, &self.repos),
                    K::Char(' ') => self.tree.toggle(&self.repos),
                    K::Enter if self.selection().is_none() => self.tree.toggle(&self.repos),
                    K::Enter => return self.operate(Operation::Checkout, c),
                    K::Char('a') => return self.operate(Operation::Create, c),
                    K::Char('d') => return self.operate(Operation::Delete, c),
                    K::Char('m') => return self.operate(Operation::Merge, c),
                    K::Char('r') => return self.operate(Operation::Rename, c),
                    _ => (),
                }
            }
            Event::Mouse(m) => match m.kind {
                M::Down(MouseButton::Left) => {
                    let hit = self
                        .hits
                        .iter()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                        .map(|(_, hit)| *hit);
                    match hit {
                        Some(Hit::Row(row)) => {
                            self.tree.cursor = row;
                            self.tree.toggle(&self.repos);
                        }
                        Some(Hit::Operation(op)) => return self.operate(op, c),
                        _ => (),
                    }
                }
                M::ScrollDown if inside(self.viewport, m.column, m.row) => self.tree.move_by(3),
                M::ScrollUp if inside(self.viewport, m.column, m.row) => self.tree.move_by(-3),
                _ => (),
            },
            _ => (),
        }
        self.keep_cursor_visible();
        Ok(Action::None)
    }
}

impl Panel for BranchPanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.request_load(c);
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, _c: &Context) {
        self.draw_content(f, area);
    }

    fn event(&mut self, e: &Event, c: &Context) -> Action {
        self.handle(e, c)
            .unwrap_or_else(|e| Action::Notice(format!("{e:#}")))
    }

    fn editing(&self) -> bool {
        self.input.is_some()
    }

    fn help(&self) -> &'static str {
        if self.input.is_some() {
            "Enter Review   Ctrl-U Clear   Esc Cancel"
        } else {
            "a New   d Delete   Enter Switch   m Merge   r Rename   F1 Help"
        }
    }

    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token != "branch:apply" {
            return Action::None;
        }
        let Some(review) = self.review.take() else {
            return Action::Notice("Review expired; select the branch again".into());
        };
        // A failing merge may have written conflict stages. Always refresh all panels.
        Action::Refresh(review.execute(c).unwrap_or_else(|e| format!("{e:#}")))
    }
}

#[cfg(test)]
mod tests;
