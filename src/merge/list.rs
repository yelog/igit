use super::{
    engine::{conflicts, operation, unique_branch, Conflict, FileSide, Operation, Stages},
    put, rect, snap, style, Hit, MergePanel,
};
use crate::{
    core::{Context, Repo},
    ui::{
        icons::Icon,
        idea::{button_style, clipped, clipped_path, GREEN, RED, TAG},
        panes::PaneId,
        Action, NoticeLevel, BG, FG, LINE, MUTED, SEL,
    },
};
use anyhow::Result;
use ratatui::{
    layout::{Constraint as C, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::{collections::BTreeMap, path::Path, time::Instant};
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod tests;

impl FileSide {
    fn status(self, stages: &Stages) -> (&'static str, &'static str, Color) {
        let base = &stages.0[0];
        let side = &stages.0[self.slot()];
        match (base, side) {
            (None, None) => ("Absent", "None", MUTED),
            (None, Some(_)) => ("Added", "Add", GREEN),
            (Some(_), None) => ("Deleted", "Del", RED),
            (a, b) if a == b => ("Unchanged", "Same", MUTED),
            _ => ("Modified", "Mod", FG),
        }
    }
}

enum Entry {
    File(Conflict),
    Ready(usize),
}

impl Entry {
    fn root(&self) -> usize {
        match self {
            Self::File(file) => file.root,
            Self::Ready(root) => *root,
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            Self::File(file) => Some(&file.path),
            Self::Ready(_) => None,
        }
    }
}

struct Sources {
    operation: Option<&'static str>,
    can_skip: bool,
    current: String,
    incoming: String,
}

impl Sources {
    fn new(repo: &Repo, op: Option<&Operation>) -> Self {
        let short = |id: &str| id.chars().take(8).collect::<String>();
        let operation = op.map(|op| op.name);
        let current = match op {
            Some(op) if op.name == "rebase" && !op.head.is_empty() => short(&op.head),
            _ => repo.branch.clone(),
        };
        let incoming = op
            .map(|op| {
                if op.name == "merge" {
                    if let Some(branch) = unique_branch(&repo.path, &op.other) {
                        return branch;
                    }
                }
                short(&op.other)
            })
            .unwrap_or_default();
        Self {
            operation,
            can_skip: op.is_some_and(|op| super::RecoveryAction::Skip.available(op)),
            current,
            incoming,
        }
    }

    fn column(&self, side: FileSide) -> (&'static str, &str) {
        match (self.operation, side) {
            (Some("merge"), FileSide::Current) => ("Yours", &self.current),
            (Some("merge"), FileSide::Incoming) => ("Theirs", &self.incoming),
            (Some("rebase"), FileSide::Current) => ("Rebased", &self.current),
            (Some("rebase"), FileSide::Incoming) => ("Replay", &self.incoming),
            (Some("cherry-pick"), FileSide::Incoming) => ("Picked", &self.incoming),
            (Some("revert"), FileSide::Incoming) => ("Revert result", ""),
            (_, FileSide::Current) => ("Current", &self.current),
            (_, FileSide::Incoming) => ("Incoming", ""),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Unresolved,
    Ready,
    Item(usize),
}

#[derive(Default)]
pub(super) struct ConflictList {
    items: Vec<Entry>,
    sources: BTreeMap<usize, Sources>,
    focus: Option<Row>,
    scroll: usize,
    folded: bool,
    full_paths: bool,
    last_click: Option<(usize, Instant)>,
}

impl ConflictList {
    pub(super) fn refresh(&mut self, ctx: &Context) -> Result<()> {
        let mut files = Vec::new();
        let mut ready = Vec::new();
        let mut sources = BTreeMap::new();
        let mut errors = Vec::new();
        for &root in &ctx.scope {
            let Some(repo) = ctx.repos.get(root) else {
                continue;
            };
            let conflicts = match conflicts(root, &repo.path) {
                Ok(conflicts) => conflicts,
                Err(error) => {
                    errors.push(format!("{}: {error}", repo.name));
                    continue;
                }
            };
            let op = match operation(&repo.path) {
                Ok(op) => op,
                Err(error) => {
                    errors.push(format!("{}: {error}", repo.name));
                    None
                }
            };
            if conflicts.is_empty() {
                if op.is_none() {
                    continue;
                }
                ready.push(Entry::Ready(root));
            }
            files.extend(conflicts.into_iter().map(Entry::File));
            sources.insert(root, Sources::new(repo, op.as_ref()));
        }
        files.extend(ready);
        let index = self
            .selected()
            .and_then(|old| {
                files
                    .iter()
                    .position(|entry| entry.root() == old.root() && entry.path() == old.path())
                    .or_else(|| files.iter().position(|entry| entry.root() == old.root()))
            })
            .unwrap_or_else(|| {
                self.selected_index()
                    .unwrap_or(0)
                    .min(files.len().saturating_sub(1))
            });
        self.focus = match files.get(index) {
            Some(Entry::File(_)) if self.folded || self.focus == Some(Row::Unresolved) => {
                Some(Row::Unresolved)
            }
            Some(_) => Some(Row::Item(index)),
            None => None,
        };
        self.items = files;
        self.sources = sources;
        self.reset_click();
        if !errors.is_empty() {
            anyhow::bail!("{}", errors.join("; "))
        }
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(super) fn files_in(&self, root: usize) -> impl Iterator<Item = &Conflict> {
        self.items.iter().filter_map(move |entry| match entry {
            Entry::File(file) if file.root == root => Some(file),
            _ => None,
        })
    }

    pub(super) fn selected_root(&self) -> Option<usize> {
        self.selected().map(Entry::root)
    }

    pub(super) fn file_at(&self, index: usize) -> Option<&Conflict> {
        match self.items.get(index)? {
            Entry::File(file) if !self.folded => Some(file),
            _ => None,
        }
    }

    pub(super) fn selected_file(&self) -> Option<&Conflict> {
        self.file_at(self.selected_index()?)
    }

    pub(super) fn select_file(&mut self, file: &Conflict) {
        if let Some(index) = self.items.iter().position(|entry| {
            entry.root() == file.root && entry.path() == Some(file.path.as_path())
        }) {
            self.focus = Some(Row::Item(index));
        }
    }

    pub(super) fn side_name(&self, root: usize, side: FileSide) -> &'static str {
        self.sources
            .get(&root)
            .map_or("selected side", |sources| sources.column(side).0)
    }

    pub(super) fn accept(&self, side: FileSide) -> Option<Hit> {
        self.selected_index()
            .map(|index| Hit::AcceptFile(index, side))
    }

    pub(super) fn open(&self) -> Hit {
        match self.focus {
            Some(Row::Unresolved) => Hit::FoldUnresolved,
            _ if self.ready() => Hit::Continue,
            _ => Hit::Open,
        }
    }

    pub(super) fn toggle_paths(&mut self) {
        self.full_paths = !self.full_paths;
        self.reset_click();
    }

    pub(super) fn toggle_unresolved(&mut self) {
        if self.unresolved() > 0 {
            self.folded = !self.folded;
            self.focus = Some(Row::Unresolved);
        }
        self.reset_click();
    }

    pub(super) fn reset_click(&mut self) {
        self.last_click = None;
    }

    pub(super) fn click(&mut self, index: usize) -> Option<Hit> {
        let entry = self.items.get(index)?;
        let open = match entry {
            Entry::File(_) if self.folded => return None,
            Entry::File(_) => Hit::Open,
            Entry::Ready(_) => Hit::Continue,
        };
        self.focus = Some(Row::Item(index));
        let now = Instant::now();
        let previous = self.last_click.replace((index, now));
        previous
            .is_some_and(|(i, at)| i == index && now.duration_since(at).as_millis() < 420)
            .then_some(open)
    }

    pub(super) fn move_by(&mut self, step: isize) {
        let mut rows = self.rows().filter(|row| *row != Row::Ready);
        let i = rows
            .clone()
            .position(|row| Some(row) == self.focus)
            .unwrap_or(0);
        let next = i
            .saturating_add_signed(step)
            .min(rows.clone().count().saturating_sub(1));
        self.focus = rows.nth(next);
        self.reset_click();
    }

    fn selected_index(&self) -> Option<usize> {
        match self.focus {
            Some(Row::Item(index)) => Some(index),
            _ => None,
        }
    }

    fn selected(&self) -> Option<&Entry> {
        self.items.get(self.selected_index()?)
    }

    fn ready(&self) -> bool {
        matches!(self.selected(), Some(Entry::Ready(_)))
    }

    fn unresolved(&self) -> usize {
        self.items
            .partition_point(|entry| matches!(entry, Entry::File(_)))
    }

    fn rows(&self) -> impl Iterator<Item = Row> + Clone {
        let n = self.unresolved();
        (n > 0)
            .then_some(Row::Unresolved)
            .into_iter()
            .chain((0..if self.folded { 0 } else { n }).map(Row::Item))
            .chain((n < self.items.len()).then_some(Row::Ready))
            .chain((n..self.items.len()).map(Row::Item))
    }
}

fn list_control(f: &mut Frame, cell: Rect, label: &str, on: bool, primary: bool) {
    put(
        f,
        cell,
        label,
        if on {
            button_style(primary)
        } else {
            button_style(false).fg(MUTED).add_modifier(Modifier::DIM)
        },
    );
}

impl MergePanel {
    pub(super) fn preview_side(&mut self, c: &Context, index: usize, side: FileSide) -> Action {
        let Some(file) = self.list.file_at(index).cloned() else {
            return Action::None;
        };
        let name = self.list.side_name(file.root, side);
        if let Err(error) = self.choose_file(c, file) {
            return Action::Notice(NoticeLevel::Error, format!("{error:#}"));
        }
        self.act(Hit::UseFile(side), c);
        Action::Notice(
            NoticeLevel::Info,
            format!(
                "Previewing the entire {name} version. {} reviews Apply Changes.",
                self.key_label("A")
            ),
        )
    }
    pub(super) fn prepare_simple(&mut self, c: &Context) -> Action {
        let Some(root) = self.list.selected_root().filter(|r| c.scope.contains(r)) else {
            return Action::None;
        };
        let mut prepared = 0;
        let mut skipped = 0;
        let mut first_error = None;
        for file in self.list.files_in(root) {
            let mut document = match self.open_document(c, file.clone()) {
                Ok(doc) => doc,
                Err(error) => {
                    first_error.get_or_insert_with(|| format!("{}: {error:#}", file.label()));
                    skipped += 1;
                    continue;
                }
            };
            let mut changed = false;
            for chunk in &mut document.chunks {
                if let Some(result) = chunk.automatic_result() {
                    let result = result.into_owned();
                    chunk.resolve(result);
                    changed = true;
                }
            }
            if changed {
                self.drafts.insert(
                    (root, file.path.clone()),
                    (
                        file.stages.clone(),
                        snap(&document, 0..document.chunks.len()),
                    ),
                );
                prepared += 1;
            }
        }
        let name = &c.repos[root].name;
        if prepared == 0 {
            return match first_error {
                Some(error) => Action::Notice(
                    NoticeLevel::Error,
                    format!("{name}: no new simple drafts · {skipped} skipped · {error}"),
                ),
                None => Action::Notice(
                    NoticeLevel::Info,
                    format!("{name}: no new non-conflicting changes to prepare"),
                ),
            };
        }
        let noun = if prepared == 1 { "draft" } else { "drafts" };
        Action::Notice(
            if skipped > 0 {
                NoticeLevel::Warning
            } else {
                NoticeLevel::Success
            },
            format!(
                "{name}: prepared {prepared} {noun}, {skipped} skipped. Open a file to review."
            ),
        )
    }
    pub(super) fn draw_list(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.panes.add(PaneId::Conflicts, area);
        let [tools, table, footer] =
            Layout::vertical([C::Length(1), C::Min(3), C::Length(2)]).areas(area);
        let root = self.list.selected_root();

        let can_prepare = root.is_some_and(|r| {
            self.list
                .files_in(r)
                .any(|file| file.stage_error().is_none())
        });
        let paths = if tools.width >= 65 {
            format!(" {} Full Paths ", self.icons.glyph(Icon::Preview))
        } else {
            format!(" {} Paths ", self.icons.glyph(Icon::Preview))
        };
        let paths_width = (paths.width() as u16).min(tools.width);
        let paths_cell = rect(tools.right() - paths_width, tools.y, paths_width, 1);
        list_control(f, paths_cell, &paths, true, self.list.full_paths);
        self.hits.push((paths_cell, Hit::TogglePaths));
        let shortcut = self.key_label("S");
        let simple = if tools.width >= 65 {
            format!("Prepare Simple Drafts [{shortcut}]")
        } else {
            format!("Prepare Simple [{shortcut}]")
        };
        let left = tools.width - paths_width;
        if left >= 4 {
            let simple = format!(" {} ", clipped(&simple, usize::from(left - 3)));
            let cell = rect(tools.x + 1, tools.y, simple.width() as u16, 1);
            list_control(f, cell, &simple, can_prepare, false);
            if can_prepare {
                self.hits.push((cell, Hit::PrepareSimple));
            }
        }
        self.draw_conflict_table(f, table, c);
        self.draw_list_footer(f, footer);
    }

    fn draw_conflict_table(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let border = Block::default()
            .borders(Borders::ALL)
            .border_style(style(LINE));
        let body = border.inner(area);
        f.render_widget(border, area);
        if body.width < 30 || body.height == 0 {
            return;
        }
        let side_width = if body.width >= 70 {
            (body.width / 5).clamp(19, 33)
        } else {
            10
        };
        let file_width = body.width - 2 * side_width;
        let columns = |y| {
            [
                rect(body.x, y, file_width, 1),
                rect(body.x + file_width, y, side_width, 1),
                rect(body.x + file_width + side_width, y, side_width, 1),
            ]
        };
        let header = columns(body.y);
        f.buffer_mut()
            .set_style(rect(body.x, body.y, body.width, 1), style(MUTED).bg(TAG));
        put(f, header[0], " File", style(MUTED));
        for (cell, side) in header[1..].iter().zip(FileSide::ALL) {
            let (label, source) = match self.list.sources.values().next() {
                Some(s) if self.list.sources.len() == 1 => s.column(side),
                _ => match side {
                    FileSide::Current => ("Current", ""),
                    FileSide::Incoming => ("Incoming", ""),
                },
            };
            let w = usize::from(cell.width).saturating_sub(label.width() + 5);
            let label = if !source.is_empty() && w >= 4 {
                format!(" {label} ({})", clipped(source, w))
            } else {
                format!(" {label}")
            };
            side_cell(f, *cell, label, MUTED);
        }

        let count = usize::from(body.height.saturating_sub(1));
        if self.list.is_empty() && count > 0 {
            put(
                f,
                rect(body.x, body.y + 1, body.width, 1),
                " No conflicts in Roots",
                style(MUTED),
            );
            return;
        }
        let rows = self.list.rows();
        let focus = self.list.focus;
        let selected = rows.clone().position(|r| Some(r) == focus).unwrap_or(0);
        self.list.scroll = self
            .list
            .scroll
            .min(selected)
            .min(rows.clone().count().saturating_sub(count));
        if count > 0 && selected >= self.list.scroll + count {
            self.list.scroll = selected + 1 - count;
        }
        for (y, row) in rows.skip(self.list.scroll).take(count).enumerate() {
            let y = body.y + y as u16 + 1;
            let line = rect(body.x, y, body.width, 1);
            let selected = Some(row) == focus;
            let bg = if selected {
                SEL
            } else if row == Row::Ready {
                TAG
            } else {
                BG
            };
            f.buffer_mut().set_style(line, Style::new().bg(bg));
            match row {
                Row::Unresolved => {
                    let n = self.list.unresolved();
                    let fold = if self.list.folded {
                        Icon::Collapsed
                    } else {
                        Icon::Expanded
                    };
                    let icons = self.icons;
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(format!(" {} ", icons.glyph(fold)), style(MUTED)),
                            Span::styled(icons.glyph(Icon::Error), style(RED)),
                            Span::styled(" Unresolved", style(FG).add_modifier(Modifier::BOLD)),
                            Span::styled(
                                format!("  {n} {}", if n == 1 { "file" } else { "files" }),
                                style(MUTED),
                            ),
                        ])),
                        line,
                    );
                    self.hits.push((line, Hit::FoldUnresolved));
                }
                Row::Ready => {
                    let n = self.list.items.len() - self.list.unresolved();
                    put(
                        f,
                        line,
                        format!(
                            " {} Ready to Continue  {n}",
                            self.icons.glyph(Icon::Success)
                        ),
                        style(GREEN),
                    );
                }
                Row::Item(i) => {
                    self.hits.push((line, Hit::File(i)));
                    match &self.list.items[i] {
                        Entry::File(file) => {
                            let [name, current, incoming] = columns(y);
                            self.draw_conflict_name(f, name, file, &c.repos[file.root]);
                            let accept = selected && file.stage_error().is_none();
                            for (col, side) in
                                [(current, FileSide::Current), (incoming, FileSide::Incoming)]
                            {
                                if let Some(button) = draw_side(
                                    f,
                                    col,
                                    if file.path.to_str().is_none() {
                                        ("External", "Ext", RED)
                                    } else {
                                        side.status(&file.stages)
                                    },
                                    accept,
                                ) {
                                    self.hits.push((button, Hit::AcceptFile(i, side)));
                                }
                            }
                        }
                        Entry::Ready(root) => {
                            let name = &c.repos[*root].name;
                            let op = self.list.sources[root].operation.unwrap_or("operation");
                            put(f, line, format!("   {name}  ·  {op}"), style(FG));
                        }
                    }
                }
            }
        }
    }

    fn draw_conflict_name(&self, f: &mut Frame, area: Rect, file: &Conflict, repo: &Repo) {
        let (icon, tint) = self.icons.file(&file.path);
        let label = file.label();
        let (parent, basename) = label.rsplit_once('/').unwrap_or(("", &label));
        let width = usize::from(area.width).saturating_sub(icon.width() + 4);
        let name = if self.list.full_paths {
            clipped_path(label.clone(), width)
        } else {
            clipped(basename, width)
        };
        let hint = if self.list.full_paths || parent.is_empty() {
            format!("  {}", repo.name)
        } else {
            format!("  {} · {parent}", repo.name)
        };
        let mut spans = vec![
            Span::raw("   "),
            Span::styled(icon, style(tint)),
            Span::styled(format!(" {name}"), style(FG)),
        ];
        let mut left = width.saturating_sub(name.width());
        if file.path.to_str().is_none() && left >= 11 {
            spans.push(Span::styled("  External", style(RED)));
            left -= 10;
        }
        if left >= 7
            && self
                .drafts
                .get(&(file.root, file.path.clone()))
                .is_some_and(|(stages, _)| stages == &file.stages)
        {
            spans.push(Span::styled("  Draft", style(GREEN)));
            left -= 7;
        }
        spans.push(Span::styled(clipped(&hint, left), style(MUTED)));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_list_footer(&mut self, f: &mut Frame, bar: Rect) {
        let ready = self.list.ready();
        let manual = self.list.selected_file().is_some();
        let sources = self
            .list
            .selected_root()
            .and_then(|root| self.list.sources.get(&root));
        let abort = sources.is_some_and(|s| s.operation.is_some());
        let skip = sources.is_some_and(|s| s.can_skip);
        let operations = rect(bar.x, bar.y, bar.width, u16::from(bar.height > 0));
        let compact = bar.width < 64;
        let labels = [("Continue", "C"), ("Abort", "B"), ("Skip", "N")].map(|(label, key)| {
            if compact {
                format!(" {label} ")
            } else {
                format!(" {label} [{}] ", self.key_label(key))
            }
        });
        let mut x = operations.x;
        for ((hit, on), label) in [
            (Hit::Continue, ready),
            (Hit::Abort, abort),
            (Hit::Skip, skip),
        ]
        .into_iter()
        .zip(&labels)
        {
            let w = label.width() as u16;
            if operations.height > 0 && operations.right().saturating_sub(x) >= w {
                let cell = rect(x, operations.y, w, 1);
                list_control(f, cell, label, on, ready && matches!(hit, Hit::Continue));
                if on {
                    self.hits.push((cell, hit));
                }
                x += w + 1;
            }
        }
        if bar.height < 2 {
            return;
        }
        let mut right = bar.right().saturating_sub(1);
        for (label, hit, on, primary) in [
            (" Resolve Manually ", Hit::Open, manual, manual),
            (" Close ", Hit::Close, true, false),
        ] {
            let w = label.width() as u16;
            if right.saturating_sub(bar.x) < w {
                continue;
            }
            let cell = rect(right - w, bar.y + 1, w, 1);
            list_control(f, cell, label, on, primary);
            if on {
                self.hits.push((cell, hit));
            }
            right = cell.x.saturating_sub(1);
        }
    }
}

fn side_cell(f: &mut Frame, cell: Rect, label: String, color: Color) {
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("│", style(LINE)),
            Span::styled(label, style(color)),
        ])),
        cell,
    );
}

fn draw_side(
    f: &mut Frame,
    col: Rect,
    (long, short, color): (&str, &str, Color),
    accept: bool,
) -> Option<Rect> {
    let compact = col.width < 19;
    if !accept {
        let status = if long.width() + 2 <= usize::from(col.width) {
            long
        } else {
            short
        };
        side_cell(f, col, format!(" {status}"), color);
        return None;
    }
    let label = if compact { " Use " } else { " Accept " };
    let w = label.width() as u16;
    side_cell(
        f,
        rect(col.x, col.y, col.width - w, 1),
        format!(" {}", if compact { short } else { long }),
        color,
    );
    let button = rect(col.right() - w, col.y, w, 1);
    list_control(f, button, label, true, false);
    Some(button)
}
