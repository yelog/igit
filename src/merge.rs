//! Three-way merge editor. The working file is untouched until Apply is confirmed.
mod engine;
mod keymap;
use self::engine::*;
use crate::{
    core::Context,
    ui::{
        icons::{Icon, IconMode},
        idea::{button_style, repo_tint, CONFLICT, GREEN},
        inside,
        key_sequence::KeySequence,
        pane,
        panes::{Axis, PaneId, PaneLayout, SplitId},
        Action, Panel, ACCENT, BG, FG, MUTED, SEL,
    },
};
use anyhow::{Context as _, Result};
use crossterm::event::{Event, KeyCode as K, KeyModifiers as M, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint as C, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::{collections::HashMap, path::Path};
use unicode_width::UnicodeWidthStr;

// Subtle blue, amber and green washes on the tool-window background.
const SAFE_BG: Color = Color::Rgb(39, 45, 60);
const CONFLICT_BG: Color = Color::Rgb(54, 47, 43);
const RESOLVED_BG: Color = Color::Rgb(39, 48, 45);
type Snapshot = Vec<(usize, String, bool)>;
#[derive(Clone, Copy)]
enum Hit {
    File(usize),
    Open,
    List,
    Next(i32),
    Auto,
    Undo,
    Choice(usize, usize),
    Select(usize, usize),
    Edit,
    Both,
    Save,
    Apply,
    Continue,
    Zoom,
    Cursor(usize, usize),
    OkEdit,
    CancelEdit,
}
struct Entry {
    root: usize,
    file: Option<Conflict>,
    label: String,
}
struct Editor {
    chars: Vec<char>,
    cursor: usize,
    scroll: usize,
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
        Self {
            chars,
            cursor,
            scroll: 0,
        }
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
        let limit = if y + 1 < starts.len() {
            starts[y + 1] - 1
        } else {
            self.chars.len()
        };
        self.cursor = (starts[y].saturating_add(x)).min(limit)
    }
}
#[derive(Default)]
pub struct MergePanel {
    pub(crate) shortcut_labels: std::collections::BTreeMap<&'static str, String>,
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
    items: Vec<Entry>,
    list_index: usize,
    list_scroll: usize,
    doc: Option<Document>,
    idx: usize,
    side: usize,
    scroll: usize,
    hscroll: usize,
    zoom: bool,
    follow: bool,
    hist: Vec<Snapshot>,
    drafts: HashMap<(usize, String), (Stages, Snapshot)>,
    edit: Option<Editor>,
    prefix: Option<char>,
    hits: Vec<(Rect, Hit)>,
    rows_height: usize,
    rows_total: usize,
    plan: Option<(usize, Operation, String)>,
    last_file_click: Option<(usize, std::time::Instant)>,
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
fn linecount(s: &str) -> usize {
    s.split_inclusive('\n').count()
}
fn snap(d: &Document, indices: impl Iterator<Item = usize>) -> Snapshot {
    indices
        .map(|i| (i, d.chunks[i].result.clone(), d.chunks[i].done))
        .collect()
}
fn replay(d: &mut Document, s: Snapshot) {
    for (i, t, done) in s {
        if let Some(c) = d.chunks.get_mut(i) {
            c.result = t;
            c.done = done
        }
    }
}
fn label(x: &Chunk, s: usize) -> &str {
    match s {
        0 => &x.left,
        1 => &x.result,
        _ => &x.right,
    }
}
fn offset_glyphs(line: &str, offset: usize) -> String {
    let mut used = 0;
    line.chars()
        .skip_while(|c| {
            let skip = used < offset;
            used += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
            skip
        })
        .collect()
}
impl MergePanel {
    fn key_label(&self, key: &'static str) -> &str {
        self.shortcut_labels.get(key).map_or(key, String::as_str)
    }
    const SIDES: [PaneId; 3] = [PaneId::Current, PaneId::Result, PaneId::Incoming];

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

