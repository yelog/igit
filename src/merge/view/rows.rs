use super::highlight::{padding_tint, revision_line, Fragment};
use super::*;
use std::ops::Range;

const CONTEXT: usize = 4;
const TAB: usize = 4;
pub(super) const EDITOR_BG: Color = Color::Rgb(23, 23, 26);
pub(super) const EDGE: Color = Color::Rgb(56, 58, 64);
const NUMBER: Color = Color::Rgb(105, 110, 120);
const GUIDE: Color = Color::Rgb(63, 66, 74);
const FOLD_BG: Color = Color::Rgb(42, 44, 49);

pub(super) struct Section<'a> {
    pub texts: [&'a str; 3],
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
        Self::layout(chunks, view, edit, None)
    }

    pub fn revision(chunks: &'a [Chunk], view: &EditorView, single_pane: Option<usize>) -> Self {
        Self::layout(chunks, view, None, single_pane)
    }

    fn layout(
        chunks: &'a [Chunk],
        view: &EditorView,
        edit: Option<(usize, &'a str, usize)>,
        single_pane: Option<usize>,
    ) -> Self {
        let mut total = 0;
        let mut numbers = [0; 3];
        let mut widths = [0; 3];
        let sections = chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                let replacement = edit.filter(|(index, _, _)| *index == i);
                let texts = [
                    chunk.left.as_str(),
                    replacement.map_or(chunk.result.as_str(), |(_, text, _)| text),
                    chunk.right.as_str(),
                ];
                let mut lines = texts.map(|text| text.split_inclusive('\n').collect::<Vec<_>>());
                let starts = numbers;
                for (pane, lines) in lines.iter().enumerate() {
                    numbers[pane] += lines.len();
                    widths[pane] = lines
                        .iter()
                        .fold(widths[pane], |w, line| w.max(cell_width(line_body(line))));
                }
                if let Some((_, _, cursor_row)) = replacement {
                    // The caret can be on the blank line after a final newline.
                    lines[1].resize(lines[1].len().max(cursor_row + 1), "");
                }
                let count = single_pane
                    .map_or_else(
                        || lines.iter().map(Vec::len).max().unwrap_or(0),
                        |pane| lines[pane].len(),
                    )
                    .max(1);
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
                    texts,
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

pub(super) fn line_body(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

#[derive(Default, Clone, Copy)]
pub(super) struct Columns {
    pub code: Rect,
    pub numbers: Rect,
    pub(super) controls: Rect,
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
            let padding_tint = if self.read_only && !section.lines[pane].is_empty() {
                None
            } else {
                padding_tint(chunk, pane, self.read_only)
            };
            let changes = if self.read_only {
                Vec::new()
            } else {
                self.view
                    .highlights
                    .line_changes(i, chunk, &section.texts, pane)
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
                let raw = section.lines[pane].get(line).copied().map(line_body);
                let change = if self.read_only {
                    revision_line(
                        section
                            .lines
                            .each_ref()
                            .map(|lines| lines.get(line).copied()),
                        pane,
                    )
                } else {
                    changes.get(line).copied().unwrap_or_default()
                };
                let tint = if raw.is_some() {
                    change.tint()
                } else if self.read_only {
                    None
                } else {
                    padding_tint
                };
                let line_bg = tint.map_or(EDITOR_BG, |t| t.line);
                let gutter_bg = tint.map_or(EDITOR_BG, |t| t.gutter);
                if let Some(x) = col.rule {
                    put(f, rect(x, y, 1, 1), glyphs.edge, style(EDGE).bg(gutter_bg));
                }
                fill(f, code, line_bg);
                fill(f, number, gutter_bg);
                fill(f, controls, gutter_bg);
                if let Some(raw) = raw {
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
                        if change.indent_marker() {
                            put(
                                f,
                                rect(number.right() - 1, y, 1, 1),
                                if self.ascii { "." } else { "·" },
                                style(NUMBER).bg(gutter_bg),
                            );
                        }
                    }
                    let fragments = if self.view.inner_diff && self.caret.is_none() {
                        change.fragments(raw, self.read_only)
                    } else {
                        Vec::new()
                    };
                    let line_style = style(FG).bg(line_bg);
                    let syntax = syntax::tokens(self.file, raw);
                    let text = self
                        .view
                        .code_line(raw, code.width, offset, line_style, &fragments, &syntax);
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
                } else if let Some(tint) = padding_tint {
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
                    if self.read_only && !number.is_empty() {
                        let x = if pane == 0 {
                            number.x
                        } else {
                            number.right() - 1
                        };
                        put(
                            f,
                            rect(x, y, 1, 1),
                            glyphs.edge,
                            style(tint.marker).bg(EDITOR_BG),
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
        changes: &[Fragment],
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
            let style = changes
                .iter()
                .find(|change| change.range.start < index + chars && index < change.range.end)
                .map_or(base, |change| base.bg(change.background));
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

#[cfg(test)]
mod tests;
