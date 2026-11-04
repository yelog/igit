//! The selected commit's paths and metadata. Tree selection refers to a visible row,
//! while a leaf retains the exact index of the byte path returned by Git.
use super::{
    cells, fg, fold_slot, model, repo_tint, Hit, Icon, LogPanel, Pane, BLUE, DIM, EDGE, GOLD, INK,
    PANEL, SELECT,
};
use crate::core::{Context, Repo};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use unicode_width::UnicodeWidthStr;

const GREEN: Color = Color::Rgb(123, 196, 145);
const RED: Color = Color::Rgb(224, 131, 135);

enum Kind {
    Repository,
    Module,
    Directory(String),
    File(usize),
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
    fn append(self, nodes: &mut Vec<Node>, depth: usize) {
        for (component, mut dir) in self.dirs {
            let mut label = model::plain(component);
            while dir.leaves.is_empty() && dir.dirs.len() == 1 {
                let (next, child) = dir.dirs.pop_first().unwrap();
                label.push('/');
                label.push_str(&model::plain(next));
                dir = child;
            }
            let i = nodes.len();
            nodes.push(Node {
                kind: Kind::Directory(label),
                depth,
                count: dir.count,
                end: 0,
            });
            dir.append(nodes, depth + 1);
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
    all_branches: bool,
}

impl Inspector {
    pub(super) fn new(files: &[model::File]) -> Self {
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
            tree.append(&mut nodes, 2);
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

    pub(super) fn toggle_branches(&mut self) {
        self.inspector.all_branches = !self.inspector.all_branches;
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
        let expand = self.icons.glyph(Icon::ExpandAll);
        let collapse = self.icons.glyph(Icon::CollapseAll);
        let full = (
            format!(" {expand} Expand "),
            format!(" {collapse} Collapse "),
        );
        let (expand, collapse) = if cells(&diff) + cells(&full.0) + cells(&full.1) <= bar.width {
            full
        } else {
            (format!(" {expand} "), format!(" {collapse} "))
        };
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
        if self.details.files.is_empty() && area.height >= 3 {
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
        let indent = (1 + node.depth * 2).min(width.saturating_sub(13));
        let mut line = Line::from(" ".repeat(indent));
        if let Kind::File(i) = node.kind {
            let file = &self.details.files[i];
            let name = file.raw.rsplit(|&b| b == b'/').next().unwrap_or_default();
            let path = model::path_from_bytes(&file.raw).unwrap_or_else(|_| Path::new(""));
            let (icon, tint) = self.icons.file(path);
            let color = match file.status.as_bytes().first() {
                Some(b'A' | b'C') => GREEN,
                Some(b'D') => RED,
                Some(b'R' | b'T') => GOLD,
                _ => BLUE,
            };
            let slot = fold_slot(self.icons, None);
            let room = width.saturating_sub(line.width() + file.status.width() + 1);
            if room >= slot.width() + icon.width() + 5 {
                line.push_span(slot);
            }
            if room >= icon.width() + 3 {
                line.push_span(Span::styled(format!("{icon} "), fg(tint)));
            }
            let space = width.saturating_sub(line.width() + file.status.width() + 1);
            line.push_span(Span::styled(
                fit(&model::plain(name), space, true),
                fg(color),
            ));
            line.push_span(Span::styled(format!(" {}", file.status), fg(DIM)));
            return line;
        }
        let open = !s.folded.contains(&id) && node.end > id + 1;
        let slot = fold_slot(self.icons, (node.end > id + 1).then_some(open));
        if width >= line.width() + slot.width() + 3 {
            line.push_span(Span::styled(slot, fg(DIM)));
        }
        let basename = repo
            .path
            .file_name()
            .map(|v| model::plain(v.as_encoded_bytes()));
        let repo_name = model::plain(repo.name.as_bytes());
        let (icon, tint, label) = match &node.kind {
            Kind::Repository => (
                self.icons.repo(repo.submodule),
                repo_tint(root),
                repo_name.as_str(),
            ),
            Kind::Module => (
                self.icons.folder(open),
                BLUE,
                basename.as_deref().unwrap_or(&repo_name),
            ),
            Kind::Directory(label) => (self.icons.folder(open), DIM, label.as_str()),
            Kind::File(_) => unreachable!(),
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
        line.push_span(Span::styled(fit(label, room, true), fg(INK)));
        line.push_span(Span::styled(count.to_owned(), fg(DIM)));
        if matches!(node.kind, Kind::Module) {
            let relative = repo
                .path
                .strip_prefix(&c.workspace)
                .ok()
                .filter(|p| !p.as_os_str().is_empty());
            let path = relative.map(|p| model::plain(p.as_os_str().as_encoded_bytes()));
            let annotation = path.as_deref().unwrap_or(label);
            if line.width() + 2 + annotation.width() <= width {
                line.push_span(Span::styled(format!("  {annotation}"), fg(DIM)));
            }
        }
        line
    }
}

struct InfoRow<'a> {
    text: Line<'a>,
    // The only links are parent hashes and the branch expander; the rest is plain text.
    link: Option<(usize, usize, Hit)>,
}

struct InfoLines<'a> {
    width: usize,
    rows: Vec<InfoRow<'a>>,
}

impl<'a> InfoLines<'a> {
    fn push(&mut self, text: impl Into<Line<'a>>) {
        self.add(text.into(), None);
    }

    // Pass the first cell of the link on its logical line. Wrapping and mouse hits use the
    // same physical rows, including after scrolling, continuation lines and wide characters.
    fn add(&mut self, line: Line<'a>, link: Option<(usize, Hit)>) {
        let width = self.width;
        if width == 0 {
            return;
        }
        let link_part = |offset: usize, len: usize| {
            let (start, hit) = link.as_ref()?;
            let left = start.saturating_sub(offset);
            (left < len).then(|| (left, len - left, hit.clone()))
        };
        if line.width() <= width {
            self.rows.push(InfoRow {
                link: link_part(0, line.width()),
                text: line,
            });
            return;
        }
        let mut offset = 0;
        let cells: Vec<_> = line
            .styled_graphemes(Style::default())
            .map(|g| {
                let start = offset;
                let w = g.symbol.width();
                offset += w;
                (g, start, w)
            })
            .collect();
        let mut start = 0;
        while start < cells.len() {
            let mut end = start;
            let mut used = 0;
            let mut has_text = false;
            let mut word_break = None;
            while let Some((g, _, w)) = cells.get(end) {
                if used + w > width {
                    break;
                }
                used += w;
                end += 1;
                if g.symbol.trim().is_empty() {
                    if has_text {
                        word_break = Some(end);
                    }
                } else {
                    has_text = true;
                }
            }
            if end == start {
                // A two-cell grapheme cannot fit in a one-cell viewport.
                self.rows.push(InfoRow {
                    text: Line::styled("…", cells[start].0.style),
                    link: link_part(cells[start].1, 1),
                });
                start += 1;
                continue;
            }
            if end < cells.len() && !cells[end].0.symbol.trim().is_empty() {
                end = word_break.unwrap_or(end);
            }
            let mut last = end;
            if end < cells.len() {
                while last > start + 1 && cells[last - 1].0.symbol.trim().is_empty() {
                    last -= 1;
                }
            }
            let mut spans: Vec<Span<'static>> = vec![];
            for (g, _, _) in &cells[start..last] {
                if let Some(span) = spans.last_mut().filter(|span| span.style == g.style) {
                    span.content.to_mut().push_str(g.symbol);
                } else {
                    spans.push(Span::styled(g.symbol.to_owned(), g.style));
                }
            }
            let text = Line::from(spans);
            self.rows.push(InfoRow {
                link: link_part(cells[start].1, text.width()),
                text,
            });
            start = end;
            while start < cells.len() && cells[start].0.symbol.trim().is_empty() {
                start += 1;
            }
        }
    }

    fn branches(&mut self, branches: &'a [String], expanded: bool) {
        let Some(first) = branches.first() else {
            self.push(Line::styled("In no branches", fg(DIM)));
            return;
        };
        let n = branches.len();
        let prefix = format!("In {n} branch{}: ", if n == 1 { "" } else { "es" });
        if n == 1 && prefix.width() + first.width() <= self.width {
            self.push(format!("{prefix}{first}"));
            return;
        }
        let action = if expanded { "Show less" } else { "Show all" };
        let link = Span::styled(action, fg(BLUE));
        let preview = if expanded {
            String::new()
        } else {
            // A wide terminal should not turn a repository with thousands of refs into
            // one enormous preview string; the full list remains available on demand.
            let mut text = branches
                .iter()
                .take(4)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            if n > 4 {
                text.push_str(", …");
            }
            text
        };
        let inline = self.width >= prefix.width() + action.width() + if expanded { 0 } else { 6 };
        let space = if inline {
            self.width
                .saturating_sub(prefix.width() + action.width() + 1)
        } else {
            (self.width * 2).saturating_sub(prefix.width())
        };
        let mut heading = Line::styled(prefix.trim_end().to_owned(), fg(INK));
        if !expanded && space > 0 {
            heading.push_span(Span::raw(" "));
            heading.push_span(Span::styled(fit(&preview, space, false), fg(INK)));
        }
        if inline {
            heading.push_span(Span::raw(" "));
            let start = heading.width();
            heading.push_span(link);
            self.add(heading, Some((start, Hit::Branches)));
        } else {
            self.push(heading);
            self.add(Line::from(link), Some((0, Hit::Branches)));
        }
        if expanded {
            for branch in branches {
                self.push(Line::from(vec![
                    Span::styled("  ", fg(DIM)),
                    Span::styled(branch.as_str(), fg(INK)),
                ]));
            }
        }
    }
}

fn info_lines<'a>(
    panel: &'a LogPanel,
    rev: &'a model::Rev,
    submodule: bool,
    width: usize,
    roomy: bool,
) -> Vec<InfoRow<'a>> {
    let detail = &panel.details;
    let mut out = InfoLines {
        width,
        rows: vec![],
    };
    out.push(Line::styled(
        rev.subject.as_str(),
        fg(INK).add_modifier(Modifier::BOLD),
    ));
    if roomy {
        out.push("");
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
        out.branches(&detail.branches, panel.inspector.all_branches);
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
    out.rows
}

impl LogPanel {
    pub(super) fn draw_info(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            return;
        }
        f.render_widget(Block::default().style(fg(INK).bg(PANEL)), area);
        let Some(rev) = self.rev() else { return };
        let pad = u16::from(area.width >= 12);
        let width = area.width - pad * 2;
        let rows = info_lines(
            self,
            rev,
            c.repos.get(rev.root).is_some_and(|r| r.submodule),
            usize::from(width),
            area.height >= 9,
        );
        let max = rows.len().saturating_sub(usize::from(area.height));
        let offset = self.inspector.info_pos.min(max);
        let mut links = Vec::new();
        for (i, row) in rows
            .into_iter()
            .skip(offset)
            .take(usize::from(area.height))
            .enumerate()
        {
            let r = Rect::new(area.x + pad, area.y + i as u16, width, 1);
            f.render_widget(Paragraph::new(row.text), r);
            if let Some((start, len, hit)) = row.link {
                links.push((Rect::new(r.x + start as u16, r.y, len as u16, 1), hit));
            }
        }
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
            author: "李平".into(),
            subject: "修复界面渲染：换行、滚动，并保留链接".into(),
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
                })
                .collect(),
            body: "The extended description of this revision.\n第二行含有 🌱 和中文".into(),
            email: "lp@example.invalid".into(),
            branches: [
                "HEAD",
                "main",
                "origin/main",
                "release/older",
                "feature/用户",
            ]
            .map(str::to_owned)
            .into(),
        };
        p.inspector = Inspector::new(&p.details.files);
        (p, c)
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
            .position(|n| matches!(&n.kind, Kind::Directory(d) if d == "src/views"))
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
        let mut terminal = Terminal::new(TestBackend::new(26, 9)).unwrap();
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
            assert!(text_at(terminal.backend().buffer(), row).contains("TS invalid-�.ts M"));
        }
        assert_eq!(panel.details.files[panel.file_index().unwrap()].raw, raw);
        for (close, icon) in [(true, "[-]"), (false, "[+]")] {
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
    fn wrapped_details_keep_branches_and_each_scrolled_parent_clickable_at_the_visible_text() {
        let (mut p, c) = fixture(&[]);
        p.icons = crate::ui::icons::IconMode::Unicode;
        let mut terminal = Terminal::new(TestBackend::new(27, 5)).unwrap();
        let draw = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            p.hits.clear();
            t.draw(|f| p.draw_info(f, f.area(), &c)).unwrap();
        };
        draw(&mut p, &mut terminal);
        assert!(text_at(terminal.backend().buffer(), Rect::new(0, 0, 27, 1)).contains("修复界面"));
        let mut expanded = false;
        for _ in 0..=p.inspector.info_max {
            if let Some((r, _)) = p.hits.iter().find(|(_, h)| matches!(h, Hit::Branches)) {
                assert_eq!(text_at(terminal.backend().buffer(), *r), "Show all");
                p.toggle_branches();
                expanded = true;
                break;
            }
            p.scroll_info(1);
            draw(&mut p, &mut terminal);
        }
        assert!(expanded);
        draw(&mut p, &mut terminal);
        let mut saw_last_branch = false;
        for _ in 0..=p.inspector.info_max {
            if (0..5).any(|y| {
                text_at(terminal.backend().buffer(), Rect::new(0, y, 27, 1))
                    .contains("feature/用户")
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
