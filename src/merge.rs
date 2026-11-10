//! Three-way merge editor. The working file is untouched until Apply is confirmed.
mod engine;
mod keymap;
mod list;
mod recovery;
mod view;
use self::engine::*;
use self::list::ConflictList;
use self::recovery::{RecoveryAction, RecoveryPlan};
use self::view::{EditorView, Overlay, Setting, Version};
use crate::{
    core::Context,
    ui::{
        icons::{Icon, IconMode},
        idea::GREEN,
        inside,
        key_sequence::KeySequence,
        pane,
        panes::{Axis, PaneId, PaneLayout, SplitId},
        Action, Panel, ACCENT, FG, MUTED,
    },
};
use anyhow::Result;
use crossterm::event::{Event, KeyCode as K, KeyModifiers as M, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint as C, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::{collections::HashMap, path::PathBuf};
use unicode_width::UnicodeWidthStr;
pub(crate) use view::{RevisionDiff, RevisionEvent};

type Snapshot = Vec<(usize, String, Resolution)>;
#[derive(Clone, Copy)]
enum Hit {
    File(usize),
    AcceptFile(usize, FileSide),
    PrepareSimple,
    FoldUnresolved,
    TogglePaths,
    Close,
    Open,
    List,
    Next(i32),
    Navigate(i32),
    Auto(Option<FileSide>),
    ResolveAutomatically,
    Undo,
    Choice(usize, FileSide),
    Decide(usize, FileSide, bool),
    UseFile(FileSide),
    Select(usize, usize),
    Edit,
    Both,
    Save,
    Apply,
    Continue,
    Abort,
    Skip,
    Zoom,
    ResultLine(usize, usize),
    EditAt(usize, usize, usize),
    OkEdit,
    CancelEdit,
    Fold(usize),
    FoldAll,
    Details(FileSide),
    CompareMenu,
    Compare(Version, Version),
    Settings,
    Setting(Setting),
    Dismiss,
}
struct Editor {
    chars: Vec<char>,
    cursor: usize,
}
impl Editor {
    fn new(s: &str) -> Self {
        let chars: Vec<_> = s.chars().collect();
        let mut cursor = chars.len();
        if cursor > 0 && chars[cursor - 1] == '\n' {
            cursor -= 1;
            if cursor > 0 && chars[cursor - 1] == '\r' {
                cursor -= 1
            }
        }
        Self { chars, cursor }
    }
    fn insert(&mut self, s: &str) {
        let v: Vec<_> = s.chars().collect();
        let n = v.len();
        self.chars.splice(self.cursor..self.cursor, v);
        self.cursor += n
    }
    fn rowcol(&self) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;
        for &ch in &self.chars[..self.cursor] {
            if ch == '\n' {
                row += 1;
                col = 0
            } else {
                col += 1
            }
        }
        (row, col)
    }
    fn pos(&mut self, y: usize, x: usize) {
        let mut starts = vec![0];
        for (i, ch) in self.chars.iter().enumerate() {
            if *ch == '\n' {
                starts.push(i + 1)
            }
        }
        let y = y.min(starts.len() - 1);
        let mut limit = if y + 1 < starts.len() {
            starts[y + 1] - 1
        } else {
            self.chars.len()
        };
        if limit > starts[y] && self.chars[limit - 1] == '\r' {
            limit -= 1;
        }
        self.cursor = (starts[y].saturating_add(x)).min(limit)
    }
}
#[derive(Default)]
pub struct MergePanel {
    pub(crate) shortcut_labels: std::collections::BTreeMap<&'static str, String>,
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
    list: ConflictList,
    doc: Option<Document>,
    idx: usize,
    side: usize,
    scroll: usize,
    view: EditorView,
    zoom: bool,
    follow: bool,
    hist: Vec<Snapshot>,
    drafts: HashMap<(usize, PathBuf), (Stages, Snapshot)>,
    edit: Option<Editor>,
    prefix: Option<char>,
    hits: Vec<(Rect, Hit)>,
    rows_height: usize,
    rows_total: usize,
    plan: Option<(usize, Operation, String)>,
    recovery_plan: Option<(usize, RecoveryPlan, String)>,
    review_serial: u64,
}
fn rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
    Rect::new(x, y, w, h)
}
fn put(f: &mut Frame, a: Rect, s: impl Into<String>, style: Style) {
    if a.width > 0 && a.height > 0 {
        f.render_widget(Paragraph::new(s.into()).style(style), a)
    }
}
fn style(c: Color) -> Style {
    Style::default().fg(c)
}
fn snap(d: &Document, indices: impl Iterator<Item = usize>) -> Snapshot {
    indices
        .map(|i| (i, d.chunks[i].result.clone(), d.chunks[i].resolution))
        .collect()
}
fn replay(d: &mut Document, s: Snapshot) {
    for (i, t, resolution) in s {
        if let Some(c) = d.chunks.get_mut(i) {
            c.result = t;
            c.resolution = resolution
        }
    }
}
impl MergePanel {
    fn key_label(&self, key: &'static str) -> &str {
        self.shortcut_labels.get(key).map_or(key, String::as_str)
    }
    const SIDES: [PaneId; 3] = [PaneId::Current, PaneId::Result, PaneId::Incoming];

