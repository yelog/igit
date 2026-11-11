//! Read-only historical comparisons plus an optional reviewed local write path.
pub(super) mod alignment;

use super::highlight::revision_range_tint;
use super::*;
use crate::ui::{
    search::{Hit as SearchHit, Input, Jump, Prompt, Search},
    selection::{TextLayout, TextSelection},
};
use std::ops::Range;

pub(crate) enum RevisionEvent {
    None,
    Close,
    File(isize),
    Copy(String),
    Apply(usize),
    ApplyAll,
}

#[derive(Clone, Copy)]
enum Target {
    Close,
    Change(isize),
    File(isize),
    FoldAll,
    Zoom,
    Pane(usize),
    Row(usize, usize),
    Fold(usize),
    Apply(usize),
    ApplyAll,
}

pub(crate) struct RevisionDiff {
    three_way: bool,
    name: String,
    revisions: [String; 3],
    annotations: [String; 3],
    texts: [String; 3],
    chunks: Vec<Chunk>,
    changes: Vec<usize>,
    view: EditorView,
    side: usize,
    selected: usize,
    scroll: usize,
    follow: bool,
    zoom: bool,
    single_pane: Option<usize>,
    total: usize,
    page: usize,
    hits: Vec<(Rect, Target)>,
    selections: [TextSelection; 3],
    code_areas: [Rect; 3],
    line_ranges: [Vec<Range<usize>>; 3],
    search: Search<(usize, usize)>,
    search_hit: Option<SearchHit<(usize, usize)>>,
    reveal_search: bool,
    local_merge: bool,
    local_apply_all: bool,
}

impl RevisionDiff {
    pub(crate) fn new(
        name: String,
        revisions: [String; 3],
        texts: [String; 3],
        annotations: [String; 3],
    ) -> Self {
        let chunks = alignment::chunks(texts.each_ref().map(String::as_str));
        let changes: Vec<_> = chunks
            .iter()
            .enumerate()
            .filter_map(|(i, chunk)| {
                (chunk.kind != Kind::Context && (i == 0 || chunks[i - 1].kind == Kind::Context))
                    .then_some(i)
            })
            .collect();
        let selected = changes.first().copied().unwrap_or(0);
        let line_ranges = texts.each_ref().map(|text| {
            let mut at = 0;
            text.split_inclusive('\n')
                .map(|line| {
                    let range = at..at + line.trim_end_matches(['\r', '\n']).len();
                    at += line.len();
                    range
                })
                .collect()
        });
        Self {
            three_way: true,
            name,
            revisions,
            texts,
            annotations,
            chunks,
            changes,
            selected,
            side: 1,
            scroll: 0,
            follow: true,
            zoom: false,
            single_pane: None,
            total: 0,
            page: 0,
            hits: Vec::new(),
            view: EditorView::default(),
            selections: Default::default(),
            code_areas: Default::default(),
            line_ranges,
            search: Search::default(),
            search_hit: None,
            reveal_search: false,
            local_merge: false,
            local_apply_all: false,
        }
    }

    pub(crate) fn two_way(
        name: String,
        revisions: [String; 2],
        texts: [String; 2],
        annotations: [String; 2],
    ) -> Self {
        let [before, after] = texts;
        let [old_id, new_id] = revisions;
        let [old_note, new_note] = annotations;
        // The hidden alignment side equals the result, so it contributes no
        // changes or extra rows. Visible panes are the old and new revisions.
        let mut diff = Self::new(
            name,
            [old_id, new_id, String::new()],
            [before, after.clone(), after],
            [old_note, new_note, String::new()],
        );
        diff.three_way = false;
        diff
    }

    /// Compare an immutable revision with the working file and allow the
    /// revision side to replace selected aligned blocks.
    pub(crate) fn local(
        name: String,
        revision: String,
        revision_text: String,
        local_text: String,
        revision_note: String,
        local_note: String,
        apply_all: bool,
    ) -> Self {
        let mut diff = Self::two_way(
            name,
            [revision, String::new()],
            [revision_text, local_text],
            [revision_note, local_note],
        );
        diff.local_merge = true;
        diff.local_apply_all = apply_all;
        diff
    }

