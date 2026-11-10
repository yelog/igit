//! The merge window owns the document viewport, controls, and read-only overlays.
mod highlight;
mod revision;
mod rows;
mod syntax;
pub(crate) use revision::{RevisionDiff, RevisionEvent};

use super::*;
use crate::ui::idea::{clipped, clipped_path};
use crossterm::event::MouseEvent;
use highlight::{side_tint, HighlightCache};
use ratatui::{
    style::Stylize,
    widgets::{Block, Clear},
};
use rows::{Columns, RowMap, Rows, EDGE, EDITOR_BG};
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeSet;

pub(super) use rows::char_at_cell;

const CHROME: Color = Color::Rgb(19, 20, 22);
const TOOLBAR: Color = Color::Rgb(25, 26, 29);
const BUTTON: Color = Color::Rgb(38, 39, 43);
const PRIMARY: Color = Color::Rgb(43, 88, 218);
const SCROLLBAR: Color = Color::Rgb(52, 53, 55);

#[derive(Clone, Copy)]
pub(super) enum Version {
    Left,
    Base,
    Result,
    Right,
}

impl Version {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Base => "Base",
            Self::Result => "Result",
            Self::Right => "Right",
        }
    }

    fn text(self, chunk: &Chunk) -> &str {
        match self {
            Self::Left => &chunk.left,
            Self::Base => &chunk.base,
            Self::Result => &chunk.result,
            Self::Right => &chunk.right,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Setting {
    Numbers,
    Guides,
    Whitespace,
    InnerDiff,
    Zoom,
}

pub(super) enum Overlay {
    Compare,
    Settings,
    Inspect {
        title: String,
        lines: Vec<Line<'static>>,
        top: usize,
        left: u16,
    },
}

const COMPARISONS: [(Version, Version); 6] = [
    (Version::Left, Version::Base),
    (Version::Base, Version::Right),
    (Version::Left, Version::Right),
    (Version::Left, Version::Result),
    (Version::Base, Version::Result),
    (Version::Result, Version::Right),
];

impl Overlay {
    pub fn details(doc: &Document, side: FileSide) -> Self {
        Self::Inspect {
            title: format!("Changes from {}", doc.source_name(side)),
            lines: doc
                .source_details(side)
                .into_iter()
                .map(|s| Line::from(clean(&s)))
                .collect(),
            top: 0,
            left: 0,
        }
    }

    pub fn compare(doc: &Document, a: Version, b: Version) -> Self {
        let old: String = doc.chunks.iter().map(|c| a.text(c)).collect();
        let new: String = doc.chunks.iter().map(|c| b.text(c)).collect();
        let diff = TextDiff::configure()
            .timeout(std::time::Duration::from_millis(500))
            .diff_lines(&old, &new);
        let lines = if diff.ratio() == 1.0 {
            vec![Line::from("No differences")]
        } else {
            diff.unified_diff()
                .context_radius(4)
                .iter_hunks()
                .flat_map(|hunk| {
                    let mut lines = vec![Line::styled(
                        hunk.header().to_string().trim_end().to_owned(),
                        style(ACCENT),
                    )];
                    lines.extend(hunk.iter_changes().map(|change| {
                        let (sign, color) = match change.tag() {
                            ChangeTag::Delete => ('-', crate::ui::idea::RED),
                            ChangeTag::Insert => ('+', GREEN),
                            ChangeTag::Equal => (' ', FG),
                        };
                        Line::styled(
                            format!(
                                "{sign}{}",
                                clean(change.value().trim_end_matches(['\n', '\r']))
                            ),
                            style(color),
                        )
                    }));
                    lines
                })
                .collect()
        };
        Self::Inspect {
            title: format!("Compare {} and {}", a.name(), b.name()),
            lines,
            top: 0,
            left: 0,
        }
    }
}

pub(super) struct EditorView {
    pub folding: bool,
    pub expanded: BTreeSet<usize>,
    pub hscroll: [usize; 3],
    limits: [usize; 3],
    numbers: bool,
    guides: bool,
    whitespace: bool,
    inner_diff: bool,
    highlights: HighlightCache,
    pub overlay: Option<Overlay>,
    menu_index: usize,
    menu_hits: Vec<Hit>,
    overlay_page: usize,
    tracks: Vec<Track>,
    drag: Option<ScrollDrag>,
}

impl Default for EditorView {
    fn default() -> Self {
        Self {
            folding: true,
            expanded: BTreeSet::new(),
            hscroll: [0; 3],
            limits: [0; 3],
            numbers: true,
            guides: true,
            whitespace: false,
            inner_diff: true,
            highlights: HighlightCache::default(),
            overlay: None,
            menu_index: 0,
            menu_hits: Vec::new(),
            overlay_page: 0,
            tracks: Vec::new(),
            drag: None,
        }
    }
}

impl EditorView {
    pub fn reset_document(&mut self) {
        self.highlights.clear();
        self.expanded.clear();
        self.hscroll = [0; 3];
        self.limits = [0; 3];
        self.drag = None;
        self.overlay = None;
    }

    pub fn open(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
        self.menu_index = 0;
        self.menu_hits.clear();
        self.drag = None;
    }

    pub fn toggle(&mut self, setting: Setting) {
        let value = match setting {
            Setting::Numbers => &mut self.numbers,
            Setting::Guides => &mut self.guides,
            Setting::Whitespace => &mut self.whitespace,
            Setting::InnerDiff => &mut self.inner_diff,
            Setting::Zoom => return,
        };
        *value = !*value;
    }

    pub fn toggle_fold(&mut self, chunk: usize) {
        if !self.expanded.remove(&chunk) {
            self.expanded.insert(chunk);
        }
    }

    pub fn scroll_horizontal(&mut self, side: usize, amount: isize) {
        self.hscroll[side] = self.hscroll[side]
            .saturating_add_signed(amount)
            .min(self.limits[side]);
    }

    pub fn scroll_mouse(&mut self, m: &MouseEvent, scroll: &mut usize, follow: &mut bool) -> bool {
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.drag = None;
                let Some(track) = self
                    .tracks
                    .iter()
                    .find(|t| inside(t.area, m.column, m.row))
                    .copied()
                else {
                    return false;
                };
                let pointer = track.pointer(m);
                let pos = track.position;
                let start = if (track.thumb..track.thumb + track.size).contains(&pointer) {
                    pos
                } else {
                    track.to_offset(pointer.saturating_sub(track.size / 2))
                };
                self.drag = Some(ScrollDrag {
                    track,
                    pointer,
                    start,
                });
                self.apply_scroll(track.axis, start, scroll, follow);
                true
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                let Some(drag) = self.drag else { return false };
                let delta = drag.track.pointer(m) as isize - drag.pointer as isize;
                let span = drag.track.travel().max(1) as isize;
                let next = drag
                    .start
                    .saturating_add_signed(delta * drag.track.limit as isize / span);
                self.apply_scroll(drag.track.axis, next.min(drag.track.limit), scroll, follow);
                if m.kind == MouseEventKind::Up(MouseButton::Left) {
                    self.drag = None;
                }
                true
            }
            _ => false,
        }
    }

    pub fn cancel_scroll_drag(&mut self) {
        self.drag = None;
    }

    fn apply_scroll(&mut self, axis: Scroll, offset: usize, scroll: &mut usize, follow: &mut bool) {
        match axis {
            Scroll::Vertical => {
                *scroll = offset;
                *follow = false;
            }
            Scroll::Horizontal(side) => self.hscroll[side] = offset,
        }
    }

    fn settings(&self, zoom: bool) -> [(Setting, &'static str, bool); 5] {
        [
            (Setting::Numbers, "Show line numbers", self.numbers),
            (Setting::Guides, "Show indent guides", self.guides),
            (Setting::Whitespace, "Show whitespaces", self.whitespace),
            (
                Setting::InnerDiff,
                "Highlight changed text",
                self.inner_diff,
            ),
            (Setting::Zoom, "Zoom focused pane", zoom),
        ]
    }

    pub fn overlay_event(&mut self, event: &Event) -> Option<Hit> {
        let overlay = self.overlay.as_mut()?;
        match event {
            Event::Key(k) if k.kind != crossterm::event::KeyEventKind::Release => match k.code {
                K::Esc => Some(Hit::Dismiss),
                K::Enter if !matches!(overlay, Overlay::Inspect { .. }) => {
                    self.menu_hits.get(self.menu_index).copied()
                }
                K::Up
                | K::Down
                | K::Char('j' | 'k')
                | K::PageDown
                | K::PageUp
                | K::Home
                | K::End => {
                    let amount = match k.code {
                        K::Up | K::Char('k') => -1,
                        K::Down | K::Char('j') => 1,
                        K::PageUp => -(self.overlay_page.max(1) as isize),
                        K::PageDown => self.overlay_page.max(1) as isize,
                        K::Home => isize::MIN,
                        _ => isize::MAX,
                    };
                    self.overlay_scroll(amount);
                    None
                }
                K::Left | K::Right => {
                    if let Overlay::Inspect { left, .. } = overlay {
                        *left = if k.code == K::Left {
                            left.saturating_sub(4)
                        } else {
                            left.saturating_add(4)
                        };
                    }
                    None
                }
                _ => None,
            },
            Event::Mouse(m)
                if matches!(
                    m.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) =>
            {
                self.overlay_scroll(if m.kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                });
                None
            }
            _ => None,
        }
    }

    fn overlay_scroll(&mut self, amount: isize) {
        if let Some(Overlay::Inspect { lines, top, .. }) = &mut self.overlay {
            *top = top
                .saturating_add_signed(amount)
                .min(lines.len().saturating_sub(self.overlay_page));
        } else {
            self.menu_index = self
                .menu_index
                .saturating_add_signed(amount)
                .min(self.menu_hits.len().saturating_sub(1));
        }
    }

    fn draw_overlay(&mut self, f: &mut Frame, area: Rect, zoom: bool, hits: &mut Vec<(Rect, Hit)>) {
        hits.clear();
        let settings = self.settings(zoom);
        self.menu_hits.clear();
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let (title, menu) = match overlay {
            Overlay::Settings => (
                "Editor Settings",
                settings
                    .into_iter()
                    .map(|(s, title, on)| {
                        (
                            format!("{} {title}", if on { "[x]" } else { "[ ]" }),
                            Hit::Setting(s),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            Overlay::Compare => (
                "Compare Contents",
                COMPARISONS
                    .into_iter()
                    .map(|(a, b)| (format!("{} and {}", a.name(), b.name()), Hit::Compare(a, b)))
                    .collect(),
            ),
            Overlay::Inspect {
                title,
                lines,
                top,
                left,
            } => {
                let width = area.width.saturating_sub(4).min(136);
                let height = area.height.saturating_sub(4);
                let r = rect(area.x + (area.width - width) / 2, area.y + 2, width, height);
                let inside = overlay_frame(f, r, title);
                self.overlay_page = usize::from(inside.height.saturating_sub(1));
                *top = (*top).min(lines.len().saturating_sub(self.overlay_page));
                let visible = lines
                    .iter()
                    .skip(*top)
                    .take(self.overlay_page)
                    .cloned()
                    .collect::<Vec<_>>();
                f.render_widget(
                    Paragraph::new(visible).scroll((0, *left)),
                    rect(inside.x, inside.y, inside.width, self.overlay_page as u16),
                );
                let bar = rect(inside.x, inside.bottom().saturating_sub(1), inside.width, 1);
                control(f, bar, " Close ", Hit::Dismiss, true, Control::Button, hits);
                return;
            }
        };
        let width = area.width.min(31);
        let height = (menu.len() as u16 + 2).min(area.height.saturating_sub(2));
        let x = if matches!(overlay, Overlay::Settings) {
            area.right() - width
        } else {
            area.x
        };
        let r = rect(x, area.y + 2, width, height);
        let inside = overlay_frame(f, r, title);
        for (i, (label, hit)) in menu.into_iter().enumerate() {
            self.menu_hits.push(hit);
            if i >= inside.height as usize {
                break;
            }
            let r = rect(inside.x, inside.y + i as u16, inside.width, 1);
            let bg = if i == self.menu_index {
                crate::ui::SEL
            } else {
                CHROME
            };
            put(f, r, format!(" {label}"), style(FG).bg(bg));
            hits.push((r, hit));
        }
        // Clicks outside a toolbar menu dismiss it without reaching the editor.
        hits.insert(0, (area, Hit::Dismiss));
    }
}

fn overlay_frame(f: &mut Frame, area: Rect, title: &str) -> Rect {
    f.render_widget(Clear, area);
    let block = pane(clipped(title, area.width.saturating_sub(4) as usize), true).bg(CHROME);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\t' {
                ' '
            } else if c.is_control() {
                '�'
            } else {
                c
            }
        })
        .collect()
}

fn fill(f: &mut Frame, area: Rect, bg: Color) {
    if !area.is_empty() {
        f.render_widget(Block::default().bg(bg), area);
    }
}

#[derive(Clone, Copy)]
enum Control {
    Tool,
    Toggled,
    Link,
    Button,
    Accent,
    Primary,
}

fn control(
    f: &mut Frame,
    area: Rect,
    label: &str,
    hit: Hit,
    enabled: bool,
    kind: Control,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let w = label.width() as u16;
    if w == 0 || w > area.width {
        return;
    }
    let r = rect(area.x, area.y, w, 1);
    let s = if enabled {
        match kind {
            Control::Tool => style(FG),
            Control::Toggled => style(FG).bg(BUTTON).bold(),
            Control::Link => style(ACCENT),
            Control::Button => style(FG).bg(BUTTON),
            Control::Accent => style(ACCENT).bg(BUTTON).bold(),
            Control::Primary => style(Color::White).bg(PRIMARY).bold(),
        }
    } else {
        style(MUTED).dim()
    };
    put(f, r, label, s);
    if enabled {
        hits.push((r, hit));
    }
}

fn left(area: &mut Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let r = rect(area.x, area.y, width, 1);
    area.x += width;
    area.width -= width;
    r
}

fn right(area: &mut Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    area.width -= width;
    rect(area.right(), area.y, width, 1)
}

fn tool(
    f: &mut Frame,
    bar: &mut Rect,
    label: &str,
    hit: Hit,
    on: bool,
    active: bool,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let width = label.width() as u16;
    if width <= bar.width {
        control(
            f,
            left(bar, width),
            label,
            hit,
            on,
            if active {
                Control::Toggled
            } else {
                Control::Tool
            },
            hits,
        );
    }
}

#[derive(Clone, Copy)]
enum Scroll {
    Vertical,
    Horizontal(usize),
}

#[derive(Clone, Copy)]
struct Track {
    axis: Scroll,
    area: Rect,
    position: usize,
    limit: usize,
    thumb: usize,
    size: usize,
}

#[derive(Clone, Copy)]
struct ScrollDrag {
    track: Track,
    pointer: usize,
    start: usize,
}

impl Track {
    fn new(axis: Scroll, area: Rect, content: usize, page: usize, pos: usize) -> Option<Self> {
        let len = match axis {
            Scroll::Vertical => area.height,
            Scroll::Horizontal(_) => area.width,
        } as usize;
        if len == 0 || page >= content {
            return None;
        }
        let limit = content - page;
        let size = (len * page / content).clamp(1, len);
        let thumb = (len - size) * pos / limit;
        Some(Self {
            axis,
            area,
            position: pos,
            limit,
            thumb,
            size,
        })
    }

    fn travel(self) -> usize {
        (match self.axis {
            Scroll::Vertical => self.area.height,
            Scroll::Horizontal(_) => self.area.width,
        } as usize)
            .saturating_sub(self.size)
    }

    fn to_offset(self, pointer: usize) -> usize {
        let span = self.travel().max(1);
        pointer.min(span) * self.limit / span
    }

    fn pointer(self, m: &MouseEvent) -> usize {
        (match self.axis {
            Scroll::Vertical => m.row.saturating_sub(self.area.y),
            Scroll::Horizontal(_) => m.column.saturating_sub(self.area.x),
        }) as usize
    }

    fn paint(self, f: &mut Frame, ascii: bool) {
        let (r, glyph) = match self.axis {
            Scroll::Vertical => (
                rect(
                    self.area.x,
                    self.area.y + self.thumb as u16,
                    1,
                    self.size as u16,
                ),
                if ascii { "|" } else { "▎" },
            ),
            Scroll::Horizontal(_) => (
                rect(
                    self.area.x + self.thumb as u16,
                    self.area.y,
                    self.size as u16,
                    1,
                ),
                if ascii { "=" } else { "━" },
            ),
        };
        if matches!(self.axis, Scroll::Vertical) {
            for y in r.y..r.bottom() {
                put(f, rect(r.x, y, 1, 1), glyph, style(SCROLLBAR));
            }
        } else {
            put(f, r, glyph.repeat(self.size), style(SCROLLBAR));
        }
    }
}

impl MergePanel {
    pub(crate) fn draw_editor(&mut self, f: &mut Frame, area: Rect, ctx: &Context, notice: &str) {
        self.panes.begin();
        self.hits.clear();
        self.rows_total = 0;
        self.rows_height = 0;
        self.view.tracks.clear();
        if area.width < 24 || area.height < 7 {
            for side in Self::SIDES {
                self.panes.add(side, Rect::default());
            }
            return;
        }
        let allowed = ctx.scope.contains(&self.doc.as_ref().unwrap().file.root);
        let editing = self.edit.is_some();
        let has_notice = !notice.is_empty() && area.height >= 9;
        let ascii = self.icons == IconMode::Ascii;
        fill(f, area, EDITOR_BG);
        let [title, toolbar, body, footer, status] = Layout::vertical([
            C::Length(1),
            C::Length(1),
            C::Min(4),
            C::Length(1),
            C::Length(u16::from(has_notice)),
        ])
        .areas(area);
        draw_toolbar(f, toolbar, self, editing);
        draw_footer(f, footer, self, allowed);
        let d = self.doc.as_ref().unwrap();
        let mut title_row = title;
        fill(f, title, CHROME);
        let back = format!(" {} ", self.icons.glyph(Icon::Left));
        control(
            f,
            left(&mut title_row, back.width() as u16),
            &back,
            Hit::List,
            !editing,
            Control::Tool,
            &mut self.hits,
        );
        let full_path = clean(
            &ctx.repos[d.file.root]
                .path
                .join(&d.file.path)
                .to_string_lossy(),
        );
        let caption = "Merge Revisions for ";
        let path_width = usize::from(title_row.width).saturating_sub(caption.width() + 1);
        put(
            f,
            title_row,
            format!(" {caption}{}", clipped_path(full_path, path_width)),
            style(MUTED).bg(CHROME).bold(),
        );
        let compact = area.width < 72;
        let split = !compact && !self.zoom && !self.panes.maximized;
        let cols = if split {
            let side_width = (u32::from(body.width) * 32 / 100) as u16;
            let [left, rest] = self.panes.divide(
                SplitId::MergeCurrent,
                body,
                Axis::Width,
                side_width,
                [20, 40],
            );
            let [result, right] = self.panes.divide(
                SplitId::MergeIncoming,
                rest,
                Axis::Width,
                rest.width.saturating_sub(side_width),
                [20, 20],
            );
            for (id, x) in [
                (SplitId::MergeCurrent, result.x),
                (SplitId::MergeIncoming, right.x),
            ] {
                self.panes
                    .handle(id, rect(x, body.y + 1, 1, body.height.saturating_sub(1)));
                for y in body.y + 1..body.bottom() {
                    put(
                        f,
                        rect(x, y, 1, 1),
                        if ascii { "|" } else { "│" },
                        style(EDGE),
                    );
                }
            }
            [left, result, right]
        } else {
            let mut cols = [Rect::default(); 3];
            cols[self.side] = body;
            cols
        };
        for (pane, col) in Self::SIDES.into_iter().zip(cols) {
            self.panes.add(pane, col);
        }

        let edit_text = self
            .edit
            .as_ref()
            .map(|edit| edit.chars.iter().collect::<String>());
        let caret = self.edit.as_ref().map(Editor::rowcol);
        let map = RowMap::new(
            &d.chunks,
            &self.view,
            edit_text
                .as_deref()
                .zip(caret)
                .map(|(s, (row, _))| (self.idx, s, row)),
        );
        let height = usize::from(body.height.saturating_sub(3));
        self.rows_height = height;
        self.rows_total = map.total;
        let focus = &map.sections[self.idx];
        if let Some((row, _)) = caret {
            self.scroll = keep_visible(self.scroll, focus.first + row, 1, height);
        } else if self.follow {
            self.scroll = keep_visible(self.scroll, focus.first, focus.height, height);
        }
        self.follow = false;
        self.scroll = self.scroll.min(self.rows_total.saturating_sub(height));

        if has_notice {
            put(
                f,
                status,
                clipped(notice, status.width as usize),
                style(ACCENT).bg(CHROME),
            );
        }
        // Toolbar and footer only update hit targets. Keep the borrowed texts
        // while drawing the visible document rows.
        let mut tracks = Vec::new();
        for (s, col) in cols.into_iter().enumerate().filter(|(_, r)| !r.is_empty()) {
            let number_width = if self.view.numbers {
                map.numbers[s].max(1).ilog10() as u16 + 1
            } else {
                0
            };
            let metrics = Columns::new(s, col, number_width, split);
            draw_heading(f, col, s, d, self.icons, !editing, &mut self.hits);
            let code_width = usize::from(metrics.code.width);
            self.view.limits[s] = map.widths[s].saturating_sub(code_width);
            if s == 1 {
                if let Some((row, ch)) = caret {
                    let raw = focus.lines[1].get(row).copied().map_or("", rows::line_body);
                    let prefix = raw.chars().take(ch).collect::<String>();
                    let x = rows::cell_width(&prefix);
                    // Leave room for a caret immediately after a full line.
                    self.view.limits[s] =
                        self.view.limits[s].max(x.saturating_add(1).saturating_sub(code_width));
                    self.view.hscroll[s] = keep_visible(self.view.hscroll[s], x, 1, code_width);
                }
            }
            self.view.hscroll[s] = self.view.hscroll[s].min(self.view.limits[s]);
            Rows {
                chunks: &d.chunks,
                read_only: false,
                file: &d.file.label(),
                view: &self.view,
                map: &map,
                selected: self.idx,
                side: self.side,
                scroll: self.scroll,
                caret,
                active: self.panes.active && self.view.overlay.is_none(),
                ascii,
            }
            .draw(f, s, col, metrics, &mut self.hits);

            if let Some(track) = Track::new(
                Scroll::Vertical,
                metrics.rail,
                map.total,
                height,
                self.scroll,
            ) {
                track.paint(f, ascii);
                tracks.push(track);
            }
            for (chunk, section) in d.chunks.iter().zip(&map.sections) {
                let tint = FileSide::ALL
                    .into_iter()
                    .filter(|side| s == 1 || side.pane() == s)
                    .find_map(|side| side_tint(chunk, side));
                if let Some(t) = tint {
                    let row = (height.saturating_sub(1) * section.first / map.total.max(1)) as u16;
                    put(
                        f,
                        rect(metrics.rail.x, metrics.rail.y + row, 1, 1),
                        if ascii { "-" } else { "╴" },
                        style(t.marker),
                    );
                }
            }
            if let Some(track) = Track::new(
                Scroll::Horizontal(s),
                metrics.scrollbar,
                code_width + self.view.limits[s],
                code_width,
                self.view.hscroll[s],
            ) {
                track.paint(f, ascii);
                tracks.push(track);
            }
        }
        self.view.tracks = tracks;
        if self.view.overlay.is_some() {
            self.view.draw_overlay(f, area, self.zoom, &mut self.hits);
        }
    }
}

fn keep_visible(scroll: usize, start: usize, n: usize, page: usize) -> usize {
    if page == 0 {
        return scroll;
    }
    if start < scroll || start >= scroll + page {
        start.saturating_sub(2.min(page - 1))
    } else if n <= page && start + n > scroll + page {
        start + n - page
    } else {
        scroll
    }
}

fn draw_toolbar(f: &mut Frame, mut bar: Rect, panel: &mut MergePanel, editing: bool) {
    let d = panel.doc.as_ref().unwrap();
    let width = bar.width;
    let ascii = panel.icons == IconMode::Ascii;
    let n_changes = d
        .chunks
        .iter()
        .filter(|c| c.kind == Kind::Safe && !c.done())
        .count();
    let n_conflicts = d
        .chunks
        .iter()
        .filter(|c| c.kind == Kind::Conflict && !c.done())
        .count();
    let can_jump = |before| {
        d.chunks.iter().enumerate().any(|(i, c)| {
            c.kind != Kind::Context
                && !c.done()
                && if before { i < panel.idx } else { i > panel.idx }
        })
    };
    fill(f, bar, TOOLBAR);
    let gear = if ascii { " # " } else { " ⚙ " };
    let settings = right(&mut bar, gear.width() as u16);
    control(
        f,
        settings,
        gear,
        Hit::Settings,
        !editing,
        Control::Tool,
        &mut panel.hits,
    );
    let counts = if width < 44 {
        format!(" {n_changes}/{n_conflicts} ")
    } else if width < 100 {
        format!(" {n_changes} chg,{n_conflicts} cf ")
    } else {
        let plural = |n| if n == 1 { "" } else { "s" };
        format!(
            " {n_changes} change{}, {n_conflicts} conflict{} ",
            plural(n_changes),
            plural(n_conflicts)
        )
    };
    put(
        f,
        right(&mut bar, counts.width() as u16),
        counts,
        style(FG).bold(),
    );
    // Reserve readable automatic resolution and Undo before optional controls.
    // Shortcuts remain available when a narrow toolbar omits the other tools.
    let automatic = d
        .chunks
        .iter()
        .filter(|c| c.automatic_result().is_some())
        .count();
    let icon = panel.icons.glyph(Icon::AutoResolve);
    let shortcut = panel.key_label("m");
    let undo = if ascii { " U " } else { " ↶ " };
    let compare = if ascii { " <> " } else { " ⇄ " };
    let navigation_width = 6 + compare.width() + 3;
    let minimum_tools = if width >= 72 {
        navigation_width + 9
    } else if width >= 44 {
        6
    } else {
        0
    };
    let budget = usize::from(bar.width).saturating_sub(undo.width() + 1 + minimum_tools);
    let automatic_label = [
        format!(" {icon} Resolve Automatically ({automatic}) [{shortcut}] "),
        format!(" {icon} Auto ({automatic}) [{shortcut}] "),
        format!(" Auto {automatic} [{shortcut}] "),
        format!(" Auto {automatic} "),
        " Auto ".into(),
    ]
    .into_iter()
    .find(|label| label.width() <= budget)
    .unwrap_or_default();
    let reserved = (automatic_label.width() + undo.width() + 1) as u16;
    let mut buttons = rect(bar.x, bar.y, bar.width.saturating_sub(reserved), 1);
    let source_labels = if ascii {
        [" >> Left ", " All ", " << Right "]
    } else {
        [" » Left ", " »« All ", " « Right "]
    };
    let source_width = source_labels.iter().map(|s| s.width()).sum::<usize>();
    let long_sources = usize::from(buttons.width) >= navigation_width + source_width;
    let caption = " Apply non-conflicting changes: ";
    let full = usize::from(buttons.width) >= navigation_width + source_width + caption.width();
    let mut add = |label, hit, on, active| {
        tool(
            f,
            &mut buttons,
            label,
            hit,
            on && !editing,
            active,
            &mut panel.hits,
        )
    };
    add(
        if ascii { " ^ " } else { " ↑ " },
        Hit::Navigate(-1),
        can_jump(true),
        false,
    );
    add(
        if ascii { " v " } else { " ↓ " },
        Hit::Navigate(1),
        can_jump(false),
        false,
    );
    if width >= 72 {
        add(compare, Hit::CompareMenu, true, false);
        add(
            if ascii { " = " } else { " ⋈ " },
            Hit::FoldAll,
            true,
            panel.view.folding,
        );
    }
    if full {
        add(caption, Hit::Auto(None), false, false);
    }
    for (side, short, long) in [
        (
            Some(FileSide::Current),
            if ascii { " > " } else { " » " },
            source_labels[0],
        ),
        (None, if ascii { " A " } else { " ⇆ " }, source_labels[1]),
        (
            Some(FileSide::Incoming),
            if ascii { " < " } else { " « " },
            source_labels[2],
        ),
    ] {
        let on = d.chunks.iter().any(|c| {
            c.kind == Kind::Safe
                && match side {
                    Some(side) => c.pending(side),
                    None => !c.done(),
                }
        });
        add(
            if long_sources { long } else { short },
            Hit::Auto(side),
            on,
            false,
        );
    }
    if width >= 90 && !panel.panes.maximized && panel.zoom {
        add(" Zoom ", Hit::Zoom, true, true);
    }
    bar = rect(buttons.x, bar.y, bar.right() - buttons.x, 1);
    left(&mut bar, 1);
    control(
        f,
        left(&mut bar, automatic_label.width() as u16),
        &automatic_label,
        Hit::ResolveAutomatically,
        automatic > 0 && !editing,
        Control::Accent,
        &mut panel.hits,
    );
    tool(
        f,
        &mut bar,
        undo,
        Hit::Undo,
        !panel.hist.is_empty() && !editing,
        false,
        &mut panel.hits,
    );
}

fn draw_heading(
    f: &mut Frame,
    area: Rect,
    pane: usize,
    doc: &Document,
    icons: IconMode,
    on: bool,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let mut row = rect(area.x, area.y, area.width, 1);
    let s = style(FG).bold();
    if let Some(source) = FileSide::ALL.into_iter().find(|s| s.pane() == pane) {
        let link = if area.width >= 31 {
            " Show Details "
        } else {
            " Details "
        };
        let at = right(&mut row, link.width() as u16);
        control(f, at, link, Hit::Details(source), on, Control::Link, hits);
        let lock = icons.glyph(Icon::Locked);
        let name = clean(doc.source_name(source));
        let label = if row.width >= 17 {
            format!(" {lock} Changes from {name}")
        } else {
            format!(" {lock} {name}")
        };
        put(f, row, clipped(&label, row.width as usize), s);
    } else {
        let file = clean(&path_label(
            doc.file
                .path
                .file_name()
                .map(std::path::Path::new)
                .unwrap_or(&doc.file.path),
        ));
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Result ", s),
                Span::styled(
                    clipped(&file, row.width.saturating_sub(8) as usize),
                    style(MUTED),
                ),
            ])),
            row,
        );
    }
    let rule = if icons == IconMode::Ascii { "-" } else { "─" };
    put(
        f,
        rect(area.x, area.y + 1, area.width, 1),
        rule.repeat(usize::from(area.width)),
        style(EDGE),
    );
}