    pub(crate) fn path_copy_target(&self, c: &Context) -> Option<crate::ui::path_copy::Target> {
        if !self.at_root() || self.editing() {
            return None;
        }
        let file = self.list.selected_file()?;
        Some(crate::ui::path_copy::Target::new(
            &c.repos.get(file.root)?.path,
            &file.path,
            false,
        ))
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        if self.doc.is_some() {
            Self::SIDES[self.side]
        } else {
            PaneId::Conflicts
        }
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        if self.doc.is_some() && !self.editing() {
            if let Some(side) = Self::SIDES.iter().position(|&p| p == id) {
                if side != self.side {
                    self.cancel_key_sequence();
                    self.side = side;
                }
            }
        }
    }

    pub(crate) fn at_root(&self) -> bool {
        self.doc.is_none() && self.edit.is_none()
    }

    pub(crate) fn has_document(&self) -> bool {
        self.doc.is_some()
    }

    pub(crate) fn has_pending_work(&self) -> bool {
        !self.list.is_empty()
    }

    /// Shared cancellation for Shell overlays and the panel's input transitions.
    pub(crate) fn cancel_key_sequence(&mut self) {
        self.prefix = None;
    }

    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        if self.doc.is_none() || self.editing() {
            return None;
        }
        match self.prefix {
            Some('[') => Some(KeySequence::MergePrevious),
            Some(']') => Some(KeySequence::MergeNext),
            _ => None,
        }
    }

    fn selected_root(&self) -> Option<usize> {
        self.doc
            .as_ref()
            .map(|d| d.file.root)
            .or_else(|| self.list.selected_root())
    }
    fn stash(&mut self) {
        if let Some(d) = &self.doc {
            let s = snap(d, 0..d.chunks.len());
            self.drafts.insert(
                (d.file.root, d.file.path.clone()),
                (d.file.stages.clone(), s),
            );
        }
    }
    fn store(&mut self, s: Snapshot) {
        if !s.is_empty() {
            self.hist.push(s);
            if self.hist.len() > 50 {
                self.hist.remove(0);
            }
        }
    }
    fn open_document(&self, c: &Context, file: Conflict) -> Result<Document> {
        anyhow::ensure!(
            c.scope.contains(&file.root),
            "Repository is outside Roots scope"
        );
        let mut d = Document::open(&c.repos[file.root].path, file)?;
        if let Some((st, changes)) = self.drafts.get(&(d.file.root, d.file.path.clone())) {
            if *st == d.file.stages {
                replay(&mut d, changes.clone())
            }
        }
        Ok(d)
    }
    fn choose_file(&mut self, c: &Context, file: Conflict) -> Result<()> {
        let d = self.open_document(c, file)?;
        self.idx = d.chunks.iter().position(|b| !b.done()).unwrap_or(0);
        self.list.select_file(&d.file);
        self.doc = Some(d);
        self.side = 1;
        self.follow = true;
        self.scroll = 0;
        self.view.reset_document();
        self.hist.clear();
        self.edit = None;
        Ok(())
    }
    fn selected_continue(&mut self, c: &Context) -> Action {
        self.recovery_plan = None;
        let Some(i) = self.selected_root() else {
            return Action::Notice("No operation selected".into());
        };
        if !c.scope.contains(&i) {
            return Action::Notice("Repository is outside Roots scope".into());
        }
        match continue_plan(&c.repos[i].path) {
            Ok((op, tree, files)) => {
                let name = op.name;
                self.plan = Some((i, op, tree));
                Action::Confirm{title:format!("Continue {} · {}",name,c.repos[i].name),body:format!("git {} --continue (accept the existing message). Hooks are enabled. All currently staged changes participate.\n{}",name,files),token:"merge:continue".into()}
            }
            Err(e) => Action::Notice(format!("{e:#}")),
        }
    }
    fn selected_recovery(&mut self, c: &Context, action: RecoveryAction) -> Action {
        self.plan = None;
        self.recovery_plan = None;
        let Some(root) = self.selected_root() else {
            return Action::Notice("No operation selected".into());
        };
        if !c.scope.contains(&root) {
            return Action::Notice("Repository is outside Roots scope".into());
        }
        let Some(repo) = c.repos.get(root) else {
            return Action::Notice("Repository is no longer available".into());
        };
        match RecoveryPlan::review(&repo.path, action) {
            Ok((plan, body)) => {
                self.review_serial = self.review_serial.wrapping_add(1);
                let token = format!("merge:recovery:{}", self.review_serial);
                let title = format!(
                    "{} {} · {}",
                    action.label(),
                    plan.operation_name(),
                    repo.name
                );
                self.recovery_plan = Some((root, plan, token.clone()));
                Action::Confirm { title, body, token }
            }
            Err(error) => Action::Notice(format!("{error:#}")),
        }
    }
    fn act(&mut self, hit: Hit, c: &Context) -> Action {
        if self.edit.is_some() {
            match hit {
                Hit::Apply | Hit::Save => self.finish_edit(false),
                Hit::OkEdit | Hit::CancelEdit | Hit::EditAt(..) => (),
                _ => return Action::None,
            }
        }
        if matches!(hit, Hit::Continue) {
            return self.selected_continue(c);
        }
        if matches!(hit, Hit::Abort | Hit::Skip) {
            return self.selected_recovery(
                c,
                if matches!(hit, Hit::Abort) {
                    RecoveryAction::Abort
                } else {
                    RecoveryAction::Skip
                },
            );
        }
        if self.doc.is_none() {
            if !matches!(hit, Hit::File(_)) {
                self.list.reset_click();
            }
            match hit {
                Hit::File(i) => {
                    if let Some(open) = self.list.click(i) {
                        return self.act(open, c);
                    }
                }
                Hit::Open => {
                    if let Some(file) = self.list.selected_file().cloned() {
                        if let Err(e) = self.choose_file(c, file) {
                            return Action::Notice(format!("{e:#}"));
                        }
                    }
                }
                Hit::AcceptFile(i, side) => return self.preview_side(c, i, side),
                Hit::PrepareSimple => return self.prepare_simple(c),
                Hit::FoldUnresolved => self.list.toggle_unresolved(),
                Hit::TogglePaths => self.list.toggle_paths(),
                Hit::Close => return Action::CloseDialog,
                _ => {}
            }
            return Action::None;
        }
        let d = self.doc.as_mut().unwrap();
        match hit {
            Hit::List | Hit::Save => {
                self.close_document();
                if matches!(hit, Hit::Save) {
                    return Action::Notice("Merge draft saved for this session".into());
                }
            }
            Hit::Auto(source) => {
                let ids = d
                    .chunks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, chunk)| {
                        (chunk.kind == Kind::Safe
                            && match source {
                                Some(side) => chunk.pending(side),
                                None => !chunk.done(),
                            })
                        .then_some(i)
                    })
                    .collect::<Vec<_>>();
                let previous = snap(d, ids.iter().copied());
                for i in ids {
                    d.chunks[i].safe(source);
                }
                if d.chunks[self.idx].done() {
                    if let Some(next) = d.chunks.iter().position(|b| !b.done()) {
                        self.idx = next;
                        self.follow = true;
                    }
                }
                self.store(previous);
            }
            Hit::ResolveAutomatically => {
                let resolved = d
                    .chunks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, chunk)| {
                        chunk.automatic_result().map(|text| (i, text.into_owned()))
                    })
                    .collect::<Vec<_>>();
                let count = resolved.len();
                if count == 0 {
                    return Action::Notice("No changes can be resolved automatically".into());
                }
                let previous = snap(d, resolved.iter().map(|(i, _)| *i));
                for (i, result) in resolved {
                    d.chunks[i].resolve(result);
                }
                if d.chunks[self.idx].done() {
                    if let Some(next) = d.chunks.iter().position(|chunk| !chunk.done()) {
                        self.idx = next;
                        self.follow = true;
                    }
                }
                let pending = d.pending();
                self.store(previous);
                let noun = if count == 1 { "change" } else { "changes" };
                return Action::Notice(format!(
                    "Automatically resolved {count} {noun}; {pending} pending. Undo [{}]",
                    self.key_label("u")
                ));
            }
            Hit::Choice(i, side) | Hit::Decide(i, side, _) => {
                let b = &d.chunks[i];
                if b.kind == Kind::Context || matches!(hit, Hit::Decide(..)) && !b.pending(side) {
                    return Action::None;
                }
                self.idx = i;
                let s = snap(d, std::iter::once(i));
                let b = &mut d.chunks[i];
                match hit {
                    Hit::Decide(_, _, accept) => b.decide(side, accept),
                    _ => b.use_side(side),
                }
                self.store(s);
                self.follow = true;
            }
            Hit::UseFile(side) => {
                let s = snap(
                    d,
                    d.chunks.iter().enumerate().filter_map(|(i, ch)| {
                        (!ch.done() || ch.result != side.text(ch)).then_some(i)
                    }),
                );
                for (i, ..) in &s {
                    d.chunks[*i].use_side(side);
                }
                self.store(s);
            }
            Hit::Both => {
                let i = self.idx;
                if d.chunks[i].kind != Kind::Conflict {
                    return Action::Notice("Select a conflict block first".into());
                }
                let s = snap(d, std::iter::once(i));
                d.chunks[i].both();
                self.store(s);
                self.follow = true;
            }
            Hit::Edit => self.begin_edit(),
            Hit::EditAt(i, row, col) => {
                if self.idx != i {
                    self.finish_edit(false);
                    self.idx = i;
                }
                self.begin_edit();
                let edit = self.edit.as_mut().unwrap();
                let text = edit.chars.iter().collect::<String>();
                let line = text
                    .split('\n')
                    .nth(row)
                    .unwrap_or("")
                    .trim_end_matches('\r');
                edit.pos(row, view::char_at_cell(line, col));
            }
            Hit::OkEdit => self.finish_edit(true),
            Hit::CancelEdit => {
                self.edit = None;
                self.follow = true;
            }
            Hit::Undo => {
                if let Some(s) = self.hist.pop() {
                    if let Some((idx, ..)) = s.first() {
                        self.idx = *idx
                    }
                    replay(d, s);
                    self.follow = true
                }
            }
            Hit::Select(i, side) => {
                self.idx = i;
                self.side = side;
            }
            Hit::Zoom => {
                self.zoom = !self.zoom;
                self.follow = true;
            }
            Hit::Fold(i) => self.view.toggle_fold(i),
            Hit::FoldAll => {
                self.view.folding = !self.view.folding;
                self.view.expanded.clear();
                self.follow = true;
            }
            Hit::Details(side) => self.view.open(Overlay::details(d, side)),
            Hit::CompareMenu => self.view.open(Overlay::Compare),
            Hit::Compare(a, b) => self.view.open(Overlay::compare(d, a, b)),
            Hit::Settings => self.view.open(Overlay::Settings),
            Hit::Setting(setting) => {
                self.view.overlay = None;
                if matches!(setting, Setting::Zoom) {
                    return self.act(Hit::Zoom, c);
                }
                self.view.toggle(setting);
            }
            Hit::Dismiss => self.view.overlay = None,
            Hit::Navigate(dir) => {
                let next = if dir < 0 {
                    d.chunks[..self.idx].iter().rposition(|b| !b.done())
                } else {
                    d.chunks
                        .iter()
                        .enumerate()
                        .skip(self.idx + 1)
                        .find_map(|(i, b)| (!b.done()).then_some(i))
                };
                if let Some(i) = next {
                    self.idx = i;
                    self.follow = true;
                }
            }
            Hit::Next(dir) => {
                let len = d.chunks.len();
                for step in 1..=len {
                    let i = (self.idx as isize + (dir as isize) * (step as isize))
                        .rem_euclid(len as isize) as usize;
                    if d.chunks[i].kind == Kind::Conflict && !d.chunks[i].done() {
                        self.idx = i;
                        self.follow = true;
                        break;
                    }
                }
            }
            Hit::Apply => {
                if !c.scope.contains(&d.file.root) {
                    return Action::Notice("Repository is outside Roots scope".into());
                }
                if d.pending() > 0 {
                    return Action::Notice(format!(
                        "{} changes still require a decision",
                        d.pending()
                    ));
                }
                return Action::Confirm{title:format!("Stage resolved file · {}",c.repos[d.file.root].name),body:format!("Write the previewed result to {} and stage ONLY this path. This does not continue the repository operation.",d.file.label()),token:"merge:apply".into()};
            }
            _ => {}
        }
        Action::None
    }

    fn begin_edit(&mut self) {
        if self.edit.is_none() {
            let d = self.doc.as_ref().unwrap();
            self.edit = Some(Editor::new(&d.chunks[self.idx].result));
        }
        self.side = 1;
    }

    fn finish_edit(&mut self, resolve_unchanged: bool) {
        if let (Some(edit), Some(doc)) = (self.edit.take(), self.doc.as_mut()) {
            let result = edit.chars.into_iter().collect::<String>();
            if resolve_unchanged || result != doc.chunks[self.idx].result {
                let previous = snap(doc, std::iter::once(self.idx));
                doc.chunks[self.idx].resolve(result);
                self.store(previous);
            }
            self.follow = true;
        }
    }

    fn close_document(&mut self) {
        self.finish_edit(false);
        self.stash();
        self.doc = None;
        self.hist.clear();
        self.view.reset_document();
    }
}

