//! The selected commit's paths and metadata. Tree selection refers to a visible row,
//! while a leaf retains the exact index of the byte path returned by Git.
use super::{
    cells, fg, fold_slot, model, ref_color, repo_tint, Hit, Icon, IconMode, LogPanel, Pane, BLUE,
    DIM, EDGE, GOLD, INK, PANEL, SELECT,
};
use crate::core::{Context, Repo};
use crate::ui::search::Input;
use crate::ui::selection::{TextLayout, Wrap};
use crossterm::event::Event;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use unicode_width::UnicodeWidthStr;

const GREEN: Color = Color::Rgb(123, 196, 145);
const DELETED: Color = Color::Rgb(116, 117, 131);

enum Kind {
    Repository,
    Module,
    Summary,
    Parent(usize),
    Directory { label: String, raw: Vec<u8> },
    File(usize),
}

impl Kind {
    fn label<'a>(&'a self, details: &model::Details, repo: &Repo) -> Cow<'a, str> {
        match self {
            Self::File(i) => {
                let raw = &details.files[*i].raw;
                Cow::Owned(model::plain(
                    raw.rsplit(|&b| b == b'/').next().unwrap_or_default(),
                ))
            }
            Self::Parent(i) => {
                let p = &details.parents[*i];
                Cow::Owned(format!(
                    "Changes to {} {}",
                    &p.oid[..8.min(p.oid.len())],
                    p.subject
                ))
            }
            Self::Repository => Cow::Owned(model::plain(repo.name.as_bytes())),
            Self::Module => Cow::Owned(model::plain(
                repo.path
                    .file_name()
                    .map_or(repo.name.as_bytes(), |v| v.as_encoded_bytes()),
            )),
            Self::Directory { label, .. } => Cow::Borrowed(label),
            Self::Summary => Cow::Borrowed(""),
        }
    }
}

struct Node {
    kind: Kind,
    depth: usize,
    count: usize,
    end: usize, // Exclusive end of this subtree in preorder.
}

#[derive(Default)]
struct Directory<'a> {
    dirs: BTreeMap<&'a [u8], Directory<'a>>,
    leaves: Vec<usize>,
    count: usize,
}

impl Directory<'_> {
    fn append(self, nodes: &mut Vec<Node>, depth: usize, parent: &[u8]) {
        for (component, mut dir) in self.dirs {
            let mut raw = parent.to_vec();
            if !raw.is_empty() {
                raw.push(b'/');
            }
            raw.extend_from_slice(component);
            let mut label = model::plain(component);
            while dir.leaves.is_empty() && dir.dirs.len() == 1 {
                let (next, child) = dir.dirs.pop_first().unwrap();
                raw.push(b'/');
                raw.extend_from_slice(next);
                label.push('/');
                label.push_str(&model::plain(next));
                dir = child;
            }
            let i = nodes.len();
            nodes.push(Node {
                kind: Kind::Directory {
                    label,
                    raw: raw.clone(),
                },
                depth,
                count: dir.count,
                end: 0,
            });
            dir.append(nodes, depth + 1, &raw);
            nodes[i].end = nodes.len();
        }
        for file in self.leaves {
            nodes.push(Node {
                kind: Kind::File(file),
                depth,
                count: 1,
                end: nodes.len() + 1,
            });
        }
    }
}

#[derive(Default)]
pub(super) struct Inspector {
    nodes: Vec<Node>,
    visible: Vec<usize>,
    folded: BTreeSet<usize>,
    cursor: usize,
    file_offset: usize,
    info_pos: usize,
    info_max: usize,
}

impl Inspector {
    pub(super) fn new(details: &model::Details) -> Self {
        if !details.parents.is_empty() {
            return Self::merge(details);
        }
        let files = &details.files;
        let mut tree = Directory::default();
        for (i, file) in files.iter().enumerate() {
            let mut dir = &mut tree;
            if let Some(slash) = file.raw.iter().rposition(|&b| b == b'/') {
                for part in file.raw[..slash].split(|&b| b == b'/') {
                    dir = dir.dirs.entry(part).or_default();
                    dir.count += 1;
                }
            }
            dir.leaves.push(i);
        }
        let mut nodes = vec![Node {
            kind: Kind::Repository,
            depth: 0,
            count: files.len(),
            end: 1,
        }];
        if !files.is_empty() {
            nodes.push(Node {
                kind: Kind::Module,
                depth: 1,
                count: files.len(),
                end: 0,
            });
            tree.append(&mut nodes, 2, b"");
            nodes[1].end = nodes.len();
            nodes[0].end = nodes.len();
        }
        let cursor = nodes
            .iter()
            .position(|n| matches!(n.kind, Kind::File(_)))
            .unwrap_or(0);
        Self {
            visible: (0..nodes.len()).collect(),
            nodes,
            cursor,
            ..Self::default()
        }
    }

