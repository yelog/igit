//! Repository-scoped branch browsing and reviewed local Git operations.
mod display;
mod keymap;
mod loading;
mod model;
mod navigation;
mod operations;
mod rows;
mod search;
mod sync;
mod tree;
mod view;

use crate::{
    core::Context,
    ui::{
        icons::IconMode,
        panes::{PaneId, PaneLayout},
        search::Search,
        *,
    },
};
use anyhow::{Context as _, Result};
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEventKind as M,
};
use model::{Branch, Repository, Snapshot};
use navigation::{Motion, Viewport};
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
    Repository(usize),
    Row(usize),
    Operation(Operation),
    ViewOptions,
    GroupBy,
    MenuSurface,
    Submit,
    Cancel,
}

#[derive(Default, PartialEq, Eq)]
enum Focus {
    Repositories,
    #[default]
    Branches,
}

#[derive(Default)]
pub struct BranchPanel {
    repos: Vec<Repository>,
    repo_root: Option<usize>,
    repo_viewport: Viewport,
    repo_search: Search,
    branch_search: Search<String>,
    tree: Tree,
    viewport: Viewport,
    focus: Focus,
    loading: loading::Loading,
    goto_pending: bool,
    view_menu: bool,
    hits: Vec<(Rect, Hit)>,
    input: Option<NameInput>,
    review: Option<Review>,
    syncing: Option<std::sync::mpsc::Receiver<String>>,
    pub(crate) panes: PaneLayout,
    pub(crate) icons: IconMode,
    pub(crate) shortcut_labels: BTreeMap<&'static str, String>,
}

impl BranchPanel {
    pub(crate) fn syncing(&self) -> bool {
        self.syncing.is_some()
    }