fn draw_footer(f: &mut Frame, mut bar: Rect, panel: &mut MergePanel, allowed: bool) {
    let editing = panel.edit.is_some();
    let compact = bar.width < 66;
    fill(f, bar, CHROME);
    if bar.width >= 40 {
        left(&mut bar, 1);
        right(&mut bar, 1);
    }
    let mut right_button = |label: &str, hit, on, kind| {
        let w = label.width() as u16;
        if w <= bar.width {
            control(f, right(&mut bar, w), label, hit, on, kind, &mut panel.hits);
            right(&mut bar, 1);
        }
    };
    right_button(
        if compact {
            " Apply "
        } else {
            " Apply Changes "
        },
        Hit::Apply,
        allowed,
        Control::Primary,
    );
    right_button(
        if compact {
            " Save & Close "
        } else {
            " Save and Close "
        },
        Hit::Save,
        true,
        Control::Button,
    );
    let mut button = |label: &str, hit, on| {
        let w = label.width() as u16;
        if w <= bar.width {
            control(
                f,
                left(&mut bar, w),
                label,
                hit,
                on,
                Control::Button,
                &mut panel.hits,
            );
            left(&mut bar, 1);
        }
    };
    if editing {
        button(
            if compact { " Done " } else { " Accept Edit " },
            Hit::OkEdit,
            true,
        );
        button(" Cancel ", Hit::CancelEdit, true);
    } else {
        button(
            if compact { " Left " } else { " Accept Left " },
            Hit::UseFile(FileSide::Current),
            true,
        );
        button(
            if compact { " Right " } else { " Accept Right " },
            Hit::UseFile(FileSide::Incoming),
            true,
        );
    }
}