impl Panel for MergePanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.cancel_key_sequence();
        let refreshed = self.list.refresh(c);
        // Keep the draft if Roots changes or the file is resolved externally,
        // but return to the list so any remaining work is reachable.
        if self.doc.as_ref().is_some_and(|doc| {
            !self
                .list
                .files_in(doc.file.root)
                .any(|file| file.path == doc.file.path)
        }) {
            self.close_document();
        }
        self.plan = None;
        self.recovery_plan = None;
        refreshed
    }
    fn help(&self) -> &'static str {
        if self.edit.is_some() {
            "Edit Result · Ctrl+S / Ctrl+Enter accept · Esc cancel"
        } else if self.doc.is_some() {
            "j/k block · [c ]c conflict · a safe · H/L/b choose · i edit · o folds · u undo · A apply · Esc files"
        } else {
            "Enter Resolve · Y/T Preview · S Drafts · C Continue · B Abort · N Skip · Esc Close"
        }
    }
    fn editing(&self) -> bool {
        self.edit.is_some() || self.view.overlay.is_some()
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if self.doc.is_some() {
            self.draw_editor(f, area, c, "")
        } else {
            self.panes.begin();
            self.hits.clear();
            self.rows_height = 0;
            self.rows_total = 0;
            self.draw_list(f, area, c)
        }
    }
    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token.starts_with("merge:recovery:") {
            let Some((root, plan, expected)) = self.recovery_plan.take() else {
                return Action::Notice("Review expired".into());
            };
            if token != expected {
                return Action::Notice("Review expired".into());
            }
            if !c.scope.contains(&root) {
                return Action::Notice("Repository out of scope".into());
            }
            let Some(repo) = c.repos.get(root) else {
                return Action::Notice("Repository is no longer available".into());
            };
            return match plan.execute(&repo.path) {
                Ok(message) => {
                    self.drafts.retain(|(id, _), _| *id != root);
                    if self.doc.as_ref().is_some_and(|d| d.file.root == root) {
                        self.doc = None;
                        self.hist.clear();
                        self.view.reset_document();
                    }
                    Action::Refresh(format!("{}: {message}", repo.name))
                }
                Err(error) => Action::Refresh(format!("{}: {error:#}", repo.name)),
            };
        }
        match token {
            "merge:apply" => {
                let Some(d) = self.doc.as_ref() else {
                    return Action::None;
                };
                if !c.scope.contains(&d.file.root) {
                    return Action::Notice("Repository out of scope".into());
                }
                match d.apply() {
                    Ok(()) => {
                        let next_step = if d.op.is_some() {
                            "Continue is a separate action."
                        } else {
                            "Review in Commit."
                        };
                        let msg = format!(
                            "{}: staged resolved {}. {next_step}",
                            c.repos[d.file.root].name,
                            d.file.label()
                        );
                        self.drafts.remove(&(d.file.root, d.file.path.clone()));
                        let root = d.file.root;
                        let path = &d.file.path;
                        let next = self
                            .list
                            .files_in(root)
                            .filter(|f| &f.path != path)
                            .min_by_key(|f| (&f.path < path, &f.path))
                            .cloned();
                        self.doc = None;
                        self.hist.clear();
                        self.view.reset_document();
                        if let Some(next) = next {
                            if let Err(error) = self.choose_file(c, next) {
                                return Action::Refresh(format!("{msg} Next file: {error:#}"));
                            }
                        }
                        Action::Refresh(msg)
                    }
                    Err(e) => Action::Notice(format!("{e:#}")),
                }
            }
            "merge:continue" => {
                let Some((i, op, tree)) = self.plan.take() else {
                    return Action::Notice("Review expired".into());
                };
                if !c.scope.contains(&i) {
                    return Action::Notice("Repository out of scope".into());
                }
                match continue_op(&c.repos[i].path, &op, &tree) {
                    Ok(msg) => Action::Refresh(msg),
                    Err(e) => Action::Refresh(format!("{e:#}")),
                }
            }
            _ => Action::None,
        }
    }
    fn event(&mut self, e: &Event, c: &Context) -> Action {
        // The shell owns the confirmation overlay. Any later panel input means
        // that review was dismissed; its destructive authorization expires.
        self.recovery_plan = None;
        if matches!(e, Event::FocusLost | Event::Resize(_, _)) {
            self.list.reset_click();
        }
        if matches!(
            e,
            Event::Key(_) | Event::Paste(_) | Event::FocusLost | Event::Resize(_, _)
        ) {
            self.view.cancel_scroll_drag();
        }
        if self.editing()
            || matches!(e, Event::Paste(_) | Event::Resize(_, _) | Event::FocusLost)
            || matches!(e, Event::Mouse(m) if m.kind != MouseEventKind::Moved)
        {
            self.cancel_key_sequence();
        }
        if self.view.overlay.is_some() {
            let hit = match e {
                Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => self
                    .hits
                    .iter()
                    .rev()
                    .find(|(r, h)| {
                        matches!(h, Hit::Dismiss | Hit::Setting(_) | Hit::Compare(..))
                            && inside(*r, m.column, m.row)
                    })
                    .map(|(_, h)| *h),
                _ => self.view.overlay_event(e),
            };
            return hit.map_or(Action::None, |h| self.act(h, c));
        }
        if let Event::Mouse(m) = e {
            if !self.editing() && self.panes.mouse(m) {
                return Action::None;
            }
            if self.has_document()
                && !self.editing()
                && self
                    .view
                    .scroll_mouse(m, &mut self.scroll, &mut self.follow)
            {
                return Action::None;
            }
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(id) = self.panes.at(m.column, m.row) {
                        self.focus_pane(id);
                    }
                    if let Some(&(r, cmd)) = self
                        .hits
                        .iter()
                        .rev()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                    {
                        let cmd = if let Hit::ResultLine(i, row) = cmd {
                            Hit::EditAt(i, row, usize::from(m.column - r.x) + self.view.hscroll[1])
                        } else {
                            cmd
                        };
                        return self.act(cmd, c);
                    }
                }
                MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
                    if self.has_document() && !self.editing() =>
                {
                    if let Some(side) = self
                        .panes
                        .at(m.column, m.row)
                        .and_then(|id| Self::SIDES.iter().position(|&p| p == id))
                    {
                        self.view.scroll_horizontal(
                            side,
                            if m.kind == MouseEventKind::ScrollLeft {
                                -4
                            } else {
                                4
                            },
                        );
                    }
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if self.edit.is_none() => {
                    let up = m.kind == MouseEventKind::ScrollUp;
                    if self.doc.is_some() {
                        if m.modifiers.contains(M::SHIFT) {
                            if let Some(side) = self
                                .panes
                                .at(m.column, m.row)
                                .and_then(|id| Self::SIDES.iter().position(|&p| p == id))
                            {
                                self.view.scroll_horizontal(side, if up { -4 } else { 4 });
                            }
                            return Action::None;
                        }
                        self.follow = false;
                        self.scroll = if up {
                            self.scroll.saturating_sub(3)
                        } else {
                            (self.scroll + 3).min(self.rows_total.saturating_sub(self.rows_height))
                        }
                    } else {
                        self.list.move_by(if up { -2 } else { 2 });
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        if let Event::Paste(t) = e {
            if let Some(edit) = &mut self.edit {
                edit.insert(t)
            }
            return Action::None;
        }
        let Event::Key(k) = e else {
            return Action::None;
        };
        if k.kind == crossterm::event::KeyEventKind::Release {
            return Action::None;
        }
        if let Some(edit) = &mut self.edit {
            match k.code {
                K::Esc => return self.act(Hit::CancelEdit, c),
                K::Enter if k.modifiers.contains(M::CONTROL) => return self.act(Hit::OkEdit, c),
                K::Enter => edit.insert("\n"),
                K::Tab => edit.insert("\t"),
                K::Char(ch) if !k.modifiers.intersects(M::CONTROL | M::ALT) => {
                    edit.chars.insert(edit.cursor, ch);
                    edit.cursor += 1
                }
                K::Backspace if edit.cursor > 0 => {
                    edit.cursor -= 1;
                    edit.chars.remove(edit.cursor);
                }
                K::Delete if edit.cursor < edit.chars.len() => {
                    edit.chars.remove(edit.cursor);
                }
                K::Left => edit.cursor = edit.cursor.saturating_sub(1),
                K::Right => edit.cursor = (edit.cursor + 1).min(edit.chars.len()),
                K::Up | K::Down | K::Home | K::End => {
                    let (row, col) = edit.rowcol();
                    match k.code {
                        K::Up => edit.pos(row.saturating_sub(1), col),
                        K::Down => edit.pos(row + 1, col),
                        K::Home => edit.pos(row, 0),
                        _ => edit.pos(row, usize::MAX),
                    }
                }
                // Ctrl-S is a portable alternative in terminals that cannot distinguish Ctrl+Enter.
                K::Char('s') if k.modifiers.contains(M::CONTROL) => {
                    return self.act(Hit::OkEdit, c)
                }
                _ => {}
            }
            return Action::None;
        }
        if self.doc.is_none() {
            if k.modifiers.intersects(M::CONTROL | M::ALT | M::SUPER) {
                return Action::None;
            }
            match k.code {
                K::Char('j') | K::Down => self.list.move_by(1),
                K::Char('k') | K::Up => self.list.move_by(-1),
                K::Char('g') | K::Home => self.list.move_by(isize::MIN),
                K::Char('G') | K::End => self.list.move_by(isize::MAX),
                K::Char('S') => return self.act(Hit::PrepareSimple, c),
                K::Char(key @ ('Y' | 'T')) => {
                    let side = if key == 'Y' {
                        FileSide::Current
                    } else {
                        FileSide::Incoming
                    };
                    return self
                        .list
                        .accept(side)
                        .map_or(Action::None, |hit| self.act(hit, c));
                }
                K::Char(' ') => return self.act(Hit::FoldUnresolved, c),
                K::Char('o') => return self.act(Hit::TogglePaths, c),
                K::Char('C') => return self.act(Hit::Continue, c),
                K::Char('B') => return self.act(Hit::Abort, c),
                K::Char('N') => return self.act(Hit::Skip, c),
                K::Enter => return self.act(self.list.open(), c),
                _ => (),
            };
            return Action::None;
        }
        let prev = self.prefix.take();
        let n = self.doc.as_ref().unwrap().chunks.len();
        let op = match k.code {
            K::Esc | K::Char('F') => Hit::List,
            K::Char('[' | ']') if !k.modifiers.intersects(M::CONTROL | M::ALT) => {
                if let K::Char(c) = k.code {
                    self.prefix = Some(c)
                }
                return Action::None;
            }
            K::Char('c') if prev.is_some() && !k.modifiers.intersects(M::CONTROL | M::ALT) => {
                Hit::Next(if prev == Some(']') { 1 } else { -1 })
            }
            K::Char('a') => Hit::Auto(None),
            K::Char('u') => Hit::Undo,
            K::Char('b') => Hit::Both,
            K::Char('H') => Hit::Choice(self.idx, FileSide::Current),
            K::Char('L') => Hit::Choice(self.idx, FileSide::Incoming),
            K::Char('Y') => Hit::UseFile(FileSide::Current),
            K::Char('T') => Hit::UseFile(FileSide::Incoming),
            K::Char('o') => Hit::FoldAll,
            K::Char('O') => Hit::Settings,
            K::Char('v') => Hit::CompareMenu,
            K::Char('m') => Hit::ResolveAutomatically,
            K::F(7) => Hit::Navigate(if k.modifiers.contains(M::SHIFT) {
                -1
            } else {
                1
            }),
            K::Char('s') => Hit::Save,
            K::Char('A') => Hit::Apply,
            K::Enter if k.modifiers.contains(M::SHIFT) => Hit::Apply,
            K::Char('i') => Hit::Edit,
            K::Enter if self.side == 1 => Hit::Edit,
            K::Enter | K::Char('x' | 'd') => {
                let Some(side) = FileSide::ALL.into_iter().find(|s| s.pane() == self.side) else {
                    return Action::None;
                };
                if k.code == K::Char('d') {
                    Hit::Details(side)
                } else {
                    Hit::Decide(self.idx, side, k.code == K::Enter)
                }
            }
            K::Char('z') => Hit::Zoom,
            K::Char('j') | K::Down => {
                self.idx = (self.idx + 1).min(n - 1);
                self.follow = true;
                return Action::None;
            }
            K::Char('k') | K::Up => {
                self.idx = self.idx.saturating_sub(1);
                self.follow = true;
                return Action::None;
            }
            K::Char('l') | K::Tab | K::F(6) if !k.modifiers.contains(M::SHIFT) => {
                self.side = (self.side + 1) % 3;
                return Action::None;
            }
            K::Char('h') | K::BackTab | K::F(6) => {
                self.side = (self.side + 2) % 3;
                return Action::None;
            }
            K::Left => {
                self.view.scroll_horizontal(self.side, -4);
                return Action::None;
            }
            K::Right => {
                self.view.scroll_horizontal(self.side, 4);
                return Action::None;
            }
            K::PageUp => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(self.rows_height.max(1));
                return Action::None;
            }
            K::PageDown => {
                self.follow = false;
                self.scroll = (self.scroll + self.rows_height.max(1))
                    .min(self.rows_total.saturating_sub(self.rows_height));
                return Action::None;
            }
            _ => return Action::None,
        };
        self.act(op, c)
    }
}