    fn merge(details: &model::Details) -> Self {
        let mut nodes = Vec::new();
        let combined = 0..details.parents[0].files.start;
        let groups = std::iter::once((None, combined)).chain(
            details
                .parents
                .iter()
                .enumerate()
                .map(|(i, p)| (Some(i), p.files.clone())),
        );
        for (parent, files) in groups {
            let start = nodes.len();
            if let Some(i) = parent {
                nodes.push(Node {
                    kind: Kind::Parent(i),
                    depth: 0,
                    count: files.len(),
                    end: 0,
                });
            }
            let depth = usize::from(parent.is_some());
            let summary = nodes.len();
            nodes.push(Node {
                kind: Kind::Summary,
                depth,
                count: files.len(),
                end: 0,
            });
            let mut tree = Directory::default();
            for i in files {
                let mut dir = &mut tree;
                if let Some(slash) = details.files[i].raw.iter().rposition(|&b| b == b'/') {
                    for part in details.files[i].raw[..slash].split(|&b| b == b'/') {
                        dir = dir.dirs.entry(part).or_default();
                        dir.count += 1;
                    }
                }
                dir.leaves.push(i);
            }
            tree.append(&mut nodes, depth + 1, b"");
            nodes[summary].end = nodes.len();
            nodes[start].end = nodes.len();
        }
        Self {
            cursor: nodes
                .iter()
                .position(|n| matches!(n.kind, Kind::File(_)))
                .unwrap_or(0),
            visible: (0..nodes.len()).collect(),
            nodes,
            ..Self::default()
        }
    }

    pub(super) fn copy_target(
        &self,
        details: &model::Details,
        repo: &Repo,
    ) -> Option<crate::ui::path_copy::Target> {
        let (raw, directory) = match &self.nodes.get(*self.visible.get(self.cursor)?)?.kind {
            Kind::File(i) => (details.files.get(*i)?.raw.as_slice(), false),
            Kind::Directory { raw, .. } => (raw.as_slice(), true),
            Kind::Repository | Kind::Module => (&[][..], true),
            Kind::Summary | Kind::Parent(_) => return None,
        };
        Some(crate::ui::path_copy::Target::new(
            &repo.path,
            model::path_from_bytes(raw).ok()?,
            directory,
        ))
    }

    fn reflow(&mut self, keep: usize) {
        self.visible.clear();
        let mut i = 0;
        while i < self.nodes.len() {
            self.visible.push(i);
            i = if self.folded.contains(&i) {
                self.nodes[i].end
            } else {
                i + 1
            };
        }
        // If a toolbar action hid the old cursor, land on its folded ancestor.
        self.cursor = self
            .visible
            .partition_point(|&n| n <= keep)
            .saturating_sub(1);
    }
}

