//! Cell geometry and colours for the IDEA Git tool window.
use super::*;
use display::Column;
use unicode_width::UnicodeWidthStr;

pub(crate) use crate::ui::idea::{clipped, repo_tint, BLUE, DIM, EDGE, INK, PANEL, SELECT};
pub(super) const GOLD: Color = Color::Rgb(238, 190, 85);
const REACHABLE: Color = Color::Rgb(32, 37, 58);
const INPUT: Color = Color::Rgb(35, 36, 43);
const TAB: Color = Color::Rgb(40, 56, 88);
const GREEN: Color = Color::Rgb(97, 168, 112);
const PURPLE: Color = Color::Rgb(176, 137, 222);

pub(super) fn fg(c: Color) -> Style {
    Style::default().fg(c)
}
pub(super) fn ref_color(kind: model::RefKind) -> Color {
    use model::RefKind::*;
    match kind {
        Head => GOLD,
        Local | Current => GREEN,
        Remote => PURPLE,
        Tag => Color::Rgb(220, 166, 94),
    }
}
fn lane(t: usize) -> Color {
    const C: [Color; 6] = [
        Color::Rgb(96, 164, 81),
        Color::Rgb(92, 124, 198),
        Color::Rgb(135, 111, 192),
        Color::Rgb(74, 165, 167),
        Color::Rgb(192, 114, 131),
        Color::Rgb(180, 152, 91),
    ];
    if t == 0 {
        Color::Rgb(184, 147, 77)
    } else {
        C[(t - 1) % C.len()]
    }
}
pub(super) fn boxof(title: String, focus: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(fg(if focus { BLUE } else { EDGE }))
        .style(fg(INK).bg(PANEL))
        .title(Span::styled(format!(" {title} "), fg(INK).bold()))
}
fn one(x: u16, y: u16, w: u16) -> Rect {
    Rect::new(x, y, w, 1)
}
fn inset_x(a: Rect, n: u16) -> Rect {
    Rect::new(
        a.x + n.min(a.width),
        a.y,
        a.width.saturating_sub(n),
        a.height,
    )
}
pub(super) fn take(a: &mut Rect, w: u16) -> Rect {
    let r = one(a.x, a.y, w.min(a.width));
    *a = inset_x(*a, r.width);
    r
}
fn take_right(a: &mut Rect, w: u16) -> Rect {
    let w = w.min(a.width);
    a.width -= w;
    one(a.right(), a.y, w)
}
pub(super) fn cells(text: &str) -> u16 {
    text.width().min(usize::from(u16::MAX)) as u16
}
pub(super) fn fold_slot(icons: IconMode, open: Option<bool>) -> String {
    let width = icons
        .disclosure(true)
        .width()
        .max(icons.disclosure(false).width());
    let mark = open.map_or("", |open| icons.disclosure(open));
    format!("{mark}{}", " ".repeat(width + 1 - mark.width()))
}
fn vline(f: &mut Frame, r: Rect) {
    f.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(fg(EDGE)),
        r,
    );
}
fn hline(f: &mut Frame, r: Rect) {
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(fg(EDGE)),
        r,
    );
}
// Reserve the close target before clipping or scrolling the input, so even a wide
// icon cannot cover its last character. Return the cells actually occupied by it.
pub(super) fn toolbar_field(
    f: &mut Frame,
    r: Rect,
    text: &str,
    editing: bool,
    icons: IconMode,
    clearable: bool,
) -> Option<Rect> {
    toolbar_field_with_suffix(f, r, text, editing, icons, clearable, 0)
}
fn toolbar_field_with_suffix(
    f: &mut Frame,
    r: Rect,
    text: &str,
    editing: bool,
    icons: IconMode,
    clearable: bool,
    suffix_width: u16,
) -> Option<Rect> {
    if r.is_empty() {
        return None;
    }
    let style = fg(if editing { INK } else { DIM }).bg(INPUT);
    f.render_widget(Paragraph::new("").style(style), r);
    let edge = if editing { BLUE } else { EDGE };
    let (left, right) = if icons == IconMode::Ascii {
        ("|", "|")
    } else {
        ("▏", "▕")
    };
    f.render_widget(Paragraph::new(left).style(fg(edge)), one(r.x, r.y, 1));
    if r.width < 3 {
        return None;
    }
    let mut area = one(r.x + 1, r.y, (r.width - 2).saturating_sub(suffix_width));
    let close = icons.glyph(Icon::Close);
    let clear = (clearable && area.width >= cells(close) + 3)
        .then(|| take_right(&mut area, cells(close) + 1));
    if let Some(clear) = clear {
        f.render_widget(Paragraph::new(close).right_aligned().style(style), clear);
    }
    let search = icons.glyph(Icon::Search);
    if area.width > cells(search) + 2 {
        let icon = take(&mut area, cells(search) + 1);
        f.render_widget(
            Paragraph::new(search).style(fg(if editing { BLUE } else { DIM })),
            icon,
        );
    }
    if editing {
        let value = format!("{text}{}", icons.glyph(Icon::Cursor));
        let shift = value
            .width()
            .saturating_sub(area.width as usize)
            .min(u16::MAX as usize) as u16;
        f.render_widget(
            Paragraph::new(value).style(fg(INK)).scroll((0, shift)),
            area,
        );
    } else {
        f.render_widget(
            Paragraph::new(clipped(text, area.width as usize)).style(fg(DIM)),
            area,
        );
    }
    f.render_widget(
        Paragraph::new(right).style(fg(edge)),
        one(r.right() - 1, r.y, 1),
    );
    clear
}
struct Badge {
    label: String,
    kinds: Vec<model::RefKind>,
    paired: bool,
}
impl Badge {
    fn fit(&self, width: usize) -> String {
        if self.label.width() <= width {
            return self.label.clone();
        }
        // With both markers visible the local name carries more information than "origin & …".
        let name = if self.paired {
            self.label
                .rsplit_once(" & ")
                .map_or(self.label.as_str(), |(_, v)| v)
        } else {
            self.label.as_str()
        };
        if name.width() <= width {
            return name.into();
        }
        if let Some((_, leaf)) = name.rsplit_once('/') {
            if width > 3 {
                return format!("…/{}", clipped(leaf, width - 2));
            }
        }
        clipped(name, width)
    }
}
fn decorations(rev: &model::Rev, hist: &History, options: &display::Options) -> Vec<Badge> {
    use model::RefKind::*;
    let mut refs: Vec<_> = rev.decorations().collect();
    refs.sort_by_key(|(k, _)| match k {
        Head | Current => 0,
        Local => 1,
        Tag => 2,
        Remote => 3,
    });
    let mut paired = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &(kind, name) in &refs {
        if kind == Remote && paired.contains(name) {
            continue;
        }
        let upstream = matches!(kind, Local | Current)
            .then(|| hist.remote_upstream(rev.root, name))
            .flatten()
            .filter(|s| refs.contains(&(Remote, *s)));
        let label = if kind == Tag && !options.tag_names {
            String::new()
        } else if let Some(u) = upstream {
            paired.insert(u);
            let remote = u
                .strip_suffix(name)
                .and_then(|s| s.strip_suffix('/'))
                .unwrap_or(u);
            format!("{remote} & {name}")
        } else {
            name.to_owned()
        };
        let mut kinds = Vec::with_capacity(3);
        if upstream.is_some() {
            kinds.push(Remote);
        }
        if kind == Current {
            // HEAD points to this local branch; neither reference replaces the other.
            kinds.extend([Local, Head]);
        } else {
            kinds.push(kind);
        }
        result.push(Badge {
            label,
            kinds,
            paired: upstream.is_some(),
        });
    }
    if options.compact_refs && result.len() > 1 {
        let hidden = result.len() - 1;
        result.truncate(1);
        result[0].label = format!("{} +{hidden}", result[0].label);
    }
    result
}
fn subject_spans(s: &str) -> Vec<Span<'_>> {
    let mut out = vec![];
    let mut left = s;
    while let Some(i) = left
        .find("https://")
        .into_iter()
        .chain(left.find("http://"))
        .min()
    {
        out.push(Span::raw(&left[..i]));
        let tail = &left[i..];
        let j = tail
            .find(|c: char| c.is_whitespace() || "'\"<>".contains(c))
            .unwrap_or(tail.len());
        out.push(Span::styled(&tail[..j], fg(BLUE).underlined()));
        left = &tail[j..];
    }
    out.push(Span::raw(left));
    out
}
impl LogPanel {
    pub(super) fn pane_visible(&self, pane: Pane) -> bool {
        match pane {
            Pane::Refs => self.show_refs,
            Pane::Graph => true,
            Pane::Files | Pane::Info => self.show_info,
        }
    }

