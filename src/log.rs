//! Multi-root history: preserve each root's topology and operate on the selected root's objects.
mod actions;
mod branches;
mod display;
mod file_history;
mod filters;
mod input;
mod inspector;
mod keymap;
mod loading;
mod model;
mod navigation;
mod operations;
mod paths;
mod preview;
mod refs;
pub(crate) mod snapshot;
pub(crate) mod view;
use self::display::Item;
use crate::{
    core::Context,
    ui::{
        icons::{Icon, IconMode},
        inside,
        panes::{pane_frame, Axis, PaneId, PaneLayout, SplitId},
        selection::{TextLayout, TextSelection, Wrap},
        Action, Panel,
    },
};
use anyhow::Result;
use crossterm::event::{
    Event, KeyCode as K, KeyModifiers as Mods, MouseButton, MouseEventKind as M,
};
use model::{DateFilter, Details, Filter, Graph, History, NodeGlyphs};
use ratatui::style::Stylize;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};
use view::*;

pub(crate) const REVIEW_TOKEN: &str = "log-commit-action";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Pane {
    Refs,
    #[default]
    Graph,
    Files,
    Info,
}
impl Pane {
    const ORDER: [Self; 4] = [Self::Refs, Self::Graph, Self::Files, Self::Info];

    fn id(self) -> PaneId {
        match self {
            Self::Refs => PaneId::Refs,
            Self::Graph => PaneId::Graph,
            Self::Files => PaneId::Files,
            Self::Info => PaneId::Info,
        }
    }

    fn delta(self, d: i32) -> Self {
        let a = Self::ORDER;
        a[(a.iter().position(|v| *v == self).unwrap() as i32 + d).rem_euclid(4) as usize]
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Refs,
    Text,
    Branch,
    Author,
    Date,
    Path,
}
#[derive(Clone, Copy)]
enum DropdownHit {
    Surface,
    Branch(branches::Target),
    Choice(usize),
    PathInput,
    PathTree(paths::TreeHit),
    PathText,
    AllRoots,
    DateInput(usize),
    Apply,
    Cancel,
}
#[derive(Clone)]
enum Hit {
    Ref(usize),
    Commit(usize),
    PickCommit(usize),
    ReviewPicks,
    CommitScroll(usize),
    CommitActions,
    GraphJump(usize),
    File(usize),
    Filter(Field),
    Clear(Field),
    Reset,
    ToggleRegex,
    ToggleCase,
    ViewOptions,
    ViewMenu(display::MenuHit),
    Refresh,
    Dropdown(DropdownHit),
    FileFold(bool),
    NewTab,
    CloseTab,
    Tab(usize),
    Diff,
    CloseDiff,
    More,
    Parent(usize),
}
#[derive(Clone, Default)]
struct DateRangeEditor {
    values: [String; 2],
    active: usize,
}
#[derive(Clone, Default)]
struct ViewState {
    filter: Filter,
    ref_query: String,
    ref_search: crate::ui::search::Search<String>,
    file_search: crate::ui::search::Search,
    regex: bool,
    case_sensitive: bool,
    selected: Option<(usize, String)>,
}
#[derive(Default)]
struct DiffView {
    lines: Vec<String>,
    name: String,
    y: usize,
    x: u16,
    waiting_for_files: bool,
    selection: TextSelection,
    file: Option<usize>,
    comparison: Option<crate::merge::RevisionDiff>,
}
pub struct LogPanel {
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
    hist: History,
    history_load: loading::Loading,
    filter: Filter,
    visible: Vec<usize>,
    graph: Graph,
    graph_origin: usize,
    graph_top: usize,
    graph_height: usize,
    key_prefix: Option<char>,
    node_glyphs: NodeGlyphs,
    refs: refs::RefTree,
    ref_query: String,
    ref_search: crate::ui::search::Search<String>,
    file_search: crate::ui::search::Search,
    inspector: inspector::Inspector,
    cursor: usize,
    choice: Option<usize>,
    suggestions: Vec<(String, String)>,
    path_options: Vec<usize>,
    path_dialog: Option<paths::Dialog>,
    regex: bool,
    case_sensitive: bool,
    show_refs: bool,
    show_info: bool,
    options: display::Options,
    view_menu: Option<display::Menu>,
    show_labels: bool,
    views: Vec<ViewState>,
    view_index: usize,
    details: Details,
    preview: preview::Preview,
    focus: Pane,
    editor: Option<(Field, String)>,
    branch_menu: Option<branches::Menu>,
    date_range: Option<DateRangeEditor>,
    diff: Option<DiffView>,
    history_diff: Option<DiffView>,
    history_mouse: bool,
    commit_dialog: Option<actions::Dialog>,
    commit_review: Option<operations::Review>,
    pick_mode: bool,
    picks: Vec<operations::Target>,
    hits: Vec<(Rect, Hit)>,
    bounds: Rect,
    info_selection: TextSelection,
    info_click: Option<(Rect, Hit)>,
    last: Option<(Instant, Pane, usize)>,
    limit: usize,
}
impl Default for LogPanel {
    fn default() -> Self {
        Self {
            icons: IconMode::default(),
            panes: PaneLayout::default(),
            hist: History::default(),
            history_load: Default::default(),
            filter: Filter::default(),
            visible: vec![],
            graph: Graph::default(),
            graph_origin: 0,
            graph_top: 0,
            graph_height: 0,
            key_prefix: None,
            node_glyphs: NodeGlyphs::detect(),
            refs: Default::default(),
            ref_query: String::new(),
            ref_search: Default::default(),
            file_search: Default::default(),
            inspector: Default::default(),
            cursor: 0,
            choice: None,
            suggestions: vec![],
            path_options: vec![],
            path_dialog: None,
            regex: false,
            case_sensitive: false,
            show_refs: true,
            show_info: true,
            options: Default::default(),
            view_menu: None,
            show_labels: true,
            views: vec![ViewState::default()],
            view_index: 0,
            details: Details::default(),
            preview: Default::default(),
            focus: Pane::Graph,
            editor: None,
            branch_menu: None,
            date_range: None,
            diff: None,
            history_diff: None,
            history_mouse: false,
            commit_dialog: None,
            commit_review: None,
            pick_mode: false,
            picks: Vec::new(),
            hits: vec![],
            bounds: Rect::default(),
            info_selection: TextSelection::default(),
            info_click: None,
            last: None,
            limit: 160,
        }
    }
}
impl LogPanel {
    pub(crate) fn focused_search(&self) -> Option<&crate::ui::search::Prompt> {
        if self.editor.is_some()
            || self.diff.is_some()
            || self.view_menu.is_some()
            || self.commit_dialog.is_some()
            || !self.pane_visible(self.focus)
        {
            return None;
        }
        match self.focus {
            Pane::Refs => Some(self.ref_search.prompt()),
            Pane::Files => Some(self.file_search.prompt()),
            _ => None,
        }
    }

