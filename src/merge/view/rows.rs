use super::*;
use similar::{DiffTag, TextDiff};
use std::ops::Range;

const CONTEXT: usize = 4;
const TAB: usize = 4;
pub(super) const EDITOR_BG: Color = Color::Rgb(23, 23, 26);
pub(super) const EDGE: Color = Color::Rgb(56, 58, 64);
const NUMBER: Color = Color::Rgb(105, 110, 120);
const GUIDE: Color = Color::Rgb(63, 66, 74);
const FOLD_BG: Color = Color::Rgb(42, 44, 49);

#[derive(Clone, Copy)]
pub(super) struct Tint {
    pub line: Color,
    pub gutter: Color,
    pub word: Color,
    pub marker: Color,
}

const MODIFIED: Tint = Tint {
    line: Color::Rgb(31, 40, 50),
    gutter: Color::Rgb(43, 66, 93),
    word: Color::Rgb(43, 66, 93),
    marker: Color::Rgb(85, 132, 165),
};
const CONFLICTED: Tint = Tint {
    line: Color::Rgb(35, 28, 28),
    gutter: Color::Rgb(53, 36, 32),
    word: Color::Rgb(64, 43, 39),
    marker: Color::Rgb(166, 90, 73),
};
const INSERTED: Tint = Tint {
    line: Color::Rgb(38, 54, 45),
    gutter: Color::Rgb(46, 73, 56),
    word: Color::Rgb(57, 92, 66),
    marker: Color::Rgb(83, 139, 99),
};
const DELETED: Tint = Tint {
    line: Color::Rgb(42, 43, 48),
    gutter: Color::Rgb(62, 63, 72),
    word: Color::Rgb(76, 78, 88),
    marker: Color::Rgb(116, 117, 131),
};

pub(super) fn side_tint(chunk: &Chunk, side: FileSide) -> Option<Tint> {
    chunk.pending(side).then(|| {
        if chunk.kind == Kind::Conflict {
            CONFLICTED
        } else {
            change_tint(&chunk.base, side.text(chunk))
        }
    })
}

fn change_tint(base: &str, result: &str) -> Tint {
    if base.is_empty() {
        INSERTED
    } else if result.is_empty() {
        DELETED
    } else {
        MODIFIED
    }
}

pub(super) struct Section<'a> {
    pub lines: [Vec<&'a str>; 3],
    pub first: usize,
    pub height: usize,
    pub numbers: [usize; 3],
    fold: Option<Range<usize>>,
    collapsed: bool,
}

impl Section<'_> {
    pub(super) fn line_at(&self, row: usize) -> Option<usize> {
        let Some(fold) = &self.fold else {
            return Some(row);
        };
        match row.cmp(&fold.start) {
            std::cmp::Ordering::Less => Some(row),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater if self.collapsed => Some(row + fold.len() - 1),
            std::cmp::Ordering::Greater => Some(row - 1),
        }
    }

    fn fold_caption(&self) -> String {
        if !self.collapsed {
            return " Collapse unchanged lines ".into();
        }
        let fold = self.fold.as_ref().expect("fold row");
        self.lines[0][fold.end..]
            .iter()
            .copied()
            .find_map(declaration_name)
            .map_or_else(
                || format!(" {} unchanged lines ", fold.len()),
                |name| format!(" {name}() "),
            )
    }
}

fn declaration_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start()
        .trim_start_matches("export ")
        .trim_start_matches("pub ")
        .trim_start_matches("async ");
    let declaration = ["function ", "fn ", "def ", "func "]
        .into_iter()
        .find_map(|prefix| line.strip_prefix(prefix))?;
    let name = declaration.split_once('(')?.0.trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '$')))
    .then_some(name)
}

pub(super) struct RowMap<'a> {
    pub sections: Vec<Section<'a>>,
    pub total: usize,
    pub widths: [usize; 3],
    pub numbers: [usize; 3],
}