    pub(super) fn cycle_focus(&mut self, d: i32) {
        for _ in 0..4 {
            self.focus = self.focus.delta(d);
            if self.pane_visible(self.focus) {
                break;
            }
        }
    }
    pub(super) fn resize(&mut self, split: Divider, x: u16, y: u16) {
        let a = self.bounds;
        let pct = |v: u16, n: u16| ((u32::from(v) * 100) / u32::from(n.max(1))) as u16;
        match split {
            Divider::Left => self.left = pct(x.saturating_sub(a.x), a.width).clamp(12, 34),
            Divider::Right => {
                self.right = 100u16
                    .saturating_sub(pct(x.saturating_sub(a.x), a.width))
                    .clamp(17, 38)
            }
            Divider::Inspector => {
                self.inspector_split = pct(y.saturating_sub(a.y), a.height).clamp(30, 82)
            }
        }
    }
    pub(super) fn button(&mut self, f: &mut Frame, r: Rect, label: &str, on: bool, h: Hit) {
        if r.is_empty() {
            return;
        }
        f.render_widget(
            Paragraph::new(clipped(label, r.width.into())).style(if on {
                fg(BLUE).bg(TAB)
            } else {
                fg(DIM)
            }),
            r,
        );
        self.hit(r, h)
    }
    pub(super) fn bar(&mut self, f: &mut Frame, a: Rect, buttons: &[(String, Hit)]) {
        let mut a = one(a.x, a.y, a.width);
        for (name, h) in buttons {
            let r = take(&mut a, cells(name).saturating_add(2));
            self.button(
                f,
                r,
                &format!(" {name} "),
                matches!(h,Hit::Pane(p) if *p==self.focus),
                h.clone(),
            );
        }
    }
    pub(super) fn draw_layout(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        f.render_widget(Block::default().style(fg(INK).bg(PANEL)), a);
        if !self.pane_visible(self.focus) {
            self.focus = Pane::Graph;
        }
        if a.width < 82 {
            let mut navigation = Rect::new(a.x, a.y, a.width, 1);
            self.view_button(f, &mut navigation);
            for (pane, label) in [
                (Pane::Refs, " Refs "),
                (Pane::Graph, " Graph "),
                (Pane::Files, " Files "),
                (Pane::Info, " Details "),
            ] {
                if self.pane_visible(pane) {
                    let tab = take(&mut navigation, cells(label));
                    self.button(f, tab, label, self.focus == pane, Hit::Pane(pane));
                }
            }
            self.areas.push((
                Rect::new(a.x, a.y + 1, a.width, a.height.saturating_sub(1)),
                self.focus,
            ));
        } else {
            let xx = |v: u16| a.x + (u32::from(a.width) * u32::from(v) / 100) as u16;
            let right = if self.show_info {
                xx(100 - self.right)
            } else {
                a.right()
            };
            let graph_x = if self.show_refs {
                let left = xx(self.left);
                self.areas
                    .push((Rect::new(a.x, a.y, left - a.x, a.height), Pane::Refs));
                let d = Rect::new(left, a.y, 1, a.height);
                vline(f, d);
                self.hit(d, Hit::Split(Divider::Left));
                d.right()
            } else {
                a.x
            };
            self.areas.push((
                Rect::new(graph_x, a.y, right - graph_x, a.height),
                Pane::Graph,
            ));
            if self.show_info {
                let d = Rect::new(right, a.y, 1, a.height);
                vline(f, d);
                self.hit(d, Hit::Split(Divider::Right));
                let split =
                    a.y + (u32::from(a.height) * u32::from(self.inspector_split) / 100) as u16;
                let w = a.right() - right - 1;
                let d = one(right + 1, split, w);
                hline(f, d);
                self.hit(d, Hit::Split(Divider::Inspector));
                self.areas
                    .push((Rect::new(right + 1, a.y, w, split - a.y), Pane::Files));
                self.areas.push((
                    Rect::new(
                        right + 1,
                        split + 1,
                        w,
                        a.bottom().saturating_sub(split + 1),
                    ),
                    Pane::Info,
                ));
            }
        }
        for (r, p) in self.areas.clone() {
            if !r.is_empty() {
                match p {
                    Pane::Refs => self.draw_refs(f, r, c),
                    Pane::Graph => self.draw_graph(f, r, c),
                    Pane::Files => self.draw_files(f, r, c),
                    Pane::Info => self.draw_info(f, r, c),
                }
            }
        }
    }
    fn graph_toolbar(&mut self, f: &mut Frame, area: Rect, c: &Context) -> u16 {
        let two = area.width < 56 && area.height >= 5;
        let mut a = one(area.x, area.y, area.width);
        if self.bounds.width >= 82 {
            self.view_button(f, &mut a);
        }
        let field_w = if two {
            a.width.saturating_sub(12)
        } else {
            (a.width / 4).clamp(13, 32)
        };
        let input = self
            .editor
            .as_ref()
            .filter(|(field, _)| *field == Field::Text);
        let editing = input.is_some();
        let show_options = two || area.width >= 77 || editing;
        let suffix_width = if show_options { 6 } else { 0 };
        let field = take(&mut a, field_w + suffix_width);
        let text = if let Some((_, s)) = input {
            s
        } else if self.filter.query.is_empty() {
            "Text or hash"
        } else {
            &self.filter.query
        };
        let clear = toolbar_field_with_suffix(
            f,
            field,
            text,
            editing,
            self.icons,
            !self.filter.query.is_empty() && !editing,
            suffix_width,
        );
        self.hit(field, Hit::Filter(Field::Text));
        if let Some(clear) = clear {
            self.hit(clear, Hit::Clear(Field::Text));
        }
        if show_options {
            // Keep both matching options inside the field's background and right edge.
            let mut inner = one(field.x + 1, field.y, field.width.saturating_sub(2));
            let mut suffix = take_right(&mut inner, suffix_width);
            let reg = take(&mut suffix, 3);
            self.button(f, reg, ".* ", self.regex, Hit::ToggleRegex);
            let case = take(&mut suffix, 3);
            self.button(f, case, "Cc ", self.case_sensitive, Hit::ToggleCase);
        }
        if two {
            a = one(area.x, area.y + 1, area.width);
        }
        let fields = [Field::Branch, Field::Author, Field::Date, Field::Path];
        let down = self.icons.disclosure(true);
        let close = self.icons.glyph(Icon::Close);
        for (i, k) in fields.into_iter().enumerate() {
            if a.width == 0 {
                break;
            }
            let val = if k == Field::Path {
                self.filter.paths_label(c)
            } else {
                self.value(k)
            };
            let active = !val.is_empty();
            let prefix = k.label();
            let mut label = if active {
                format!("{prefix}: {val}")
            } else {
                format!("{prefix} {down}")
            };
            let reserve: u16 = fields[i + 1..]
                .iter()
                .map(|next| cells(next.label()) + 1)
                .sum();
            let max = a.width.saturating_sub(reserve).max(cells(prefix) + 1);
            let ideal = cells(&label).saturating_add(if active { cells(close) + 2 } else { 1 });
            let chip = take(&mut a, ideal.min(max).min(30));
            let mut value = chip;
            let clear = (active && chip.width >= cells(close) + 4)
                .then(|| take_right(&mut value, cells(close) + 1));
            if !active && value.width <= cells(&label) {
                label = prefix.to_owned();
            }
            f.render_widget(
                Paragraph::new(clipped(&label, value.width.saturating_sub(1).into()))
                    .style(fg(if active { INK } else { DIM })),
                value,
            );
            self.hit(chip, Hit::Filter(k));
            if let Some(clear) = clear {
                self.button(f, clear, close, false, Hit::Clear(k));
            }
        }
        let mut ops = vec![(Icon::Refresh, Hit::Refresh, false)];
        if self.limit < 3200
            && self
                .hist
                .more
                .iter()
                .any(|&id| self.filter.includes_root(id))
        {
            ops.insert(0, (Icon::Add, Hit::More, false));
        }
        let mut used = 0;
        let count = ops
            .iter()
            .take_while(|(icon, _, _)| {
                let w = cells(self.icons.glyph(*icon)) + 1;
                if used + w > a.width {
                    return false;
                }
                used += w;
                true
            })
            .count();
        let mut tail = take_right(&mut a, used);
        for (icon, h, on) in ops.into_iter().take(count) {
            let label = self.icons.glyph(icon);
            let r = take(&mut tail, cells(label) + 1);
            self.button(f, r, label, on, h);
        }
        if two {
            2
        } else {
            1
        }
    }
    fn draw_message_column(&self, f: &mut Frame, mut line: Rect, rev: &model::Rev, selected: bool) {
        let tags = if self.show_labels {
            decorations(rev, &self.hist, &self.options)
        } else {
            Vec::new()
        };
        // IDEA uses an outline label for every reference, distinguished by colour.
        let marker = self.icons.glyph(Icon::Tag);
        let marker_width = marker.width() + 1;
        let full = tags
            .iter()
            .map(|b| b.label.width() + b.kinds.len() * marker_width + 1)
            .sum::<usize>();
        let tag_width = if full == 0 || line.width < 14 {
            0
        } else {
            let spare = usize::from(line.width).saturating_sub(rev.subject.width() + 2);
            // Keep the first reference's complete set of identities, even when its name is clipped.
            let minimum = tags.first().map_or(0, |b| {
                b.kinds.len() * marker_width + usize::from(!b.label.is_empty())
            });
            full.min(
                spare
                    .max(usize::from(line.width / 3))
                    .max(minimum)
                    .min(usize::from(line.width)),
            ) as u16
        };
        let tag_area = if self.options.refs_left {
            take(&mut line, tag_width)
        } else {
            take_right(&mut line, tag_width)
        };
        let subject = line;
        let title = clipped(
            &rev.subject,
            usize::from(subject.width.saturating_sub(u16::from(tag_width > 0))),
        );
        f.render_widget(Paragraph::new(Line::from(subject_spans(&title))), subject);
        if tag_width > 0 {
            let mut spans = Vec::new();
            let mut left = usize::from(tag_width);
            for badge in tags {
                let prefix = badge.kinds.len() * marker_width;
                if left < prefix {
                    break;
                }
                for &kind in &badge.kinds {
                    spans.push(Span::styled(format!("{marker} "), fg(ref_color(kind))));
                }
                let t = badge.fit(left - prefix);
                let used = t.width() + prefix;
                spans.push(Span::styled(t, fg(if selected { INK } else { DIM })));
                left -= used;
                if left > 0 {
                    spans.push(Span::raw(" "));
                    left -= 1;
                }
            }
            let refs = Paragraph::new(Line::from(spans));
            f.render_widget(
                if self.options.refs_left {
                    refs
                } else {
                    refs.right_aligned()
                },
                tag_area,
            );
        }
    }