    pub(crate) fn path_copy_target(&self, c: &Context) -> Option<crate::ui::path_copy::Target> {
        if self.path_dialog.is_some() {
            return match self.path_dialog.as_ref()? {
                paths::Dialog::Tree(tree) => tree.copy_target(c),
                paths::Dialog::Text(_) => None,
            };
        }
        if self.focus != Pane::Files || !self.pane_visible(self.focus) || self.editing() {
            return None;
        }
        self.inspector_copy_target(c)
    }

    fn inspector_copy_target(&self, c: &Context) -> Option<crate::ui::path_copy::Target> {
        self.inspector
            .copy_target(&self.details, c.repos.get(self.rev()?.root)?)
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        self.focus.id()
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        if let Some(pane) = Pane::ORDER
            .into_iter()
            .find(|p| p.id() == id && self.pane_visible(*p))
        {
            if self.focus != pane {
                self.cancel_key_sequence();
                self.last = None;
                self.focus = pane;
            }
        }
    }

    pub(crate) fn graph_motion(&self, e: &Event) -> bool {
        self.focus == Pane::Graph
            && !self.editing()
            && matches!(e, Event::Key(k)
            if k.kind != crossterm::event::KeyEventKind::Release && k.modifiers.is_empty()
            && matches!(k.code, K::Char('j' | 'k') | K::Down | K::Up))
    }

    fn rev(&self) -> Option<&model::Rev> {
        self.visible
            .get(self.cursor)
            .and_then(|i| self.hist.revs.get(*i))
    }
    fn select_row(&mut self, row: usize) {
        let row = row.min(self.visible.len().saturating_sub(1));
        if self.cursor != row {
            self.cursor = row;
            if self.history_load.stale() {
                self.history_load.selected = self.rev().map(|r| (r.root, r.oid.clone()));
            } else {
                self.request_preview(preview::NAV_SETTLE);
            }
        }
        self.keep_graph_cursor_visible();
    }
    fn reload(&mut self, c: &Context, keep: bool) -> Result<()> {
        let prev = keep.then(|| self.history_selection()).flatten();
        self.reload_history(c, prev)
    }
    fn rebuild_visible(&mut self, prev: Option<(usize, String)>) -> Result<()> {
        let prev = if self.history_load.stale()
            && prev.is_some()
            && prev == self.rev().map(|r| (r.root, r.oid.clone()))
        {
            self.history_selection()
        } else {
            prev
        };
        let (visible, graph) = loading::Projection::new(self).build(&self.hist)?;
        self.visible = visible;
        self.graph = graph;
        if self.history_load.stale() {
            self.history_load.selected = prev.clone();
        }
        self.restore_history_cursor(prev);
        self.refresh_history_preview();
        Ok(())
    }
    fn restore_history_cursor(&mut self, prev: Option<(usize, String)>) {
        self.graph_origin = 0;
        self.graph_top = 0;
        self.cancel_key_sequence();
        self.cursor = prev
            .and_then(|(root, oid)| {
                self.visible
                    .iter()
                    .position(|&i| self.hist.revs[i].root == root && self.hist.revs[i].oid == oid)
            })
            .unwrap_or(0);
    }
    fn open_diff(&mut self, c: &Context) -> Result<()> {
        self.require_current_history(c)?;
        if self.filter.file_history.is_some() {
            self.focus = Pane::Info;
            self.preview.expedite();
            return Ok(());
        }
        if self.preview.loading() {
            if let Some(r) = self.rev() {
                self.diff = Some(DiffView {
                    name: format!("{} · {}", c.repos[r.root].name, &r.oid[..8]),
                    lines: vec!["Loading changed files…".into()],
                    waiting_for_files: true,
                    ..Default::default()
                });
                self.preview.expedite();
            }
            return Ok(());
        }
        let i = self
            .file_index()
            .or_else(|| (self.focus == Pane::Graph && !self.details.files.is_empty()).then_some(0));
        if let Some(i) = i {
            self.open_diff_file(c, i)?;
        }
        Ok(())
    }
    fn open_diff_file(&mut self, c: &Context, index: usize) -> Result<()> {
        let (Some(rev), Some(file)) = (self.rev(), self.details.files.get(index)) else {
            return Ok(());
        };
        self.diff = Some(file_history::comparison(
            &c.repos[rev.root],
            rev,
            file,
            index,
        )?);
        Ok(())
    }

