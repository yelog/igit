mod ai;
mod backend;
mod discard;
#[cfg(test)]
mod discard_tests;
mod display;
mod keymap;
#[cfg(test)]
mod message_tests;
mod refresh;
mod rollback;
mod rows;
mod stash;
mod tree;
mod view;
#[cfg(test)]
mod view_tests;
use crate::{
    core::{branch_label, Context},
    ui::{
        icons::{Icon, IconMode},
        idea::{BLUE, DIM as MUTED, INK as FG, PANEL, SELECT as SEL},
        inside,
        panes::{PaneId, PaneLayout},
        selection::{self, TextLayout, TextSelection, Wrap as TextWrap},
        Action, Panel,
    },
};
use anyhow::{bail, Result};
use backend::{Change, Plan};
use crossterm::event::{
    Event, KeyCode as K, KeyModifiers as Mod, MouseButton, MouseEventKind as M,
};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
pub(crate) use rollback::{Rollback, RollbackEvent};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::{Duration, Instant},
};
use tree::{Kind, Row};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    groups: Vec<String>,
    membership: BTreeMap<String, String>,
    message: Option<String>,
    // Retain legacy per-repository drafts for recovery after migration.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    drafts: BTreeMap<String, String>,
    endpoint: String,
    model: String,
    prompt: String,
}
#[derive(Clone, PartialEq, Eq)]
struct File {
    key: String,
    root: usize,
    change: Change,
    group: String,
}
impl File {
    fn movable(&self) -> bool {
        self.change.selectable() && self.change.xy != *b"??"
    }
}
#[derive(Default, Clone, Copy, PartialEq)]
enum Focus {
    #[default]
    Tree,
    Diff,
    Message,
}
#[derive(Clone)]
enum Edit {
    Message,
    Stash,
    Group,
    Move,
    AISettings,
    AIDraft,
}
#[derive(Clone)]
enum Hit {
    Row(usize),
    ResolveConflicts,
    Show(Focus),
    Refresh,
    UndoCheck,
    TogglePreview,
    ViewOptions,
    ViewItem(display::Item),
    Directories,
    Status,
    Expand(bool),
    New,
    Move,
    Msg,
    Submit(bool),
    Amend,
    Ai,
    Settings,
    EditSave,
    EditCancel,
    MoveTo(String),
}
impl Hit {
    fn hint(&self) -> Option<&'static str> {
        Some(match self {
            Self::Refresh => "Refresh  F5",
            Self::ResolveConflicts => "Open Conflicts  Enter on group",
            Self::UndoCheck => "Undo last checkbox change  u",
            Self::Move => "Move to changelist  m",
            Self::New => "New changelist  n",
            Self::TogglePreview => "Show or hide preview  v",
            Self::ViewOptions => "View Options  o",
            Self::Directories => "Group by directory  t",
            Self::Status => "Show Git status columns  T",
            Self::Expand(true) => "Expand all  Z",
            Self::Expand(false) => "Collapse all  z",
            Self::Amend => "Amend the highlighted repository's last commit  a",
            Self::Ai => "Review patches for an AI commit message  A",
            Self::Settings => "AI settings  S",
            _ => return None,
        })
    }
}

pub struct CommitPanel {
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
    saved: Saved,
    loaded: bool,
    files: Vec<File>,
    checks: BTreeMap<String, bool>,
    fold: BTreeSet<String>,
    options: display::Options,
    view_menu: Option<display::Menu>,
    preview_open: bool,
    stacked: bool,
    hover: Option<&'static str>,
    last_click: Option<(String, Instant)>,
    last_check: Vec<(String, bool)>,
    cursor: usize,
    offset: usize,
    focus: Focus,
    edit: Option<Edit>,
    buf: String,
    caret: usize,
    ai_scroll: u16,
    amend: BTreeSet<usize>,
    view: Vec<Row>,
    hits: Vec<(Rect, Hit)>,
    tree: Rect,
    diff: Rect,
    dragged: Option<(usize, bool)>,
    edit_view: (Rect, u16, u16),
    edit_follow: bool,
    diff_key: String,
    diff_text: String,
    diff_scroll: u16,
    diff_selection: TextSelection,
    message_selection: TextSelection,
    plans: Vec<Plan>,
    stash: stash::State,
    rollback: Option<Box<Rollback>>,
    push: bool,
    move_to: usize,
    move_names: Vec<String>,
    pub(crate) ai_config: Option<crate::config::AiConfig>,
    ai_settings: crate::config::AiConfig,
    ai_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    ai_draft: String,
    ai_request: Option<ai::Request>,
    ai_error: String,
    refresh: refresh::Refresh,
}