impl<'a> RowMap<'a> {
    pub fn new(
        chunks: &'a [Chunk],
        view: &EditorView,
        edit: Option<(usize, &'a str, usize)>,
    ) -> Self {
        let mut total = 0;
        let mut numbers = [0; 3];
        let mut widths = [0; 3];
        let sections = chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                let replacement = edit.filter(|(index, _, _)| *index == i);
                let mut lines = [
                    split(&chunk.left),
                    split(replacement.map_or(chunk.result.as_str(), |(_, text, _)| text)),
                    split(&chunk.right),
                ];
                let starts = numbers;
                for (pane, lines) in lines.iter().enumerate() {
                    numbers[pane] += lines.len();
                    widths[pane] = lines.iter().fold(widths[pane], |w, l| w.max(cell_width(l)));
                }
                if let Some((_, _, cursor_row)) = replacement {
                    // The caret can be on the blank line after a final newline.
                    lines[1].resize(lines[1].len().max(cursor_row + 1), "");
                }
                let count = lines.iter().map(Vec::len).max().unwrap_or(0).max(1);
                let fold = (view.folding
                    && replacement.is_none()
                    && chunk.kind == Kind::Context
                    && chunk.result == chunk.base
                    && count > 2 * CONTEXT + 1)
                    .then_some(CONTEXT..count.saturating_sub(CONTEXT));
                let collapsed = !view.expanded.contains(&i);
                let height = count + usize::from(fold.is_some())
                    - fold.as_ref().filter(|_| collapsed).map_or(0, Range::len);
                let first = total;
                total += height;
                Section {
                    lines,
                    first,
                    height,
                    numbers: starts,
                    fold,
                    collapsed,
                }
            })
            .collect();
        Self {
            sections,
            total,
            widths,
            numbers,
        }
    }
}

fn split(text: &str) -> Vec<&str> {
    text.split_inclusive('\n')
        .map(|s| s.trim_end_matches(['\n', '\r']))
        .collect()
}

#[derive(Default, Clone, Copy)]
pub(super) struct Columns {
    pub code: Rect,
    pub numbers: Rect,
    controls: Rect,
    pub rail: Rect,
    pub scrollbar: Rect,
    rule: Option<u16>,
}

impl Columns {
    pub fn new(pane: usize, area: Rect, numbers: u16, split: bool) -> Self {
        Self::with_actions(pane, area, numbers, split, true)
    }

    pub fn read_only(pane: usize, area: Rect, numbers: u16, split: bool) -> Self {
        Self::with_actions(pane, area, numbers, split, false)
    }

    fn with_actions(pane: usize, area: Rect, numbers: u16, split: bool, actions: bool) -> Self {
        if area.is_empty() {
            return Self::default();
        }
        let y = area.y + 2;
        let h = area.height.saturating_sub(3);
        let rail_x = if pane == 0 { area.x } else { area.right() - 1 };
        let border = u16::from(split && pane > 0);
        let edge = u16::from(split && pane != 1);
        let actions = if actions && pane != 1 { 5 } else { 0 };
        let digits = if numbers == 0 {
            0
        } else {
            (numbers + 2).max(if pane == 1 { 5 } else { 4 })
        };
        let spare = area
            .width
            .saturating_sub(1 + border + edge + actions + digits);
        let (text_x, num_x, action_x, rule) = match pane {
            0 => {
                let start = area.x + 1;
                let edge_x = start + spare;
                (
                    start,
                    edge_x + edge + actions,
                    edge_x + edge,
                    (edge != 0).then_some(edge_x),
                )
            }
            1 => {
                let start = area.x + border;
                (start + digits, start, 0, None)
            }
            _ => {
                let start = area.x + border;
                let edge_x = start + digits + actions;
                (
                    edge_x + edge,
                    start,
                    start + digits,
                    (edge != 0).then_some(edge_x),
                )
            }
        };
        Self {
            code: rect(text_x, y, spare, h),
            numbers: rect(num_x, y, digits, h),
            controls: rect(action_x, y, actions, h),
            rail: rect(rail_x, y, 1, h),
            scrollbar: rect(text_x, y + h, spare, 1),
            rule,
        }
    }
}