fn files_count(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

// Truncate at terminal graphemes; retaining the end keeps the innermost folder or extension.
fn fit(text: &str, width: usize, keep_end: bool) -> String {
    if text.width() <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let span = Span::raw(text);
    let graphemes: Vec<_> = span.styled_graphemes(Style::default()).collect();
    let mut used = 1;
    if keep_end {
        let mut start = graphemes.len();
        while start > 0 && used + graphemes[start - 1].symbol.width() <= width {
            start -= 1;
            used += graphemes[start].symbol.width();
        }
        std::iter::once("…")
            .chain(graphemes[start..].iter().map(|g| g.symbol))
            .collect()
    } else {
        let mut result = String::new();
        for g in graphemes {
            used += g.symbol.width();
            if used > width {
                break;
            }
            result.push_str(g.symbol);
        }
        result.push('…');
        result
    }
}

impl LogPanel {
    pub(super) fn file_search_event(&mut self, event: &Event, c: &Context) -> bool {
        let direction = match self.file_search.event(event) {
            Input::Pass => return false,
            Input::Handled => return true,
            Input::Jump(direction) => direction,
        };
        let repo = self.rev().and_then(|rev| c.repos.get(rev.root));
        let tree = &mut self.inspector;
        let hit = self.file_search.jump(
            direction,
            tree.visible.get(tree.cursor),
            repo.into_iter().flat_map(|repo| {
                tree.nodes
                    .iter()
                    .enumerate()
                    .map(|(id, node)| (id, [node.kind.label(&self.details, repo)]))
            }),
        );
        if let Some(hit) = hit {
            tree.folded
                .retain(|&id| !(id < hit.row && hit.row < tree.nodes[id].end));
            tree.reflow(hit.row);
        }
        true
    }

    pub(super) fn file_cursor(&self) -> usize {
        self.inspector.cursor
    }

    fn leaf_at(&self, n: usize) -> Option<usize> {
        match self
            .inspector
            .nodes
            .get(*self.inspector.visible.get(n)?)?
            .kind
        {
            Kind::File(i) => Some(i),
            _ => None,
        }
    }

    pub(super) fn file_is_leaf(&self, n: usize) -> bool {
        self.leaf_at(n).is_some()
    }

    pub(super) fn file_index(&self) -> Option<usize> {
        self.leaf_at(self.file_cursor())
    }

    pub(super) fn file_move(&mut self, d: isize) {
        let s = &mut self.inspector;
        s.cursor = s
            .cursor
            .saturating_add_signed(d)
            .min(s.visible.len().saturating_sub(1));
    }

    pub(super) fn select_file(&mut self, index: usize) {
        if let Some(row) = self.inspector.visible.iter().position(
            |&node| matches!(self.inspector.nodes[node].kind, Kind::File(file) if file == index),
        ) {
            self.inspector.cursor = row;
        }
    }

    pub(super) fn file_activate(&mut self, n: usize, toggle: bool) {
        let s = &mut self.inspector;
        let Some(&id) = s.visible.get(n) else { return };
        s.cursor = n;
        if toggle && s.nodes[id].end > id + 1 {
            if !s.folded.remove(&id) {
                s.folded.insert(id);
            }
            s.reflow(id);
        }
    }

    pub(super) fn file_horizontal(&mut self, left: bool) -> bool {
        let s = &self.inspector;
        let Some(&id) = s.visible.get(s.cursor) else {
            return false;
        };
        if s.nodes[id].end <= id + 1 || s.folded.contains(&id) == left {
            return false;
        }
        self.file_activate(s.cursor, true);
        true
    }

    pub(super) fn file_fold_all(&mut self, collapse: bool) {
        let s = &mut self.inspector;
        let Some(&keep) = s.visible.get(s.cursor) else {
            return;
        };
        s.folded.clear();
        if collapse {
            s.folded.extend(
                s.nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, n)| n.end > i + 1)
                    .map(|(i, _)| i),
            );
        }
        s.reflow(keep);
    }

    pub(super) fn scroll_info(&mut self, d: isize) {
        let s = &mut self.inspector;
        s.info_pos = s.info_pos.saturating_add_signed(d).min(s.info_max);
    }

    fn inspector_button(
        &mut self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        hit: Hit,
        enabled: bool,
    ) {
        if area.is_empty() {
            return;
        }
        f.render_widget(
            Paragraph::new(label).style(fg(if enabled { INK } else { EDGE })),
            area,
        );
        if enabled {
            self.hit(area, hit);
        }
    }

    pub(super) fn draw_files(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            return;
        }
        f.render_widget(Block::default().style(fg(INK).bg(PANEL)), area);
        let Some(root) = self.rev().map(|r| r.root) else {
            return;
        };
        let Some(repo) = c.repos.get(root) else {
            return;
        };
        let bar = Rect::new(area.x, area.y, area.width, 1);
        let diff = format!(" {} Diff ", self.icons.glyph(Icon::Diff));
        let expand = format!(" {} ", self.icons.glyph(Icon::ExpandAll));
        let collapse = format!(" {} ", self.icons.glyph(Icon::CollapseAll));
        let controls = cells(&expand) + cells(&collapse);
        let diff_width = cells(&diff).min(bar.width);
        self.inspector_button(
            f,
            Rect::new(bar.x, bar.y, diff_width, 1),
            &diff,
            Hit::Diff,
            self.file_index().is_some(),
        );
        if bar.width >= controls + diff_width {
            let mut x = bar.right() - controls;
            for (label, close) in [(&expand, false), (&collapse, true)] {
                let enabled = if close {
                    self.inspector.visible.iter().any(|&id| {
                        self.inspector.nodes[id].end > id + 1
                            && !self.inspector.folded.contains(&id)
                    })
                } else {
                    !self.inspector.folded.is_empty()
                };
                let w = cells(label);
                self.inspector_button(
                    f,
                    Rect::new(x, bar.y, w, 1),
                    label,
                    Hit::FileFold(close),
                    enabled,
                );
                x += w;
            }
        }
        let h = usize::from(area.height - 1);
        let s = &mut self.inspector;
        // Keep the repository header in view on commit changes, even when the first leaf
        // sits deep in a branching tree. Only scroll for the cursor when the tree is focused.
        s.file_offset = s.file_offset.min(s.visible.len().saturating_sub(h));
        if self.focus == Pane::Files && h > 0 {
            s.file_offset = s
                .file_offset
                .min(s.cursor)
                .max((s.cursor + 1).saturating_sub(h));
        }
        let start = s.file_offset;
        for n in start..(start + h).min(self.inspector.visible.len()) {
            let id = self.inspector.visible[n];
            let row = Rect::new(area.x, area.y + 1 + (n - start) as u16, area.width, 1);
            let line = self.file_line(id, c, repo, root, usize::from(row.width));
            let style = if self.focus == Pane::Files && n == self.file_cursor() {
                fg(INK).bg(SELECT)
            } else {
                fg(INK)
            };
            f.render_widget(Paragraph::new(line).style(style), row);
            self.hit(row, Hit::File(n));
        }
        if self.details.files.is_empty() && self.details.parents.is_empty() && area.height >= 3 {
            f.render_widget(
                Paragraph::new(self.preview.message().unwrap_or("   No changed files"))
                    .style(fg(DIM)),
                Rect::new(area.x, area.y + 2, area.width, 1),
            );
        }
    }

    fn file_line(
        &self,
        id: usize,
        c: &Context,
        repo: &Repo,
        root: usize,
        width: usize,
    ) -> Line<'static> {
        let s = &self.inspector;
        let node = &s.nodes[id];
        let label = node.kind.label(&self.details, repo);
        let highlight = self.file_search.field(&id, 0);
        let indent = (1 + node.depth * 2).min(width.saturating_sub(13));
        let mut line = Line::from(" ".repeat(indent));
        if let Kind::File(i) = node.kind {
            let file = &self.details.files[i];
            let path = model::path_from_bytes(&file.raw).unwrap_or_else(|_| Path::new(""));
            let (icon, tint) = self.icons.file(path);
            let color = match file.status.as_bytes().first() {
                Some(b'A' | b'C') => GREEN,
                Some(b'D') => DELETED,
                Some(b'R' | b'T') => GOLD,
                _ => BLUE,
            };
            let slot = fold_slot(self.icons, None);
            let room = width.saturating_sub(line.width());
            if room >= slot.width() + icon.width() + 5 {
                line.push_span(slot);
            }
            if room >= icon.width() + 3 {
                line.push_span(Span::styled(format!("{icon} "), fg(tint)));
            }
            let space = width.saturating_sub(line.width());
            line.spans.extend(
                highlight.clipped_with(&label, space, fg(color), |text, width| {
                    fit(text, width, true)
                }),
            );
            return line;
        }
        let open = !s.folded.contains(&id) && node.end > id + 1;
        let slot = fold_slot(self.icons, (node.end > id + 1).then_some(open));
        if width >= line.width() + slot.width() + 3 {
            line.push_span(Span::styled(slot, fg(DIM)));
        }
        if let Kind::Parent(_) = node.kind {
            let room = width.saturating_sub(line.width());
            line.spans.extend(
                highlight
                    .clipped_with(&label, room, fg(INK), |text, width| fit(text, width, false)),
            );
            return line;
        }
        let (icon, tint) = match &node.kind {
            Kind::Repository => (self.icons.repo(repo.submodule), repo_tint(root)),
            Kind::Module => (self.icons.folder(open), BLUE),
            Kind::Directory { .. } | Kind::Summary => (self.icons.folder(open), DIM),
            Kind::File(_) | Kind::Parent(_) => unreachable!(),
        };
        let min_label = label.width().min(4);
        if width >= line.width() + icon.width() + 1 + min_label {
            line.push_span(Span::styled(format!("{icon} "), fg(tint)));
        }
        let count = format!(" {}", files_count(node.count));
        let count = if width >= line.width() + count.width() + min_label {
            count.as_str()
        } else {
            ""
        };
        let room = width.saturating_sub(line.width() + count.width());
        line.spans.extend(
            highlight.clipped_with(&label, room, fg(INK), |text, width| fit(text, width, true)),
        );
        line.push_span(Span::styled(count.to_owned(), fg(DIM)));
        if matches!(node.kind, Kind::Module) {
            let relative = repo
                .path
                .strip_prefix(&c.workspace)
                .ok()
                .filter(|p| !p.as_os_str().is_empty());
            let path = relative.map(|p| model::plain(p.as_os_str().as_encoded_bytes()));
            let annotation = path.as_deref().unwrap_or(&label);
            if line.width() + 2 + annotation.width() <= width {
                line.push_span(Span::styled(format!("  {annotation}"), fg(DIM)));
            }
        }
        line
    }
}