impl Default for CommitPanel {
    fn default() -> Self {
        Self {
            icons: Default::default(),
            panes: Default::default(),
            saved: Default::default(),
            loaded: Default::default(),
            files: Default::default(),
            checks: Default::default(),
            fold: Default::default(),
            options: Default::default(),
            view_menu: Default::default(),
            preview_open: true,
            stacked: Default::default(),
            hover: Default::default(),
            last_click: Default::default(),
            last_check: Default::default(),
            cursor: Default::default(),
            offset: Default::default(),
            focus: Default::default(),
            edit: Default::default(),
            buf: Default::default(),
            caret: Default::default(),
            ai_scroll: Default::default(),
            amend: Default::default(),
            view: Default::default(),
            hits: Default::default(),
            tree: Default::default(),
            diff: Default::default(),
            dragged: Default::default(),
            edit_view: Default::default(),
            edit_follow: Default::default(),
            diff_key: Default::default(),
            diff_text: Default::default(),
            diff_scroll: Default::default(),
            diff_selection: Default::default(),
            message_selection: Default::default(),
            plans: Default::default(),
            stash: Default::default(),
            rollback: Default::default(),
            push: Default::default(),
            move_to: Default::default(),
            move_names: Default::default(),
            ai_pending: Default::default(),
            ai_config: Default::default(),
            ai_settings: Default::default(),
            ai_draft: Default::default(),
            ai_request: Default::default(),
            ai_error: Default::default(),
            refresh: Default::default(),
        }
    }
}
const DEF: &str = "Changes";
const NEW: &str = "Unversioned Files";
const IGNORED: &str = "Ignored Files";
fn display_path(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\n', "↵")
        .replace('\r', "␍")
        .replace('\t', "⇥")
}
fn keypart(p: &Path) -> String {
    let b = p.as_os_str().as_encoded_bytes();
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn rid(c: &Context, r: usize) -> String {
    keypart(&c.repos[r].path)
}
fn slice(r: Rect, y: u16) -> Rect {
    Rect::new(r.x, r.y.saturating_add(y), r.width, u16::from(y < r.height))
}
fn word(f: &mut Frame, r: Rect, s: &str, st: Style) {
    f.render_widget(Paragraph::new(s.to_owned()).style(st), r)
}
fn err(e: impl ToString) -> Action {
    Action::Notice(e.to_string())
}
impl CommitPanel {
    pub(crate) fn focused_pane(&self) -> PaneId {
        match self.focus {
            Focus::Tree => PaneId::Changes,
            Focus::Diff => PaneId::Preview,
            Focus::Message => PaneId::Message,
        }
    }

    pub(crate) fn focus_pane(&mut self, pane: PaneId) {
        self.focus = match pane {
            PaneId::Changes => Focus::Tree,
            PaneId::Preview if self.preview_open => Focus::Diff,
            PaneId::Message => Focus::Message,
            _ => return,
        };
    }

    fn pane_focused(&self, pane: PaneId) -> bool {
        self.panes.active
            && (!self.editing() || self.message_editing())
            && self.focused_pane() == pane
    }

    pub(crate) fn message_editing(&self) -> bool {
        matches!(self.edit, Some(Edit::Message))
    }

    pub(crate) fn finish_message(&mut self, c: &Context) -> Result<()> {
        if self.message_editing() {
            self.accept(c)?;
        }
        Ok(())
    }

    fn save(&self, c: &Context) -> Result<()> {
        use std::io::Write;
        fs::create_dir_all(&c.state_dir)?;
        let mut w = tempfile::NamedTempFile::new_in(&c.state_dir)?;
        w.write_all(&serde_json::to_vec_pretty(&self.saved)?)?;
        w.persist(c.state_dir.join("commit.json"))?;
        Ok(())
    }
    fn sync(&mut self, c: &Context) {
        let old = self.view.get(self.cursor).map(|r| r.id.clone());
        self.view = self.render_rows(c);
        self.cursor = match old {
            Some(k) => self
                .view
                .iter()
                .position(|r| r.id == k)
                .unwrap_or(self.cursor.min(self.view.len().saturating_sub(1))),
            None => self
                .view
                .iter()
                .position(|r| !r.files.is_empty())
                .unwrap_or(0),
        };
    }
    fn active(&self) -> Option<&Row> {
        self.view.get(self.cursor)
    }
    fn selection(&self, c: &Context) -> Vec<&File> {
        self.files
            .iter()
            .filter(|f| {
                c.scope.contains(&f.root)
                    && self.checks.get(&f.key) == Some(&true)
                    && f.change.selectable()
            })
            .collect()
    }
    fn checked_repos(&self, c: &Context) -> Vec<usize> {
        self.selection(c)
            .iter()
            .map(|f| f.root)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    fn highlighted_repo(&self) -> Option<usize> {
        let row = self.active()?;
        let root = self.files[*row.files.first()?].root;
        row.files
            .iter()
            .all(|&i| self.files[i].root == root)
            .then_some(root)
    }

    fn browse_repo(&mut self, next: bool, c: &Context) {
        let roots: Vec<_> = c
            .scope
            .iter()
            .copied()
            .filter(|r| self.files.iter().any(|f| f.root == *r))
            .collect();
        if roots.is_empty() {
            return;
        }
        let current = self.highlighted_repo();
        let i = roots.iter().position(|r| Some(*r) == current).unwrap_or(0);
        let root = roots[(i + if next { 1 } else { roots.len() - 1 }) % roots.len()];
        let all = self.changes_tree().rows_with_folds(c, &BTreeSet::new());
        if let Some(i) = all.iter().position(|row| {
            !row.files.is_empty() && row.files.iter().all(|&i| self.files[i].root == root)
        }) {
            let target = all[i].id.clone();
            let mut depth = all[i].depth;
            for row in all[..i].iter().rev() {
                if row.depth < depth {
                    self.fold.remove(&row.id);
                    depth = row.depth;
                }
            }
            self.sync(c);
            self.cursor = self
                .view
                .iter()
                .position(|row| row.id == target)
                .unwrap_or(0);
            self.diff_scroll = 0;
            self.focus = Focus::Tree;
        }
    }
    fn toggle(&mut self, index: usize) {
        let Some(row) = self.view.get(index) else {
            return;
        };
        let (checked, eligible) = self.check_counts(row);
        if eligible == 0 {
            return;
        }
        self.last_check.clear();
        for &i in &row.files {
            let f = &self.files[i];
            if f.change.selectable() {
                let was = self.checks.get(&f.key) == Some(&true);
                if was != (checked < eligible) {
                    self.last_check.push((f.key.clone(), was));
                    self.checks.insert(f.key.clone(), !was);
                }
            }
        }
    }
    fn fold(&mut self, index: usize, v: Option<bool>, c: &Context) {
        if let Some(r) = self.view.get(index) {
            if !r.foldable() {
                return;
            }
            let id = r.id.clone();
            if v.unwrap_or(!self.fold.contains(&id)) {
                if index < self.cursor
                    && self.view[index + 1..=self.cursor]
                        .iter()
                        .all(|child| child.depth > r.depth)
                {
                    self.cursor = index;
                }
                self.fold.insert(id);
            } else {
                self.fold.remove(&id);
            }
            self.sync(c)
        }
    }
    fn move_cursor(&mut self, n: isize) {
        self.cursor = self
            .cursor
            .saturating_add_signed(n)
            .min(self.view.len().saturating_sub(1));
        self.diff_scroll = 0
    }
    fn edit_message(&mut self, _c: &Context) {
        self.buf = self.saved.message.clone().unwrap_or_default();
        self.caret = self.buf.len();
        self.message_selection.clear();
        self.edit_view = (self.edit_view.0, 0, 0);
        self.edit_follow = true;
        self.edit = Some(Edit::Message);
        self.focus = Focus::Message
    }
    fn move_menu(&mut self) {
        if self
            .active()
            .is_none_or(|r| r.files.iter().all(|&i| !self.files[i].movable()))
        {
            return;
        }
        self.move_to = 0;
        self.move_names = std::iter::once(DEF.to_string())
            .chain(self.saved.groups.iter().cloned())
            .collect();
        self.edit = Some(Edit::Move)
    }
    fn accept(&mut self, c: &Context) -> Result<()> {
        let previous = self.saved.clone();
        let previous_ai = self.ai_settings.clone();
        match self.edit.clone() {
            Some(Edit::Stash) => bail!("Review the stash before creating it"),
            Some(Edit::Message) => {
                self.saved.message = Some(self.buf.clone());
            }
            Some(Edit::Group) => {
                let n = self.buf.trim();
                if n.is_empty()
                    || n == DEF
                    || n == NEW
                    || n == IGNORED
                    || self.saved.groups.iter().any(|v| v == n)
                {
                    bail!("Enter a unique changelist name")
                }
                self.saved.groups.push(n.to_string());
            }
            Some(Edit::Move) => {
                let g = self
                    .move_names
                    .get(self.move_to)
                    .cloned()
                    .unwrap_or(DEF.into());
                for i in self.active().map(|r| r.files.clone()).unwrap_or_default() {
                    let f = &mut self.files[i];
                    if f.movable() {
                        f.group = g.clone();
                        self.saved.membership.insert(f.key.clone(), g.clone());
                    }
                }
            }
            Some(Edit::AISettings) => {
                let mut p = self.buf.splitn(3, '\n');
                let mut settings = self.ai_settings.clone();
                settings.endpoint = p.next().unwrap_or("").trim().into();
                settings.model = p.next().unwrap_or("").trim().into();
                settings.prompt = p.next().unwrap_or("").trim().into();
                settings.validate()?;
                self.saved.endpoint = settings.endpoint.clone();
                self.saved.model = settings.model.clone();
                self.saved.prompt = settings.prompt.clone();
                self.ai_settings = settings;
            }
            Some(Edit::AIDraft) => {
                self.saved.message = Some(self.ai_draft.clone());
            }
            None => {}
        }
        if let Err(error) = self.save(c) {
            self.saved = previous;
            self.ai_settings = previous_ai;
            return Err(error);
        }
        self.message_selection.clear();
        self.edit = None;
        self.sync(c);
        Ok(())
    }
    fn review(&mut self, c: &Context, push: bool) -> Result<Action> {
        let mut plans = vec![];
        let rs = self.checked_repos(c);
        if rs.is_empty() {
            bail!("Select files first")
        }
        let msg = self.saved.message.clone().unwrap_or_default();
        if msg.trim().is_empty() {
            self.edit_message(c);
            bail!("Enter a commit message for the selected files")
        }
        for r in rs {
            let fs = self
                .selection(c)
                .into_iter()
                .filter(|f| f.root == r)
                .map(|f| f.change.clone())
                .collect();
            plans.push(
                backend::plan(
                    r,
                    &c.repos[r].path,
                    fs,
                    msg.clone(),
                    self.amend.contains(&r),
                )
                .map_err(|e| anyhow::anyhow!("{}: {e}", c.repos[r].name))?,
            );
        }
        plans.sort_by_key(|p| std::cmp::Reverse(p.repo.components().count()));
        let mut body = String::new();
        for p in &plans {
            body.push_str(&format!(
                "{} / {} · {}{}\n  {}\n",
                c.repos[p.root].name,
                branch_label(&p.branch, p.head.as_deref()),
                &p.tree[..8],
                if p.amend { " · AMEND" } else { "" },
                p.msg.trim()
            ));
            for f in &p.files {
                body.push_str(&format!(
                    "    {}  {}\n",
                    if f.staged() { "INDEX" } else { "WORK " },
                    f.label()
                ));
            }
        }
        for p in &plans {
            body.push_str(&format!(
                "\n{} / reviewed patch\n{}",
                c.repos[p.root].name, p.preview
            ));
        }
        self.plans = plans;
        self.push = push;
        Ok(Action::Confirm {
            title: format!(
                "Review commits · {} repo{}",
                self.plans.len(),
                if self.plans.len() == 1 { "" } else { "s" }
            ),
            body,
            token: "commit".into(),
        })
    }
    fn submit(&mut self, c: &Context) -> Action {
        let mut success = vec![];
        let mut out = vec![];
        let mut failed: BTreeSet<usize> = BTreeSet::new();
        for p in std::mem::take(&mut self.plans) {
            if !c.scope.contains(&p.root) {
                out.push(format!(
                    "{} is outside the current scope; not committed",
                    c.repos[p.root].name
                ));
                failed.insert(p.root);
                continue;
            }
            let dep = failed.iter().any(|&r| {
                c.repos[r].parent == Some(p.root)
                    && p.files
                        .iter()
                        .any(|f| p.repo.join(&f.path) == c.repos[r].path)
            });
            if dep {
                out.push(format!(
                    "{} skipped because a child repository failed",
                    c.repos[p.root].name
                ));
                failed.insert(p.root);
                continue;
            }
            match backend::execute(&p) {
                Ok(result) => {
                    out.push(format!(
                        "{} {} {}",
                        c.repos[p.root].name,
                        self.icons.glyph(Icon::Success),
                        result.hash
                    ));
                    if let Some(note) = result.warning {
                        self.push = false;
                        out.push(format!("{} {note}", self.icons.glyph(Icon::Warning)));
                    }
                    success.push(p.root);
                    self.amend.remove(&p.root);
                    for f in p.files {
                        self.checks
                            .insert(format!("{}:{}", rid(c, p.root), keypart(&f.path)), false);
                    }
                }
                Err(e) => {
                    failed.insert(p.root);
                    out.push(format!(
                        "{} {} {e:#}",
                        c.repos[p.root].name,
                        self.icons.glyph(Icon::Error)
                    ));
                }
            }
        }
        if !success.is_empty()
            && failed.is_empty()
            && !self
                .files
                .iter()
                .any(|f| f.change.selectable() && self.checks.get(&f.key) == Some(&true))
        {
            self.saved.message = Some(String::new());
        }
        if let Err(e) = self.save(c) {
            out.push(format!("Failed to save local state: {e}"))
        }
        Action::Committed {
            repos: success.clone(),
            push: self.push && !success.is_empty(),
            message: out.join(" | "),
        }
    }
    pub(crate) fn hover_at(&mut self, x: u16, y: u16) -> bool {
        if self.edit.is_some() {
            return false;
        }
        if let Some(menu) = &mut self.view_menu {
            let item = self.hits.iter().rev().find_map(|(r, h)| {
                if inside(*r, x, y) {
                    if let Hit::ViewItem(item) = h {
                        return Some(*item);
                    }
                }
                None
            });
            return item.is_some_and(|item| {
                let changed = menu.row != item as usize;
                menu.row = item as usize;
                changed
            });
        }
        let new = self
            .hits
            .iter()
            .find(|(area, _)| inside(*area, x, y))
            .and_then(|(_, hit)| hit.hint());
        let changed = self.hover != new;
        self.hover = new;
        changed
    }
    fn launch(&mut self, h: Hit, c: &Context) -> Action {
        match h {
            Hit::Refresh => return Action::Refresh("Changes refreshed".into()),
            Hit::ResolveConflicts => return Action::OpenConflicts,
            Hit::UndoCheck => {
                for (key, was) in self.last_check.drain(..) {
                    self.checks.insert(key, was);
                }
            }
            Hit::TogglePreview => {
                // A narrow window can show the message/tree while the preview
                // is still in the focus cycle. The eye always opens it there.
                self.preview_open =
                    !self.preview_open || (self.stacked && self.focus != Focus::Diff);
                if !self.preview_open {
                    self.diff_selection.reset();
                }
                self.focus = if self.preview_open && self.stacked {
                    Focus::Diff
                } else {
                    Focus::Tree
                };
            }
            Hit::ViewOptions => self.toggle_view_menu(),
            Hit::ViewItem(item) => {
                if let Err(e) = self.toggle_view_item(item, c) {
                    return err(e);
                }
            }
            Hit::Directories => self.set_directory_grouping(!self.options.directories, c),
            Hit::Status => self.options.status = !self.options.status,
            Hit::Expand(expanded) => self.set_all_expanded(expanded, c),
            Hit::New => {
                self.buf.clear();
                self.caret = 0;
                self.edit = Some(Edit::Group)
            }
            Hit::Move => self.move_menu(),
            Hit::Msg => self.edit_message(c),
            Hit::Amend => {
                let Some(root) = self.highlighted_repo() else {
                    return err("Highlight a file or group from one repository to toggle Amend");
                };
                if !self.amend.remove(&root) {
                    self.amend.insert(root);
                }
            }
            Hit::Submit(p) => return self.review(c, p).unwrap_or_else(err),
            Hit::Ai => return self.ai_review(c).unwrap_or_else(err),
            Hit::Settings => {
                self.buf = format!(
                    "{}\n{}\n{}",
                    self.ai_settings.endpoint, self.ai_settings.model, self.ai_settings.prompt
                );
                self.caret = self.buf.len();
                self.edit = Some(Edit::AISettings)
            }
            Hit::EditSave => {
                if matches!(self.edit, Some(Edit::Stash)) {
                    return self.review_stash(c).unwrap_or_else(err);
                }
                if let Err(e) = self.accept(c) {
                    return err(e);
                }
            }
            Hit::EditCancel => {
                self.edit = None;
            }
            Hit::MoveTo(s) => {
                if let Some(k) = self.move_names.iter().position(|x| *x == s) {
                    self.move_to = k;
                    if let Err(e) = self.accept(c) {
                        return err(e);
                    }
                }
            }
            Hit::Show(focus) => {
                self.focus = focus;
                self.preview_open = focus == Focus::Diff;
            }
            Hit::Row(_) => {}
        }
        Action::None
    }
}
impl Panel for CommitPanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        if !self.loaded {
            let p = c.state_dir.join("commit.json");
            if p.exists() {
                self.saved = serde_json::from_slice(&fs::read(p)?)?
            }
            if self.saved.message.is_none() {
                self.saved.message = Some(
                    (0..c.repos.len())
                        .filter_map(|r| self.saved.drafts.get(&rid(c, r)))
                        .find(|message| !message.trim().is_empty())
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            self.ai_settings = self.ai_config.clone().unwrap_or_else(|| {
                let mut settings = crate::config::AiConfig {
                    endpoint: self.saved.endpoint.clone(),
                    model: self.saved.model.clone(),
                    ..Default::default()
                };
                if !self.saved.prompt.is_empty() {
                    settings.prompt = self.saved.prompt.clone();
                }
                settings
            });
            self.loaded = true
        }
        self.refresh_files(c)
    }
    fn draw(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.draw_content(f, a, c);
        self.draw_overlays(f, a, c, true);
    }
    fn tick(&mut self, _c: &Context) -> Action {
        let pending = self.ai_pending.is_some();
        self.poll_ai();
        if pending && self.ai_pending.is_none() {
            if self.ai_error.is_empty() {
                Action::Notice("AI message ready. Open Commit to review".into())
            } else {
                Action::Notice(self.ai_error.clone())
            }
        } else {
            Action::None
        }
    }
    fn editing(&self) -> bool {
        self.edit.is_some() || self.view_menu.is_some()
    }
    fn selected_text(&self) -> Option<&str> {
        if self.view_menu.is_some() {
            return None;
        }
        if matches!(self.edit, Some(Edit::Message)) {
            self.message_selection.selected_text()
        } else if self.edit.is_none()
            && self.preview_open
            && self.focus == Focus::Diff
            && self.active().is_some_and(|r| r.id == self.diff_key)
        {
            self.diff_selection.selected_text()
        } else {
            None
        }
    }
    fn help(&self) -> &'static str {
        if self.message_editing() {
            "F2/Ctrl-S Save · Esc Cancel · Enter Newline · F1 Help"
        } else if matches!(self.edit, Some(Edit::Stash)) {
            "Enter Review · Esc Cancel · F1 Help"
        } else {
            "Space check · Enter/v preview · s stash · d discard · i message · c review · ? keys"
        }
    }
    fn confirm(&mut self, t: &str, c: &Context) -> Action {
        match t {
            "commit" => self.submit(c),
            "commit-stash" => self.submit_stash(c),
            "commit-ai" => self.start_ai(c).unwrap_or_else(err),
            _ => Action::None,
        }
    }
    fn event(&mut self, e: &Event, c: &Context) -> Action {
        if matches!(e, Event::Key(k) if k.kind == crossterm::event::KeyEventKind::Release) {
            return Action::None;
        }
        if matches!(e, Event::FocusLost) {
            self.dragged = None;
            self.diff_selection.clear();
            self.message_selection.clear();
        }
        if let Some(action) = self.view_menu_event(e, c) {
            return action;
        }
        // Inline editing releases its draft before the same click/key navigates.
        // Save/Cancel retain their explicit behavior instead of triggering blur.
        if self.message_editing()
            && (matches!(e, Event::Key(k)
                if matches!(k.code, K::Tab | K::BackTab | K::F(6))
                    && (k.modifiers.is_empty() || k.modifiers == Mod::SHIFT))
                || matches!(e, Event::Mouse(m)
                if matches!(m.kind, M::Down(MouseButton::Left | MouseButton::Right))
                    && !inside(self.edit_view.0, m.column, m.row)
                    && !self.hits.iter().any(|(r, h)|
                        matches!(h, Hit::EditSave | Hit::EditCancel)
                            && inside(*r, m.column, m.row))))
        {
            if let Err(error) = self.finish_message(c) {
                return err(error);
            }
        }
        if self.edit.is_some() {
            return self.edit_event(e, c);
        }
        if self.active().is_none_or(|r| r.id != self.diff_key) {
            self.diff_selection.reset();
        }
        if self.focus == Focus::Diff && self.preview_open {
            if let Some(action) = self.diff_selection.key(e, true) {
                return action;
            }
        }
        if let Event::Mouse(m) = e {
            if self.panes.mouse(m) {
                self.dragged = None;
                self.last_click = None;
                return Action::None;
            }
            if m.kind == M::Down(MouseButton::Left) {
                if let Some(pane) = self.panes.at(m.column, m.row) {
                    self.focus_pane(pane);
                }
            }
            if let Some(action) = self
                .preview_open
                .then(|| self.diff_selection.mouse(m))
                .flatten()
            {
                self.focus = Focus::Diff;
                self.dragged = None;
                self.last_click = None;
                return action;
            }
        }
        if !matches!(e, Event::Mouse(m) if matches!(m.kind, M::Down(MouseButton::Left) | M::Up(MouseButton::Left)))
        {
            self.last_click = None;
        }
        self.hover = None;
        match e {
            Event::Mouse(m) => {
                if m.kind == M::Drag(MouseButton::Left) {
                    self.last_click = None;
                    if let Some(v) = self.dragged.as_mut() {
                        v.1 = true;
                    }
                    return Action::None;
                }
                if m.kind == M::Up(MouseButton::Left) {
                    let item = self.dragged.take();
                    if let Some((i, true)) = item {
                        let Some(file) = self.files.get(i) else {
                            return Action::None;
                        };
                        let target = self.hits.iter().find_map(|(area, hit)| {
                            if inside(*area, m.column, m.row) {
                                if let Hit::Row(idx) = hit {
                                    let row = self.view.get(*idx)?;
                                    (row.kind == Kind::Group).then(|| row.label.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });
                        if let Some(g) = target {
                            if g == NEW || g == IGNORED || !file.movable() {
                                return err("Only tracked files without conflicts can move between changelists");
                            }
                            self.saved.membership.insert(file.key.clone(), g.clone());
                            self.files[i].group = g;
                            self.sync(c);
                            return self.save(c).map(|_| Action::None).unwrap_or_else(err);
                        }
                    }
                    return Action::None;
                }
                if m.kind == M::Down(MouseButton::Left) {
                    self.dragged = None;
                    for (r, h) in self.hits.clone() {
                        if inside(r, m.column, m.row) {
                            if let Hit::Row(i) = h {
                                self.focus = Focus::Tree;
                                let row = &self.view[i];
                                let (arrow, check) = self.row_controls(row, r);
                                let previous = self.last_click.take();
                                if inside(check, m.column, m.row) {
                                    self.toggle(i);
                                } else if inside(arrow, m.column, m.row) {
                                    self.fold(i, None, c)
                                } else {
                                    self.cursor = i;
                                    if let Some(file) = row.file() {
                                        if previous.is_some_and(|(id, time)| {
                                            id == row.id
                                                && time.elapsed() <= Duration::from_millis(450)
                                        }) {
                                            return self.launch(Hit::Show(Focus::Diff), c);
                                        }
                                        self.last_click = Some((row.id.clone(), Instant::now()));
                                        if self.files[file].movable() {
                                            self.dragged = Some((file, false));
                                        }
                                    }
                                }
                                return Action::None;
                            } else {
                                self.last_click = None;
                                if matches!(h, Hit::Msg) {
                                    self.edit_message(c);
                                    return self.edit_event(e, c);
                                }
                                return self.launch(h, c);
                            }
                        }
                    }
                    if inside(self.diff, m.column, m.row) {
                        self.focus = Focus::Diff
                    }
                } else if m.kind == M::Down(MouseButton::Right)
                    && inside(self.tree, m.column, m.row)
                {
                    let i = self.offset + (m.row - self.tree.y) as usize;
                    if i < self.view.len() {
                        self.cursor = i;
                        self.focus = Focus::Tree;
                        self.move_menu()
                    }
                } else if matches!(m.kind, M::ScrollUp | M::ScrollDown) {
                    let d = if m.kind == M::ScrollUp { -3 } else { 3 };
                    if inside(self.tree, m.column, m.row) {
                        self.move_cursor(d)
                    } else if inside(self.diff, m.column, m.row) {
                        self.diff_scroll = self.diff_scroll.saturating_add_signed(d as i16)
                    }
                }
            }
            Event::Key(k) => match k.code {
                K::Char('j') | K::Down => {
                    if self.focus == Focus::Diff {
                        self.diff_scroll = self.diff_scroll.saturating_add(1)
                    } else {
                        self.move_cursor(1)
                    }
                }
                K::Char('k') | K::Up => {
                    if self.focus == Focus::Diff {
                        self.diff_scroll = self.diff_scroll.saturating_sub(1)
                    } else {
                        self.move_cursor(-1)
                    }
                }
                K::Char('g') | K::Home if self.focus == Focus::Diff => self.diff_scroll = 0,
                K::Char('G') | K::End if self.focus == Focus::Diff => self.diff_scroll = u16::MAX,
                K::Char('g') | K::Home => self.cursor = 0,
                K::Char('G') | K::End => self.cursor = self.view.len().saturating_sub(1),
                K::PageUp | K::PageDown => {
                    let forward = k.code == K::PageDown;
                    if self.focus == Focus::Diff {
                        let step = self.diff.height.saturating_sub(1).max(1);
                        self.diff_scroll = if forward {
                            self.diff_scroll.saturating_add(step)
                        } else {
                            self.diff_scroll.saturating_sub(step)
                        };
                    } else {
                        let step = self.tree.height.saturating_sub(1).max(1) as isize;
                        self.focus = Focus::Tree;
                        self.move_cursor(if forward { step } else { -step });
                    }
                }
                K::Char(' ') if self.focus == Focus::Tree => self.toggle(self.cursor),
                K::Char('d') if self.focus == Focus::Tree && k.modifiers == Mod::NONE => {
                    return self.discard_review(c).unwrap_or_else(err);
                }
                K::Char('h') | K::Left if self.focus == Focus::Tree => {
                    if let Some(row) = self.active() {
                        if !row.foldable() || self.fold.contains(&row.id) {
                            let d = row.depth;
                            if let Some(i) =
                                self.view[..self.cursor].iter().rposition(|x| x.depth < d)
                            {
                                self.cursor = i;
                            }
                        } else {
                            self.fold(self.cursor, Some(true), c)
                        }
                    }
                }
                K::Char('l') | K::Right if self.focus == Focus::Tree => {
                    if let Some(row) = self.active() {
                        if row.foldable() && !self.fold.contains(&row.id) {
                            if self
                                .view
                                .get(self.cursor + 1)
                                .is_some_and(|x| x.depth > row.depth)
                            {
                                self.move_cursor(1)
                            }
                        } else {
                            self.fold(self.cursor, Some(false), c)
                        }
                    }
                }
                K::Char('t') => return self.launch(Hit::Directories, c),
                K::Char('o') => return self.launch(Hit::ViewOptions, c),
                K::Char('s') if k.modifiers == Mod::NONE => return self.edit_stash(c),
                K::Char('T') => return self.launch(Hit::Status, c),
                K::Char('v') => return self.launch(Hit::TogglePreview, c),
                K::Char('u') => return self.launch(Hit::UndoCheck, c),
                K::Char('z') => return self.launch(Hit::Expand(false), c),
                K::Char('Z') => return self.launch(Hit::Expand(true), c),
                K::Enter => {
                    if self.focus == Focus::Message {
                        self.edit_message(c)
                    } else if self.focus == Focus::Diff {
                        return self.launch(Hit::Show(Focus::Tree), c);
                    } else if self.active().is_some_and(|row| row.kind == Kind::Conflicts) {
                        return self.launch(Hit::ResolveConflicts, c);
                    } else if self.active().is_some_and(|x| x.file().is_some()) {
                        return self.launch(Hit::Show(Focus::Diff), c);
                    } else {
                        self.fold(self.cursor, None, c)
                    }
                }
                K::Tab | K::BackTab | K::F(6) => {
                    let fs = if self.preview_open {
                        &[Focus::Tree, Focus::Diff, Focus::Message][..]
                    } else {
                        &[Focus::Tree, Focus::Message][..]
                    };
                    let i = fs.iter().position(|s| *s == self.focus).unwrap_or(0);
                    self.focus =
                        fs[(i + if k.modifiers.contains(Mod::SHIFT) || k.code == K::BackTab {
                            fs.len() - 1
                        } else {
                            1
                        }) % fs.len()];
                }
                K::Char('i') => self.edit_message(c),
                K::Char('n') => return self.launch(Hit::New, c),
                K::Char('m') => self.move_menu(),
                K::Char('a') => return self.launch(Hit::Amend, c),
                K::Char('A') => return self.launch(Hit::Ai, c),
                K::Char('S') => return self.launch(Hit::Settings, c),
                K::Char('c') => return self.launch(Hit::Submit(false), c),
                K::Char('P') => return self.launch(Hit::Submit(true), c),
                K::Char(']') | K::Char('[') => self.browse_repo(k.code == K::Char(']'), c),
                K::Esc => return self.launch(Hit::Show(Focus::Tree), c),
                _ => {}
            },
            _ => {}
        }
        Action::None
    }
}
impl CommitPanel {
    pub(crate) fn draw_content(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.draw_workbench(f, a, c);
        if self.diff.is_empty() {
            self.diff_selection.reset();
        }
    }
    pub(crate) fn draw_overlays(&mut self, f: &mut Frame, a: Rect, c: &Context, cursor: bool) {
        if self.message_editing() {
            self.draw_input_cursor(f, cursor);
        } else if self.edit.is_some() {
            self.draw_edit(f, a, c, cursor);
        }
        self.draw_view_menu(f, a);
    }

    fn draw_edit(&mut self, f: &mut Frame, a: Rect, _c: &Context, cursor: bool) {
        let h = (a.height.saturating_sub(2)).min(17);
        let w = a.width.saturating_sub(4).min(86);
        let area = Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h);
        f.render_widget(Clear, area);
        self.hits.clear();
        let ed = self.edit.clone().unwrap();
        let title: String = match ed {
            Edit::Message => "Edit commit message".into(),
            Edit::Stash => "Create stash".into(),
            Edit::Group => "New changelist".into(),
            Edit::Move => "Move to changelist".into(),
            Edit::AISettings => "AI settings".into(),
            Edit::AIDraft => "AI candidate · review before replacing message".into(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(FG).bg(PANEL))
            .border_style(Style::default().fg(BLUE))
            .title(format!(" {title} "));
        let inn = block.inner(area);
        f.render_widget(block, area);
        if inn.height < 3 {
            return;
        }
        let key_hint = format!(
            "Lines: URL, model, prompt. Key: {}",
            self.ai_settings.api_key_env
        );
        let hint = match ed {
            Edit::Group => "Enter Save · Esc Cancel",
            Edit::Stash if inn.width >= 48 => "All changes in Roots, including untracked files",
            Edit::Stash => "All Roots changes + untracked",
            Edit::Message if inn.width >= 60 => {
                "Drag to copy · Shift+arrows Select · Enter Newline · Ctrl-A All"
            }
            Edit::Message => "Drag to copy · F1 Help",
            Edit::Move => "j/k Select · Enter Apply · Esc Cancel",
            Edit::AISettings if inn.width >= 62 => &key_hint,
            Edit::AISettings => "Lines: API URL, model, prompt",
            Edit::AIDraft => "Save replaces shared message · Esc Discard",
        };
        word(f, slice(inn, 0), hint, Style::default().fg(MUTED));
        let text = Rect::new(inn.x, inn.y + 1, inn.width, inn.height - 2);
        if matches!(ed, Edit::Move) {
            let start = self
                .move_to
                .saturating_sub(text.height.saturating_sub(1) as usize);
            for (n, s) in self
                .move_names
                .iter()
                .enumerate()
                .skip(start)
                .take(text.height as usize)
            {
                let r = slice(text, (n - start) as u16);
                word(
                    f,
                    r,
                    s,
                    Style::default()
                        .fg(FG)
                        .bg(if n == self.move_to { SEL } else { Color::Reset }),
                );
                self.hits.push((r, Hit::MoveTo(s.clone())))
            }
        } else if matches!(ed, Edit::AIDraft) {
            f.render_widget(
                Paragraph::new(self.ai_draft.as_str())
                    .wrap(Wrap { trim: false })
                    .scroll((self.ai_scroll, 0)),
                text,
            )
        } else {
            self.draw_input(f, text, cursor);
        }
        let foot = slice(inn, inn.height - 1);
        let mut x = foot.x;
        let save_key = if matches!(ed, Edit::Message | Edit::AISettings) {
            "F2"
        } else {
            self.icons.glyph(Icon::Enter)
        };
        let save_label = if matches!(ed, Edit::Stash) {
            format!("Review {save_key}")
        } else {
            format!("Save {save_key}")
        };
        self.hit_button(f, foot, &mut x, &save_label, Hit::EditSave);
        self.hit_button(f, foot, &mut x, "Cancel Esc", Hit::EditCancel);
    }
    fn input_position(&self) -> (u16, u16) {
        let pos = self.caret.min(self.buf.len());
        let prefix = &self.buf[..pos];
        let row = prefix.bytes().filter(|b| *b == b'\n').count();
        let line = prefix.rsplit('\n').next().unwrap_or("");
        let column = unicode_width::UnicodeWidthStr::width(line);
        (
            row.min(u16::MAX as usize) as u16,
            column.min(u16::MAX as usize) as u16,
        )
    }

    fn draw_input_cursor(&self, f: &mut Frame, cursor: bool) {
        let (text, off, hor) = self.edit_view;
        let (row, col) = self.input_position();
        if cursor && row >= off && row - off < text.height && col >= hor && col - hor < text.width {
            f.set_cursor_position((text.x + col - hor, text.y + row - off));
        }
    }

    fn draw_input(&mut self, f: &mut Frame, text: Rect, cursor: bool) {
        let lines: Vec<_> = self.buf.split('\n').map(Line::from).collect();
        let (cr, col) = self.input_position();
        let (off, hor) = if self.message_editing() {
            let (_, old_y, old_x) = self.edit_view;
            if self.edit_follow {
                (
                    old_y
                        .min(cr)
                        .max(cr.saturating_sub(text.height.saturating_sub(1))),
                    old_x
                        .min(col)
                        .max(col.saturating_sub(text.width.saturating_sub(1))),
                )
            } else {
                (
                    old_y.min(
                        (lines.len().saturating_sub(usize::from(text.height)))
                            .min(u16::MAX as usize) as u16,
                    ),
                    old_x.min(
                        lines
                            .iter()
                            .map(Line::width)
                            .max()
                            .unwrap_or(0)
                            .saturating_sub(usize::from(text.width.saturating_sub(1)))
                            .min(u16::MAX as usize) as u16,
                    ),
                )
            }
        } else {
            (
                cr.saturating_sub(text.height.saturating_sub(1)),
                col.saturating_sub(text.width.saturating_sub(1)),
            )
        };
        self.edit_view = (text, off, hor);
        if self.message_editing() {
            self.message_selection.render(
                f,
                text,
                TextLayout::plain(&self.buf, text.width, TextWrap::Clip),
                usize::from(off),
                usize::from(hor),
            );
        } else {
            f.render_widget(Paragraph::new(lines).scroll((off, hor)), text);
        }
        self.draw_input_cursor(f, cursor);
    }
    fn edit_event(&mut self, e: &Event, c: &Context) -> Action {
        if matches!(self.edit, Some(Edit::Message)) {
            self.message_selection.sync_source(&self.buf);
            if matches!(e, Event::Key(_) | Event::Paste(_)) {
                self.edit_follow = true;
            }
            if let Some(action) = self.message_selection.key(e, false) {
                if matches!(e, Event::Key(k) if k.code == K::Char('a') && k.modifiers == Mod::CONTROL)
                {
                    self.caret = self.message_selection.head();
                }
                return action;
            }
            if let Event::Mouse(m) = e {
                if matches!(m.kind, M::ScrollUp | M::ScrollDown)
                    && inside(self.edit_view.0, m.column, m.row)
                {
                    self.edit_view.1 = self
                        .edit_view
                        .1
                        .saturating_add_signed(if m.kind == M::ScrollUp { -3 } else { 3 });
                    self.edit_follow = false;
                    return Action::None;
                }
                if let Some(action) = self.message_selection.mouse(m) {
                    self.caret = self.message_selection.head();
                    self.edit_follow = false;
                    if m.kind == M::Drag(MouseButton::Left) {
                        let (a, off, hor) = &mut self.edit_view;
                        if m.row < a.y {
                            *off = off.saturating_sub(1);
                        } else if m.row >= a.bottom() {
                            *off = off.saturating_add(1);
                        }
                        if m.column < a.x {
                            *hor = hor.saturating_sub(1);
                        } else if m.column >= a.right() {
                            *hor = hor.saturating_add(1);
                        }
                    }
                    return action;
                }
            }
        }
        if let Event::Mouse(m) = e {
            if m.kind == M::Down(MouseButton::Left) {
                if let Some((_, h)) = self
                    .hits
                    .iter()
                    .find(|(r, h)| {
                        inside(*r, m.column, m.row)
                            && !(self.message_editing() && matches!(h, Hit::Msg))
                    })
                    .cloned()
                {
                    return self.launch(h, c);
                }
                let (a, off, hor) = self.edit_view;
                if matches!(
                    self.edit,
                    Some(Edit::Message | Edit::AISettings | Edit::Group | Edit::Stash)
                ) && inside(a, m.column, m.row)
                {
                    let (line, cell) = (
                        (off + m.row - a.y) as usize,
                        (hor + m.column - a.x) as usize,
                    );
                    let mut pos = 0;
                    for (i, s) in self.buf.split('\n').enumerate() {
                        if i == line {
                            let mut width = 0;
                            let mut bytes = 0;
                            for ch in s.chars() {
                                let n = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                                if width + n > cell {
                                    break;
                                }
                                width += n;
                                bytes += ch.len_utf8()
                            }
                            self.caret = pos + bytes;
                            return Action::None;
                        }
                        pos += s.len() + 1
                    }
                    self.caret = self.buf.len()
                }
            } else if matches!(self.edit, Some(Edit::AIDraft)) {
                match m.kind {
                    M::ScrollDown => self.ai_scroll = self.ai_scroll.saturating_add(2),
                    M::ScrollUp => self.ai_scroll = self.ai_scroll.saturating_sub(2),
                    _ => {}
                }
            }
            return Action::None;
        }
        if let Event::Paste(s) = e {
            if matches!(
                self.edit,
                Some(Edit::Message | Edit::AISettings | Edit::Group | Edit::Stash)
            ) {
                let s = if matches!(self.edit, Some(Edit::Group | Edit::Stash)) {
                    s.replace(['\n', '\r'], " ")
                } else {
                    s.clone()
                };
                self.remove_message_selection();
                self.buf.insert_str(self.caret, &s);
                self.caret += s.len()
            }
            return Action::None;
        }
        let Event::Key(k) = e else {
            return Action::None;
        };
        if k.code == K::Esc {
            self.edit = None;
            self.focus = Focus::Tree;
            return Action::None;
        }
        if k.code == K::F(2)
            || (matches!(k.code, K::Char('s') | K::Enter) && k.modifiers.contains(Mod::CONTROL))
            || k.code == K::Enter
                && matches!(
                    self.edit,
                    Some(Edit::Move | Edit::Group | Edit::AIDraft | Edit::Stash)
                )
        {
            return self.launch(Hit::EditSave, c);
        }
        if matches!(self.edit, Some(Edit::Move)) {
            match k.code {
                K::Char('j') | K::Down => {
                    self.move_to = (self.move_to + 1).min(self.move_names.len().saturating_sub(1))
                }
                K::Char('k') | K::Up => self.move_to = self.move_to.saturating_sub(1),
                _ => {}
            }
            return Action::None;
        }
        if matches!(self.edit, Some(Edit::AIDraft)) {
            match k.code {
                K::Char('j') | K::Down => self.ai_scroll = self.ai_scroll.saturating_add(1),
                K::Char('k') | K::Up => self.ai_scroll = self.ai_scroll.saturating_sub(1),
                _ => {}
            }
            return Action::None;
        }
        if self.message_editing()
            && k.modifiers == Mod::CONTROL
            && matches!(k.code, K::Char('w' | 'u'))
        {
            if !self.remove_message_selection() {
                let prefix = &self.buf[..self.caret];
                let start = if k.code == K::Char('u') {
                    prefix.rfind('\n').map_or(0, |i| i + 1)
                } else {
                    let word = prefix.trim_end();
                    word.char_indices()
                        .rev()
                        .find(|(_, ch)| ch.is_whitespace())
                        .map_or(0, |(i, ch)| i + ch.len_utf8())
                };
                self.buf.replace_range(start..self.caret, "");
                self.caret = start;
            }
            return Action::None;
        }
        let navigation = matches!(
            k.code,
            K::Left | K::Right | K::Up | K::Down | K::Home | K::End
        );
        let shift = k.modifiers.contains(Mod::SHIFT);
        if matches!(self.edit, Some(Edit::Message)) {
            if matches!(k.code, K::Char(_) | K::Enter | K::Backspace | K::Delete)
                && !k.modifiers.intersects(Mod::CONTROL | Mod::ALT)
            {
                let removed = self.remove_message_selection();
                if removed && matches!(k.code, K::Backspace | K::Delete) {
                    return Action::None;
                }
            } else if navigation && !shift {
                if let Some(range) = self.message_selection.range() {
                    self.message_selection.clear();
                    if matches!(k.code, K::Left | K::Right) {
                        self.caret = if k.code == K::Left {
                            range.start
                        } else {
                            range.end
                        };
                        return Action::None;
                    }
                }
            }
        }
        let pos = self.caret.min(self.buf.len());
        let prev = selection::previous(&self.buf, pos);
        let next = selection::next(&self.buf, pos);
        match k.code {
            K::Char(ch) if !k.modifiers.intersects(Mod::CONTROL | Mod::ALT) => {
                self.buf.insert(pos, ch);
                self.caret = pos + ch.len_utf8()
            }
            K::Enter => {
                self.buf.insert(pos, '\n');
                self.caret = pos + 1
            }
            K::Backspace => {
                self.buf.replace_range(prev..pos, "");
                self.caret = prev
            }
            K::Delete => {
                self.buf.replace_range(pos..next, "");
            }
            K::Left => self.caret = prev,
            K::Right => self.caret = next,
            K::Home => self.caret = self.buf[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0),
            K::End => self.caret = pos + self.buf[pos..].find('\n').unwrap_or(self.buf.len() - pos),
            K::Up | K::Down => {
                let start = self.buf[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let right = pos + self.buf[pos..].find('\n').unwrap_or(self.buf.len() - pos);
                let column = unicode_width::UnicodeWidthStr::width(&self.buf[start..pos]);
                let (s, e) = if k.code == K::Up && start > 0 {
                    let e = start - 1;
                    (self.buf[..e].rfind('\n').map(|i| i + 1).unwrap_or(0), e)
                } else if k.code == K::Down && right < self.buf.len() {
                    let s = right + 1;
                    (
                        s,
                        s + self.buf[s..].find('\n').unwrap_or(self.buf.len() - s),
                    )
                } else {
                    return Action::None;
                };
                self.caret = s + selection::at_column(&self.buf[s..e], column);
            }
            _ => {}
        }
        if navigation && matches!(self.edit, Some(Edit::Message)) {
            if shift {
                self.message_selection.extend(pos, self.caret);
            } else {
                self.message_selection.clear();
            }
        }
        Action::None
    }

    fn remove_message_selection(&mut self) -> bool {
        if matches!(self.edit, Some(Edit::Message)) {
            if let Some(range) = self.message_selection.range() {
                self.caret = range.start;
                self.buf.replace_range(range, "");
                self.message_selection.clear();
                return true;
            }
        }
        false
    }
}
#[cfg(test)]
mod flow_tests {
    use super::*;
    use crate::core::{git, Repo};
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn external_branch_switch_review_names_the_target_that_is_checked() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("app");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        for (k, v) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@invalid.test"),
            ("core.hooksPath", "/dev/null"),
            ("commit.gpgSign", "false"),
        ] {
            git(&repo, &["config", k, v]).unwrap();
        }
        let mut c = Context::discover(&repo).unwrap();
        c.state_dir = d.path().join("state");
        fs::write(repo.join("file"), "base\n").unwrap();
        git(&repo, &["add", "--", "file"]).unwrap();
        let mut panel = CommitPanel::default();
        panel.refresh(&c).unwrap();
        panel.saved.message = Some("edit".into());
        let review = |panel: &mut CommitPanel| {
            let Action::Confirm { body, .. } = panel.review(&c, false).unwrap() else {
                panic!("missing commit review");
            };
            body.lines().next().unwrap().to_owned()
        };

        // Before the first commit HEAD already names a branch, even without an OID.
        git(&repo, &["symbolic-ref", "HEAD", "refs/heads/feature/first"]).unwrap();
        assert!(review(&mut panel).starts_with("app / feature/first · "));
        git(&repo, &["commit", "-qm", "base"]).unwrap();
        let base = git(&repo, &["rev-parse", "HEAD"]).unwrap();
        fs::write(repo.join("file"), "pending\n").unwrap();
        panel.refresh(&c).unwrap();
        git(&repo, &["switch", "-qc", "reviewed"]).unwrap();
        assert!(review(&mut panel).starts_with("app / reviewed · "));

        // Moving to another branch at the same commit after review must refuse the write.
        git(&repo, &["switch", "-q", "feature/first"]).unwrap();
        let Action::Committed { repos, message, .. } = panel.confirm("commit", &c) else {
            panic!("missing commit result");
        };
        assert!(
            repos.is_empty() && message.contains("Branch or HEAD changed"),
            "{message}"
        );
        for rev in ["reviewed", "feature/first"] {
            assert_eq!(git(&repo, &["rev-parse", rev]).unwrap(), base);
        }
        assert!(git(&repo, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty());
        assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "pending\n");

        git(&repo, &["switch", "-q", "--detach", &base]).unwrap();
        assert!(review(&mut panel).starts_with(&format!("app / detached@{} · ", &base[..8])));

        // A fresh review commits to the branch it names, leaving the former target untouched.
        git(&repo, &["switch", "-q", "reviewed"]).unwrap();
        assert!(review(&mut panel).starts_with("app / reviewed · "));
        let Action::Committed { repos, message, .. } = panel.confirm("commit", &c) else {
            panic!("missing commit result");
        };
        assert_eq!(repos, [0], "{message}");
        assert_eq!(git(&repo, &["rev-parse", "reviewed^"]).unwrap(), base);
        assert_eq!(git(&repo, &["rev-parse", "feature/first"]).unwrap(), base);
        assert_eq!(git(&repo, &["show", "reviewed:file"]).unwrap(), "pending");
    }

    #[test]
    fn filtered_batch_and_mouse_move_preserve_the_other_repo_check() {
        let d = tempfile::tempdir().unwrap();
        let repos = ["web", "api"]
            .into_iter()
            .map(|n| {
                let p = d.path().join(n);
                fs::create_dir(&p).unwrap();
                for args in [
                    &["init", "-q"][..],
                    &["config", "user.name", "Fixture"],
                    &["config", "user.email", "fixture@invalid.test"],
                    &["config", "core.hooksPath", ".git/hooks"],
                    &["config", "commit.gpgsign", "false"],
                ] {
                    git(&p, args).unwrap();
                }
                fs::write(p.join("settings"), "base").unwrap();
                git(&p, &["add", "settings"]).unwrap();
                git(&p, &["commit", "-qm", "base"]).unwrap();
                fs::write(p.join("settings"), "change").unwrap();
                Repo {
                    path: p,
                    name: n.into(),
                    branch: "main".into(),
                    parent: None,
                    submodule: false,
                }
            })
            .collect();
        let mut c = Context {
            repos,
            scope: [0, 1].into_iter().collect(),
            workspace: d.path().into(),
            state_dir: d.path().join("state"),
        };
        let mut panel = CommitPanel::default();
        panel.refresh(&c).unwrap();
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        let key = |ch| Event::Key(KeyEvent::new(K::Char(ch), Mod::NONE));
        panel.event(&key('n'), &c);
        panel.event(&Event::Paste("Later".into()), &c);
        panel.event(&Event::Key(KeyEvent::new(K::Enter, Mod::NONE)), &c);
        term.draw(|f| panel.draw(f, f.area(), &c)).unwrap();
        let (mut source, mut group) = (None, None);
        for (rect, hit) in &panel.hits {
            if let Hit::Row(i) = hit {
                let row = &panel.view[*i];
                if row.file().is_some_and(|f| panel.files[f].root == 0) {
                    source = Some(*rect)
                }
                if row.id == "g:Later" {
                    group = Some(*rect)
                }
            }
        }
        let send = |panel: &mut CommitPanel, kind, r: Rect| {
            panel.event(
                &Event::Mouse(MouseEvent {
                    kind,
                    column: r.x + 12,
                    row: r.y,
                    modifiers: Mod::NONE,
                }),
                &c,
            );
        };
        send(&mut panel, M::Down(MouseButton::Left), source.unwrap());
        send(&mut panel, M::Drag(MouseButton::Left), group.unwrap());
        send(&mut panel, M::Up(MouseButton::Left), group.unwrap());
        assert_eq!(
            panel.files.iter().find(|f| f.root == 0).unwrap().group,
            "Later"
        );
        assert_eq!(panel.checked_repos(&c), vec![0, 1]);
        c.scope = [1].into_iter().collect();
        panel.refresh(&c).unwrap();
        panel.event(&key('i'), &c);
        panel.event(&Event::Paste("API only".into()), &c);
        panel.event(&Event::Key(KeyEvent::new(K::F(2), Mod::NONE)), &c);
        let Action::Confirm { body, token, .. } = panel.event(&key('P'), &c) else {
            panic!("missing review")
        };
        assert!(body.contains("api") && !body.contains("web"));
        let Action::Committed { repos, push, .. } = panel.confirm(&token, &c) else {
            panic!("missing result")
        };
        assert_eq!(repos, vec![1]);
        assert!(push);
        assert_eq!(
            git(&c.repos[1].path, &["log", "-1", "--format=%s"]).unwrap(),
            "API only"
        );
        assert_eq!(
            git(&c.repos[0].path, &["log", "-1", "--format=%s"]).unwrap(),
            "base"
        );
        c.scope.insert(0);
        panel.refresh(&c).unwrap();
        assert_eq!(panel.checked_repos(&c), vec![0]);
    }
}