#[derive(Clone, Copy)]
struct Glyphs {
    left: &'static str,
    right: &'static str,
    ignore: &'static str,
    wave: &'static str,
    edge: &'static str,
}

impl Glyphs {
    fn new(ascii: bool) -> Self {
        if ascii {
            Self {
                left: ">>",
                right: "<<",
                ignore: "x",
                wave: "~",
                edge: "|",
            }
        } else {
            Self {
                left: "»",
                right: "«",
                ignore: "×",
                wave: "∿",
                edge: "│",
            }
        }
    }
}

pub(super) struct Rows<'a> {
    pub chunks: &'a [Chunk],
    pub read_only: bool,
    pub file: &'a str,
    pub view: &'a EditorView,
    pub map: &'a RowMap<'a>,
    pub selected: usize,
    pub side: usize,
    pub scroll: usize,
    pub caret: Option<(usize, usize)>,
    pub active: bool,
    pub ascii: bool,
}

impl Rows<'_> {
    pub fn draw(
        &self,
        f: &mut Frame,
        pane: usize,
        area: Rect,
        col: Columns,
        hits: &mut Vec<(Rect, Hit)>,
    ) {
        if col.code.is_empty() {
            return;
        }
        let glyphs = Glyphs::new(self.ascii);
        let end = self.scroll + usize::from(col.code.height);
        let sections = &self.map.sections;
        let first = sections.partition_point(|s| s.first + s.height <= self.scroll);
        let source = FileSide::ALL.into_iter().find(|s| s.pane() == pane);
        let offset = self.view.hscroll[pane];
        for (i, section) in sections
            .iter()
            .enumerate()
            .skip(first)
            .take_while(|(_, s)| s.first < end)
        {
            let chunk = &self.chunks[i];
            let tint = if self.read_only {
                let changed = match pane {
                    0 => chunk.left != chunk.result,
                    2 => chunk.right != chunk.result,
                    _ => chunk.left != chunk.result || chunk.right != chunk.result,
                };
                changed.then_some(MODIFIED)
            } else if let Some(s) = source {
                side_tint(chunk, s)
            } else {
                FileSide::ALL
                    .into_iter()
                    .find_map(|s| side_tint(chunk, s))
                    .or_else(|| {
                        (chunk.result != chunk.base)
                            .then(|| change_tint(&chunk.base, &chunk.result))
                    })
            };
            for row in self.scroll.max(section.first)..end.min(section.first + section.height) {
                let local = row - section.first;
                let y = col.code.y + (row - self.scroll) as u16;
                let rect_row = rect(area.x, y, area.width, 1);
                hits.push((rect_row, Hit::Select(i, pane)));
                let Some(line) = section.line_at(local) else {
                    let message = section.fold_caption();
                    put(
                        f,
                        rect_row,
                        glyphs.wave.repeat(usize::from(area.width)),
                        style(EDGE),
                    );
                    let width = (message.width() as u16).min(col.code.width);
                    let r = rect(col.code.x, y, width, 1);
                    put(
                        f,
                        r,
                        clipped(&message, width as usize),
                        style(MUTED).bg(FOLD_BG),
                    );
                    // The overview rail can become scrollable when a fold
                    // opens. Keep the fold target inside the editor columns.
                    let fold_hit = rect(area.x + u16::from(pane == 0), y, area.width - 1, 1);
                    hits.push((fold_hit, Hit::Fold(i)));
                    continue;
                };
                let code = rect(col.code.x, y, col.code.width, 1);
                let number = rect(col.numbers.x, y, col.numbers.width, 1);
                let controls = rect(col.controls.x, y, col.controls.width, 1);
                let line_bg = tint.map_or(EDITOR_BG, |t| t.line);
                let gutter_bg = tint.map_or(EDITOR_BG, |t| t.gutter);
                if let Some(x) = col.rule {
                    put(f, rect(x, y, 1, 1), glyphs.edge, style(EDGE).bg(gutter_bg));
                }
                fill(f, code, line_bg);
                fill(f, number, gutter_bg);
                fill(f, controls, gutter_bg);
                if let Some(raw) = section.lines[pane].get(line) {
                    let n = section.numbers[pane] + line + 1;
                    let focused = self.active && i == self.selected && pane == self.side;
                    if !number.is_empty() {
                        let w = number.width as usize;
                        let text = if pane == 0 {
                            format!("{n:<w$}")
                        } else {
                            format!("{:>w$} ", n, w = w - 1)
                        };
                        put(
                            f,
                            number,
                            text,
                            style(if focused { FG } else { NUMBER }).bg(gutter_bg),
                        );
                    }
                    let ranges = if tint.is_some() && self.view.inner_diff && self.caret.is_none() {
                        changed_ranges(section, line, pane, chunk)
                    } else {
                        Vec::new()
                    };
                    let line_style = style(FG).bg(line_bg);
                    let syntax = syntax::tokens(self.file, raw);
                    let text = self
                        .view
                        .code_line(raw, code.width, offset, line_style, &ranges, tint, &syntax);
                    f.render_widget(Paragraph::new(text), code);
                    if !self.read_only && pane == 1 && i == self.selected {
                        if let Some((_, ch)) = self.caret.filter(|(r, _)| *r == line) {
                            let prefix: String = raw.chars().take(ch).collect();
                            let cursor = cell_width(&prefix)
                                .saturating_sub(offset)
                                .min(usize::from(code.width - 1));
                            if self.active {
                                f.set_cursor_position((code.x + cursor as u16, y));
                            }
                        } else if self.caret.is_none() && line == 0 && focused {
                            f.set_cursor_position((code.x, y));
                        }
                    }
                } else if let Some(tint) = tint {
                    // A source deletion/insertion still has a visible boundary
                    // and a target even when this side contributes no lines.
                    let empty = if self.ascii { '-' } else { '─' };
                    if line == section.lines[pane].len() {
                        put(
                            f,
                            code,
                            empty.to_string().repeat(code.width as usize),
                            style(tint.marker).bg(line_bg),
                        );
                    }
                }
                if pane == 1 && !self.read_only {
                    hits.push((code, Hit::ResultLine(i, line)));
                }
                if local == 0 && !self.read_only {
                    if let Some(source) = source.filter(|&s| chunk.pending(s)) {
                        draw_gutter(f, controls, source, glyphs, gutter_bg, hits, i);
                    }
                }
            }
        }
    }
}

