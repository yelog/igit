//! A historical comparison has no writable Document, stage state, or apply path.
use super::*;
use crate::ui::selection::{TextLayout, TextSelection};
use std::ops::Range;

pub(crate) enum RevisionEvent {
    None,
    Close,
    File(isize),
    Copy(String),
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
    total: usize,
    page: usize,
    hits: Vec<(Rect, Target)>,
    selections: [TextSelection; 3],
    code_areas: [Rect; 3],
    line_ranges: [Vec<Range<usize>>; 3],
}

impl RevisionDiff {
    pub(crate) fn new(
        name: String,
        revisions: [String; 3],
        texts: [String; 3],
        annotations: [String; 3],
    ) -> Self {
        // Use the committed result as the alignment axis. A disagreement with
        // both parents is an ordinary historical change, not a pending conflict.
        let chunks = diff3(&texts[1], &texts[0], &texts[2]);
        let changes: Vec<_> = chunks
            .iter()
            .enumerate()
            .filter_map(|(i, c)| (c.left != c.result || c.right != c.result).then_some(i))
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
            total: 0,
            page: 0,
            hits: Vec::new(),
            view: EditorView::default(),
            selections: Default::default(),
            code_areas: Default::default(),
            line_ranges,
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

    pub(crate) fn is_three_way(&self) -> bool {
        self.three_way
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selections[self.side].selected_text()
    }

    fn pane_count(&self) -> usize {
        if self.three_way {
            3
        } else {
            2
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
        self.hits.clear();
        self.code_areas.fill(Rect::default());
        self.view.tracks.clear();
        f.render_widget(Clear, area);
        fill(f, area, EDITOR_BG);
        if area.width < 16 || area.height < 6 {
            put(f, area, "Diff · Esc Close", style(FG));
            return;
        }
        let ascii = icons == IconMode::Ascii;
        let [title, toolbar, body, footer] =
            Layout::vertical([C::Length(1), C::Length(1), C::Min(3), C::Length(1)]).areas(area);
        fill(f, title, CHROME);
        let mut title_space = title;
        let close = right(&mut title_space, 7.min(title.width));
        put(f, close, " Close ", style(MUTED).bg(CHROME));
        self.hits.push((close, Target::Close));
        put(
            f,
            title_space,
            clipped(
                &format!(" Repository Diff: {}", self.name),
                title_space.width as usize,
            ),
            style(FG).bg(CHROME),
        );

        fill(f, toolbar, TOOLBAR);
        let mut tools = toolbar;
        let current = self.changes.iter().position(|&i| i == self.selected);
        let summary = if self.three_way {
            format!(" {} changes, No conflicts ", self.changes.len())
        } else {
            format!(
                " {} difference{} ",
                self.changes.len(),
                if self.changes.len() == 1 { "" } else { "s" }
            )
        };
        if tools.width >= 70 {
            let status = right(&mut tools, summary.width() as u16);
            put(f, status, summary, style(MUTED).bg(TOOLBAR));
        }
        self.button(
            f,
            &mut tools,
            if ascii { " ^ " } else { " ↑ " },
            Target::Change(-1),
            current != Some(0) && !self.changes.is_empty(),
        );
        self.button(
            f,
            &mut tools,
            if ascii { " v " } else { " ↓ " },
            Target::Change(1),
            current != self.changes.len().checked_sub(1) && !self.changes.is_empty(),
        );
        self.button(f, &mut tools, " < ", Target::File(-1), file > 0);
        let counter = format!("{}/{} files", file + 1, count);
        if tools.width >= counter.width() as u16 {
            let at = left(&mut tools, counter.width() as u16);
            put(f, at, counter, style(ACCENT).bg(TOOLBAR));
        }
        self.button(f, &mut tools, " > ", Target::File(1), file + 1 < count);
        self.button(
            f,
            &mut tools,
            if self.view.folding {
                " Unfold "
            } else {
                " Fold "
            },
            Target::FoldAll,
            true,
        );
        self.button(
            f,
            &mut tools,
            if self.zoom { " Split " } else { " Zoom " },
            Target::Zoom,
            true,
        );

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
        let map = RowMap::new(&self.chunks, &self.view, None);
        self.total = map.total;
        self.page = body.height.saturating_sub(3) as usize;
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
        self.scroll = self.scroll.min(self.total.saturating_sub(self.page));
        for (pane, col) in cols.into_iter().enumerate().filter(|(_, c)| !c.is_empty()) {
            let role = if self.three_way {
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
            let caption = format!(" {} {}  {role}", icons.glyph(Icon::Locked), short);
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
            put(
                f,
                rect(col.x, col.y + 1, col.width, 1),
                clipped(&format!(" {}", self.annotations[pane]), col.width as usize),
                style(MUTED),
            );
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
            let metrics = Columns::read_only(pane, col, numbers, split);
            let width = metrics.code.width as usize;
            self.view.limits[pane] = map.widths[pane].saturating_sub(width);
            self.view.hscroll[pane] = self.view.hscroll[pane].min(self.view.limits[pane]);
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
            let projected = map
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
                            let range = self.line_ranges[pane]
                                .get(section.numbers[pane] + line)
                                .cloned()
                                .unwrap_or(self.texts[pane].len()..self.texts[pane].len());
                            (
                                if decoration {
                                    range.start..range.start
                                } else {
                                    range
                                },
                                decoration,
                            )
                        })
                });
            let layout = TextLayout::projected(&self.texts[pane], projected);
            self.selections[pane].overlay(f, metrics.code, layout, self.view.hscroll[pane]);
            self.hits
                .extend(row_hits.into_iter().filter_map(|(r, hit)| match hit {
                    Hit::Select(i, side) => Some((r, Target::Row(i, side))),
                    Hit::Fold(i) => Some((r, Target::Fold(i))),
                    _ => None,
                }));
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
            for &i in &self.changes {
                let row =
                    (self.page.saturating_sub(1) * map.sections[i].first / map.total.max(1)) as u16;
                put(
                    f,
                    rect(metrics.rail.x, metrics.rail.y + row, 1, 1),
                    if ascii { "-" } else { "╴" },
                    style(ACCENT),
                );
            }
        }
        fill(f, footer, CHROME);
        let hint = if footer.width >= 105 {
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

    fn activate(&mut self, target: Target) -> RevisionEvent {
        match target {
            Target::Close => return RevisionEvent::Close,
            Target::File(d) => return RevisionEvent::File(d),
            Target::Change(d) => {
                let next = if d > 0 {
                    self.changes.iter().copied().find(|&i| i > self.selected)
                } else {
                    self.changes
                        .iter()
                        .copied()
                        .rev()
                        .find(|&i| i < self.selected)
                };
                if let Some(next) = next {
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
            Target::Pane(side) => {
                if side != self.side {
                    self.selections[self.side].clear();
                }
                self.side = side;
            }
            Target::Row(i, side) => {
                if side != self.side {
                    self.selections[self.side].clear();
                }
                self.selected = i;
                self.side = side;
            }
        }
        RevisionEvent::None
    }

    pub(crate) fn event(&mut self, event: &Event) -> RevisionEvent {
        if matches!(event, Event::FocusLost) {
            self.selections.iter_mut().for_each(TextSelection::clear);
        }
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