    pub(crate) fn is_three_way(&self) -> bool {
        self.three_way
    }

    pub(crate) fn local_merge(&self) -> bool {
        self.local_merge
    }

    pub(crate) fn selected_change(&self) -> Option<usize> {
        self.change_at(self.selected)
            .or_else(|| self.adjacent_change(1))
            .or_else(|| self.adjacent_change(-1))
    }

    pub(crate) fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// Build the local text with one aligned change, or every change, taken
    /// from the revision. The hidden alignment pane equals Local, so `result`
    /// is the visible right-hand text for each section.
    pub(crate) fn merged_local_text(&self, change: Option<usize>) -> Option<String> {
        if !self.local_merge {
            return None;
        }
        match change {
            None => Some(self.texts[0].clone()),
            Some(index) => {
                let start = self.change_at(index)?;
                let end = self.change_end(start);
                (start < end).then(|| {
                    self.chunks
                        .iter()
                        .enumerate()
                        .map(|(i, chunk)| {
                            if (start..end).contains(&i) {
                                chunk.left.as_str()
                            } else {
                                chunk.result.as_str()
                            }
                        })
                        .collect()
                })
            }
        }
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selections[self.side].selected_text()
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selections.iter_mut().for_each(TextSelection::clear);
        self.view.cancel_scroll_drag();
    }

    pub(crate) fn mouse_active(&self) -> bool {
        self.view.drag.is_some() || self.selections.iter().any(TextSelection::mouse_active)
    }

    pub(crate) fn cancel_mouse(&mut self) {
        self.selections
            .iter_mut()
            .for_each(TextSelection::cancel_mouse);
        self.view.cancel_scroll_drag();
    }

    pub(crate) fn side(&self) -> usize {
        self.side
    }

    pub(crate) fn search_prompt(&self) -> &Prompt {
        self.search.prompt()
    }

    /// Carry session input to new sources without retaining old hit positions,
    /// selections, folds, or viewport state.
    pub(crate) fn inherit_search(&mut self, previous: &Self) {
        self.inherit_query(previous.search_prompt());
    }

    pub(crate) fn inherit_query(&mut self, prompt: &Prompt) {
        self.search.inherit_query(prompt);
        self.search_hit = None;
        self.reveal_search = false;
    }

    /// Embedded panes opt into search before forwarding ordinary viewer events.
    /// Historical Log/Push viewers keep their existing change navigation.
    pub(crate) fn search_event(&mut self, event: &Event) -> bool {
        match self.search.event(event) {
            Input::Pass => false,
            Input::Handled => {
                if !self.search.prompt().visible() {
                    self.search_hit = None;
                    self.reveal_search = false;
                }
                true
            }
            Input::Jump(direction) => {
                let side = self.side();
                let direction = if direction == Jump::Next && self.search_hit.is_none() {
                    Jump::First
                } else {
                    direction
                };
                let anchor = self.search_hit.as_ref().map(|hit| hit.row).or_else(|| {
                    let map = RowMap::revision(&self.chunks, &self.view, self.single_pane);
                    map.sections
                        .iter()
                        .find(|section| section.first + section.height > self.scroll)
                        .map(|section| {
                            let local = self.scroll.saturating_sub(section.first);
                            let line = section
                                .line_at(local)
                                .or_else(|| section.line_at(local.saturating_sub(1)))
                                .unwrap_or(0);
                            (
                                side,
                                (section.numbers[side] + line)
                                    .min(self.line_ranges[side].len().saturating_sub(1)),
                            )
                        })
                });
                self.search_hit = self.search.jump(
                    direction,
                    anchor.as_ref(),
                    self.line_ranges[side]
                        .iter()
                        .enumerate()
                        .map(|(line, bytes)| ((side, line), [&self.texts[side][bytes.clone()]])),
                );
                self.reveal_search = self.search_hit.is_some();
                true
            }
        }
    }

    fn pane_count(&self) -> usize {
        if self.three_way {
            3
        } else {
            2
        }
    }

    fn select_side(&mut self, side: usize) {
        if side != self.side {
            self.selections[self.side].clear();
            self.side = side;
            self.search.invalidate();
            self.search_hit = None;
            self.reveal_search = false;
            self.view.cancel_scroll_drag();
        }
    }