fn draw_gutter(
    f: &mut Frame,
    area: Rect,
    side: FileSide,
    g: Glyphs,
    bg: Color,
    hits: &mut Vec<(Rect, Hit)>,
    i: usize,
) {
    let (take, accept_x, ignore_x) = match side {
        FileSide::Current => (g.left, area.x + 3, area.x + 1),
        FileSide::Incoming => (g.right, area.x, area.x + 3),
    };
    for (x, label, accept) in [(accept_x, take, true), (ignore_x, g.ignore, false)] {
        let r = rect(x, area.y, label.width() as u16, 1).intersection(area);
        put(f, r, label, style(FG).bg(bg));
        if !r.is_empty() {
            hits.push((r, Hit::Decide(i, side, accept)));
        }
    }
}

fn changed_ranges(
    section: &Section<'_>,
    row: usize,
    pane: usize,
    chunk: &Chunk,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let Some(current) = section.lines[pane].get(row) else {
        return ranges;
    };
    if pane == 1 && chunk.done() {
        if let Some(base) = chunk.base.split_inclusive('\n').nth(row) {
            add_changed_ranges(&mut ranges, current, base.trim_end_matches(['\n', '\r']));
        }
        return ranges;
    }
    let others: &[usize] = match pane {
        1 => &[0, 2],
        _ => &[1],
    };
    for &other in others {
        let source = if pane == 1 { other } else { pane };
        if !FileSide::ALL
            .into_iter()
            .any(|s| s.pane() == source && chunk.pending(s))
        {
            continue;
        }
        let Some(next) = section.lines[other].get(row) else {
            continue;
        };
        add_changed_ranges(&mut ranges, current, next);
    }
    ranges
}