struct InfoLines {
    width: usize,
    layout: TextLayout,
    links: Vec<(usize, usize, usize, Hit)>,
}

impl InfoLines {
    fn push<'a>(&mut self, text: impl Into<Line<'a>>) {
        self.add(text.into(), None);
    }

    // Pass the first cell of the link on its logical line. Wrapping and mouse hits use the
    // same physical rows, including after scrolling, continuation lines and wide characters.
    fn add(&mut self, line: Line<'_>, link: Option<(usize, Hit)>) {
        let rows = self.layout.push(line, self.width as u16, Wrap::Word);
        if let Some((start, hit)) = link {
            for i in rows {
                let row = &self.layout.rows[i];
                let left = start.saturating_sub(row.column);
                if left < row.line.width() {
                    self.links
                        .push((i, left, row.line.width() - left, hit.clone()));
                }
            }
        }
    }

    fn branches(&mut self, branches: &[(model::RefKind, String)], icons: IconMode) {
        if branches.is_empty() {
            self.push(Line::styled("In no branches", fg(DIM)));
            return;
        }
        let n = branches.len();
        self.push(Line::styled(
            format!("In {n} branch{}:", if n == 1 { "" } else { "es" }),
            fg(INK),
        ));
        let marker = icons.glyph(Icon::Tag);
        for (kind, branch) in branches {
            self.push(Line::from(vec![
                Span::styled(format!("{marker} "), fg(ref_color(*kind))),
                Span::styled(branch.as_str(), fg(INK)),
            ]));
        }
    }
}

