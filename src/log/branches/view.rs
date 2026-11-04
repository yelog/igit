use super::*;
use ratatui::widgets::BorderType;

const POPUP: Color = Color::Rgb(37, 39, 43);
const BORDER: Color = Color::Rgb(64, 66, 70);

struct Paint<'a, 'b> {
    f: &'a mut Frame<'b>,
    hits: &'a mut Vec<(Rect, Hit)>,
    icons: IconMode,
}
impl Paint<'_, '_> {
    fn hit(&mut self, r: Rect, h: DropdownHit) {
        if !r.is_empty() {
            self.hits.push((r, Hit::Dropdown(h)));
        }
    }
    fn target(&mut self, r: Rect, t: Target) {
        self.hit(r, DropdownHit::Branch(t));
    }
    fn popup(&mut self, r: Rect) -> Rect {
        self.f.render_widget(Clear, r);
        let b = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(fg(BORDER))
            .style(fg(INK).bg(POPUP));
        let inner = b.inner(r);
        self.f.render_widget(b, r);
        self.hit(r, DropdownHit::Surface);
        inner
    }
    fn row(
        &mut self,
        r: Rect,
        label: &str,
        id: Row,
        selected: bool,
        star: Option<bool>,
        group: bool,
    ) {
        if r.is_empty() {
            return;
        }
        let bg = if selected { SELECT } else { POPUP };
        self.f
            .render_widget(Paragraph::new("").style(fg(INK).bg(bg)), r);
        self.target(r, Target::Row(id));
        let mut body = r;
        take(&mut body, 1);
        let mark = take(
            &mut body,
            cells(self.icons.glyph(Icon::Favorite)).max(cells(self.icons.glyph(Icon::NotFavorite)))
                + 1,
        );
        if let Some(on) = star {
            if on || selected {
                self.f.render_widget(
                    Paragraph::new(self.icons.glyph(if on {
                        Icon::Favorite
                    } else {
                        Icon::NotFavorite
                    }))
                    .style(fg(GOLD).bg(bg)),
                    mark,
                );
            }
            self.target(mark, Target::Star(id));
        }
        let name_width = body.width.saturating_sub(if group { 3 } else { 1 });
        let name = take(&mut body, name_width);
        self.f.render_widget(
            Paragraph::new(clipped(label, name.width.into())).style(fg(INK).bg(bg)),
            name,
        );
        if group {
            self.f.render_widget(
                Paragraph::new(self.icons.glyph(Icon::Next)).style(fg(INK).bg(bg)),
                body,
            );
        }
    }
    fn list(
        &mut self,
        m: &Menu,
        r: Rect,
        ids: &[usize],
        selected: Option<usize>,
        start: usize,
        row: fn(usize) -> Row,
    ) {
        if r.is_empty() {
            return;
        }
        let h = usize::from(r.height);
        let scroll = ids.len() > h;
        for (dy, &id) in ids.iter().skip(start).take(h).enumerate() {
            let i = start + dy;
            let name = &m.refs[id].name;
            let label = if m.level == Level::Select && name.starts_with("refs/tags/") {
                name.as_str()
            } else {
                short_ref(name)
            };
            self.row(
                Rect::new(r.x, r.y + dy as u16, r.width - u16::from(scroll), 1),
                label,
                row(i),
                selected == Some(i),
                Some(m.star(id)),
                false,
            );
        }
        if scroll {
            self.scrollbar(r, start, ids.len(), row);
        }
    }
    fn scrollbar(&mut self, r: Rect, start: usize, n: usize, row: fn(usize) -> Row) {
        let h = usize::from(r.height);
        if !r.is_empty() && n > h {
            let thumb = (h * h / n).max(1);
            let top = start * (h - thumb) / (n - h);
            for i in 0..h {
                let track = Rect::new(r.right() - 1, r.y + i as u16, 1, 1);
                let ink = if (top..top + thumb).contains(&i) {
                    DIM
                } else {
                    BORDER
                };
                let glyph = if self.icons == IconMode::Ascii {
                    "|"
                } else {
                    "┃"
                };
                self.f
                    .render_widget(Paragraph::new(glyph).style(fg(ink).bg(POPUP)), track);
                self.target(
                    track,
                    Target::Seek(row(i * (n - 1) / h.saturating_sub(1).max(1))),
                );
            }
        }
    }
}

fn anchored(a: Rect, anchor: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(a.width);
    let below = a.bottom().saturating_sub(anchor.bottom());
    let h = h.min(if below >= 3 { below } else { a.height });
    Rect::new(
        anchor.x.clamp(a.x, a.right() - w),
        anchor.bottom().min(a.bottom() - h).max(a.y),
        w,
        h,
    )
}
fn line(r: Rect, dy: usize) -> Rect {
    Rect::new(r.x, r.y + dy as u16, r.width, 1)
}
fn height(n: usize) -> u16 {
    n.min(usize::from(u16::MAX)) as u16
}
// Hover must not recenter the list: a following click must still hit the row under the mouse.
fn viewport(top: usize, cursor: usize, h: usize, n: usize) -> usize {
    top.min(n.saturating_sub(h))
        .clamp(cursor.saturating_sub(h.saturating_sub(1)), cursor)
}