fn add_changed_ranges(ranges: &mut Vec<Range<usize>>, old: &str, new: &str) {
    // A line tint is enough for generated and minified lines.
    if old == new || old.len().max(new.len()) > 2048 {
        return;
    }
    ranges.extend(
        TextDiff::configure()
            .timeout(std::time::Duration::from_millis(2))
            .diff_chars(old, new)
            .ops()
            .iter()
            .filter(|op| op.tag() != DiffTag::Equal)
            .map(|op| op.old_range()),
    );
}

pub(super) fn cell_width(s: &str) -> usize {
    if !s.contains('\t') {
        return s.width();
    }
    s.split('\t').enumerate().fold(0, |col, (i, part)| {
        (if i == 0 { col } else { col + TAB - col % TAB }) + part.width()
    })
}

pub(in crate::merge) fn char_at_cell(text: &str, target: usize) -> usize {
    let span = Span::raw(text);
    let mut x = 0;
    let mut index = 0;
    for g in span.styled_graphemes(Style::new()) {
        let width = if g.symbol == "\t" {
            TAB - x % TAB
        } else {
            g.symbol.width().max(1)
        };
        if x + width > target {
            return index;
        }
        x += width;
        index += g.symbol.chars().count();
    }
    index
}

impl EditorView {
    #[allow(clippy::too_many_arguments)]
    fn code_line(
        &self,
        text: &str,
        width: u16,
        offset: usize,
        base: Style,
        changes: &[Range<usize>],
        tint: Option<Tint>,
        syntax: &[(Range<usize>, Color)],
    ) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut x = 0;
        let end = offset.saturating_add(width as usize);
        let mut index = 0;
        let mut indent = true;
        let glyphs = Span::raw(text);
        let mut push = |s: &str, style| {
            if let Some(last) = spans.last_mut().filter(|last| last.style == style) {
                last.content.to_mut().push_str(s);
            } else {
                spans.push(Span::styled(s.to_owned(), style));
            }
        };
        for g in glyphs.styled_graphemes(base) {
            if x >= end {
                break;
            }
            let symbol = g.symbol;
            let chars = symbol.chars().count();
            let base = syntax
                .get(syntax.partition_point(|(r, _)| r.end <= index))
                .filter(|(r, _)| r.start <= index)
                .map_or(base, |(_, color)| base.fg(*color));
            let style = tint
                .filter(|_| {
                    changes
                        .iter()
                        .any(|r| r.start < index + chars && index < r.end)
                })
                .map_or(base, |t| base.bg(t.word));
            index += chars;
            if matches!(symbol, " " | "\t") {
                let w = if symbol == "\t" { TAB - x % TAB } else { 1 };
                for col in x.max(offset)..(x + w).min(end) {
                    if self.whitespace {
                        push(
                            if symbol == "\t" && col == x {
                                "→"
                            } else {
                                "·"
                            },
                            style.fg(GUIDE),
                        );
                    } else if self.guides && indent && col > 0 && col % TAB == 0 {
                        push("│", style.fg(GUIDE));
                    } else {
                        push(" ", style);
                    }
                }
                x += w;
            } else {
                indent = false;
                let symbol = if symbol.chars().any(char::is_control) {
                    "�"
                } else {
                    symbol
                };
                let w = symbol.width().max(1);
                if x >= offset && x + w <= end {
                    push(symbol, style);
                } else if x < end && x + w > offset {
                    // Preserve alignment when the viewport starts inside a wide grapheme.
                    push(&" ".repeat((x + w).min(end) - x.max(offset)), style);
                }
                x += w;
            }
        }
        Line::from(spans)
    }
}