    fn glyph_width(symbol: &str, column: usize) -> usize {
        if symbol == "\t" {
            4 - column % 4
        } else {
            symbol.width().max(1)
        }
    }

    fn match_cells(text: &str, bytes: &Range<usize>) -> Range<usize> {
        let mut byte = 0;
        let mut column = 0;
        let mut first = None;
        for glyph in Span::raw(text).styled_graphemes(Style::default()) {
            byte += glyph.symbol.len();
            if byte > bytes.start && first.is_none() {
                first = Some(column);
            }
            column += Self::glyph_width(glyph.symbol, column);
            if byte >= bytes.end {
                break;
            }
        }
        first.unwrap_or(column)..column
    }

    fn overlay_search(
        &self,
        f: &mut Frame,
        pane: usize,
        code: Rect,
        projected: &[(Range<usize>, bool, usize)],
    ) {
        if pane != self.side || code.is_empty() || !self.search.prompt().visible() {
            return;
        }
        let left = self.view.hscroll[pane];
        let right = left + usize::from(code.width);
        for (y, (bytes, decoration, line)) in
            projected.iter().take(usize::from(code.height)).enumerate()
        {
            if *decoration {
                continue;
            }
            let spans = self
                .search
                .field(&(pane, *line), 0)
                .spans(&self.texts[pane][bytes.clone()], Style::default());
            let mut column = 0;
            for glyph in spans
                .iter()
                .flat_map(|span| span.styled_graphemes(Style::default()))
            {
                let width = Self::glyph_width(glyph.symbol, column);
                if glyph.style != Style::default() {
                    for x in column.max(left)..(column + width).min(right) {
                        f.buffer_mut()[(code.x + (x - left) as u16, code.y + y as u16)]
                            .set_style(glyph.style);
                    }
                }
                column += width;
                if column >= right {
                    break;
                }
            }
        }
    }

    fn button(
        &mut self,
        f: &mut Frame,
        bar: &mut Rect,
        label: &str,
        target: Target,
        enabled: bool,
    ) {
        let width = label.width() as u16;
        if bar.width < width {
            return;
        }
        let area = left(bar, width);
        put(
            f,
            area,
            label,
            style(if enabled { FG } else { EDGE }).bg(TOOLBAR),
        );
        if enabled {
            self.hits.push((area, target));
        }
    }