    fn diff_file(&mut self, c: &Context, delta: isize) -> Result<()> {
        if let Some(i) = self.diff.as_ref().and_then(|d| d.file) {
            let next = i
                .saturating_add_signed(delta)
                .min(self.details.files.len().saturating_sub(1));
            if next != i {
                self.open_diff_file(c, next)?;
            }
        }
        Ok(())
    }
    fn mv(&mut self, d: isize) {
        self.scroll_pane(self.focus.id(), d);
    }
    fn scroll_pane(&mut self, pane: PaneId, d: isize) {
        fn next(i: usize, d: isize, n: usize) -> usize {
            i.saturating_add_signed(d).min(n.saturating_sub(1))
        }
        match pane {
            PaneId::Refs => self.refs.move_by(d),
            PaneId::Graph => self.select_row(next(self.cursor, d, self.visible.len())),
            PaneId::Files => self.file_move(d),
            PaneId::Info => self.scroll_info(d),
            _ => (),
        }
    }
    fn hit(&mut self, r: Rect, h: Hit) {
        if !r.is_empty() {
            self.hits.push((r, h))
        }
    }
    fn mouse_down(&mut self, button: MouseButton, x: u16, y: u16, c: &Context) -> Result<()> {
        if button == MouseButton::Right && !self.editing() {
            // A graph-edge arrow can be drawn on top of a commit row. Context
            // actions address the row itself, not the arrow's destination.
            let row = self.hits.iter().find_map(|(r, hit)| match hit {
                Hit::Commit(row) if inside(*r, x, y) => Some(*row),
                _ => None,
            });
            if let Some(row) = row {
                self.focus = Pane::Graph;
                self.select_row(row);
                self.require_current_history(c)?;
                self.open_commit_actions(c, Some((x, y)))?;
            }
            return Ok(());
        }
        let target = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, h)| h.clone());
        if let Some(dialog) = &mut self.path_dialog {
            if button == MouseButton::Left {
                match (dialog, target) {
                    (paths::Dialog::Text(input), Some(Hit::Dropdown(DropdownHit::PathText))) => {
                        input.place(x, y)
                    }
                    (
                        _,
                        Some(
                            hit @ Hit::Dropdown(
                                DropdownHit::PathTree(_) | DropdownHit::Apply | DropdownHit::Cancel,
                            ),
                        ),
                    ) => self.click(hit, c)?,
                    _ => (),
                }
            }
            return Ok(());
        }
        if self.diff.is_some() && !matches!(target, Some(Hit::CloseDiff)) {
            return Ok(());
        }
        if self.view_menu.is_some() && !matches!(target, Some(Hit::ViewMenu(_))) {
            self.view_menu = None;
            if matches!(target, Some(Hit::ViewOptions)) {
                return Ok(());
            }
        }
        if let Some((field, _)) = self.editor {
            if field.inline() {
                if button == MouseButton::Left {
                    let next = match target {
                        Some(h @ (Hit::ToggleCase | Hit::ToggleRegex)) => return self.click(h, c),
                        Some(h @ (Hit::Filter(_) | Hit::ViewOptions)) => Some(h),
                        Some(h @ Hit::Ref(_)) if field == Field::Refs => Some(h),
                        _ => None,
                    };
                    self.apply(c)?;
                    if let Some(h) = next {
                        self.click(h, c)?;
                    }
                }
                return Ok(());
            }
            if !matches!(target, Some(Hit::Dropdown(_))) {
                // Cancel the draft, then let this same press reach its original target.
                self.discard_editor();
                if matches!(target, Some(Hit::Filter(trigger)) if trigger == field) {
                    return Ok(()); // The same button toggles its menu closed.
                }
            }
        }
        if button == MouseButton::Left {
            if !self.editing() {
                if let Some(id) = self.panes.at(x, y) {
                    self.focus_pane(id);
                }
            }
            if let Some(h) = target {
                self.click(h, c)?;
            }
        }
        Ok(())
    }
    fn click(&mut self, hit: Hit, c: &Context) -> Result<()> {
        match hit {
            Hit::Ref(n) => {
                self.focus = Pane::Refs;
                self.refs.set_cursor(n);
                self.activate_ref(c)?
            }
            Hit::PickCommit(n) => {
                self.focus = Pane::Graph;
                self.last = None;
                self.select_row(n);
                self.toggle_pick(c)?;
            }
            Hit::ReviewPicks => {}
            Hit::Commit(n) => {
                let dbl = self.double(Pane::Graph, n);
                self.focus = Pane::Graph;
                self.select_row(n);
                if dbl {
                    self.open_diff(c)?
                }
            }
            Hit::GraphJump(n) | Hit::CommitScroll(n) => {
                self.focus = Pane::Graph;
                self.last = None;
                self.select_row(n);
            }
            Hit::CommitActions => {
                self.focus = Pane::Graph;
                self.require_current_history(c)?;
                self.open_commit_actions(c, None)?;
            }
            Hit::File(n) => {
                let dbl = self.double(Pane::Files, n);
                let leaf = self.file_is_leaf(n);
                self.file_activate(n, true);
                self.focus = Pane::Files;
                if leaf && dbl {
                    self.open_diff(c)?
                }
            }
            Hit::Filter(field) => self.begin(field, c)?,
            Hit::Dropdown(item) => match item {
                DropdownHit::Surface => (),
                DropdownHit::Branch(t) => self.branch_input(branches::Input::Click(t), c)?,
                DropdownHit::PathInput => self.choice = None,
                DropdownHit::PathText => (),
                DropdownHit::PathTree(hit) => {
                    if let Some(paths::Dialog::Tree(tree)) = &mut self.path_dialog {
                        tree.click(hit, c)?;
                    }
                }
                DropdownHit::AllRoots => self.toggle_listed_roots(c)?,
                DropdownHit::Choice(i) => self.accept_choice(i, c)?,
                DropdownHit::DateInput(i) => {
                    if let Some(range) = &mut self.date_range {
                        range.active = i.min(1);
                    }
                }
                DropdownHit::Apply => self.apply(c)?,
                DropdownHit::Cancel => self.discard_editor(),
            },
            Hit::FileFold(v) => {
                self.focus = Pane::Files;
                self.file_fold_all(v);
            }
            Hit::Clear(k) => self.clear_field(k, c)?,
            Hit::ToggleRegex | Hit::ToggleCase => self.toggle_match_mode(hit)?,
            Hit::ViewOptions => self.toggle_view_menu(),
            Hit::ViewMenu(item) => self.view_menu_hit(item, c)?,
            Hit::Refresh => self.reload(c, true)?,
            Hit::NewTab => self.new_tab(c)?,
            Hit::Tab(i) => self.switch_tab(i, c)?,
            Hit::CloseTab => self.close_tab(c)?,
            Hit::Reset => {
                self.filter = self
                    .filter
                    .file_history
                    .as_ref()
                    .map_or_else(Filter::default, file_history::Target::filter);
                self.ref_query.clear();
                self.reload(c, false)?
            }
            Hit::Diff => self.open_diff(c)?,
            Hit::More => {
                self.limit = self.limit.saturating_add(160).min(3200);
                self.reload(c, true)?
            }
            Hit::Parent(i) => {
                if let Some(r) = self.rev() {
                    if let Some(p) = r.parents.get(i) {
                        if let Some(n) = self.visible.iter().position(|&n| {
                            self.hist.revs[n].root == r.root && self.hist.revs[n].oid == *p
                        }) {
                            self.focus = Pane::Graph;
                            self.select_row(n)
                        } else {
                            anyhow::bail!(
                                "Parent commit is outside the current scope or not loaded"
                            )
                        }
                    }
                }
            }
            Hit::CloseDiff => self.diff = None,
        };
        Ok(())
    }
    fn double(&mut self, p: Pane, n: usize) -> bool {
        let now = Instant::now();
        let yes = self.last.is_some_and(|(t, pp, nn)| {
            pp == p && nn == n && now.duration_since(t) < Duration::from_millis(420)
        });
        self.last = if yes { None } else { Some((now, p, n)) };
        yes
    }
}
fn window(sel: usize, h: usize, n: usize) -> usize {
    sel.saturating_sub(h / 2).min(n.saturating_sub(h))
}
fn popup(a: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(a.width);
    let h = height.min(a.height);
    Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h)
}
impl LogPanel {
    pub(crate) fn draw_content(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.bounds = a;
        self.hits.clear();
        self.panes.begin();
        self.graph_height = 0;
        if !a.is_empty() {
            self.draw_layout(f, a, c);
        } else {
            for pane in Pane::ORDER {
                if self.pane_visible(pane) {
                    self.panes.add(pane.id(), Rect::default());
                }
            }
        }
        if self.panes.rect(PaneId::Info).is_none_or(|r| r.is_empty()) {
            self.info_selection.reset();
            self.info_click = None;
        }
    }
    pub(crate) fn draw_overlays(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        if a.is_empty() {
            return;
        }
        if self.commit_dialog.is_some() {
            self.draw_commit_dialog(f, a, c);
            return;
        }
        if self.editor.is_some() {
            self.draw_editor(f, a, c);
        }
        self.draw_view_menu(f, a);
        if let Some(d) = &mut self.diff {
            if let Some(merge) = &mut d.comparison {
                merge.draw(
                    f,
                    a,
                    self.icons,
                    d.file.unwrap_or(0),
                    self.details.files.len(),
                );
                return;
            }
            let p = popup(a, a.width.saturating_sub(4), a.height.saturating_sub(2));
            f.render_widget(Clear, p);
            let b = boxof(
                format!("{} Diff · {}", self.icons.glyph(Icon::Diff), d.name),
                true,
            );
            let ar = b.inner(p);
            f.render_widget(b, p);
            if !ar.is_empty() {
                let h = ar.height.saturating_sub(1);
                let lines = d
                    .lines
                    .iter()
                    .map(|s| {
                        Line::styled(
                            s.as_str(),
                            fg(if s.starts_with('+') {
                                Color::LightGreen
                            } else if s.starts_with('-') {
                                Color::LightRed
                            } else if s.starts_with("@@") {
                                BLUE
                            } else {
                                INK
                            }),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut layout = TextLayout::default();
                for line in lines {
                    layout.push(line, ar.width, Wrap::Clip);
                }
                d.selection.render(
                    f,
                    Rect::new(ar.x, ar.y, ar.width, h),
                    layout,
                    d.y,
                    usize::from(d.x),
                );
                self.bar(
                    f,
                    Rect::new(ar.x, ar.bottom() - 1, ar.width, 1),
                    &[(
                        format!("{} Back Esc", self.icons.glyph(Icon::Back)),
                        Hit::CloseDiff,
                    )],
                )
            }
        }
    }
}
impl Panel for LogPanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.reload(c, true)
    }
    fn editing(&self) -> bool {
        self.editor.is_some()
            || self.diff.is_some()
            || self.view_menu.is_some()
            || self.commit_dialog.is_some()
            || self.focused_search().is_some_and(|prompt| prompt.editing())
    }
    fn selected_text(&self) -> Option<&str> {
        if self.commit_dialog.is_some() {
            return None;
        }
        if let Some(diff) = &self.diff {
            diff.comparison.as_ref().map_or_else(
                || diff.selection.selected_text(),
                crate::merge::RevisionDiff::selected_text,
            )
        } else if self.filter.file_history.is_some() && self.focus == Pane::Info && !self.editing()
        {
            self.history_diff.as_ref().and_then(|diff| {
                diff.comparison.as_ref().map_or_else(
                    || diff.selection.selected_text(),
                    crate::merge::RevisionDiff::selected_text,
                )
            })
        } else if self.editor.is_none() && self.view_menu.is_none() && self.focus == Pane::Info {
            self.info_selection.selected_text()
        } else {
            None
        }
    }
    fn help(&self) -> &'static str {
        "j/k Navigate · gg/G First/last · H/M/L Screen top/middle/bottom · Ctrl-F/B Page · Ctrl-D/U Half-page · yy/ym/ya Copy · a Actions · F2 Edit message · Tab Panes · Ctrl-W h/j/k/l Focus · / Search · Space b/u/d/p Filters · o View · Enter Diff · Ctrl-T New Log tab · Ctrl-W c Close"
    }
    fn draw(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.draw_content(f, a, c);
        self.draw_overlays(f, a, c);
    }
    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token == REVIEW_TOKEN {
            if let Err(error) = self.require_current_history(c) {
                self.commit_review = None;
                return Action::Notice(format!("Log: {error:#}"));
            }
        }
        self.confirm_commit_action(token, c)
    }
    fn event(&mut self, e: &Event, c: &Context) -> Action {
        self.check_history_context(c);
        if self.focused_search().is_some()
            && match self.focus {
                Pane::Refs => self.ref_search_event(e, c),
                Pane::Files => self.file_search_event(e, c),
                _ => false,
            }
        {
            self.panes.cancel_drag();
            self.cancel_key_sequence();
            self.last = None;
            return Action::None;
        }
        if let Some(action) = self.history_event(e) {
            return action;
        }
        if self.commit_dialog.is_some() {
            return self.commit_dialog_event(e, c);
        }
        if let Some(merge) = self.diff.as_mut().and_then(|d| d.comparison.as_mut()) {
            use crate::merge::RevisionEvent;
            let event = merge.event(e);
            self.cancel_key_sequence();
            return match event {
                RevisionEvent::None => Action::None,
                RevisionEvent::Close => {
                    self.diff = None;
                    Action::None
                }
                RevisionEvent::File(delta) => self
                    .diff_file(c, delta)
                    .map_or_else(|error| Action::Notice(error.to_string()), |_| Action::None),
                RevisionEvent::Copy(text) => Action::Copy {
                    text,
                    label: "revision",
                },
            };
        }
        if matches!(
            e,
            Event::FocusLost | Event::Resize(_, _) | Event::Key(_) | Event::Paste(_)
        ) {
            self.panes.cancel_drag();
        }
        if matches!(e, Event::FocusLost) {
            self.info_selection.clear();
            self.info_click = None;
            if let Some(d) = &mut self.diff {
                d.selection.clear();
            }
        }
        let selection_action = if let Some(d) = &mut self.diff {
            d.selection.key(e, true)
        } else if self.editor.is_none() && self.view_menu.is_none() && self.focus == Pane::Info {
            self.info_selection.key(e, true)
        } else {
            None
        };
        if let Some(action) = selection_action {
            return action;
        }
        let mut action = Action::None;
        if self.editing()
            || matches!(e, Event::Paste(_) | Event::Resize(_, _) | Event::FocusLost)
            || matches!(e, Event::Mouse(m) if m.kind != M::Moved)
        {
            self.cancel_key_sequence();
        }
        let answer = (|| -> Result<()> {
            if let Event::Mouse(m) = e {
                if !self.editing() && self.panes.mouse(m) {
                    return Ok(());
                }
                let selection_action = if let Some(d) = &mut self.diff {
                    d.selection.mouse(m)
                } else if self.editor.is_none() && self.view_menu.is_none() {
                    self.info_selection.mouse(m)
                } else {
                    None
                };
                if let Some(selected_action) = selection_action {
                    if self.diff.is_none() {
                        self.focus = Pane::Info;
                        if m.kind == M::Down(MouseButton::Left) {
                            self.info_click = self
                                .hits
                                .iter()
                                .rev()
                                .find(|(r, h)| {
                                    inside(*r, m.column, m.row) && matches!(h, Hit::Parent(_))
                                })
                                .cloned();
                        } else if m.kind == M::Up(MouseButton::Left) {
                            if let Some((r, hit)) = self.info_click.take() {
                                if matches!(selected_action, Action::None)
                                    && inside(r, m.column, m.row)
                                {
                                    self.click(hit, c)?;
                                }
                            }
                        } else if m.kind == M::Drag(MouseButton::Left) {
                            self.info_click = None;
                        }
                    }
                    action = selected_action;
                    return Ok(());
                }
                if let M::Down(button) = m.kind {
                    if button == MouseButton::Left
                        && !self.editing()
                        && self
                            .hits
                            .iter()
                            .rev()
                            .find(|(r, _)| inside(*r, m.column, m.row))
                            .is_some_and(|(_, hit)| matches!(hit, Hit::ReviewPicks))
                    {
                        action = self.review_picks(c, None)?;
                    } else {
                        self.mouse_down(button, m.column, m.row, c)?;
                    }
                } else if m.kind == M::Moved {
                    self.hover_menu_at(m.column, m.row);
                } else if matches!(m.kind, M::ScrollUp | M::ScrollDown) {
                    let dir = if m.kind == M::ScrollDown { 3 } else { -3 };
                    if let Some(d) = &mut self.diff {
                        d.y =
                            d.y.saturating_add_signed(dir)
                                .min(d.lines.len().saturating_sub(1));
                    } else if let Some(dialog) = &mut self.path_dialog {
                        dialog.scroll(dir);
                    } else if self.view_menu.is_some() {
                        self.move_view_menu(dir);
                    } else if self.branch_menu.is_some() {
                        let row = self
                            .hits
                            .iter()
                            .rev()
                            .find(|(r, _)| inside(*r, m.column, m.row))
                            .and_then(|(_, h)| match h {
                                Hit::Dropdown(DropdownHit::Branch(
                                    branches::Target::Row(r)
                                    | branches::Target::Star(r)
                                    | branches::Target::Seek(r),
                                )) => Some(*r),
                                _ => None,
                            });
                        self.branch_input(branches::Input::Scroll(dir, row), c)?;
                    } else if self.editor.is_some() && self.choice_count() > 0 {
                        self.choice = Some(
                            self.choice
                                .unwrap_or(0)
                                .saturating_add_signed(dir)
                                .min(self.choice_count() - 1),
                        );
                    } else if self.editor.is_none() {
                        if let Some(id) = self.panes.at(m.column, m.row) {
                            self.scroll_pane(id, dir);
                        }
                    }
                }
                return Ok(());
            }
            if let Event::Paste(v) = e {
                if let Some(dialog) = &mut self.path_dialog {
                    if let paths::Dialog::Text(input) = dialog {
                        input.paste(v);
                    }
                } else if self.editing_paths() && v.contains(['\r', '\n']) {
                    self.open_path_text_for_paste(v);
                } else if self.branch_menu.is_some() {
                    self.branch_input(branches::Input::Paste(v), c)?;
                } else if let Some(range) = &mut self.date_range {
                    range.values[range.active].extend(v.chars().filter(|c| !c.is_control()));
                } else if let Some((field, text)) = &mut self.editor {
                    if *field == Field::Date {
                        return Ok(());
                    }
                    text.extend(v.chars().filter(|c| !c.is_control()));
                    self.edit_changed(c);
                }
                return Ok(());
            }
            let Event::Key(k) = e else { return Ok(()) };
            if k.kind == crossterm::event::KeyEventKind::Release {
                return Ok(());
            }
            let key = k.code;
            if self.diff.is_some() && matches!(key, K::Char('[' | ']')) {
                self.diff_file(c, if key == K::Char('[') { -1 } else { 1 })?;
                return Ok(());
            }
            if let Some(d) = &mut self.diff {
                match key {
                    K::Esc | K::Char('q') | K::Enter => self.diff = None,
                    K::Char('j') | K::Down => d.y = (d.y + 1).min(d.lines.len().saturating_sub(1)),
                    K::Char('k') | K::Up => d.y = d.y.saturating_sub(1),
                    K::Char('h') | K::Left => d.x = d.x.saturating_sub(4),
                    K::Char('l') | K::Right => d.x = d.x.saturating_add(4),
                    K::PageDown => d.y = (d.y + 12).min(d.lines.len().saturating_sub(1)),
                    K::PageUp => d.y = d.y.saturating_sub(12),
                    K::Char('g') => d.y = 0,
                    K::Char('G') => d.y = d.lines.len().saturating_sub(1),
                    _ => {}
                }
                return Ok(());
            }
            if self.editor.is_some() {
                self.edit_key(k, c)?;
                return Ok(());
            }
            if self.view_menu.is_some() {
                self.view_menu_key(k, c)?;
                return Ok(());
            }
            if self.focus == Pane::Graph && matches!(k.modifiers, Mods::NONE | Mods::SHIFT) {
                match key {
                    K::Char('v') => {
                        self.set_pick_mode(!self.pick_mode);
                        return Ok(());
                    }
                    K::Esc if self.pick_mode => {
                        self.set_pick_mode(false);
                        return Ok(());
                    }
                    K::Char(' ') if self.pick_mode => {
                        self.toggle_pick(c)?;
                        return Ok(());
                    }
                    K::Char('C') => {
                        action = self.review_picks(c, None)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if self.filter_key(k, c)? {
                return Ok(());
            }
            if let Some(a) = self.graph_key(k, c)? {
                action = a;
                return Ok(());
            }
            match key {
                K::Char('t') if k.modifiers.contains(Mods::CONTROL) => self.new_tab(c)?,
                K::Char('[') => self.switch_tab(
                    (self.view_index + self.views.len() - 1) % self.views.len(),
                    c,
                )?,
                K::Char(']') => self.switch_tab((self.view_index + 1) % self.views.len(), c)?,
                K::Char('/')
                    if matches!(self.focus, Pane::Graph | Pane::Info)
                        && !k
                            .modifiers
                            .intersects(Mods::ALT | Mods::CONTROL | Mods::SUPER) =>
                {
                    self.begin(Field::Text, c)?
                }
                K::Char('S') if self.focus == Pane::Refs => self.begin(Field::Refs, c)?,
                K::Char('B') => self.toggle_view_item(Item::BranchOrTag, c)?,
                K::Char('I') => self.toggle_view_item(Item::CommitDetails, c)?,
                K::Char('H') => self.toggle_view_item(Item::CurrentBranch, c)?,
                K::Char('L') => self.toggle_view_item(Item::ReferenceLabels, c)?,
                K::Char('o') => self.toggle_view_menu(),
                K::Char('a') if self.focus == Pane::Graph && k.modifiers.is_empty() => {
                    self.require_current_history(c)?;
                    self.open_commit_actions(c, None)?
                }
                K::F(2) if self.focus == Pane::Graph && k.modifiers.is_empty() => {
                    self.require_current_history(c)?;
                    self.edit_commit_message(c)?
                }
                K::Char('z' | 'Z') if self.focus == Pane::Files => {
                    self.file_fold_all(key == K::Char('z'))
                }
                K::Char('x') => self.click(Hit::Reset, c)?,
                K::Char('+') => self.click(Hit::More, c)?,
                K::Char('j') | K::Down => self.mv(1),
                K::Char('k') | K::Up => self.mv(-1),
                K::PageDown => self.mv(12),
                K::PageUp => self.mv(-12),
                K::Char('g') => self.mv(-1_000_000),
                K::Char('G') => self.mv(1_000_000),
                K::Tab | K::F(6) => self.cycle_focus(if k.modifiers.contains(Mods::SHIFT) {
                    -1
                } else {
                    1
                }),
                K::BackTab => self.cycle_focus(-1),
                K::Char('h') | K::Char('l') | K::Left | K::Right => {
                    let left = matches!(key, K::Char('h') | K::Left);
                    if self.focus == Pane::Refs && self.ref_horizontal(left, c) {
                        return Ok(());
                    }
                    if self.focus == Pane::Files && self.file_horizontal(left) {
                        return Ok(());
                    }
                    self.cycle_focus(if left { -1 } else { 1 })
                }
                K::Enter => {
                    if self.focus == Pane::Refs {
                        self.activate_ref(c)?
                    } else if self.focus == Pane::Files
                        && !self.preview.loading()
                        && !self.file_is_leaf(self.file_cursor())
                    {
                        self.file_activate(self.file_cursor(), true);
                    } else {
                        self.open_diff(c)?
                    }
                }
                K::Char('s') if self.focus == Pane::Refs && k.modifiers.is_empty() => {
                    self.activate_ref(c)?
                }
                K::Esc => self.focus = Pane::Graph,
                _ => {}
            }
            Ok(())
        })();
        match answer {
            Ok(()) => action,
            Err(e) => Action::Notice(format!("Log: {e:#}")),
        }
    }
}

#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path, process::Command};
    use tempfile::TempDir;
    pub(super) fn finish_preview(panel: &mut LogPanel, c: &Context) {
        let timeout = Instant::now() + Duration::from_secs(5);
        while panel.preview.loading() {
            panel.poll_preview(c).unwrap();
            assert!(Instant::now() < timeout, "preview did not complete");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub(super) fn git(dir: &Path, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_DATE", "2026-02-17T01:30:00Z")
            .env("GIT_COMMITTER_DATE", "2026-02-17T01:30:00Z")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().into()
    }
    pub(super) fn init() -> TempDir {
        let d = TempDir::new().unwrap();
        git(d.path(), &["init", "-q", "-b", "main"]);
        git(d.path(), &["config", "user.name", "Log tester"]);
        git(d.path(), &["config", "user.email", "test@example.invalid"]);
        d
    }
    pub(super) fn commit(path: &Path, file: &str, msg: &str) {
        fs::write(path.join(file), msg).unwrap();
        git(path, &["--literal-pathspecs", "add", "--", file]);
        git(path, &["commit", "-qm", msg]);
    }
    #[test]
    fn native_history_and_literal_patch() {
        let d = init();
        let p = d.path();
        let name = if cfg!(windows) {
            "[literal].txt"
        } else {
            "[literal]*.txt"
        };
        commit(p, name, "root");
        commit(p, "Z.txt", "second");
        git(p, &["checkout", "-qb", "topic", "HEAD~1"]);
        commit(p, "branch", "topic work");
        git(p, &["checkout", "-q", "main"]);
        git(p, &["merge", "--no-ff", "-qm", "integrate", "topic"]);
        let c = Context::discover(p).unwrap();
        let r = model::load(&c, &Filter::default(), 160).revs;
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].parents.len(), 2);
        assert_eq!(r[0].subject, "integrate");
        let dd = model::details(p, &r[0]).unwrap();
        assert_eq!(dd.files[0].raw, b"branch");
        let root = r.iter().find(|v| v.subject == "root").unwrap();
        let f = model::details(p, root).unwrap();
        assert_eq!(f.files[0].raw, name.as_bytes());
        assert!(model::patch(p, root, &f.files[0])
            .unwrap()
            .iter()
            .any(|s| s == "+root"));
        let filter = Filter {
            paths: model::PathFilter::parse(name, &c).unwrap(),
            ..Default::default()
        };
        let selected = model::load(&c, &filter, 10).revs;
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].oid, root.oid);
    }
    #[test]
    fn same_ids_in_different_roots_never_become_parents() {
        let make = |root, oid: &str, parents: Vec<&str>| model::Rev {
            root,
            oid: oid.into(),
            parents: parents.into_iter().map(String::from).collect(),
            ts: 0,
            date: String::new(),
            authored_date: String::new(),
            author: String::new(),
            email: String::new(),
            subject: String::new(),
            refs: String::new(),
            file: None,
        };
        let rows = vec![
            make(0, "c", vec!["p"]),
            make(1, "p", vec![]),
            make(0, "p", vec![]),
        ];
        let drawn = |rows: &[model::Rev]| {
            let g = Graph::new(rows);
            let w = g.window(0..rows.len(), 0, 20, 0);
            (0..rows.len())
                .map(|i| {
                    g.cells(i, w, NodeGlyphs::Unicode, false)
                        .iter()
                        .map(|v| v.0)
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(drawn(&rows), ["●  ", "│ ●", "●  "]);
        // The other root's copy of p must not complete the missing edge.
        assert_eq!(drawn(&rows[..2]), ["●", "●"]);
    }
    #[test]
    fn pointer_and_keyboard_inspect_real_multi_root_patches() {
        use crossterm::event::{KeyEvent, MouseEvent};
        use ratatui::{backend::TestBackend, Terminal};
        let d = TempDir::new().unwrap();
        let mut repos = vec![];
        for name in ["one", "two"] {
            let p = d.path().join(name);
            fs::create_dir_all(p.join("src")).unwrap();
            git(&p, &["init", "-q", "-b", "main"]);
            git(&p, &["config", "user.name", "Testing"]);
            git(&p, &["config", "user.email", "unit@example.invalid"]);
            commit(&p, "src/read me", name);
            repos.push(crate::core::Repo {
                path: p,
                name: name.into(),
                branch: "main".into(),
                submodule: false,
                parent: None,
            })
        }
        let mut c = Context {
            repos,
            scope: [0, 1].into(),
            workspace: d.path().into(),
            state_dir: d.path().join("state"),
        };
        let mut panel = LogPanel::default();
        panel.refresh(&c).unwrap();
        assert_eq!(panel.visible.len(), 2);
        let mut term = Terminal::new(TestBackend::new(130, 38)).unwrap();
        term.draw(|f| panel.draw(f, f.area(), &c)).unwrap();
        let hit = panel
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::Commit(1)).then_some(*r))
            .unwrap();
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: M::Down(MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: Mods::NONE,
            }),
            &c,
        );
        let root = panel.rev().unwrap().root;
        assert_eq!(panel.cursor, 1);
        let key = |k| Event::Key(KeyEvent::new(k, Mods::NONE));
        panel.event(&key(K::Enter), &c);
        finish_preview(&mut panel, &c);
        assert!(!panel
            .diff
            .as_ref()
            .unwrap()
            .comparison
            .as_ref()
            .unwrap()
            .is_three_way());
        assert!(
            matches!(panel.event(&key(K::Char('y')), &c), Action::Copy { text, .. } if text == c.repos[root].name)
        );
        panel.event(&key(K::Esc), &c);
        c.scope.remove(&root);
        panel.refresh(&c).unwrap();
        assert_eq!(panel.visible.len(), 1);
        assert_ne!(panel.rev().unwrap().root, root);
        panel.event(&key(K::Char('/')), &c);
        panel.event(&Event::Paste("absent".into()), &c);
        panel.event(&key(K::Enter), &c);
        assert!(panel.visible.is_empty());
        panel.event(&key(K::Char('x')), &c);
        assert_eq!(panel.visible.len(), 1);
    }
}
