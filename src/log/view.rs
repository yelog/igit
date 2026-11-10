//! Cell geometry and colours for the IDEA Git tool window.
use super::*;
use crate::ui::idea::{LOCAL_REF as GREEN, REMOTE_REF as PURPLE};
use display::Column;
use unicode_width::UnicodeWidthStr;

pub(crate) use crate::ui::idea::{clipped, repo_tint, BLUE, DIM, EDGE, GOLD, INK, PANEL, SELECT};
const REACHABLE: Color = Color::Rgb(32, 37, 58);
const INPUT: Color = Color::Rgb(35, 36, 43);
const TAB: Color = Color::Rgb(40, 56, 88);

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
    model::graph_color(t)
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
        if self.filter.file_history.is_some() {
            return matches!(pane, Pane::Graph | Pane::Info);
        }
        match pane {
            Pane::Refs => self.show_refs,
            Pane::Graph => true,
            Pane::Files | Pane::Info => self.show_info,
        }
    }

    pub(super) fn cycle_focus(&mut self, d: i32) {
        let mut pane = self.focus;
        for _ in Pane::ORDER {
            pane = pane.delta(d);
            if self.pane_visible(pane) {
                self.focus_pane(pane.id());
                break;
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
            self.button(f, r, &format!(" {name} "), false, h.clone());
        }
    }
    pub(super) fn draw_layout(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        f.render_widget(Block::default().style(fg(INK).bg(PANEL)), a);
        if !self.pane_visible(self.focus) {
            self.focus = Pane::Graph;
        }
        if self.filter.file_history.is_some() {
            self.draw_history(f, a, c);
            return;
        }
        let percent = |n: u16, p: u32| (u32::from(n) * p / 100) as u16;
        // At narrow widths, give the sidebars less space while keeping the graph
        // wide enough for its filter labels.
        let refs_width = percent(a.width, 17);
        let refs_min = refs_width.clamp(8, 12);
        let graph_min = 25;
        let inspector_min = percent(a.width, 26).clamp(11, 16);
        let [main, inspector] = if self.show_info {
            self.panes.divide(
                SplitId::LogInspector,
                a,
                Axis::Width,
                percent(a.width, 74),
                [
                    graph_min + refs_min * u16::from(self.show_refs),
                    inspector_min,
                ],
            )
        } else {
            [a, Rect::default()]
        };
        let [refs, graph] = if self.show_refs {
            self.panes.divide(
                SplitId::LogRefs,
                main,
                Axis::Width,
                refs_width,
                [refs_min, graph_min],
            )
        } else {
            [Rect::default(), main]
        };
        let [files, info] = if self.show_info {
            self.panes.divide(
                SplitId::LogDetails,
                inspector,
                Axis::Height,
                percent(a.height, 65),
                [2, 1],
            )
        } else {
            [Rect::default(); 2]
        };
        let areas = [refs, graph, files, info];
        let active = self.panes.active
            && (!self.editing() || self.focused_search().is_some_and(|prompt| prompt.editing()));
        for (p, outer) in Pane::ORDER.into_iter().zip(areas) {
            if !self.pane_visible(p) {
                continue;
            }
            self.panes.add(p.id(), outer);
            if outer.is_empty() {
                continue;
            }
            let r = pane_frame(f, outer, active && self.focus == p);
            match p {
                Pane::Refs => self.draw_refs(f, r, c),
                Pane::Graph => self.draw_graph(f, r, c),
                Pane::Files => self.draw_files(f, r, c),
                Pane::Info => self.draw_info(f, r, c),
            }
        }
        if files.height.min(info.height) <= 4
            && !info.is_empty()
            && self.panes.can_resize(files, Axis::Height)
        {
            // Short frames only draw sides. Use whichever horizontal border is
            // visible or mark a grip on the right edge; never cover a content row.
            let handle = if files.height > 4 {
                one(files.x, files.bottom() - 1, files.width)
            } else if info.height > 4 {
                one(info.x, info.y, info.width)
            } else {
                let grip = one(files.right() - 1, files.bottom() - 1, 1);
                f.buffer_mut()[(grip.x, grip.y)].set_symbol("┤");
                grip
            };
            self.panes.handle(SplitId::LogDetails, handle);
        }
    }
    fn graph_toolbar(&mut self, f: &mut Frame, area: Rect, c: &Context) -> u16 {
        if self.filter.file_history.is_some() {
            return self.history_toolbar(f, area);
        }
        let two = area.width < 56 && area.height >= 5;
        let mut a = one(area.x, area.y, area.width);
        self.view_button(f, &mut a);
        if a.width >= 27 && self.rev().is_some() {
            let r = take_right(&mut a, 4);
            self.button(
                f,
                r,
                " ...",
                self.commit_dialog.is_some(),
                Hit::CommitActions,
            );
        }
        if self.pick_mode && !self.picks.is_empty() && a.width >= 25 {
            let label = format!(" Pick {} ", self.picks.len());
            let r = take_right(&mut a, cells(&label) as u16);
            self.button(f, r, &label, false, Hit::ReviewPicks);
        }
        let input = self
            .editor
            .as_ref()
            .filter(|(field, _)| *field == Field::Text);
        let editing = input.is_some();
        let show_options = two || area.width >= 77 || editing;
        let suffix_width = if show_options { 6 } else { 0 };
        let field_w = if two {
            a.width
        } else {
            (a.width / 4).clamp(13, 32) + suffix_width
        };
        let field = take(&mut a, field_w);
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
        // Reserve space for a short title as well as every marker of the first
        // reference (e.g. remote + local + HEAD), including in a narrow frame.
        let minimum = tags.first().map_or(0, |b| {
            b.kinds.len() * marker_width + usize::from(!b.label.is_empty())
        });
        let tag_width = if full == 0 || usize::from(line.width) < minimum + 4 {
            0
        } else {
            let spare = usize::from(line.width).saturating_sub(rev.subject.width() + 2);
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

    fn repository_marker(&self, n: usize) -> &'static str {
        let root = self.hist.revs[self.visible[n]].root;
        let same_root = |i: usize| {
            self.visible
                .get(i)
                .is_some_and(|&rev| self.hist.revs[rev].root == root)
        };
        // Use filtered history, including neighbors beyond the viewport. A
        // scroll boundary is not a repository boundary, nor is a reused tint.
        match (n > 0 && same_root(n - 1), same_root(n + 1)) {
            (true, true) => "┃",
            (false, true) => "╻",
            (true, false) => "╹",
            (false, false) => "❙",
        }
    }

    pub(super) fn draw_graph(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            return;
        }
        let tools = self.graph_toolbar(f, area, c).min(area.height);
        let errors = self
            .hist
            .errors
            .iter()
            .find_map(|(id, e)| self.filter.includes_root(*id).then_some(e.as_str()));
        let status = self.history_status().or(errors).map(str::to_owned);
        let warning = u16::from(status.is_some() && area.height > tools + 1);
        let avail = area.height.saturating_sub(tools + warning) as usize;
        self.graph_height = avail;
        if avail == 0 {
            return;
        }
        if let Some(e) = status.as_deref() {
            if warning != 0 {
                let (icon, tint) = if self.history_pending() {
                    (Icon::Refresh, DIM)
                } else {
                    (Icon::Warning, GOLD)
                };
                let message = format!("{} {e}", self.icons.glyph(icon));
                f.render_widget(
                    Paragraph::new(clipped(&message, area.width as usize)).style(fg(tint)),
                    one(area.x, area.bottom() - 1, area.width),
                );
            }
        }
        self.keep_graph_cursor_visible();
        let start = self.graph_top;
        let end = (start + avail).min(self.visible.len());
        if self.visible.is_empty() {
            if let Some(status) = self.history_status() {
                if warning == 0 {
                    f.render_widget(
                        Paragraph::new(status).style(fg(DIM)),
                        one(area.x, area.y + tools, area.width),
                    );
                }
                return;
            }
            if let Some(file) = &self.filter.file_history {
                let message = if c.scope.contains(&file.root) {
                    "No commits for this file with the current filters"
                } else {
                    "File's repository is outside Roots"
                };
                f.render_widget(
                    Paragraph::new(message).style(fg(DIM)),
                    one(area.x, area.y + tools, area.width),
                );
                return;
            }
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
        let marker_width = 1 + u16::from(w >= 50);
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
        let pick_width = if self.pick_mode { 4 } else { 0 };
        let stripe_width = marker_width + root_width + pick_width;
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
            self.graph.window_with_edges(
                start..end,
                self.cursor,
                usize::from(budget),
                self.graph_origin,
                self.options.long_edges,
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
            let repo_id = rev.root;
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
            let marker = take(&mut line, marker_width);
            // Center the strip away from the frame. Half-height caps separate
            // roots without extra rows; keep the gutter independent of highlights.
            f.render_widget(
                Paragraph::new(self.repository_marker(n))
                    .style(fg(repo_tint(repo_id)).bg(PANEL).not_bold().not_reversed()),
                marker,
            );
            if self.pick_mode {
                let checkbox = take(&mut line, pick_width);
                let checked = self
                    .picks
                    .iter()
                    .any(|p| p.root == rev.root && p.oid == rev.oid);
                let label = if checked { "[x] " } else { "[ ] " };
                f.render_widget(
                    Paragraph::new(label).style(fg(if checked { BLUE } else { DIM })),
                    checkbox,
                );
                self.hit(checkbox, Hit::PickCommit(n));
            }
            if root_width > 0 {
                let root = take(&mut line, root_width);
                f.render_widget(
                    Paragraph::new(clipped(
                        &c.repos[repo_id].name,
                        root.width.saturating_sub(1).into(),
                    ))
                    .style(fg(repo_tint(repo_id))),
                    root,
                );
            }
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
        if has_scroll {
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
                    Hit::CommitScroll(i * (total - 1) / avail.saturating_sub(1).max(1)),
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
        if let Some(file) = &f.file_history {
            return file.title();
        }
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