fn info_lines<'a>(
    panel: &'a LogPanel,
    rev: &'a model::Rev,
    submodule: bool,
    width: usize,
    roomy: bool,
) -> InfoLines {
    let detail = &panel.details;
    let mut out = InfoLines {
        width,
        layout: TextLayout::default(),
        links: vec![],
    };
    out.push(Line::styled(
        rev.subject.as_str(),
        fg(INK).add_modifier(Modifier::BOLD),
    ));
    if roomy {
        out.layout.decoration(Line::raw(""));
    }
    let mut identity = Line::from(vec![
        Span::styled(
            format!("{} ", panel.icons.repo(submodule)),
            fg(repo_tint(rev.root)),
        ),
        Span::styled(
            format!("{} {}", rev.oid.get(..8).unwrap_or(&rev.oid), rev.author),
            fg(INK),
        ),
    ]);
    if !detail.email.is_empty() {
        identity.push_span(Span::styled(format!(" <{}>", detail.email), fg(BLUE)));
    }
    if !rev.date.is_empty() {
        let on = if let Some((date, time)) = rev.date.split_once(", ") {
            format!(" on {date} at {time}")
        } else {
            format!(" on {}", rev.date)
        };
        identity.push_span(Span::styled(on, fg(DIM)));
    }
    out.push(identity);
    if let Some(message) = panel.preview.message() {
        out.push(Line::styled(message, fg(DIM)));
    } else {
        out.branches(&detail.branches, panel.icons);
    }
    if !detail.body.is_empty() {
        out.push("");
        for line in detail.body.lines() {
            out.push(Line::styled(line, fg(INK)));
        }
    }
    if !rev.parents.is_empty() {
        out.push("");
        for (i, p) in rev.parents.iter().enumerate() {
            let up = panel.icons.glyph(Icon::Up);
            let label = if rev.parents.len() == 1 {
                format!("{up} Parent ")
            } else {
                format!("{up} Parent {}  ", i + 1)
            };
            out.add(
                Line::from(vec![
                    Span::styled(label, fg(DIM)),
                    Span::styled(p.get(..8).unwrap_or(p), fg(BLUE)),
                ]),
                Some((0, Hit::Parent(i))),
            );
        }
    }
    out
}

