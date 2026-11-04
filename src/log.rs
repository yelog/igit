//! Multi-root history: preserve each root's topology and operate on the selected root's objects.
mod branches;
mod display;
mod filters;
mod inspector;
mod keymap;
mod model;
mod navigation;
mod paths;
mod preview;
mod refs;
pub(crate) mod view;
use self::FilterControl::{Rail, Toolbar};
use crate::{
    core::Context,
    ui::{
        icons::{Icon, IconMode},
        inside,
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Pane {
    Refs,
    #[default]
    Graph,
    Files,
    Info,
}
impl Pane {
    fn delta(self, d: i32) -> Self {
        let a = [Self::Refs, Self::Graph, Self::Files, Self::Info];
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterControl {
    Toolbar(Field),
    Rail(Field),
}
impl FilterControl {
    fn field(self) -> Field {
        match self {
            Toolbar(field) | Rail(field) => field,
        }
    }
}
#[derive(Clone, Copy)]
enum DropdownHit {
    Surface,
    Branch(branches::Target),
    Choice(usize),
    PathInput,
    AllRoots,
    DateInput(usize),
    Apply,
    Cancel,
}
#[derive(Clone)]
enum Hit {
    Pane(Pane),
    Ref(usize),
    Commit(usize),
    GraphJump(usize),
    File(usize),
    Filter(FilterControl),
    Clear(Field),
    Reset,
    ToggleRegex,
    ToggleCase,
    ToggleRefs,
    ToggleInfo,
    ToggleHighlight,
    ToggleLabels,
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
    Split(Divider),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Divider {
    Left,
    Right,
    Inspector,
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
}
pub struct LogPanel {
    pub(crate) icons: IconMode,
    hist: History,
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
    inspector: inspector::Inspector,
    cursor: usize,
    choice: Option<usize>,
    suggestions: Vec<(String, String)>,
    path_options: Vec<usize>,
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
    editor: Option<(FilterControl, String)>,
    branch_menu: Option<branches::Menu>,
    date_range: Option<DateRangeEditor>,
    diff: Option<DiffView>,
    hits: Vec<(Rect, Hit)>,
    areas: Vec<(Rect, Pane)>,
    bounds: Rect,
    left: u16,
    right: u16,
    inspector_split: u16,
    drag: Option<Divider>,
    info_selection: TextSelection,
    info_click: Option<(Rect, Hit)>,
    last: Option<(Instant, Pane, usize)>,
    limit: usize,
}
impl Default for LogPanel {
    fn default() -> Self {
        Self {
            icons: IconMode::default(),
            hist: History::default(),
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
            inspector: Default::default(),
            cursor: 0,
            choice: None,
            suggestions: vec![],
            path_options: vec![],
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
            hits: vec![],
            areas: vec![],
            bounds: Rect::default(),
            left: 17,
            right: 26,
            inspector_split: 65,
            drag: None,
            info_selection: TextSelection::default(),
            info_click: None,
            last: None,
            limit: 160,
        }
    }
}
impl LogPanel {
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
            self.request_preview(preview::NAV_SETTLE);
        }
        self.keep_graph_cursor_visible();
    }
    fn reload(&mut self, c: &Context, keep: bool) -> Result<()> {
        let prev = if keep {
            self.rev().map(|r| (r.root, r.oid.clone()))
        } else {
            None
        };
        self.hist = model::load(c, &self.filter, self.limit);
        self.read_presentation(c);
        self.rebuild_refs(c);
        self.rebuild_visible(prev)
    }
    fn rebuild_visible(&mut self, prev: Option<(usize, String)>) -> Result<()> {
        let matcher = self.matcher(&self.filter.query)?;
        self.visible = self
            .hist
            .revs
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                self.filter.includes_root(r.root)
                    && matcher.as_ref().is_none_or(|m| {
                        [&r.oid, &r.subject, &r.refs]
                            .into_iter()
                            .any(|v| m.is_match(v))
                    })
            })
            .map(|(i, _)| i)
            .collect();
        self.graph = Graph::new(self.visible.iter().map(|&i| &self.hist.revs[i]));
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
        self.request_preview(Duration::ZERO);
        Ok(())
    }
    fn open_diff(&mut self, c: &Context) -> Result<()> {
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
        if let (Some(r), Some(f)) = (self.rev(), i.and_then(|i| self.details.files.get(i))) {
            let lines = model::patch(&c.repos[r.root].path, r, &f.raw)?;
            self.diff = Some(DiffView {
                lines,
                name: format!(
                    "{} · {} · {}",
                    c.repos[r.root].name,
                    &r.oid[..8],
                    model::plain(&f.raw)
                ),
                ..Default::default()
            })
        }
        Ok(())
    }
    fn mv(&mut self, d: isize) {
        fn next(i: usize, d: isize, n: usize) -> usize {
            i.saturating_add_signed(d).min(n.saturating_sub(1))
        }
        match self.focus {
            Pane::Refs => self.refs.move_by(d),
            Pane::Graph => self.select_row(next(self.cursor, d, self.visible.len())),
            Pane::Files => self.file_move(d),
            Pane::Info => self.scroll_info(d),
        }
    }
    fn hit(&mut self, r: Rect, h: Hit) {
        if !r.is_empty() {
            self.hits.push((r, h))
        }
    }
    fn mouse_down(&mut self, button: MouseButton, x: u16, y: u16, c: &Context) -> Result<()> {
        let target = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, h)| h.clone());
        if self.diff.is_some() && !matches!(target, Some(Hit::CloseDiff)) {
            return Ok(());
        }
        if self.view_menu.is_some() && !matches!(target, Some(Hit::ViewMenu(_))) {
            self.view_menu = None;
            if matches!(target, Some(Hit::ViewOptions)) {
                return Ok(());
            }
        }
        if let Some((control, _)) = self.editor {
            let field = control.field();
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
                if matches!(target, Some(Hit::Filter(trigger)) if trigger == control) {
                    return Ok(()); // The same button toggles its menu closed.
                }
            }
        }
        if button == MouseButton::Left {
            if let Some(h) = target {
                self.click(h, c)?;
            } else if let Some((_, pane)) = self.areas.iter().find(|(r, _)| inside(*r, x, y)) {
                self.focus = *pane;
            }
        }
        Ok(())
    }
    fn click(&mut self, hit: Hit, c: &Context) -> Result<()> {
        match hit {
            Hit::Pane(p) => self.focus = p,
            Hit::Ref(n) => {
                self.focus = Pane::Refs;
                self.refs.set_cursor(n);
                self.activate_ref(c)?
            }
            Hit::Commit(n) => {
                let dbl = self.double(Pane::Graph, n);
                self.focus = Pane::Graph;
                self.select_row(n);
                if dbl {
                    self.open_diff(c)?
                }
            }
            Hit::GraphJump(n) => {
                self.focus = Pane::Graph;
                self.last = None;
                self.select_row(n);
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
            Hit::Filter(control) => self.begin(control, c)?,
            Hit::Dropdown(item) => match item {
                DropdownHit::Surface => (),
                DropdownHit::Branch(t) => self.branch_input(branches::Input::Click(t), c)?,
                DropdownHit::PathInput => self.choice = None,
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
            Hit::ToggleRefs => {
                self.show_refs = !self.show_refs;
                if self.focus == Pane::Refs && !self.show_refs {
                    self.focus = Pane::Graph;
                }
            }
            Hit::ToggleInfo => {
                self.show_info = !self.show_info;
                if !self.show_info && matches!(self.focus, Pane::Info | Pane::Files) {
                    self.focus = Pane::Graph;
                }
            }
            Hit::ToggleHighlight => self.options.current_branch = !self.options.current_branch,
            Hit::ToggleLabels => self.show_labels = !self.show_labels,
            Hit::ViewOptions => self.toggle_view_menu(),
            Hit::ViewMenu(item) => self.view_menu_hit(item, c)?,
            Hit::Refresh => self.reload(c, true)?,
            Hit::NewTab => self.new_tab(c)?,
            Hit::Tab(i) => self.switch_tab(i, c)?,
            Hit::CloseTab => self.close_tab(c)?,
            Hit::Reset => {
                self.filter = Filter::default();
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
                            anyhow::bail!("父提交不在当前范围 / 尚未加载")
                        }
                    }
                }
            }
            Hit::Split(left) => self.drag = Some(left),
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
impl Panel for LogPanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.reload(c, true)
    }
    fn editing(&self) -> bool {
        self.editor.is_some() || self.diff.is_some() || self.view_menu.is_some()
    }
    fn selected_text(&self) -> Option<&str> {
        if let Some(diff) = &self.diff {
            diff.selection.selected_text()
        } else if self.editor.is_none() && self.view_menu.is_none() && self.focus == Pane::Info {
            self.info_selection.selected_text()
        } else {
            None
        }
    }
    fn help(&self) -> &'static str {
        "j/k 浏览 · gg/G 首/末 · H/M/L 屏顶/中/底 · Ctrl-F/B 整页 · Ctrl-D/U 半页 · yy/ym/ya 复制 · Tab/h/l 切栏 · / 搜索 · b/a/d/p 筛选 · o 显示选项 · x 清筛选 · + 更多 · Enter Diff · Ctrl-T/W Log 页签"
    }
    fn draw(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.bounds = a;
        self.hits.clear();
        self.areas.clear();
        if a.is_empty() {
            return;
        }
        self.draw_layout(f, a, c);
        if !self.areas.iter().any(|(_, p)| *p == Pane::Info) {
            self.info_selection.reset();
        }
        if self.editor.is_some() {
            self.draw_editor(f, a, c);
        }
        self.draw_view_menu(f, a);
        if let Some(d) = &mut self.diff {
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
    fn event(&mut self, e: &Event, c: &Context) -> Action {
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
                    self.mouse_down(button, m.column, m.row, c)?;
                } else if m.kind == M::Moved {
                    self.hover_menu_at(m.column, m.row);
                } else if matches!(m.kind, M::Drag(MouseButton::Left)) {
                    if let Some(split) = self.drag {
                        self.resize(split, m.column, m.row);
                    }
                } else if matches!(m.kind, M::Up(_)) {
                    self.drag = None
                } else if matches!(m.kind, M::ScrollUp | M::ScrollDown) {
                    let dir = if m.kind == M::ScrollDown { 3 } else { -3 };
                    if let Some(d) = &mut self.diff {
                        d.y =
                            d.y.saturating_add_signed(dir)
                                .min(d.lines.len().saturating_sub(1));
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
                        if let Some((_, p)) =
                            self.areas.iter().find(|(a, _)| inside(*a, m.column, m.row))
                        {
                            self.focus = *p;
                            self.mv(dir)
                        }
                    }
                }
                return Ok(());
            }
            if let Event::Paste(v) = e {
                if self.branch_menu.is_some() {
                    self.branch_input(branches::Input::Paste(v), c)?;
                } else if let Some(range) = &mut self.date_range {
                    range.values[range.active].extend(v.chars().filter(|c| !c.is_control()));
                } else if let Some((control, text)) = &mut self.editor {
                    if control.field() == Field::Date {
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
            if let Some(a) = self.graph_key(k, c)? {
                action = a;
                return Ok(());
            }
            match key {
                K::Char('t') if k.modifiers.contains(Mods::CONTROL) => self.new_tab(c)?,
                K::Char('w') if k.modifiers.contains(Mods::CONTROL) => self.close_tab(c)?,
                K::Char('[') => self.switch_tab(
                    (self.view_index + self.views.len() - 1) % self.views.len(),
                    c,
                )?,
                K::Char(']') => self.switch_tab((self.view_index + 1) % self.views.len(), c)?,
                K::Char('/') => self.begin(
                    Toolbar(if self.focus == Pane::Refs {
                        Field::Refs
                    } else {
                        Field::Text
                    }),
                    c,
                )?,
                K::Char('b') => self.begin(Toolbar(Field::Branch), c)?,
                K::Char('B') => self.click(Hit::ToggleRefs, c)?,
                K::Char('I') => self.click(Hit::ToggleInfo, c)?,
                K::Char('H') => self.click(Hit::ToggleHighlight, c)?,
                K::Char('L') => self.click(Hit::ToggleLabels, c)?,
                K::Char('o') => self.toggle_view_menu(),
                K::Char('z' | 'Z') if self.focus == Pane::Files => {
                    self.file_fold_all(key == K::Char('z'))
                }
                K::Char('a') => self.begin(Toolbar(Field::Author), c)?,
                K::Char('d') => self.begin(Toolbar(Field::Date), c)?,
                K::Char('p') => self.begin(Toolbar(Field::Path), c)?,
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
                K::Char(' ') if self.focus == Pane::Refs => self.activate_ref(c)?,
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
        commit(p, "[literal]*.txt", "root");
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
        assert_eq!(f.files[0].raw, b"[literal]*.txt");
        assert!(model::patch(p, root, &f.files[0].raw)
            .unwrap()
            .iter()
            .any(|s| s == "+root"));
        let filter = Filter {
            path: "[literal]*.txt".into(),
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
        assert!(panel
            .diff
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .any(|l| l == &format!("+{}", c.repos[root].name)));
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