    pub(crate) fn poll_sync(&mut self) -> Option<String> {
        use std::sync::mpsc::TryRecvError;
        let message = match self.syncing.as_ref()?.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => "Branch sync worker stopped unexpectedly".into(),
        };
        self.syncing = None;
        Some(message)
    }

    pub(crate) fn has_repository_pane(&self) -> bool {
        self.repos.len() > 1
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        if self.focus == Focus::Repositories && self.has_repository_pane() {
            PaneId::BranchRepos
        } else {
            PaneId::Branches
        }
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        match id {
            PaneId::BranchRepos if self.has_repository_pane() => self.focus = Focus::Repositories,
            PaneId::Branches => self.focus = Focus::Branches,
            _ => (),
        }
    }

    fn repository_index(&self) -> Option<usize> {
        let root = self.repo_root?;
        self.repos.iter().position(|repo| repo.root == root)
    }

    fn repository(&self) -> Option<&Repository> {
        self.repository_index().map(|index| &self.repos[index])
    }

    fn select_repository(&mut self, index: usize) {
        let Some(repo) = self.repos.get(index) else {
            return;
        };
        self.repo_viewport.reveal(index, self.repos.len());
        if self.repo_root != Some(repo.root) {
            self.repo_root = Some(repo.root);
            self.branch_search.invalidate();
            self.tree.rebuild(Some(repo));
            self.viewport.top = 0;
            self.keep_branch_visible();
            // Right-hand rows and actions refer to the tree from the last frame.
            self.hits
                .retain(|(_, hit)| matches!(hit, Hit::Repository(_)));
        }
    }

    fn selection(&self) -> Option<(usize, Branch)> {
        let row = self.tree.rows.get(self.tree.cursor)?;
        if self.focused_pane() != PaneId::Branches || Some(row.root) != self.repo_root {
            return None;
        }
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
        if matches!(operation, Operation::Checkout | Operation::Track) {
            return Ok(Action::Refresh(
                review.execute(c).unwrap_or_else(|e| format!("{e:#}")),
            ));
        }
        let action = Action::Confirm {
            title: operation.title().into(),
            body: review.body.clone(),
            token: "branch:apply".into(),
        };
        self.review = Some(review);
        Ok(action)
    }

    fn operate(&mut self, operation: Operation, c: &Context) -> Result<Action> {
        anyhow::ensure!(!self.syncing(), "Wait for the branch sync to finish");
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
        if self.view_menu && self.view_menu_event(event) {
            return Ok(Action::None);
        }
        if self.search_event(event) {
            self.cancel_key_sequence();
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
                if k.code == K::Char('o') && !c.repos.is_empty() {
                    self.open_view_menu();
                    return Ok(Action::None);
                }
                if self.focused_pane() == PaneId::BranchRepos {
                    if matches!(k.code, K::Enter | K::Char('l') | K::Right) {
                        self.focus_pane(PaneId::Branches);
                    }
                    return Ok(Action::None);
                }
                let repo = self.repository_index().map(|index| &self.repos[index]);
                match k.code {
                    K::Char('h') | K::Left => self.tree.horizontal(true, repo),
                    K::Char('l') | K::Right => self.tree.horizontal(false, repo),
                    K::Char(' ') => self.tree.toggle(repo),
                    K::Enter if self.selection().is_none() => self.tree.toggle(repo),
                    K::Enter => return self.operate(Operation::Checkout, c),
                    K::Char('a') => return self.operate(Operation::Create, c),
                    K::Char('d') => return self.operate(Operation::Delete, c),
                    K::Char('m') => return self.operate(Operation::Merge, c),
                    K::Char('r') => return self.operate(Operation::Rename, c),
                    K::Char('U') => return self.operate(Operation::Pull, c),
                    K::Char('P') => return self.operate(Operation::Push, c),
                    _ => (),
                }
            }
            Event::Mouse(m) => match m.kind {
                M::Down(MouseButton::Left) => {
                    if let Some(id) = self.panes.at(m.column, m.row) {
                        self.focus_pane(id);
                    }
                    let hit = self.hit_at(m.column, m.row);
                    match hit {
                        Some(Hit::ViewOptions) => self.open_view_menu(),
                        Some(Hit::Repository(index)) => self.select_repository(index),
                        Some(Hit::Row(row)) => {
                            self.tree.cursor = row;
                            let repo = self.repository_index().map(|index| &self.repos[index]);
                            self.tree.toggle(repo);
                        }
                        Some(Hit::Operation(op)) => return self.operate(op, c),
                        _ => (),
                    }
                }
                M::ScrollDown | M::ScrollUp => {
                    let pane = if inside(self.repo_viewport.area, m.column, m.row) {
                        Some(PaneId::BranchRepos)
                    } else if inside(self.viewport.area, m.column, m.row) {
                        Some(PaneId::Branches)
                    } else {
                        None
                    };
                    if let Some(id) = pane {
                        self.focus_pane(id);
                        let delta = if m.kind == M::ScrollDown { 3 } else { -3 };
                        self.navigate(Motion::Move(delta));
                    }
                }
                _ => (),
            },
            _ => (),
        }
        self.keep_branch_visible();
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
            || self.view_menu
            || self.focused_search().is_some_and(|prompt| prompt.editing())
    }

    fn help(&self) -> &'static str {
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.operation == Operation::Track)
        {
            "Enter Switch   Ctrl-U Clear   Esc Cancel"
        } else if self.input.is_some() {
            "Enter Review   Ctrl-U Clear   Esc Cancel"
        } else if self.view_menu {
            "Space/Enter Toggle   Esc Close   F1 Help"
        } else if self.focused_pane() == PaneId::BranchRepos {
            "j/k Repos   Enter Branches   Tab Panes   F1 Help"
        } else if self.has_repository_pane() {
            "Tab Repos   o Group By   a New   d Delete   Enter Switch   m Merge   r Rename   U Pull   P Push   F1 Help"
        } else {
            "o Group By   a New   d Delete   Enter Switch   m Merge   r Rename   U Pull   P Push   F1 Help"
        }
    }

    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token != "branch:apply" {
            return Action::None;
        }
        let Some(review) = self.review.take() else {
            return Action::Notice("Review expired; select the branch again".into());
        };
        if review.is_sync() {
            let context = c.clone();
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let message = review
                    .execute(&context)
                    .unwrap_or_else(|e| format!("{e:#}"));
                let _ = send.send(message);
            });
            self.syncing = Some(receive);
            return Action::Notice("Branch sync running…".into());
        }
        // A failing merge may have written conflict stages. Always refresh all panels.
        Action::Refresh(review.execute(c).unwrap_or_else(|e| format!("{e:#}")))
    }
}

#[cfg(test)]
mod tests;