    fn draw_graph_column(
        &mut self,
        f: &mut Frame,
        area: Rect,
        n: usize,
        window: model::GraphWindow,
    ) {
        let glyphs = self.node_glyphs.for_icons(self.icons);
        let cells = self.graph.cells_with_edges(
            n,
            window,
            glyphs,
            self.hist.revs[self.visible[n]].is_head_tip(),
            self.options.long_edges,
        );
        let mut spans = Vec::with_capacity(cells.len());
        for (x, (ch, tint, target)) in cells.into_iter().take(usize::from(area.width)).enumerate() {
            if let Some(target) = target {
                self.hit(one(area.x + x as u16, area.y, 1), Hit::GraphJump(target));
            }
            spans.push(Span::styled(
                ch.to_string(),
                fg(if glyphs.is_continuation(ch) {
                    DIM
                } else {
                    lane(tint)
                }),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_log_column(
        &mut self,
        f: &mut Frame,
        area: Rect,
        col: Column,
        n: usize,
        graph_window: Option<model::GraphWindow>,
    ) {
        let rev = &self.hist.revs[self.visible[n]];
        let width = usize::from(area.width.saturating_sub(1));
        let (text, style) = match col {
            Column::Graph => {
                if let Some(window) = graph_window {
                    self.draw_graph_column(f, area, n, window);
                }
                return;
            }
            Column::Message => return self.draw_message_column(f, area, rev, n == self.cursor),
            Column::Author => (clipped(&rev.author, width), Style::default()),
            Column::Hash => (rev.oid.chars().take(width).collect(), fg(DIM)),
            Column::Date => {
                let mut value = if self.options.commit_timestamp {
                    self.hist.display_date(rev)
                } else {
                    self.hist.display_author_date(rev)
                };
                if area.width < 18 {
                    if let Some((_, short)) = value.split_once('/') {
                        value = short.replace(',', "");
                    }
                    value = value.replace("Yesterday", "Yest.");
                }
                (clipped(&value, width), Style::default())
            }
            Column::Signature => {
                let (label, status) = self.hist.signature(rev);
                let color = match status {
                    'G' => GREEN,
                    'B' | 'R' => Color::LightRed,
                    'U' | 'X' | 'Y' => GOLD,
                    _ => DIM,
                };
                (clipped(label, width), fg(color))
            }
        };
        f.render_widget(Paragraph::new(text).style(style), area);
    }

    fn draw_graph(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let tools = self.graph_toolbar(f, area, c).min(area.height);
        let errors = self
            .hist
            .errors
            .iter()
            .find_map(|(id, e)| self.filter.includes_root(*id).then_some(e.as_str()));
        let warning = u16::from(errors.is_some() && area.height > tools + 1);
        let avail = area.height.saturating_sub(tools + warning) as usize;
        self.graph_height = avail;
        if let Some(e) = errors {
            if warning != 0 {
                let message = format!("{} {e}", self.icons.glyph(Icon::Warning));
                f.render_widget(
                    Paragraph::new(clipped(&message, area.width as usize)).style(fg(GOLD)),
                    one(area.x, area.bottom() - 1, area.width),
                );
            }
        }
        self.keep_graph_cursor_visible();
        let start = self.graph_top;
        let end = (start + avail).min(self.visible.len());
        if self.visible.is_empty() && avail > 0 {
            let reset;
            let msg = if c.scope.is_empty() {
                "No repositories in scope"
            } else if !c.scope.iter().any(|&id| self.filter.includes_root(id)) {
                let r = one(area.x, area.y + tools, area.width);
                f.render_widget(
                    Paragraph::new("No selected roots in scope · clear Paths").style(fg(DIM)),
                    r,
                );
                self.hit(r, Hit::Clear(Field::Path));
                return;
            } else if self.filter.branch.is_none()
                && self.filter.roots.is_none()
                && self.filter.paths.is_empty()
                && [Field::Author, Field::Date, Field::Text]
                    .iter()
                    .all(|k| self.value(*k).is_empty())
            {
                "No commits in this scope"
            } else {
                reset = format!(
                    "No matching commits · {} clear filters",
                    self.icons.glyph(Icon::Close)
                );
                &reset
            };
            let r = one(area.x, area.y + tools, area.width);
            f.render_widget(Paragraph::new(msg).style(fg(DIM)), r);
            self.hit(r, Hit::Reset);
            return;
        }
        let has_scroll = self.visible.len() > avail;
        let w = area.width.saturating_sub(u16::from(has_scroll));
        let show_graph = self.options.shows(Column::Graph);
        let show_message = self.options.shows(Column::Message);
        // Without the expanding message column, try full metadata widths first.
        let fits = |threshold| !show_message || w >= threshold;
        let mut auth_width = if self.options.shows(Column::Author) && fits(54) {
            self.visible[start..end]
                .iter()
                .map(|&i| self.hist.revs[i].author.width())
                .max()
                .unwrap_or(0)
                .clamp(4, if fits(95) { 13 } else { 8 }) as u16
                + 1
        } else {
            0
        };
        let mut date_width = if !self.options.shows(Column::Date) {
            0
        } else if fits(72) {
            18
        } else if fits(49) {
            12
        } else {
            0
        };
        let icons = self.icons;
        let repo_glyph = |root: usize| icons.repo(c.repos.get(root).is_some_and(|r| r.submodule));
        let icon_width = self.visible[start..end]
            .iter()
            .map(|&i| cells(repo_glyph(self.hist.revs[i].root)))
            .max()
            .unwrap_or(1)
            + u16::from(w >= 50);
        let root_width = if self.options.root_names {
            self.visible[start..end]
                .iter()
                .map(|&i| cells(&c.repos[self.hist.revs[i].root].name))
                .max()
                .unwrap_or(0)
                .min((w / 5).min(18))
                + 1
        } else {
            0
        };
        let stripe_width = icon_width + root_width;
        let mut hash_width = if self.options.shows(Column::Hash) && fits(40) {
            9
        } else {
            0
        };
        let mut signature_width = if self.options.shows(Column::Signature) && fits(40) {
            10
        } else {
            0
        };
        // Keep at least a short subject and graph when optional columns compete.
        let reserved = 4 * u16::from(show_graph) + 14 * u16::from(show_message);
        let budget = w.saturating_sub(stripe_width + reserved);
        if auth_width + date_width + hash_width + signature_width > budget {
            auth_width = auth_width.min(5);
        }
        if auth_width + date_width + hash_width + signature_width > budget {
            date_width = date_width.min(12);
        }
        if auth_width + date_width + hash_width + signature_width > budget {
            auth_width = 0;
        }
        if date_width + hash_width + signature_width > budget {
            date_width = 0;
        }
        if hash_width + signature_width > budget {
            signature_width = 0;
        }
        if hash_width > budget {
            hash_width = 0;
        }
        let remaining =
            w.saturating_sub(stripe_width + auth_width + date_width + hash_width + signature_width);
        let graph_window = show_graph.then(|| {
            let budget = if show_message {
                (remaining / 3).max(4)
            } else {
                remaining.saturating_sub(1)
            };
            self.graph.window(
                start..end,
                self.cursor,
                usize::from(budget),
                self.graph_origin,
            )
        });
        let graph_width = graph_window.map_or(0, |window| {
            self.graph_origin = window.first;
            (window.width() + 1).min(usize::from(remaining)) as u16
        });
        let columns = self.options.columns.map(|(col, visible)| {
            let width = match col {
                Column::Author => auth_width,
                Column::Hash => hash_width,
                Column::Date => date_width,
                Column::Signature => signature_width,
                Column::Graph => graph_width,
                Column::Message => remaining - graph_width,
            };
            (col, if visible { width } else { 0 })
        });
        for n in start..end {
            let y = area.y + tools + (n - start) as u16;
            let row = one(area.x, y, w);
            // Graph jumps take precedence over the enclosing commit row.
            self.hit(row, Hit::Commit(n));
            let rev = &self.hist.revs[self.visible[n]];
            let selected = n == self.cursor;
            let bg = if selected {
                SELECT
            } else if self.options.current_branch && self.hist.on_head(rev) {
                REACHABLE
            } else {
                PANEL
            };
            let ink = if !selected
                && (self.options.merge_commits && rev.parents.len() > 1
                    || self.options.not_cherry_picked && self.hist.is_cherry_picked(rev))
            {
                DIM
            } else {
                INK
            };
            let mut row_style =
                if selected && crossterm::style::Colored::ansi_color_disabled_memoized() {
                    fg(ink).bg(bg).reversed()
                } else {
                    fg(ink).bg(bg)
                };
            if self.options.my_commits && self.hist.is_mine(rev) {
                row_style = row_style.bold();
            }
            f.render_widget(Block::default().style(row_style), row);
            let mut line = row;
            let stripe = take(&mut line, stripe_width);
            f.render_widget(
                Paragraph::new(if root_width > 0 {
                    format!(
                        "{} {}",
                        repo_glyph(rev.root),
                        clipped(&c.repos[rev.root].name, root_width.saturating_sub(1).into())
                    )
                } else {
                    repo_glyph(rev.root).to_owned()
                })
                .style(fg(repo_tint(rev.root))),
                stripe,
            );
            for (col, width) in columns {
                let mut cell = take(&mut line, width);
                if col == Column::Message && line.width > 0 {
                    // Leave a gutter before the next column, even after a long title or a ref badge.
                    cell.width = cell.width.saturating_sub(1);
                }
                if cell.width > 0 {
                    self.draw_log_column(f, cell, col, n, graph_window);
                }
            }
        }
        if has_scroll && avail > 0 {
            let total = self.visible.len();
            let thumb = (avail * avail / total).max(1);
            let pos = start * (avail - thumb) / (total - avail);
            for i in 0..avail {
                let r = one(area.right() - 1, area.y + tools + i as u16, 1);
                f.render_widget(
                    Paragraph::new(if (pos..pos + thumb).contains(&i) {
                        "┃"
                    } else {
                        "│"
                    })
                    .style(fg(if (pos..pos + thumb).contains(&i) {
                        DIM
                    } else {
                        EDGE
                    })),
                    r,
                );
                self.hit(
                    r,
                    Hit::Commit(i * (total - 1) / avail.saturating_sub(1).max(1)),
                );
            }
        }
    }
    pub(super) fn draw_editor(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        let Some((k, v)) = self.editor.clone() else {
            return;
        };
        if k.inline() {
            return;
        }
        let anchor = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::Filter(field) if *field == k).then_some(*r))
            .unwrap_or(one(a.x, a.y, 1));
        if let Some(menu) = &mut self.branch_menu {
            menu.draw(f, a, anchor, self.icons, &mut self.hits);
            return;
        }
        if k == Field::Date {
            self.draw_date_editor(f, a, anchor);
            return;
        }
        if k == Field::Path {
            self.draw_paths(f, a, anchor, c, &v);
            return;
        }
        let w = a.width.min(40);
        let x = anchor.x.min(a.right() - w);
        let y = (anchor.bottom()).min(a.bottom().saturating_sub(6)).max(a.y);
        let h = (self.suggestions.len() + 4).clamp(5, 13) as u16;
        let h = h.min(a.bottom() - y);
        let p = Rect::new(x, y, w, h);
        // Cover underlying rows even where the menu has no button (input, border, padding).
        self.hit(p, Hit::Dropdown(DropdownHit::Surface));
        f.render_widget(Clear, p);
        let b = boxof(k.label().to_string(), true);
        let inner = b.inner(p);
        f.render_widget(b, p);
        if inner.is_empty() {
            return;
        }
        toolbar_field(
            f,
            one(inner.x, inner.y, inner.width),
            &v,
            true,
            self.icons,
            false,
        );
        let h = inner.height.saturating_sub(2) as usize;
        let start = window(self.choice.unwrap_or(0), h, self.suggestions.len());
        for (dy, i) in (start..(start + h).min(self.suggestions.len())).enumerate() {
            let r = one(inner.x, inner.y + 1 + dy as u16, inner.width);
            let label = &self.suggestions[i].1;
            f.render_widget(
                Paragraph::new(clipped(label, usize::from(r.width))).style(
                    if self.choice == Some(i) {
                        fg(INK).bg(SELECT)
                    } else {
                        fg(INK)
                    },
                ),
                r,
            );
            self.hit(r, Hit::Dropdown(DropdownHit::Choice(i)));
        }
        if inner.height > 1 {
            self.bar(
                f,
                one(inner.x, inner.bottom() - 1, inner.width),
                &[
                    (
                        format!("Apply {}", self.icons.glyph(Icon::Enter)),
                        Hit::Dropdown(DropdownHit::Apply),
                    ),
                    ("Cancel Esc".into(), Hit::Dropdown(DropdownHit::Cancel)),
                ],
            );
        }
    }
    fn draw_date_editor(&mut self, f: &mut Frame, a: Rect, anchor: Rect) {
        let range = self.date_range.clone();
        let w = a.width.min(if range.is_some() { 40 } else { 22 });
        let h = a.height.min(if range.is_some() { 7 } else { 5 });
        let x = anchor.x.min(a.right() - w);
        let y = anchor.bottom().min(a.bottom() - h).max(a.y);
        let p = Rect::new(x, y, w, h);
        self.hit(p, Hit::Dropdown(DropdownHit::Surface));
        f.render_widget(Clear, p);
        let b = boxof(
            if range.is_some() {
                "Date range"
            } else {
                "Date"
            }
            .into(),
            true,
        );
        let inner = b.inner(p);
        f.render_widget(b, p);
        if inner.is_empty() {
            return;
        }
        if let Some(range) = range {
            for (i, label) in ["From", "To"].into_iter().enumerate() {
                if i >= usize::from(inner.height.saturating_sub(1)) {
                    break;
                }
                let row = one(inner.x, inner.y + i as u16, inner.width);
                let mut input = row;
                let caption = take(&mut input, 6);
                f.render_widget(Paragraph::new(label).style(fg(DIM)), caption);
                let value = &range.values[i];
                toolbar_field(
                    f,
                    input,
                    if value.is_empty() {
                        "YYYY-MM-DD"
                    } else {
                        value
                    },
                    range.active == i,
                    self.icons,
                    false,
                );
                self.hit(row, Hit::Dropdown(DropdownHit::DateInput(i)));
            }
            for (i, hint) in [
                "YYYY-MM-DD · includes both dates",
                "Tab switch · Ctrl-U clear",
            ]
            .into_iter()
            .enumerate()
            {
                let dy = i as u16 + 2;
                if dy < inner.height.saturating_sub(1) {
                    f.render_widget(
                        Paragraph::new(clipped(hint, inner.width.into())).style(fg(DIM)),
                        one(inner.x, inner.y + dy, inner.width),
                    );
                }
            }
            self.bar(
                f,
                one(inner.x, inner.bottom() - 1, inner.width),
                &[
                    (
                        format!("Apply {}", self.icons.glyph(Icon::Enter)),
                        Hit::Dropdown(DropdownHit::Apply),
                    ),
                    ("Cancel Esc".into(), Hit::Dropdown(DropdownHit::Cancel)),
                ],
            );
        } else {
            for (i, (_, label)) in self.suggestions.clone().into_iter().enumerate() {
                if i >= usize::from(inner.height) {
                    break;
                }
                let row = one(inner.x, inner.y + i as u16, inner.width);
                f.render_widget(
                    Paragraph::new(label).style(if self.choice == Some(i) {
                        fg(INK).bg(SELECT)
                    } else {
                        fg(INK)
                    }),
                    row,
                );
                self.hit(row, Hit::Dropdown(DropdownHit::Choice(i)));
            }
        }
    }
    pub(crate) fn draw_tabs(&mut self, f: &mut Frame, header: Rect, c: &Context) {
        let mut a = header;
        let multi = self.views.len() > 1;
        let mut controls = Vec::new();
        // Keep a readable current page first. Switching pages remains possible
        // from their titles and the keyboard when the secondary arrow won't fit.
        let mut controls_width = 0;
        for (icon, hit) in [
            (Icon::Add, Hit::NewTab),
            (Icon::Close, Hit::CloseTab),
            (
                Icon::Next,
                Hit::Tab((self.view_index + 1) % self.views.len()),
            ),
        ]
        .into_iter()
        .take(if multi { 3 } else { 1 })
        {
            let label = self.icons.glyph(icon);
            let w = cells(label) + 2;
            if controls_width + w > a.width.saturating_sub(5) {
                break;
            }
            controls.push((label, hit));
            controls_width += w;
        }
        if controls.len() > 1 {
            controls.swap(0, 1); // Close belongs next to the page it closes.
        }
        let mut nums = vec![self.view_index];
        let max = usize::from(a.width - controls_width);
        let mut used = Self::title(&self.filter, c).width().min(37) + 2;
        for i in (0..self.views.len()).filter(|i| *i != self.view_index) {
            let w = Self::title(&self.views[i].filter, c).width().min(37) + 2;
            if used + w > max {
                break;
            }
            nums.push(i);
            used += w;
        }
        nums.sort_unstable();
        for i in nums {
            let title = Self::title(
                if i == self.view_index {
                    &self.filter
                } else {
                    &self.views[i].filter
                },
                c,
            );
            let tw = (title.width() + 2).min(39) as u16;
            let page_width = tw.min(a.width - controls_width);
            let r = take(&mut a, page_width);
            f.render_widget(
                Paragraph::new(format!(
                    " {}",
                    clipped(&title, r.width.saturating_sub(1).into())
                ))
                .style(if i == self.view_index {
                    fg(INK).bg(TAB).bold()
                } else {
                    fg(DIM)
                }),
                r,
            );
            self.hit(r, Hit::Tab(i));
        }
        for (icon, hit) in controls {
            let r = take(&mut a, cells(icon) + 2);
            self.button(f, r, &format!(" {icon} "), false, hit);
        }
    }
    fn title(f: &Filter, c: &Context) -> String {
        let mut v = Vec::new();
        let paths = f.paths_label(c);
        if !paths.is_empty() {
            v.push(format!("in {paths}"));
        }
        if !f.author.is_empty() {
            v.push(format!("by {}", f.author));
        }
        if let Some(b) = &f.branch {
            v.push(b.text(true));
        }
        if let Some(date) = &f.date {
            v.push(date.label());
        }
        if !f.query.is_empty() {
            v.push(format!("“{}”", f.query));
        }
        if v.is_empty() {
            "Log".into()
        } else {
            format!("Log: {}", v.join(" "))
        }
    }
}