impl LogPanel {
    pub(super) fn draw_info(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            self.info_selection.reset();
            return;
        }
        f.render_widget(Block::default().style(fg(INK).bg(PANEL)), area);
        let Some(rev) = self.rev() else {
            self.info_selection.reset();
            return;
        };
        let pad = u16::from(area.width >= 12);
        let width = area.width - pad * 2;
        let info = info_lines(
            self,
            rev,
            c.repos.get(rev.root).is_some_and(|r| r.submodule),
            usize::from(width),
            area.height >= 9,
        );
        let max = info
            .layout
            .rows
            .len()
            .saturating_sub(usize::from(area.height));
        let offset = self.inspector.info_pos.min(max);
        let mut links = Vec::new();
        for (row, start, len, hit) in info.links {
            if row >= offset && row - offset < usize::from(area.height) {
                links.push((
                    Rect::new(
                        area.x + pad + start as u16,
                        area.y + (row - offset) as u16,
                        len as u16,
                        1,
                    ),
                    hit,
                ));
            }
        }
        self.info_selection.render(
            f,
            Rect::new(area.x + pad, area.y, width, area.height),
            info.layout,
            offset,
            0,
        );
        self.hits.extend(links);
        self.inspector.info_pos = offset;
        self.inspector.info_max = max;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn fixture(paths: &[&[u8]]) -> (LogPanel, Context) {
        let c = Context {
            workspace: "/workspace".into(),
            state_dir: "/state".into(),
            scope: [0].into(),
            repos: vec![Repo {
                path: "/workspace/app".into(),
                name: "app".into(),
                branch: "main".into(),
                parent: None,
                submodule: false,
            }],
        };
        let mut p = LogPanel::default();
        p.hist.revs.push(model::Rev {
            root: 0,
            oid: "c321f1234567".into(),
            parents: vec!["aaaaaaaa0123".into(), "bbbbbbbb0123".into()],
            ts: 0,
            date: "2026/1/2, 13:45".into(),
            author: "Lee Park".into(),
            subject: "Fix UI rendering: wrap, scroll, and preserve links".into(),
            refs: String::new(),
            ..Default::default()
        });
        p.visible.push(0);
        p.details = model::Details {
            files: paths
                .iter()
                .map(|raw| model::File {
                    raw: raw.to_vec(),
                    status: "M".into(),
                    parent: None,
                })
                .collect(),
            parents: Vec::new(),
            body: "The extended description of this revision.\nSecond line with 🌱 and ＵＩ".into(),
            email: "lp@example.invalid".into(),
            branches: [
                (model::RefKind::Head, "HEAD"),
                (model::RefKind::Current, "main"),
                (model::RefKind::Remote, "origin/main"),
                (model::RefKind::Local, "release/older"),
                (model::RefKind::Local, "feature/ｕｓｅｒ"),
            ]
            .map(|(kind, name)| (kind, name.to_owned()))
            .into(),
        };
        p.inspector = Inspector::new(&p.details);
        (p, c)
    }