impl Menu {
    pub(in crate::log) fn draw(
        &mut self,
        f: &mut Frame,
        a: Rect,
        anchor: Rect,
        icons: IconMode,
        hits: &mut Vec<(Rect, Hit)>,
    ) {
        let mut p = Paint { f, hits, icons };
        if self.level == Level::Select {
            let r = anchored(a, anchor, 52, height(self.matches.len().clamp(1, 9) + 5));
            let inner = p.popup(r);
            if inner.is_empty() {
                return;
            }
            let input = line(inner, 0);
            toolbar_field(
                p.f,
                input,
                &self.query.replace('\n', " | "),
                self.picked.is_none(),
                icons,
                false,
            );
            p.target(input, Target::Input);
            let h = inner.height.saturating_sub(3);
            if h > 0 {
                let rows = Rect::new(inner.x, inner.y + 1, inner.width, h);
                if self.matches.is_empty() {
                    p.f.render_widget(
                        Paragraph::new(" No matching names").style(fg(DIM)),
                        line(rows, 0),
                    );
                } else {
                    self.list_top = viewport(
                        self.list_top,
                        self.picked.unwrap_or(0),
                        h.into(),
                        self.matches.len(),
                    );
                    p.list(
                        self,
                        rows,
                        &self.matches,
                        self.picked,
                        self.list_top,
                        Row::Match,
                    );
                }
            }
            if inner.height >= 3 {
                p.f.render_widget(
                    Paragraph::new(" Use | for multiple refs; -name to exclude").style(fg(DIM)),
                    line(inner, inner.height as usize - 2),
                );
            }
            if inner.height >= 2 {
                let mut bar = line(inner, inner.height as usize - 1);
                for (label, t) in [(" Apply ↵ ", Target::Apply), (" Back Esc ", Target::Back)] {
                    let btn = take(&mut bar, cells(label));
                    p.f.render_widget(Paragraph::new(label).style(fg(INK)), btn);
                    p.target(btn, t);
                }
            }
            return;
        }
        let label = |it: Item| match it {
            Item::Select => "Select...",
            Item::Favorites => "Favorites",
            Item::Ref(i) => short_ref(&self.refs[i].name),
            Item::Group(i) => self.groups[i].label.as_str(),
            Item::Divider => "",
        };
        let width = self
            .main
            .iter()
            .map(|&it| cells(label(it)).saturating_add(7))
            .max()
            .unwrap_or(0)
            .clamp(28, 44);
        let main = anchored(a, anchor, width, height(self.main.len().saturating_add(2)));
        let h = main.height.saturating_sub(2) as usize;
        self.main_top = viewport(self.main_top, self.row, h, self.main.len());
        let start = self.main_top;
        let child = self
            .group()
            .filter(|_| self.level == Level::Child)
            .map(|g| {
                let wanted = g.width.saturating_add(7).clamp(30, 90);
                let right = a.right() - main.right();
                let left = main.x - a.x;
                let min = wanted.min(24);
                let (x, w, back) = if right >= wanted || (right >= min && right >= left) {
                    (main.right(), wanted.min(right), false)
                } else if left >= min {
                    let w = wanted.min(left);
                    (main.x - w, w, false)
                } else {
                    let w = wanted.min(a.width);
                    (anchor.x.clamp(a.x, a.right() - w), w, true)
                };
                let desired = height(g.entries.len().saturating_add(2 + usize::from(back)));
                let y = if back {
                    main.y
                } else {
                    main.y
                        .saturating_add(height(self.row - start))
                        .min(a.bottom().saturating_sub(desired.min(7)))
                        .max(main.y)
                };
                (g, Rect::new(x, y, w, desired.min(a.bottom() - y)), back)
            });
        if !child.is_some_and(|(_, _, back)| back) {
            let inner = p.popup(main);
            for (dy, &it) in self.main.iter().skip(start).take(h).enumerate() {
                let mut r = line(inner, dy);
                r.width = r.width.saturating_sub(u16::from(self.main.len() > h));
                if it == Item::Divider {
                    p.f.render_widget(
                        Paragraph::new("─".repeat(r.width.into())).style(fg(BORDER)),
                        r,
                    );
                } else {
                    let mark = if let Item::Ref(id) = it {
                        Some(self.star(id))
                    } else {
                        None
                    };
                    p.row(
                        r,
                        label(it),
                        Row::Main(start + dy),
                        self.row == start + dy,
                        mark,
                        matches!(it, Item::Group(_)),
                    );
                }
            }
            p.scrollbar(inner, start, self.main.len(), Row::Main);
        }
        if let Some((g, r, back)) = child {
            let mut inner = p.popup(r);
            if back && !inner.is_empty() {
                let backrow = line(inner, 0);
                p.f.render_widget(
                    Paragraph::new(format!(" {}  {}", icons.glyph(Icon::Previous), g.label))
                        .style(fg(DIM)),
                    backrow,
                );
                p.target(backrow, Target::Back);
                inner.y += 1;
                inner.height -= 1;
            }
            let start = viewport(
                self.list_top,
                self.child,
                inner.height.into(),
                g.entries.len(),
            );
            p.list(self, inner, &g.entries, Some(self.child), start, Row::Child);
            self.list_top = start;
        }
    }
}
