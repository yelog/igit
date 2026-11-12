//! Repository-scoped branch browsing and reviewed local Git operations.
mod compare;
mod display;
mod keymap;
mod loading;
mod model;
mod navigation;
mod operations;
mod recent;
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
pub(crate) use operations::checkout::{
    Decision as CheckoutDecision, TOKEN as CHECKOUT_REVIEW_TOKEN,
};
use operations::{checkout, Operation, Review};
use ratatui::{layout::Rect, Frame};
use std::path::PathBuf;
use tree::{Kind, Sort, Tree};

pub(crate) const MERGE_REVIEW_TOKEN: &str = "branch:merge";
pub(crate) const PULL_REVIEW_TOKEN: &str = "branch:pull";
pub(crate) const PUSH_REVIEW_TOKEN: &str = "branch:push";

#[derive(Clone)]
pub(crate) struct CheckoutDone {
    pub(crate) root: usize,
    pub(crate) path: PathBuf,
    pub(crate) reference: String,
    pub(crate) branch: String,
    pub(crate) head: String,
    pub(crate) upstream: String,
    last_used: std::collections::BTreeMap<String, recent::LastUsed>,
}

pub(crate) struct SyncOutcome {
    pub(crate) level: NoticeLevel,
    pub(crate) message: String,
    pub(crate) checkout: Option<CheckoutDone>,
    pub(crate) prompt: bool,
    pub(crate) refresh: bool,
}

struct SyncDone {
    level: NoticeLevel,
    message: String,
    checkout: Option<CheckoutDone>,
    retry: Option<NameInput>,
    refresh: bool,
}

enum SyncReport {
    Done(Box<SyncDone>),
    Prompt(Box<checkout::Checkout>),
}

#[derive(Clone)]
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
    ViewOptions,
    GroupBy,
    Sort(Sort),
    MenuSurface,
    Submit,
    Cancel,
    Compare,
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
    view_menu_cursor: usize,
    hits: Vec<(Rect, Hit)>,
    input: Option<NameInput>,
    review: Option<Review>,
    checkout: Option<checkout::Checkout>,
    checkout_root: Option<usize>,
    writes_blocked: bool,
    comparison: Option<Box<crate::log::compare::Comparison>>,
    compare_menu: Option<(u16, u16)>,
    syncing: Option<std::sync::mpsc::Receiver<SyncReport>>,
    pub(crate) panes: PaneLayout,
    pub(crate) icons: IconMode,
}

impl BranchPanel {
    pub(crate) fn block_writes(&mut self, blocked: bool) {
        self.writes_blocked = blocked;
    }

    pub(crate) fn syncing(&self) -> bool {
        self.syncing.is_some()
    }

    pub(crate) fn poll_sync(&mut self) -> Option<SyncOutcome> {
        use std::sync::mpsc::TryRecvError;
        let report = match self.syncing.as_ref()?.try_recv() {
            Ok(report) => report,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => SyncReport::Done(Box::new(SyncDone {
                level: NoticeLevel::Error,
                message: "Branch sync worker stopped unexpectedly".into(),
                checkout: None,
                retry: None,
                refresh: true,
            })),
        };
        self.syncing = None;
        Some(match report {
            SyncReport::Done(done) => {
                let SyncDone {
                    level,
                    message,
                    checkout,
                    retry,
                    refresh,
                } = *done;
                if self.input.is_none() {
                    self.input = retry.map(|mut input| {
                        input.error = message.clone();
                        input
                    });
                }
                SyncOutcome {
                    level,
                    message,
                    checkout,
                    prompt: false,
                    refresh,
                }
            }
            SyncReport::Prompt(checkout) => {
                self.checkout = Some(*checkout);
                SyncOutcome {
                    level: NoticeLevel::Warning,
                    message: String::new(),
                    checkout: None,
                    prompt: true,
                    refresh: false,
                }
            }
        })
    }

    pub(crate) fn has_repository_pane(&self) -> bool {
        self.repos.len() > 1
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        if let Some(comparison) = &self.comparison {
            return comparison.focused_pane();
        }
        if self.focus == Focus::Repositories && self.has_repository_pane() {
            PaneId::BranchRepos
        } else {
            PaneId::Branches
        }
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        if let Some(comparison) = &mut self.comparison {
            comparison.focus_pane(id);
            return;
        }
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
        if operation == Operation::Checkout {
            return Ok(self.start_checkout(c, root, branch, operation, name, None));
        }
        let review = Review::prepare(c, root, branch, operation, name)?;
        let action = Action::Confirm {
            title: operation.title().into(),
            body: review.body.clone(),
            token: match operation {
                Operation::Merge => MERGE_REVIEW_TOKEN,
                Operation::Pull => PULL_REVIEW_TOKEN,
                Operation::Push => PUSH_REVIEW_TOKEN,
                _ => "branch:apply",
            }
            .into(),
        };
        self.review = Some(review);
        Ok(action)
    }