    #[test]
    fn pane_search_log_diff_selects_the_exact_path_even_with_duplicate_names() {
        use crate::ui::Panel;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let paths: &[&[u8]] = &[b"a/same.txt", b"b/same.txt", b"c/other.txt"];
        let (mut p, c) = fixture(paths);
        p.focus = Pane::Files;
        p.file_fold_all(true);
        let press = |p: &mut LogPanel, code| {
            p.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), &c);
        };
        press(&mut p, KeyCode::Char('/'));
        p.event(&Event::Paste("same.txt".into()), &c);
        press(&mut p, KeyCode::Enter);
        assert_eq!(p.file_index(), Some(0));
        press(&mut p, KeyCode::Char('n'));
        assert_eq!(p.file_index(), Some(1));
        assert_eq!(p.details.files[p.file_index().unwrap()].raw, paths[1]);
        press(&mut p, KeyCode::Char('N'));
        assert_eq!(p.file_index(), Some(0));
        assert!(p.diff.is_none() && p.filter.query.is_empty());
        assert!(!p
            .inspector
            .visible
            .iter()
            .any(|&id| matches!(p.inspector.nodes[id].kind, Kind::File(2))));
    }

    #[test]
    fn compact_dirs_count_descendants_and_fold_without_reusing_an_old_leaf() {
        let paths: &[&[u8]] = &[
            b"src/views/user/index.vue",
            b"src/views/admin/index.vue",
            b"assets/\xff",
            b"assets/\xfe",
        ];
        let (mut p, c) = fixture(paths);
        let s = &p.inspector;
        let src = s
            .nodes
            .iter()
            .position(|n| matches!(&n.kind, Kind::Directory { label, .. } if label == "src/views"))
            .unwrap();
        assert_eq!(s.nodes[src].count, 2);
        assert_eq!(s.nodes[0].count, 4);
        assert_eq!(p.file_index(), Some(2));
        p.file_move(1);
        assert_eq!(
            p.file_index().map(|i| p.details.files[i].raw.as_slice()),
            Some(paths[3])
        );
        let file_id = *p
            .inspector
            .visible
            .iter()
            .find(|&&id| matches!(p.inspector.nodes[id].kind, Kind::File(0)))
            .unwrap();
        let line = p.file_line(file_id, &c, &c.repos[0], 0, 48);
        assert!(line
            .spans
            .iter()
            .any(|s| s.content == "index.vue" && s.style.fg == Some(BLUE)));
        p.file_activate(src, true);
        assert_eq!(p.file_index(), None);
        assert!(!p.inspector.visible.contains(&file_id));
        assert!(p.file_horizontal(false));
        assert!(p.inspector.visible.contains(&file_id));
        p.file_activate(file_id, false);
        assert_eq!(p.file_index(), Some(0));
        p.file_fold_all(true);
        assert_eq!(p.inspector.visible, [0]);
        assert_eq!(p.file_index(), None);
        p.file_fold_all(false);
        assert!(p.inspector.visible.contains(&file_id));
        assert_eq!(p.file_index(), None);
    }

    fn text_at(buf: &Buffer, r: Rect) -> String {
        let mut text = String::new();
        let mut x = r.x;
        while x < r.right() {
            let s = buf[(x, r.y)].symbol();
            text.push_str(s);
            x += s.width().max(1) as u16;
        }
        text
    }

    #[test]
    fn ascii_file_actions_use_every_cell_and_keep_git_path_identity() {
        use crate::ui::icons::IconMode;
        use crossterm::event::MouseButton;

        let raw = b"ui/invalid-\xff.ts";
        let (mut panel, c) = fixture(&[raw]);
        panel.icons = IconMode::Ascii;
        let mut terminal = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let draw = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            p.hits.clear();
            t.draw(|f| p.draw_files(f, f.area(), &c)).unwrap();
        };
        draw(&mut panel, &mut terminal);
        #[cfg(unix)]
        {
            let row = panel
                .hits
                .iter()
                .find(|(_, hit)| matches!(hit, Hit::File(n) if panel.file_is_leaf(*n)))
                .unwrap()
                .0;
            assert!(text_at(terminal.backend().buffer(), row)
                .trim_end()
                .ends_with("TS invalid-�.ts"));
        }
        assert_eq!(panel.details.files[panel.file_index().unwrap()].raw, raw);
        for (close, icon) in [(true, "v^"), (false, "^v")] {
            let hit = panel
                .hits
                .iter()
                .find(|(_, hit)| matches!(hit, Hit::FileFold(v) if *v == close))
                .unwrap()
                .0;
            assert_eq!(text_at(terminal.backend().buffer(), hit).trim(), icon);
            let last_glyph_cell = hit.x + cells(icon);
            panel
                .mouse_down(MouseButton::Left, last_glyph_cell, hit.y, &c)
                .unwrap();
            assert_eq!(panel.inspector.visible.len() == 1, close);
            draw(&mut panel, &mut terminal);
        }
    }

    #[test]
    fn branch_details_always_show_every_tag_with_the_commit_list_colours() {
        use crate::ui::Panel;
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
            for width in [24, 80] {
                for count in [0, 1, 5] {
                    let (mut p, c) = fixture(&[]);
                    p.icons = mode;
                    p.focus = Pane::Info;
                    p.details.branches.truncate(count);
                    let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
                    terminal.draw(|f| p.draw_info(f, f.area(), &c)).unwrap();
                    let lines = (0..36)
                        .map(|y| text_at(terminal.backend().buffer(), Rect::new(0, y, width, 1)))
                        .collect::<Vec<_>>();
                    let text = lines.join("\n");
                    assert!(!text.contains("Show all") && !text.contains("Show less"));
                    let heading = match count {
                        0 => "In no branches",
                        1 => "In 1 branch:",
                        _ => "In 5 branches:",
                    };
                    assert!(text.contains(heading), "{mode:?}, {width} columns");
                    let marker = mode.glyph(Icon::Tag);
                    for (kind, name) in &p.details.branches {
                        let row = format!("{marker} {name}");
                        let y = lines.iter().position(|line| line.trim() == row).unwrap() as u16;
                        let expected = match kind {
                            model::RefKind::Head => GOLD,
                            model::RefKind::Local | model::RefKind::Current => {
                                Color::Rgb(97, 168, 112)
                            }
                            model::RefKind::Remote => Color::Rgb(176, 137, 222),
                            model::RefKind::Tag => unreachable!(),
                        };
                        let buf = terminal.backend().buffer();
                        assert_eq!(buf[(1, y)].symbol(), marker);
                        assert_eq!(buf[(1, y)].fg, expected);
                        assert_eq!(buf[(1 + cells(marker) + 1, y)].fg, INK);
                    }
                    p.event(
                        &Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
                        &c,
                    );
                    terminal.draw(|f| p.draw_info(f, f.area(), &c)).unwrap();
                    let after = (0..36)
                        .map(|y| text_at(terminal.backend().buffer(), Rect::new(0, y, width, 1)))
                        .collect::<Vec<_>>();
                    assert_eq!(after, lines, "Space must not collapse the branches");
                }
            }
        }
    }

    #[test]
    fn wrapped_details_show_all_branches_and_keep_each_scrolled_parent_clickable() {
        let (mut p, c) = fixture(&[]);
        p.icons = crate::ui::icons::IconMode::Unicode;
        let mut terminal = Terminal::new(TestBackend::new(27, 5)).unwrap();
        let draw = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            p.hits.clear();
            t.draw(|f| p.draw_info(f, f.area(), &c)).unwrap();
        };
        draw(&mut p, &mut terminal);
        assert!(text_at(terminal.backend().buffer(), Rect::new(0, 0, 27, 1)).contains("Fix UI"));
        let mut saw_last_branch = false;
        for _ in 0..=p.inspector.info_max {
            for y in 0..5 {
                let line = text_at(terminal.backend().buffer(), Rect::new(0, y, 27, 1));
                assert!(!line.contains("Show all") && !line.contains("Show less"));
            }
            if (0..5).any(|y| {
                text_at(terminal.backend().buffer(), Rect::new(0, y, 27, 1))
                    .contains("feature/ｕｓｅｒ")
            }) {
                saw_last_branch = true;
                break;
            }
            p.scroll_info(1);
            draw(&mut p, &mut terminal);
        }
        assert!(saw_last_branch);
        p.scroll_info(isize::MAX);
        draw(&mut p, &mut terminal);
        let parents = p
            .hits
            .iter()
            .filter_map(|(r, h)| match h {
                Hit::Parent(i) => Some((*i, text_at(terminal.backend().buffer(), *r))),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parents,
            [
                (0, "↑ Parent 1  aaaaaaaa".into()),
                (1, "↑ Parent 2  bbbbbbbb".into())
            ]
        );
    }
}