    /// Shared cancellation for Shell overlays and the panel's input transitions.
    pub(crate) fn cancel_key_sequence(&mut self) {
        self.prefix = None;
    }

    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        if self.doc.is_none() || self.edit.is_some() {
            return None;
        }
        match self.prefix {
            Some('[') => Some(KeySequence::MergePrevious),
            Some(']') => Some(KeySequence::MergeNext),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)] // Geometry, command, enabled state, and emphasis are independent.
    fn button(
        &mut self,
        f: &mut Frame,
        bar: Rect,
        x: &mut u16,
        text: &str,
        hit: Hit,
        on: bool,
        hot: bool,
    ) {
        let w = text.width() as u16 + 2;
        if x.saturating_add(w) > bar.right() {
            return;
        }
        let r = rect(*x, bar.y, w, 1);
        put(
            f,
            r,
            format!(" {text} "),
            if !on {
                style(MUTED).add_modifier(Modifier::DIM)
            } else {
                button_style(hot)
            },
        );
        if on {
            self.hits.push((r, hit))
        }
        *x = x.saturating_add(w + 1)
    }
    fn selected_root(&self) -> Option<usize> {
        self.doc
            .as_ref()
            .map(|d| d.file.root)
            .or_else(|| self.items.get(self.list_index).map(|e| e.root))
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
    fn choose_file(&mut self, c: &Context) -> Result<()> {
        let f = self
            .items
            .get(self.list_index)
            .and_then(|e| e.file.clone())
            .context("No unresolved file; use Continue for a ready operation")?;
        let mut d = Document::open(&c.repos[f.root].path, f)?;
        if let Some((st, changes)) = self.drafts.get(&(d.file.root, d.file.path.clone())) {
            if *st == d.file.stages {
                replay(&mut d, changes.clone())
            }
        }
        self.idx = d
            .chunks
            .iter()
            .position(|b| b.kind == Kind::Conflict && !b.done)
            .unwrap_or_else(|| d.chunks.iter().position(|b| !b.done).unwrap_or(0));
        self.doc = Some(d);
        self.side = 1;
        self.follow = true;
        self.scroll = 0;
        self.hscroll = 0;
        self.hist.clear();
        self.edit = None;
        Ok(())
    }
    fn selected_continue(&mut self, c: &Context) -> Action {
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
    fn act(&mut self, hit: Hit, c: &Context) -> Action {
        if self.edit.is_some() && !matches!(hit, Hit::OkEdit | Hit::CancelEdit | Hit::Cursor(_, _))
        {
            return Action::None;
        }
        if matches!(hit, Hit::Continue) {
            return self.selected_continue(c);
        }
        if self.doc.is_none() {
            match hit {
                Hit::File(i) => {
                    let double = self
                        .last_file_click
                        .is_some_and(|(id, t)| id == i && t.elapsed().as_millis() < 420);
                    self.list_index = i;
                    self.last_file_click = Some((i, std::time::Instant::now()));
                    if double {
                        return self.act(Hit::Open, c);
                    }
                }
                Hit::Open => {
                    if let Err(e) = self.choose_file(c) {
                        return Action::Notice(format!("{e:#}"));
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        let d = self.doc.as_mut().unwrap();
        match hit {
            Hit::List => {
                self.stash();
                self.doc = None;
                self.hist.clear();
                self.edit = None
            }
            Hit::Save => {
                self.stash();
                return Action::Notice(
                    "Merge draft saved in this session; working tree unchanged".into(),
                );
            }
            Hit::Auto => {
                let ids = (0..d.chunks.len())
                    .filter(|i| d.chunks[*i].kind == Kind::Safe && !d.chunks[*i].done)
                    .collect::<Vec<_>>();
                let s = snap(d, ids.iter().copied());
                for i in &ids {
                    d.chunks[*i].safe()
                }
                self.store(s);
                return Action::Notice(format!(
                    "Applied {} non-conflicting blocks; conflicts unchanged",
                    ids.len()
                ));
            }
            Hit::Choice(i, side) => {
                if d.chunks[i].kind == Kind::Context {
                    return Action::None;
                }
                self.idx = i;
                self.side = side;
                let s = snap(d, std::iter::once(i));
                let b = &mut d.chunks[i];
                b.result = if side == 0 {
                    b.left.clone()
                } else {
                    b.right.clone()
                };
                b.done = true;
                self.store(s);
                self.follow = true
            }
            Hit::Both => {
                let i = self.idx;
                if d.chunks[i].kind != Kind::Conflict {
                    return Action::Notice("Select a conflict block first".into());
                }
                let s = snap(d, std::iter::once(i));
                let b = &mut d.chunks[i];
                b.result = b.left.clone() + &b.right;
                b.done = true;
                self.store(s)
            }
            Hit::Edit => {
                self.edit = Some(Editor::new(&d.chunks[self.idx].result));
                self.side = 1;
                self.follow = true
            }
            Hit::Cursor(row, col) => {
                if let Some(e) = self.edit.as_mut() {
                    e.pos(row, col)
                }
            }
            Hit::OkEdit => {
                let s = snap(d, std::iter::once(self.idx));
                let e = self.edit.take().unwrap();
                let b = &mut d.chunks[self.idx];
                b.result = e.chars.iter().collect();
                b.done = true;
                self.store(s);
                self.follow = true
            }
            Hit::CancelEdit => self.edit = None,
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
                self.side = side
            }
            Hit::Zoom => {
                self.zoom = !self.zoom;
                self.follow = true
            }
            Hit::Next(dir) => {
                let len = d.chunks.len();
                for step in 1..=len {
                    let i = (self.idx as isize + (dir as isize) * (step as isize))
                        .rem_euclid(len as isize) as usize;
                    if d.chunks[i].kind == Kind::Conflict && !d.chunks[i].done {
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
                        "{} blocks still require a decision (Apply non-conflicting first)",
                        d.pending()
                    ));
                }
                return Action::Confirm{title:format!("Stage resolved file · {}",c.repos[d.file.root].name),body:format!("Write the previewed result to {} and stage ONLY this path. This does not continue the repository operation.",d.file.path),token:"merge:apply".into()};
            }
            _ => {}
        }
        Action::None
    }
    fn draw_list(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let icons = self.icons;
        self.panes.add(PaneId::Conflicts, area);
        let outer = pane(
            format!(
                "{} Merge conflicts {} scoped repositories",
                icons.glyph(Icon::Merge),
                icons.glyph(Icon::Dot)
            ),
            self.panes.active,
        );
        let body = outer.inner(area);
        f.render_widget(outer, area);
        if body.height < 2 {
            return;
        }
        let bar = rect(body.x, body.bottom() - 1, body.width, 1);
        let current = icons.glyph(Icon::Current);
        let cols = Layout::horizontal([
            C::Length(current.width() as u16 + 2),
            C::Length((body.width / 3).min(23)),
            C::Min(1),
        ])
        .split(body);
        let cell = |i: usize, y| rect(cols[i].x, y, cols[i].width, 1);
        put(f, cell(1, body.y), "REPOSITORY", style(MUTED));
        put(
            f,
            cell(2, body.y),
            " CONFLICT PATH / OPERATION",
            style(MUTED),
        );
        let count = body.height.saturating_sub(2) as usize;
        self.rows_height = count;
        self.list_scroll = self.list_scroll.min(self.list_index);
        if count > 0 && self.list_index >= self.list_scroll + count {
            self.list_scroll = self.list_index + 1 - count
        }
        if self.items.is_empty() {
            put(f,rect(body.x+1,body.y+1,body.width.saturating_sub(1),count as u16),format!("No unresolved files or ongoing merges in the selected roots.\n{} refreshes after a Git operation.", self.key_label("F5")),style(MUTED))
        }
        for (i, item) in self
            .items
            .iter()
            .enumerate()
            .skip(self.list_scroll)
            .take(count)
        {
            let r = rect(
                body.x,
                body.y + 1 + (i - self.list_scroll) as u16,
                body.width,
                1,
            );
            let selected = i == self.list_index;
            let row_style = style(FG).bg(if selected { SEL } else { BG });
            let repo = &c.repos[item.root];
            let (icon, tint) = item
                .file
                .as_ref()
                .map_or((icons.glyph(Icon::Success), GREEN), |file| {
                    icons.file(Path::new(&file.path))
                });
            put(
                f,
                cell(0, r.y),
                if selected {
                    format!(" {current}")
                } else {
                    String::new()
                },
                row_style,
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(icons.repo(repo.submodule), style(repo_tint(item.root))),
                    Span::raw(format!(" {}", repo.name)),
                ]))
                .style(row_style),
                cell(1, r.y),
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(icon, style(tint)),
                    Span::styled(
                        format!(" {}", item.label),
                        style(if item.file.is_some() { CONFLICT } else { GREEN }),
                    ),
                ]))
                .style(row_style),
                cell(2, r.y),
            );
            self.hits.push((r, Hit::File(i)))
        }
        let selected = self.items.get(self.list_index);
        let open = selected.is_some_and(|x| x.file.is_some());
        let ready = selected.is_some_and(|x| x.file.is_none());
        let mut x = bar.x;
        let open_label = format!(
            "Open file [{}]",
            self.shortcut_labels
                .get("Enter")
                .map_or(icons.glyph(Icon::Enter), String::as_str)
        );
        let continue_label = format!("Continue operation [{}]", self.key_label("C"));
        let compact_continue = format!("Continue [{}]", self.key_label("C"));
        self.button(f, bar, &mut x, &open_label, Hit::Open, open, true);
        self.button(
            f,
            bar,
            &mut x,
            if open_label.width() + continue_label.width() + 5 <= usize::from(bar.width) {
                &continue_label
            } else {
                &compact_continue
            },
            Hit::Continue,
            ready,
            true,
        );
    }
    fn editbox(&mut self, f: &mut Frame, r: Rect) {
        if r.width < 4 || r.height < 3 {
            return;
        }
        let boxr = r;
        f.render_widget(Clear, boxr);
        let p = pane(
            format!("Edit Result {} current block", self.icons.glyph(Icon::Dot)),
            self.panes.active,
        );
        let b = p.inner(boxr);
        f.render_widget(p, boxr);
        let e = self.edit.as_mut().unwrap();
        let (row, col) = e.rowcol();
        let shown = b.height.saturating_sub(1).max(1) as usize;
        e.scroll = e.scroll.min(row);
        if row >= e.scroll + shown {
            e.scroll = row + 1 - shown
        }
        let txt = e.chars.iter().collect::<String>();
        let line = txt.split('\n').nth(row).unwrap_or("");
        let off = UnicodeWidthStr::width(line.chars().take(col).collect::<String>().as_str())
            .saturating_sub(b.width.saturating_sub(1) as usize);
        let lines: Vec<_> = txt
            .split('\n')
            .skip(e.scroll)
            .take(shown)
            .enumerate()
            .map(|(j, l)| {
                let l = l.trim_end_matches('\r');
                if j + e.scroll == row {
                    let prefix = l.chars().take(col).collect::<String>();
                    let suffix = l.chars().skip(col + 1).collect::<String>();
                    Line::from(vec![
                        Span::raw(offset_glyphs(&prefix, off)),
                        Span::styled(
                            l.chars().nth(col).unwrap_or(' ').to_string(),
                            style(BG).bg(ACCENT),
                        ),
                        Span::raw(suffix),
                    ])
                } else {
                    Line::from(offset_glyphs(l, off))
                }
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines).style(style(FG).bg(BG)),
            rect(b.x, b.y, b.width, shown as u16),
        );
        self.hits.clear();
        for (j, l) in txt.split('\n').skip(e.scroll).take(shown).enumerate() {
            let mut columns = Vec::new();
            for (i, ch) in l.trim_end_matches('\r').chars().enumerate() {
                for _ in 0..unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) {
                    columns.push(i)
                }
            }
            for x in 0..b.width {
                let ix = columns
                    .get(x as usize + off)
                    .copied()
                    .unwrap_or(l.chars().count());
                self.hits.push((
                    rect(b.x + x, b.y + j as u16, 1, 1),
                    Hit::Cursor(e.scroll + j, ix),
                ))
            }
        }
        let bar = rect(b.x, b.bottom() - 1, b.width, 1);
        let mut x = bar.x;
        self.button(f, bar, &mut x, "Accept", Hit::OkEdit, true, true);
        self.button(f, bar, &mut x, "Cancel", Hit::CancelEdit, true, false);
    }
    fn draw_doc(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.height < 5 {
            return;
        }
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                C::Length(1),
                C::Length(1),
                C::Min(2),
                C::Length(1),
                C::Length(1),
            ])
            .split(area);
        let d = self.doc.as_ref().unwrap();
        let safe = d
            .chunks
            .iter()
            .filter(|b| b.kind == Kind::Safe && !b.done)
            .count();
        let left = d
            .chunks
            .iter()
            .filter(|b| b.kind == Kind::Conflict && !b.done)
            .count();
        let allowed = c.scope.contains(&d.file.root);
        let ready = d.pending() == 0 && allowed;
        let change = d.chunks[self.idx].kind != Kind::Context;
        let conflicted = d.chunks[self.idx].kind == Kind::Conflict;
        let icons = self.icons;
        let compact = area.width < 72;
        let dot = icons.glyph(Icon::Dot);
        let repo = &c.repos[d.file.root];
        let (file_icon, file_color) = icons.file(Path::new(&d.file.path));
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", icons.repo(repo.submodule)),
                    style(repo_tint(d.file.root)),
                ),
                Span::styled(
                    format!("{} / ", repo.name),
                    style(FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(file_icon, style(file_color)),
                Span::styled(
                    format!(" {}   ", d.file.path),
                    style(FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    d.op.as_ref().map(|o| o.name).unwrap_or("resolve"),
                    style(MUTED),
                ),
                Span::styled(
                    format!(
                        "  {} {left} conflict(s) left",
                        icons.glyph(if left == 0 {
                            Icon::Success
                        } else {
                            Icon::Conflict
                        })
                    ),
                    style(if left > 0 { CONFLICT } else { GREEN }),
                ),
                Span::styled(
                    if allowed {
                        ""
                    } else {
                        "   [outside scope: read only]"
                    },
                    style(CONFLICT),
                ),
            ])),
            vert[0],
        );
        let mut x = vert[1].x;
        self.button(
            f,
            vert[1],
            &mut x,
            &format!("{} Files", icons.glyph(Icon::Back)),
            Hit::List,
            true,
            false,
        );
        for (icon, key, dir) in [(Icon::Up, "[c", -1), (Icon::Down, "]c", 1)] {
            self.button(
                f,
                vert[1],
                &mut x,
                &format!(
                    "{}{}",
                    icons.glyph(icon),
                    if compact {
                        String::new()
                    } else {
                        format!(" {}", self.key_label(key))
                    }
                ),
                Hit::Next(dir),
                left > 0,
                false,
            );
        }
        let auto = if compact {
            format!("Safe {safe}")
        } else {
            format!(
                "Apply non-conflicting {dot} {safe} [{}]",
                self.key_label("a")
            )
        };
        self.button(f, vert[1], &mut x, &auto, Hit::Auto, safe > 0, false);
        self.button(
            f,
            vert[1],
            &mut x,
            &if compact {
                "Undo".into()
            } else {
                format!("Undo [{}]", self.key_label("u"))
            },
            Hit::Undo,
            !self.hist.is_empty(),
            false,
        );
        self.button(
            f,
            vert[1],
            &mut x,
            &if compact {
                "Zoom".into()
            } else {
                format!("Zoom [{}]", self.key_label("z"))
            },
            Hit::Zoom,
            true,
            false,
        );
        let d = self.doc.as_ref().unwrap();
        let fields = [
            format!("Current {dot} {}", d.left_label),
            format!("Result {dot} editable"),
            format!("Incoming {dot} {}", d.right_label),
        ];
        let body = vert[2];
        let cols = if self.zoom || compact {
            let mut cols = [Rect::default(); 3];
            cols[self.side] = body;
            cols
        } else {
            let [current, rest] = self.panes.divide(
                SplitId::MergeCurrent,
                body,
                Axis::Width,
                body.width / 3,
                [12, 24],
            );
            let [result, incoming] = self.panes.divide(
                SplitId::MergeIncoming,
                rest,
                Axis::Width,
                rest.width / 2,
                [12, 12],
            );
            [current, result, incoming]
        };
        for (id, col) in Self::SIDES.into_iter().zip(cols) {
            self.panes.add(id, col);
        }
        let height = vert[2].height.saturating_sub(2) as usize;
        self.rows_height = height;
        let mut offsets = vec![];
        let mut count = 0;
        for ch in &d.chunks {
            let n = (0..3)
                .map(|s| linecount(label(ch, s)))
                .max()
                .unwrap_or(0)
                .max(1);
            offsets.push((count, n));
            count += n
        }
        self.rows_total = count;
        if self.follow {
            let (start, n) = offsets[self.idx];
            if start < self.scroll || start >= self.scroll + height {
                self.scroll = start.saturating_sub(2)
            } else if n <= height && start + n > self.scroll + height {
                self.scroll = start + n - height
            }
            self.follow = false
        }
        self.scroll = self.scroll.min(count.saturating_sub(height));
        let marker_width = [Icon::Success, Icon::Conflict, Icon::Add]
            .into_iter()
            .map(|i| icons.glyph(i).width())
            .max()
            .unwrap_or(0);
        // Each source arrow points into the result. Reserve one spacer next to
        // the text; only the cells painting the arrow take the change.
        let take_left = icons.glyph(Icon::Right);
        let take_right = icons.glyph(Icon::Left);
        for (s, col) in cols.into_iter().enumerate().filter(|(_, r)| !r.is_empty()) {
            let block = pane(
                &fields[s],
                self.panes.active && self.edit.is_none() && s == self.side,
            );
            let inner = block.inner(col);
            f.render_widget(block, col);
            let arrow = match s {
                0 => take_left,
                2 => take_right,
                _ => "",
            };
            let arrow_width = arrow.width() as u16;
            let gutter = if arrow_width > 0 {
                (arrow_width + 1).min(inner.width)
            } else {
                0
            };
            let mut lineno = 0;
            for (i, ch) in d.chunks.iter().enumerate() {
                let (start, n) = offsets[i];
                let text = label(ch, s);
                let ln = linecount(text);
                if start >= self.scroll + height {
                    break;
                }
                if start + n <= self.scroll {
                    lineno += ln;
                    continue;
                }
                let rows: Vec<_> = text.split_inclusive('\n').collect();
                for j in 0..n {
                    let abs = start + j;
                    if abs < self.scroll || abs >= self.scroll + height {
                        continue;
                    }
                    let y = inner.y + (abs - self.scroll) as u16;
                    let rectrow = rect(inner.x, y, inner.width, 1);
                    self.hits.push((rectrow, Hit::Select(i, s)));
                    let (bg, mark, mark_color) = match (ch.kind, ch.done) {
                        (Kind::Context, _) => (BG, "", MUTED),
                        (_, true) => (RESOLVED_BG, icons.glyph(Icon::Success), GREEN),
                        (Kind::Safe, false) => (SAFE_BG, icons.glyph(Icon::Add), ACCENT),
                        (Kind::Conflict, false) => {
                            (CONFLICT_BG, icons.glyph(Icon::Conflict), CONFLICT)
                        }
                    };
                    let bg = if i == self.idx && s == self.side {
                        SEL
                    } else {
                        bg
                    };
                    let number = if j < ln {
                        format!("{:>3}", lineno + j + 1)
                    } else {
                        "   ".into()
                    };
                    let mut r = rectrow;
                    if s == 2 {
                        r.x += gutter
                    }
                    r.width -= gutter;
                    let mut content = offset_glyphs(
                        rows.get(j).unwrap_or(&"").trim_end_matches(['\n', '\r']),
                        self.hscroll,
                    );
                    content = content.replace('\t', "    ");
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(mark, style(mark_color)),
                            Span::styled(
                                format!("{}{number} ", " ".repeat(marker_width - mark.width())),
                                style(MUTED),
                            ),
                            Span::raw(content),
                        ]))
                        .style(style(FG).bg(bg)),
                        r,
                    );
                    if arrow_width > 0
                        && arrow_width <= gutter
                        && ch.kind != Kind::Context
                        && j == 0
                    {
                        let g = rect(
                            if s == 0 {
                                inner.right() - arrow_width
                            } else {
                                inner.x
                            },
                            y,
                            arrow_width,
                            1,
                        );
                        put(
                            f,
                            g,
                            arrow,
                            style(ACCENT).bg(bg).add_modifier(Modifier::BOLD),
                        );
                        self.hits.push((g, Hit::Choice(i, s)))
                    }
                }
                lineno += ln;
            }
        }
        let mut x = vert[3].x;
        self.button(
            f,
            vert[3],
            &mut x,
            &format!(
                "{take_left} Left{}",
                if compact {
                    String::new()
                } else {
                    format!(" [{}]", self.key_label("H"))
                }
            ),
            Hit::Choice(self.idx, 0),
            change,
            false,
        );
        self.button(
            f,
            vert[3],
            &mut x,
            &format!(
                "Right {take_right}{}",
                if compact {
                    String::new()
                } else {
                    format!(" [{}]", self.key_label("L"))
                }
            ),
            Hit::Choice(self.idx, 2),
            change,
            false,
        );
        self.button(
            f,
            vert[3],
            &mut x,
            &format!("Both [{}]", self.key_label("b")),
            Hit::Both,
            conflicted,
            false,
        );
        self.button(
            f,
            vert[3],
            &mut x,
            &format!("Edit [{}]", self.key_label("i")),
            Hit::Edit,
            true,
            false,
        );
        let mut x = vert[4].x;
        self.button(
            f,
            vert[4],
            &mut x,
            &format!("Save session draft [{}]", self.key_label("s")),
            Hit::Save,
            true,
            false,
        );
        self.button(
            f,
            vert[4],
            &mut x,
            &format!("Apply file [{}]", self.key_label("A")),
            Hit::Apply,
            ready,
            true,
        );
        if self.edit.is_some() {
            let col = cols[1];
            let h = col.height.saturating_sub(1).min(13);
            let start = offsets[self.idx].0.saturating_sub(self.scroll) as u16;
            let y = col
                .y
                .saturating_add(1 + start)
                .min(col.bottom().saturating_sub(h));
            self.editbox(f, rect(col.x, y, col.width, h))
        }
    }
}
impl Panel for MergePanel {
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.cancel_key_sequence();
        let mut items = vec![];
        let mut errors = vec![];
        for &i in &c.scope {
            if let Some(r) = c.repos.get(i) {
                match conflicts(i, &r.path) {
                    Ok(fs) => {
                        if fs.is_empty() {
                            if let Ok(Some(o)) = operation(&r.path) {
                                items.push(Entry {
                                    root: i,
                                    file: None,
                                    label: format!(
                                        "{} {} no unmerged paths {} review Continue",
                                        o.name,
                                        self.icons.glyph(Icon::Dot),
                                        self.icons.glyph(Icon::Right)
                                    ),
                                })
                            }
                        } else {
                            for file in fs {
                                items.push(Entry {
                                    root: i,
                                    label: file.path.clone(),
                                    file: Some(file),
                                })
                            }
                        }
                    }
                    Err(e) => errors.push(format!("{}: {e}", r.name)),
                }
            }
        }
        self.items = items;
        self.list_index = self.list_index.min(self.items.len().saturating_sub(1));
        self.plan = None;
        if !errors.is_empty() {
            anyhow::bail!("{}", errors.join("; "))
        }
        Ok(())
    }
    fn help(&self) -> &'static str {
        if self.edit.is_some() {
            "Edit Result · arrows/click move · Ctrl+S / Ctrl+Enter accept block · Esc cancel"
        } else if self.doc.is_some() {
            "j/k block · h/l pane · [c ]c conflict · a safe · H/L/b choose · i edit · u undo · A apply · Esc files"
        } else {
            "j/k select file · Enter open · C continue operation · mouse: select then Open, or double-click"
        }
    }
    fn editing(&self) -> bool {
        self.edit.is_some()
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.panes.begin();
        self.hits.clear();
        if self.doc.is_some() {
            self.draw_doc(f, area, c)
        } else {
            self.draw_list(f, area, c)
        }
    }
    fn confirm(&mut self, token: &str, c: &Context) -> Action {
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
                        let msg = format!(
                            "{}: staged resolved {}. Continue is a separate action.",
                            c.repos[d.file.root].name, d.file.path
                        );
                        self.drafts.remove(&(d.file.root, d.file.path.clone()));
                        self.doc = None;
                        self.hist.clear();
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
        if self.editing()
            || matches!(e, Event::Paste(_) | Event::Resize(_, _) | Event::FocusLost)
            || matches!(e, Event::Mouse(m) if m.kind != MouseEventKind::Moved)
        {
            self.cancel_key_sequence();
        }
        if let Event::Mouse(m) = e {
            if !self.editing() && self.panes.mouse(m) {
                return Action::None;
            }
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(id) = self.panes.at(m.column, m.row) {
                        self.focus_pane(id);
                    }
                    if let Some(cmd) = self
                        .hits
                        .iter()
                        .rev()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                        .map(|(_, h)| *h)
                    {
                        return self.act(cmd, c);
                    }
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if self.edit.is_none() => {
                    let up = m.kind == MouseEventKind::ScrollUp;
                    if self.doc.is_some() {
                        self.follow = false;
                        self.scroll = if up {
                            self.scroll.saturating_sub(3)
                        } else {
                            (self.scroll + 3).min(self.rows_total.saturating_sub(self.rows_height))
                        }
                    } else {
                        self.list_index = if up {
                            self.list_index.saturating_sub(2)
                        } else {
                            (self.list_index + 2).min(self.items.len().saturating_sub(1))
                        }
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
            return match k.code {
                K::Char('j') | K::Down => {
                    self.list_index = (self.list_index + 1).min(self.items.len().saturating_sub(1));
                    Action::None
                }
                K::Char('k') | K::Up => {
                    self.list_index = self.list_index.saturating_sub(1);
                    Action::None
                }
                K::Char('g') | K::Home => {
                    self.list_index = 0;
                    Action::None
                }
                K::Char('G') | K::End => {
                    self.list_index = self.items.len().saturating_sub(1);
                    Action::None
                }
                K::Enter
                    if self
                        .items
                        .get(self.list_index)
                        .is_some_and(|e| e.file.is_none()) =>
                {
                    self.act(Hit::Continue, c)
                }
                K::Char('C') => self.act(Hit::Continue, c),
                K::Enter => self.act(Hit::Open, c),
                _ => Action::None,
            };
        }
        let prev = self.prefix.take();
        let n = self.doc.as_ref().unwrap().chunks.len();
        let op = match k.code {
            K::Esc | K::Char('f') => Hit::List,
            K::Char('[' | ']') if !k.modifiers.intersects(M::CONTROL | M::ALT) => {
                if let K::Char(c) = k.code {
                    self.prefix = Some(c)
                }
                return Action::None;
            }
            K::Char('c') if prev.is_some() && !k.modifiers.intersects(M::CONTROL | M::ALT) => {
                Hit::Next(if prev == Some(']') { 1 } else { -1 })
            }
            K::Char('a') => Hit::Auto,
            K::Char('u') => Hit::Undo,
            K::Char('b') => Hit::Both,
            K::Char('H') => Hit::Choice(self.idx, 0),
            K::Char('L') => Hit::Choice(self.idx, 2),
            K::Char('s') => Hit::Save,
            K::Char('A') => Hit::Apply,
            K::Enter if k.modifiers.contains(M::SHIFT) => Hit::Apply,
            K::Char('i') => Hit::Edit,
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
                self.hscroll = self.hscroll.saturating_sub(4);
                return Action::None;
            }
            K::Right => {
                self.hscroll = self.hscroll.saturating_add(4);
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