    fn start_checkout(
        &mut self,
        c: &Context,
        root: usize,
        branch: &Branch,
        operation: Operation,
        name: &str,
        retry: Option<NameInput>,
    ) -> Action {
        let context = c.clone();
        let branch = branch.clone();
        let name = name.to_owned();
        let refresh_on_error = retry.is_none();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let report = match Review::prepare(&context, root, &branch, operation, &name) {
                Ok(review) => match review.execute_switch() {
                    Ok(message) => match review.checkout_outcome() {
                        Ok(checkout) => SyncReport::Done(Box::new(SyncDone {
                            level: NoticeLevel::Success,
                            message,
                            checkout: Some(checkout),
                            retry: None,
                            refresh: false,
                        })),
                        Err(error) => SyncReport::Done(Box::new(SyncDone {
                            level: NoticeLevel::Error,
                            message: format!(
                                "Checkout finished, but its final HEAD could not be verified: {error:#}"
                            ),
                            checkout: None,
                            retry: None,
                            refresh: true,
                        })),
                    },
                    Err(error) => match checkout::Checkout::prepare(review, &context, error) {
                        Ok(prompt) => SyncReport::Prompt(Box::new(prompt)),
                        Err(error) => SyncReport::Done(Box::new(SyncDone {
                            level: NoticeLevel::Error,
                            message: format!("{error:#}"),
                            checkout: None,
                            retry: None,
                            refresh: true,
                        })),
                    },
                },
                Err(error) => SyncReport::Done(Box::new(SyncDone {
                    level: NoticeLevel::Error,
                    message: format!("{error:#}"),
                    checkout: None,
                    retry,
                    refresh: refresh_on_error,
                })),
            };
            let _ = send.send(report);
        });
        self.syncing = Some(receive);
        Action::Notice(
            NoticeLevel::Info,
            if operation == Operation::Track {
                "Tracking remote branch…"
            } else {
                "Switching branch…"
            }
            .into(),
        )
    }

    pub(crate) fn draw_checkout(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(checkout) = &mut self.checkout {
            checkout.draw(frame, area, self.icons);
        }
    }

    pub(crate) fn checkout_event(&mut self, event: &Event) -> CheckoutDecision {
        self.checkout
            .as_mut()
            .map_or(CheckoutDecision::Cancel, |checkout| checkout.event(event))
    }

    pub(crate) fn cancel_checkout(&mut self) {
        self.checkout = None;
        self.checkout_root = None;
    }

    pub(crate) fn pending_checkout(&self) -> bool {
        self.checkout.is_some()
    }

    pub(crate) fn execute_checkout(&mut self, c: &Context, force: bool) -> Action {
        let Some(checkout) = self.checkout.take() else {
            return Action::Notice(
                NoticeLevel::Warning,
                "Checkout review expired; select the branch again".into(),
            );
        };
        self.checkout_root = Some(checkout.root());
        let context = c.clone();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (level, message, checkout) = checkout.execute_outcome(&context, force);
            let refresh = checkout.is_none();
            let _ = send.send(SyncReport::Done(Box::new(SyncDone {
                level,
                message,
                checkout,
                retry: None,
                refresh,
            })));
        });
        self.syncing = Some(receive);
        Action::Notice(
            NoticeLevel::Info,
            if force {
                "Force Checkout running…"
            } else {
                "Smart Checkout running…"
            }
            .into(),
        )
    }

    pub(crate) fn take_checkout_conflicts(&mut self, c: &Context) -> bool {
        self.checkout_root.take().is_some_and(|root| {
            c.scope.contains(&root)
                && c.repos
                    .get(root)
                    .is_some_and(|repo| crate::core::update::conflicts(&repo.path))
        })
    }

    fn operate(&mut self, operation: Operation, c: &Context) -> Result<Action> {
        if self.syncing() || self.writes_blocked {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Wait for the Git operation to finish".into(),
            ));
        }
        if self.pending_checkout() {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Resolve the checkout review first".into(),
            ));
        }
        if let Some(status) = self.read_status() {
            return Ok(Action::Notice(
                if self.reads_pending() {
                    NoticeLevel::Warning
                } else {
                    NoticeLevel::Error
                },
                status.into(),
            ));
        }
        self.review = None;
        let Some((root, mut source)) = self.selection() else {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                "Select a branch first".into(),
            ));
        };
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
            if source.remote {
                return Ok(Action::Notice(
                    NoticeLevel::Warning,
                    "Rename is available for local branches only".into(),
                ));
            }
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
        if input.operation == Operation::Track {
            let retry = Some(input.clone());
            return self.start_checkout(
                c,
                input.root,
                &input.source,
                input.operation,
                &input.name,
                retry,
            );
        }
        match self.prepare(c, input.root, &input.source, input.operation, &input.name) {
            Ok(action) => action,
            Err(error) => {
                input.error = format!("{error:#}");
                let action = Action::Notice(NoticeLevel::Error, input.error.clone());
                self.input = Some(input);
                action
            }
        }
    }

    fn handle(&mut self, event: &Event, c: &Context) -> Result<Action> {
        if matches!(event, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return Ok(Action::None);
        }
        if let Some(comparison) = &mut self.comparison {
            if comparison.wants_close(event) {
                self.close_comparison();
                return Ok(Action::None);
            }
            return Ok(comparison.event(event, c, &self.panes));
        }
        if self.compare_menu.is_some() {
            return self.compare_menu_event(event, c);
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
                    K::Char('c') => return self.open_comparison(c),
                    K::Char('p' | 'U') => return self.operate(Operation::Pull, c),
                    K::Char('P') => return self.operate(Operation::Push, c),
                    _ => (),
                }
            }
            Event::Mouse(m) => match m.kind {
                M::Down(MouseButton::Right) => {
                    if let Some(Hit::Row(row)) = self.hit_at(m.column, m.row) {
                        self.focus = Focus::Branches;
                        self.tree.cursor = row;
                        self.cancel_key_sequence();
                        if self.comparison_label().is_some() {
                            self.compare_menu = Some((m.column, m.row));
                        }
                    }
                }
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
    fn cancel_mouse(&mut self) {
        if let Some(comparison) = &mut self.comparison {
            comparison.active_mut().cancel_mouse();
        }
    }

    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.request_load(c);
        if let Some(comparison) = &mut self.comparison {
            comparison.refresh(c);
        }
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if let Some(comparison) = &mut self.comparison {
            comparison.draw(f, area, c, &mut self.panes);
            return;
        }
        self.draw_content(f, area);
        self.draw_compare_menu(f, area);
    }

    fn event(&mut self, e: &Event, c: &Context) -> Action {
        self.handle(e, c)
            .unwrap_or_else(|e| Action::Notice(NoticeLevel::Error, format!("{e:#}")))
    }

    fn editing(&self) -> bool {
        if let Some(comparison) = &self.comparison {
            return comparison.active().editing();
        }
        self.input.is_some()
            || self.view_menu
            || self.compare_menu.is_some()
            || self.focused_search().is_some_and(|prompt| prompt.editing())
    }

    fn help(&self) -> &'static str {
        if self.comparison.is_some() {
            "Tab Panes   [/] Compare sides   / Search   Enter Diff   Esc Branches   F1 Help"
        } else if self.compare_menu.is_some() {
            "Enter Compare   Esc Close   F1 Help"
        } else if self
            .input
            .as_ref()
            .is_some_and(|input| input.operation == Operation::Track)
        {
            "Enter Switch   Ctrl-U Clear   Esc Cancel"
        } else if self.input.is_some() {
            "Enter Review   Ctrl-U Clear   Esc Cancel"
        } else if self.view_menu {
            "j/k Select   Space/Enter Apply   Esc Close   F1 Help"
        } else if self.focused_pane() == PaneId::BranchRepos {
            "j/k Repos   Enter Branches   Tab Panes   F1 Help"
        } else if self.has_repository_pane() {
            "Tab Repos   c Compare   o View Options   a New   d Delete   Enter Switch   m Merge   r Rename   p Pull   P Push   F1 Help"
        } else {
            "c Compare   o View Options   a New   d Delete   Enter Switch   m Merge   r Rename   p Pull   P Push   F1 Help"
        }
    }

    fn selected_text(&self) -> Option<&str> {
        self.comparison.as_ref()?.active().selected_text()
    }

    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if !matches!(
            token,
            "branch:apply" | MERGE_REVIEW_TOKEN | PULL_REVIEW_TOKEN | PUSH_REVIEW_TOKEN
        ) {
            return Action::None;
        }
        let Some(review) = self.review.take() else {
            return Action::Notice(
                NoticeLevel::Warning,
                "Review expired; select the branch again".into(),
            );
        };
        if review.is_sync() {
            let context = c.clone();
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let report = match review.execute(&context) {
                    Ok(message) => SyncReport::Done(Box::new(SyncDone {
                        level: NoticeLevel::Success,
                        message,
                        checkout: None,
                        retry: None,
                        refresh: true,
                    })),
                    Err(error) => SyncReport::Done(Box::new(SyncDone {
                        level: NoticeLevel::Error,
                        message: format!("{error:#}"),
                        checkout: None,
                        retry: None,
                        refresh: true,
                    })),
                };
                let _ = send.send(report);
            });
            self.syncing = Some(receive);
            return Action::Notice(NoticeLevel::Info, "Branch sync running…".into());
        }
        // A failing merge may have written conflict stages. Always refresh all panels.
        match review.execute(c) {
            Ok(message) => Action::Refresh(NoticeLevel::Success, message),
            Err(error) => Action::Refresh(NoticeLevel::Error, format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests;