    pub(crate) fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        file: usize,
        count: usize,
    ) {
        self.draw_comparison(f, area, icons, Some((file, count)));
    }

    pub(crate) fn draw_inline(&mut self, f: &mut Frame, area: Rect, icons: IconMode) {
        self.draw_comparison(f, area, icons, None);
    }

    fn draw_comparison(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        files: Option<(usize, usize)>,
    ) {
        let inline = files.is_none();
        self.hits.clear();
        let previous_code = self.code_areas;
        self.code_areas.fill(Rect::default());
        self.view.tracks.clear();
        f.render_widget(Clear, area);
        fill(f, area, EDITOR_BG);
        if area.width < 16 || area.height < if inline { 4 } else { 6 } {
            put(
                f,
                area,
                if inline { "Diff" } else { "Diff · Esc Close" },
                style(FG),
            );
            return;
        }
        let ascii = icons == IconMode::Ascii;
        let (toolbar, body, footer) = if inline {
            let [toolbar, body] = Layout::vertical([C::Length(1), C::Min(3)]).areas(area);
            (toolbar, body, None)
        } else {
            let [title, toolbar, body, footer] =
                Layout::vertical([C::Length(1), C::Length(1), C::Min(3), C::Length(1)]).areas(area);
            fill(f, title, CHROME);
            let mut title_space = title;
            let close = right(&mut title_space, 7.min(title.width));
            put(f, close, " Close ", style(MUTED).bg(CHROME));
            self.hits.push((close, Target::Close));
            let title = if self.local_merge {
                format!(" Compare with Local: {}", self.name)
            } else {
                format!(" Repository Diff: {}", self.name)
            };
            put(
                f,
                title_space,
                clipped(&title, title_space.width as usize),
                style(FG).bg(CHROME),
            );
            (toolbar, body, Some(footer))
        };

        fill(f, toolbar, TOOLBAR);
        let mut tools = toolbar;
        let summary = if self.three_way {
            format!(" {} changes, No conflicts ", self.changes.len())
        } else {
            format!(
                " {} difference{} ",
                self.changes.len(),
                if self.changes.len() == 1 { "" } else { "s" }
            )
        };
        let fold = if self.view.folding {
            " Unfold "
        } else {
            " Fold "
        };
        let zoom = if self.zoom { " Split " } else { " Zoom " };
        let summary_min_width = if inline {
            (6 + fold.width() + zoom.width() + summary.width()) as u16
        } else {
            70
        };
        if tools.width >= summary_min_width {
            let status = right(&mut tools, summary.width() as u16);
            put(f, status, summary, style(MUTED).bg(TOOLBAR));
        }
        self.button(
            f,
            &mut tools,
            if ascii { " ^ " } else { " ↑ " },
            Target::Change(-1),
            self.adjacent_change(-1).is_some(),
        );
        self.button(
            f,
            &mut tools,
            if ascii { " v " } else { " ↓ " },
            Target::Change(1),
            self.adjacent_change(1).is_some(),
        );
        if let Some((file, count)) = files.filter(|(_, count)| !self.local_merge || *count > 1) {
            self.button(f, &mut tools, " < ", Target::File(-1), file > 0);
            let counter = format!("{}/{} files", file + 1, count);
            if tools.width >= counter.width() as u16 {
                let at = left(&mut tools, counter.width() as u16);
                put(f, at, counter, style(ACCENT).bg(TOOLBAR));
            }
            self.button(f, &mut tools, " > ", Target::File(1), file + 1 < count);
        }
        self.button(f, &mut tools, fold, Target::FoldAll, true);
        self.button(f, &mut tools, zoom, Target::Zoom, true);
        if self.local_merge {
            self.button(
                f,
                &mut tools,
                " Apply ",
                Target::Apply(self.selected_change().unwrap_or(0)),
                self.selected_change().is_some(),
            );
            self.button(
                f,
                &mut tools,
                " All ",
                Target::ApplyAll,
                self.local_apply_all && !self.changes.is_empty(),
            );
        }

        let split = area.width >= if self.three_way { 72 } else { 48 } && !self.zoom;
        let cols = if split {
            if self.three_way {
                Layout::horizontal([C::Percentage(32), C::Percentage(36), C::Percentage(32)])
                    .areas(body)
            } else {
                let [before, after] =
                    Layout::horizontal([C::Percentage(50), C::Percentage(50)]).areas(body);
                [before, after, Rect::default()]
            }
        } else {
            let mut cols = [Rect::default(); 3];
            cols[self.side] = body;
            cols
        };
        let single_pane = (!split).then_some(self.side);
        let mut map = RowMap::revision(&self.chunks, &self.view, single_pane);
        if single_pane != self.single_pane && !self.follow {
            let previous = RowMap::revision(&self.chunks, &self.view, self.single_pane);
            if let Some((i, section)) = previous.sections.iter().enumerate().find(|(_, section)| {
                (section.first..section.first + section.height).contains(&self.scroll)
            }) {
                let local = self.scroll - section.first;
                self.scroll = map.sections[i].first + local.min(map.sections[i].height - 1);
            }
        }
        self.single_pane = single_pane;
        self.page = body.height.saturating_sub(if inline { 2 } else { 3 }) as usize;
        let position_search =
            self.reveal_search || usize::from(previous_code[self.side].height) != self.page;
        let search_line = self
            .search_hit
            .as_ref()
            .filter(|_| position_search)
            .and_then(|hit| {
                let (side, line) = hit.row;
                map.sections.iter().enumerate().find_map(|(i, section)| {
                    (section.numbers[side]..section.numbers[side] + section.lines[side].len())
                        .contains(&line)
                        .then(|| (i, line - section.numbers[side]))
                })
            });
        if self.reveal_search {
            if let Some((i, line)) = search_line {
                let section = &map.sections[i];
                if !(0..section.height).any(|row| section.line_at(row) == Some(line)) {
                    self.view.expanded.insert(i);
                    map = RowMap::revision(&self.chunks, &self.view, single_pane);
                }
                self.selected = i;
                self.follow = false;
            }
        }
        self.total = map.total;
        if self.follow {
            let section = &map.sections[self.selected];
            self.scroll = keep_visible(
                self.scroll,
                section.first.saturating_sub(2),
                section.height + 4,
                self.page,
            );
            self.follow = false;
        }
        if let Some((i, line)) = search_line {
            let section = &map.sections[i];
            if let Some(row) = (0..section.height).find(|&row| section.line_at(row) == Some(line)) {
                self.scroll = keep_visible(self.scroll, section.first + row, 1, self.page);
            }
        }
        self.scroll = self.scroll.min(self.total.saturating_sub(self.page));
        for (pane, col) in cols.into_iter().enumerate().filter(|(_, c)| !c.is_empty()) {
            let role = if self.local_merge {
                ["Revision", "Local", ""][pane]
            } else if self.three_way {
                ["Parent 1", "Result", "Parent 2"][pane]
            } else {
                ["Before", "After", ""][pane]
            };
            let heading = rect(col.x, col.y, col.width, 1);
            let revision = &self.revisions[pane];
            let short = if revision.len() > 8 && revision.chars().all(|c| c.is_ascii_hexdigit()) {
                &revision[..8]
            } else {
                revision
            };
            let mut caption = if self.local_merge && pane == 1 {
                format!(" {role}")
            } else {
                format!(" {} {}  {role}", icons.glyph(Icon::Locked), short)
            };
            if inline {
                if pane == 1 {
                    let name = if caption.width() + 2 + self.name.width() <= col.width as usize {
                        self.name.as_str()
                    } else {
                        self.name.rsplit('/').next().unwrap_or(&self.name)
                    };
                    if caption.width() + 2 + name.width() <= col.width as usize {
                        caption.push_str("  ");
                        caption.push_str(name);
                    }
                }
                let note = &self.annotations[pane];
                if !note.is_empty() && caption.width() + 3 + note.width() <= col.width as usize {
                    caption.push_str(" · ");
                    caption.push_str(note);
                }
            }
            put(
                f,
                heading,
                clipped(&caption, col.width as usize),
                style(if self.side == pane { FG } else { MUTED }).bg(if self.side == pane {
                    BUTTON
                } else {
                    CHROME
                }),
            );
            self.hits.push((heading, Target::Pane(pane)));
            if !inline {
                put(
                    f,
                    rect(col.x, col.y + 1, col.width, 1),
                    clipped(&format!(" {}", self.annotations[pane]), col.width as usize),
                    style(MUTED),
                );
            }
            if split && pane > 0 {
                for y in col.y + 1..col.bottom() {
                    put(
                        f,
                        rect(col.x, y, 1, 1),
                        if ascii { "|" } else { "│" },
                        style(EDGE),
                    );
                }
            }
            let numbers = map.numbers[pane].max(1).ilog10() as u16 + 1;
            let mut metrics = if self.local_merge && pane == 0 {
                Columns::new(pane, col, numbers, split)
            } else {
                Columns::read_only(pane, col, numbers, split)
            };
            if inline {
                // The inline view has one revision heading and no annotation
                // row. Keep the same gutters and scrollbar, reclaiming its row.
                for area in [&mut metrics.code, &mut metrics.numbers, &mut metrics.rail] {
                    area.y -= 1;
                    area.height += 1;
                }
            }
            let width = metrics.code.width as usize;
            self.view.limits[pane] = map.widths[pane].saturating_sub(width);
            self.view.hscroll[pane] = self.view.hscroll[pane].min(self.view.limits[pane]);
            if pane == self.side
                && width > 0
                && (self.reveal_search || previous_code[pane].width != metrics.code.width)
            {
                if let Some(hit) = &self.search_hit {
                    let text = &self.texts[pane][self.line_ranges[pane][hit.row.1].clone()];
                    let cells = Self::match_cells(text, &hit.bytes);
                    let left = &mut self.view.hscroll[pane];
                    if cells.start < *left {
                        *left = cells.start.saturating_sub(width / 4);
                    } else if cells.end > left.saturating_add(width) {
                        *left = (cells.end + width / 4)
                            .saturating_sub(width)
                            .min(cells.start);
                    }
                    *left = (*left).min(self.view.limits[pane]);
                }
            }
            let mut row_hits = Vec::new();
            Rows {
                chunks: &self.chunks,
                read_only: true,
                file: &self.name,
                view: &self.view,
                map: &map,
                selected: self.selected,
                side: self.side,
                scroll: self.scroll,
                caret: None,
                active: false,
                ascii,
            }
            .draw(f, pane, col, metrics, &mut row_hits);
            self.code_areas[pane] = metrics.code;
            let visible = self.scroll..self.scroll + self.page;
            let projected: Vec<_> = map
                .sections
                .iter()
                .filter(|s| s.first < visible.end && s.first + s.height > visible.start)
                .flat_map(|section| {
                    (visible.start.max(section.first)
                        ..visible.end.min(section.first + section.height))
                        .map(|row| {
                            let local = row - section.first;
                            let (line, decoration) = match section.line_at(local) {
                                Some(line) if line < section.lines[pane].len() => (line, false),
                                Some(_) => (section.lines[pane].len(), true),
                                None => (
                                    section.line_at(local.saturating_sub(1)).unwrap_or(0) + 1,
                                    true,
                                ),
                            };
                            let source_line = section.numbers[pane] + line;
                            let range = self.line_ranges[pane]
                                .get(source_line)
                                .cloned()
                                .unwrap_or(self.texts[pane].len()..self.texts[pane].len());
                            (
                                if decoration {
                                    range.start..range.start
                                } else {
                                    range
                                },
                                decoration,
                                source_line,
                            )
                        })
                })
                .collect();
            self.overlay_search(f, pane, metrics.code, &projected);
            let layout = TextLayout::projected(
                &self.texts[pane],
                projected
                    .into_iter()
                    .map(|(bytes, decoration, _)| (bytes, decoration)),
            );
            self.selections[pane].overlay(f, metrics.code, layout, self.view.hscroll[pane]);
            self.hits
                .extend(row_hits.into_iter().filter_map(|(r, hit)| match hit {
                    Hit::Select(i, side) => Some((r, Target::Row(i, side))),
                    Hit::Fold(i) => Some((r, Target::Fold(i))),
                    _ => None,
                }));
            if self.local_merge && pane == 0 && !metrics.controls.is_empty() {
                let label = if ascii { " >> " } else { " » " };
                let width = label.width() as u16;
                for (i, section) in map.sections.iter().enumerate() {
                    if self.change_at(i) != Some(i)
                        || section.first >= self.scroll + usize::from(metrics.controls.height)
                        || section.first + section.height <= self.scroll
                    {
                        continue;
                    }
                    let row = section.first.max(self.scroll) - self.scroll;
                    let button = rect(
                        metrics.controls.right().saturating_sub(width),
                        metrics.controls.y + row as u16,
                        width,
                        1,
                    )
                    .intersection(metrics.controls);
                    put(f, button, label, style(ACCENT).bg(BUTTON));
                    if !button.is_empty() {
                        self.hits.push((button, Target::Apply(i)));
                    }
                }
            }
            let rail_height = usize::from(metrics.rail.height);
            for (chunk, section) in self.chunks.iter().zip(&map.sections) {
                let Some(tint) = revision_range_tint(chunk, pane) else {
                    continue;
                };
                let top = rail_height * section.first / map.total.max(1);
                let end = rail_height * (section.first + section.height) / map.total.max(1);
                for row in top..end.max(top + 1).min(rail_height) {
                    put(
                        f,
                        rect(metrics.rail.x, metrics.rail.y + row as u16, 1, 1),
                        if ascii { "-" } else { "╴" },
                        style(tint.marker).bg(tint.line),
                    );
                }
            }
            // The thumb stays visible above the overview, including when a
            // single large change spans most of the rail.
            for track in [
                Track::new(
                    Scroll::Vertical,
                    metrics.rail,
                    map.total,
                    self.page,
                    self.scroll,
                ),
                Track::new(
                    Scroll::Horizontal(pane),
                    metrics.scrollbar,
                    map.widths[pane],
                    width,
                    self.view.hscroll[pane],
                ),
            ]
            .into_iter()
            .flatten()
            {
                track.paint(f, ascii);
                self.view.tracks.push(track);
            }
        }
        self.reveal_search = false;
        if let Some(footer) = footer {
            fill(f, footer, CHROME);
            if self.local_merge && self.search.prompt().visible() {
                self.search.prompt().draw(f, footer);
                return;
            }
            let hint = if self.local_merge && footer.width >= 105 {
                " Esc Close · L Apply change · A Apply all · n/p Change · Tab Pane · f Fold · z Zoom"
            } else if self.local_merge {
                " Esc Close · L Apply · A Apply all · n/p Change"
            } else if footer.width >= 105 {
                " Esc Close · Tab Pane · n/p Change · [/] File · f Fold · z Zoom · h/l Scroll · y Copy"
            } else if footer.width >= 65 {
                " Esc Close · Tab Pane · n/p Change · [/] File · f Fold · z Zoom"
            } else {
                " Esc Close · Tab Pane · n/p Change"
            };
            put(
                f,
                footer,
                clipped(hint, footer.width as usize),
                style(MUTED).bg(CHROME),
            );
        }
    }

    fn adjacent_change(&self, direction: isize) -> Option<usize> {
        let mut anchor = self.selected;
        if self.chunks[anchor].kind != Kind::Context {
            anchor = self.changes[self.changes.partition_point(|&i| i <= anchor) - 1];
        }
        if direction > 0 {
            self.changes.iter().copied().find(|&i| i > anchor)
        } else {
            self.changes.iter().copied().rfind(|&i| i < anchor)
        }
    }

    fn change_at(&self, index: usize) -> Option<usize> {
        if self.chunks.get(index)?.kind == Kind::Context {
            return None;
        }
        let next = self.changes.partition_point(|&i| i <= index);
        (next > 0).then(|| self.changes[next - 1])
    }

    fn change_end(&self, start: usize) -> usize {
        self.changes
            .iter()
            .copied()
            .find(|&i| i > start)
            .unwrap_or(self.chunks.len())
    }

    fn activate(&mut self, target: Target) -> RevisionEvent {
        match target {
            Target::Close => return RevisionEvent::Close,
            Target::File(d) => return RevisionEvent::File(d),
            Target::Apply(i) => {
                let i = self.change_at(i).unwrap_or(i);
                self.selected = i;
                self.follow = true;
                return RevisionEvent::Apply(i);
            }
            Target::ApplyAll => return RevisionEvent::ApplyAll,
            Target::Change(d) => {
                if let Some(next) = self.adjacent_change(d) {
                    self.selected = next;
                    self.follow = true;
                }
            }
            Target::FoldAll => {
                self.view.folding = !self.view.folding;
                self.view.expanded.clear();
                self.follow = true;
            }
            Target::Fold(i) => {
                self.view.toggle_fold(i);
                self.selected = i;
                self.follow = true;
            }
            Target::Zoom => self.zoom = !self.zoom,
            Target::Pane(side) => self.select_side(side),
            Target::Row(i, side) => {
                self.selected = i;
                self.select_side(side);
            }
        }
        RevisionEvent::None
    }

    pub(crate) fn event(&mut self, event: &Event) -> RevisionEvent {
        if matches!(event, Event::FocusLost) {
            self.clear_selection();
        }
        self.selections[self.side].sync_source(&self.texts[self.side]);
        if let Some(action) = self.selections[self.side].key(event, true) {
            return Self::selection_event(action);
        }
        if let Event::Mouse(m) = event {
            if self
                .view
                .scroll_mouse(m, &mut self.scroll, &mut self.follow)
            {
                self.selections[self.side].clear();
                return RevisionEvent::None;
            }
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                let target = self
                    .hits
                    .iter()
                    .rev()
                    .find(|(r, _)| inside(*r, m.column, m.row))
                    .map(|(_, t)| *t);
                if let Some(Target::Fold(i)) = target {
                    self.selections[self.side].clear();
                    return self.activate(Target::Fold(i));
                }
                if let Some(side) = self
                    .code_areas
                    .iter()
                    .position(|r| inside(*r, m.column, m.row))
                {
                    self.activate(Target::Pane(side));
                    if let Some(Target::Row(i, _)) = target {
                        self.selected = i;
                    }
                }
            }
            if let Some(action) = self.selections[self.side].mouse(m) {
                return Self::selection_event(action);
            }
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((_, target)) = self
                        .hits
                        .iter()
                        .rev()
                        .find(|(r, _)| inside(*r, m.column, m.row))
                    {
                        return self.activate(*target);
                    }
                }
                MouseEventKind::ScrollUp => self.move_scroll(-3),
                MouseEventKind::ScrollDown => self.move_scroll(3),
                MouseEventKind::ScrollLeft => self.view.scroll_horizontal(self.side, -4),
                MouseEventKind::ScrollRight => self.view.scroll_horizontal(self.side, 4),
                _ => {}
            }
            return RevisionEvent::None;
        }
        self.view.cancel_scroll_drag();
        let Event::Key(key) = event else {
            return RevisionEvent::None;
        };
        if key.kind == crossterm::event::KeyEventKind::Release {
            return RevisionEvent::None;
        }
        let ctrl = key.modifiers.contains(M::CONTROL);
        let target = match key.code {
            K::Esc | K::Char('q') => Some(Target::Close),
            K::Char('n') => Some(Target::Change(1)),
            K::Char('p') => Some(Target::Change(-1)),
            K::Char('[') => Some(Target::File(-1)),
            K::Char(']') => Some(Target::File(1)),
            K::Char('L') if self.local_merge => {
                return self
                    .selected_change()
                    .map_or(RevisionEvent::None, RevisionEvent::Apply);
            }
            K::Char('A')
                if self.local_merge && self.local_apply_all && !self.changes.is_empty() =>
            {
                return RevisionEvent::ApplyAll;
            }
            K::Char('f') if !ctrl => Some(Target::FoldAll),
            K::Char('z') => Some(Target::Zoom),
            K::Tab => Some(Target::Pane((self.side + 1) % self.pane_count())),
            K::BackTab => Some(Target::Pane(
                (self.side + self.pane_count() - 1) % self.pane_count(),
            )),
            K::Enter | K::Char(' ') => Some(Target::Fold(self.selected)),
            K::Char('y') => return RevisionEvent::Copy(self.texts[self.side].clone()),
            K::Char('j') | K::Down => {
                self.move_scroll(1);
                None
            }
            K::Char('k') | K::Up => {
                self.move_scroll(-1);
                None
            }
            K::Char('h') | K::Left => {
                self.view.scroll_horizontal(self.side, -4);
                None
            }
            K::Char('l') | K::Right => {
                self.view.scroll_horizontal(self.side, 4);
                None
            }
            K::PageDown => {
                self.move_scroll(self.page as isize);
                None
            }
            K::PageUp => {
                self.move_scroll(-(self.page as isize));
                None
            }
            K::Char('f') if ctrl => {
                self.move_scroll(self.page as isize);
                None
            }
            K::Char('b') if ctrl => {
                self.move_scroll(-(self.page as isize));
                None
            }
            K::Char('d') if ctrl => {
                self.move_scroll((self.page / 2) as isize);
                None
            }
            K::Char('u') if ctrl => {
                self.move_scroll(-((self.page / 2) as isize));
                None
            }
            K::Char('g') | K::Home => {
                self.scroll = 0;
                self.follow = false;
                None
            }
            K::Char('G') | K::End => {
                self.scroll = self.total.saturating_sub(self.page);
                self.follow = false;
                None
            }
            _ => None,
        };
        target.map_or(RevisionEvent::None, |t| self.activate(t))
    }

    fn move_scroll(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.total.saturating_sub(self.page));
        self.follow = false;
    }

    fn selection_event(action: Action) -> RevisionEvent {
        match action {
            Action::Copy { text, .. } => RevisionEvent::Copy(text),
            _ => RevisionEvent::None,
        }
    }
}

#[cfg(test)]
mod tests;
