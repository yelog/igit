//! Rendered rows retain their byte positions in the source. Soft wraps and terminal
//! padding are presentation only: copying never reconstructs text from the screen.
use super::{idea, inside, Action};
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Block,
    Frame,
};
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    Clip,
    Character,
    Word,
}

pub(crate) struct TextRow {
    pub line: Line<'static>,
    pub column: usize,
    start: usize,
    end: usize,
    decoration: bool,
}

#[derive(Default)]
pub(crate) struct TextLayout {
    text: String,
    pub rows: Vec<TextRow>,
    lines: usize,
}

impl TextLayout {
    pub fn plain(text: &str, width: u16, wrap: Wrap) -> Self {
        let mut layout = Self::default();
        for line in text.split('\n') {
            layout.push(Line::raw(line), width, wrap);
        }
        layout
    }

    /// Returns the physical row range, for links that share this layout.
    pub fn push(&mut self, line: Line<'_>, width: u16, wrap: Wrap) -> Range<usize> {
        let first = self.rows.len();
        if self.lines > 0 {
            self.text.push('\n');
        }
        self.lines += 1;
        let base = self.text.len();
        for span in &line.spans {
            self.text.push_str(&span.content);
        }
        if width == 0 {
            return first..first;
        }
        if wrap == Wrap::Clip || line.width() <= usize::from(width) {
            self.rows.push(TextRow {
                line: Line::from(
                    line.spans
                        .iter()
                        .map(|s| Span::styled(s.content.to_string(), s.style))
                        .collect::<Vec<_>>(),
                )
                .style(line.style),
                column: 0,
                start: base,
                end: self.text.len(),
                decoration: false,
            });
            return first..self.rows.len();
        }
        let mut bytes = base;
        let mut column = 0;
        let glyphs: Vec<_> = line
            .styled_graphemes(Style::default())
            .map(|g| {
                let start = bytes;
                let col = column;
                bytes += g.symbol.len();
                column += g.symbol.width();
                (g, start, col)
            })
            .collect();
        let mut start = 0;
        while start < glyphs.len() {
            let mut end = start;
            let mut used = 0;
            let mut word_break = None;
            let mut has_text = false;
            while let Some((g, _, _)) = glyphs.get(end) {
                if used + g.symbol.width() > usize::from(width) {
                    break;
                }
                used += g.symbol.width();
                end += 1;
                if g.symbol.trim().is_empty() {
                    if has_text {
                        word_break = Some(end);
                    }
                } else {
                    has_text = true;
                }
            }
            // Even a one-cell viewport must make progress past a wide grapheme.
            end = end.max(start + 1);
            if wrap == Wrap::Word && end < glyphs.len() && !glyphs[end].0.symbol.trim().is_empty() {
                end = word_break.unwrap_or(end);
            }
            let mut last = end;
            if wrap == Wrap::Word && end < glyphs.len() {
                while last > start + 1 && glyphs[last - 1].0.symbol.trim().is_empty() {
                    last -= 1;
                }
            }
            let mut spans: Vec<Span<'static>> = vec![];
            for (g, _, _) in &glyphs[start..last] {
                if let Some(span) = spans.last_mut().filter(|s| s.style == g.style) {
                    span.content.to_mut().push_str(g.symbol);
                } else {
                    spans.push(Span::styled(g.symbol.to_owned(), g.style));
                }
            }
            self.rows.push(TextRow {
                line: Line::from(spans),
                column: glyphs[start].2,
                start: glyphs[start].1,
                end: glyphs[last - 1].1 + glyphs[last - 1].0.symbol.len(),
                decoration: false,
            });
            start = end;
            if wrap == Wrap::Word {
                while start < glyphs.len() && glyphs[start].0.symbol.trim().is_empty() {
                    start += 1;
                }
            }
        }
        first..self.rows.len()
    }

    pub fn decoration(&mut self, line: Line<'static>) {
        self.rows.push(TextRow {
            line,
            column: 0,
            start: self.text.len(),
            end: self.text.len(),
            decoration: true,
        });
    }
}

#[derive(Default)]
pub(crate) struct TextSelection {
    layout: TextLayout,
    area: Rect,
    top: usize,
    left: usize,
    anchor: usize,
    head: usize,
    press: Option<(u16, u16)>,
    dragged: bool,
}

impl TextSelection {
    /// Editing can receive several keys between frames. Keep copying in sync even
    /// before the new source has been laid out for the next draw.
    pub fn sync_source(&mut self, text: &str) {
        if self.layout.text != text {
            self.clear();
            self.layout.text = text.to_owned();
            self.layout.rows.clear();
            self.area = Rect::default();
        }
    }
    pub fn clear(&mut self) {
        self.anchor = 0;
        self.head = 0;
        self.press = None;
        self.dragged = false;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn range(&self) -> Option<Range<usize>> {
        (self.anchor != self.head).then(|| self.anchor.min(self.head)..self.anchor.max(self.head))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.layout.text.get(self.range()?)
    }

    pub fn head(&self) -> usize {
        self.head
    }

    pub fn extend(&mut self, from: usize, to: usize) {
        if self.range().is_none() {
            self.anchor = from;
        }
        self.head = to;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.head = self.layout.text.len();
    }

    pub fn key(&mut self, e: &Event, read_only: bool) -> Option<Action> {
        let Event::Key(k) = e else { return None };
        if k.kind == KeyEventKind::Release {
            return None;
        }
        self.press = None;
        self.dragged = false;
        if copy_key(e) || read_only && k.code == KeyCode::Char('y') && k.modifiers.is_empty() {
            return self.selected_text().map(|s| Action::Copy {
                text: s.to_owned(),
                label: "选中文本",
            });
        }
        if k.code == KeyCode::Char('a') && k.modifiers == KeyModifiers::CONTROL {
            self.select_all();
            return Some(Action::None);
        }
        if k.code == KeyCode::Esc && self.range().is_some() {
            self.clear();
            return Some(Action::None);
        }
        None
    }

    /// Only a Drag that crosses a screen cell starts selection. A repaint can
    /// move the viewport between Down and Up; remapping a plain click on Up
    /// would invent a range. Clicks keep their Down caret and never copy.
    pub fn mouse(&mut self, m: &MouseEvent) -> Option<Action> {
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if inside(self.area, m.column, m.row) => {
                let at = self.byte_at(m.column, m.row);
                self.anchor = at;
                self.head = at;
                self.press = Some((m.column, m.row));
                self.dragged = false;
                Some(Action::None)
            }
            MouseEventKind::Drag(MouseButton::Left) if self.press.is_some() => {
                if self.dragged || self.press != Some((m.column, m.row)) {
                    self.dragged = true;
                    self.head = self.byte_at(m.column, m.row);
                }
                Some(Action::None)
            }
            MouseEventKind::Up(MouseButton::Left) if self.press.is_some() => {
                self.press = None;
                let action = if self.dragged {
                    self.head = self.byte_at(m.column, m.row);
                    self.selected_text()
                        .map(|text| Action::Copy {
                            text: text.to_owned(),
                            label: "选中文本",
                        })
                        .unwrap_or(Action::None)
                } else {
                    Action::None
                };
                self.dragged = false;
                self.anchor = self.head;
                Some(action)
            }
            MouseEventKind::Down(_) => {
                self.clear();
                None
            }
            _ => None,
        }
    }

    fn byte_at(&self, x: u16, y: u16) -> usize {
        if self.area.is_empty() {
            return 0;
        }
        let y = y.clamp(self.area.y, self.area.bottom() - 1);
        let Some(row) = self
            .layout
            .rows
            .get(self.top + usize::from(y - self.area.y))
        else {
            return self.layout.text.len();
        };
        if row.start == row.end {
            return row.start;
        }
        let cell = self.left + usize::from(x.clamp(self.area.x, self.area.right()) - self.area.x);
        let mut col = 0;
        let mut byte = row.start;
        for g in row.line.styled_graphemes(Style::default()) {
            if col < self.left && col + g.symbol.width() > self.left {
                col += g.symbol.width();
                byte += g.symbol.len();
                continue;
            }
            if col + g.symbol.width() > cell {
                return byte;
            }
            col += g.symbol.width();
            byte += g.symbol.len();
        }
        row.end
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        layout: TextLayout,
        top: usize,
        left: usize,
    ) {
        if self.layout.text != layout.text {
            self.clear();
        }
        self.layout = layout;
        self.area = area;
        self.top = top;
        self.left = left;
        if area.is_empty() {
            self.clear();
            return;
        }
        f.render_widget(
            Block::default().style(Style::new().fg(idea::INK).bg(idea::PANEL)),
            area,
        );
        let selected = self.range();
        for (i, row) in self
            .layout
            .rows
            .iter()
            .skip(top)
            .take(usize::from(area.height))
            .enumerate()
        {
            let mut col = 0;
            let mut byte = row.start;
            for g in row.line.styled_graphemes(Style::default()) {
                let width = g.symbol.width();
                if col >= left && col + width <= left + usize::from(area.width) && width > 0 {
                    let style = if byte < row.end
                        && selected
                            .as_ref()
                            .is_some_and(|r| byte < r.end && byte + g.symbol.len() > r.start)
                    {
                        g.style.fg(idea::INK).bg(idea::SELECT)
                    } else {
                        g.style
                    };
                    f.buffer_mut().set_stringn(
                        area.x + (col - left) as u16,
                        area.y + i as u16,
                        g.symbol,
                        width,
                        style,
                    );
                }
                col += width;
                byte += g.symbol.len();
                if col >= left + usize::from(area.width) {
                    break;
                }
            }
            // Give selected hard line breaks one visible cell, including empty
            // lines. The cell is feedback only and never enters copied text.
            if !row.decoration
                && col >= left
                && col < left + usize::from(area.width)
                && self.layout.text[row.end..].starts_with('\n')
                && selected.as_ref().is_some_and(|r| r.contains(&row.end))
            {
                f.buffer_mut()[(area.x + (col - left) as u16, area.y + i as u16)]
                    .set_style(Style::new().fg(idea::INK).bg(idea::SELECT));
            }
        }
    }
}

pub(crate) fn copy_key(e: &Event) -> bool {
    matches!(e, Event::Key(k) if k.kind != KeyEventKind::Release
        && k.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(k.code, KeyCode::Char('c' | 'C') | KeyCode::Insert))
}

pub(crate) fn previous(text: &str, pos: usize) -> usize {
    if text[..pos].ends_with('\n') {
        return pos - 1;
    }
    let span = Span::raw(&text[..pos]);
    pos - span
        .styled_graphemes(Style::default())
        .last()
        .map_or(0, |g| g.symbol.len())
}

pub(crate) fn at_column(text: &str, column: usize) -> usize {
    let span = Span::raw(text);
    let mut width = 0;
    let mut bytes = 0;
    for g in span.styled_graphemes(Style::default()) {
        if width + g.symbol.width() > column {
            break;
        }
        width += g.symbol.width();
        bytes += g.symbol.len();
    }
    bytes
}

pub(crate) fn next(text: &str, pos: usize) -> usize {
    if text[pos..].starts_with('\n') {
        return pos + 1;
    }
    let span = Span::raw(&text[pos..]);
    let bytes = span
        .styled_graphemes(Style::default())
        .next()
        .map_or(0, |g| g.symbol.len());
    pos + bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }
    fn draw(
        s: &mut TextSelection,
        text: &str,
        width: u16,
        height: u16,
        wrap: Wrap,
        top: usize,
        left: usize,
    ) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| s.render(f, f.area(), TextLayout::plain(text, width, wrap), top, left))
            .unwrap();
        t
    }
    fn drag(s: &mut TextSelection, a: (u16, u16), b: (u16, u16)) {
        assert!(s
            .mouse(&mouse(MouseEventKind::Down(MouseButton::Left), a.0, a.1))
            .is_some());
        assert!(s
            .mouse(&mouse(MouseEventKind::Drag(MouseButton::Left), b.0, b.1))
            .is_some());
    }

    #[test]
    fn selection_mouse_release_copies_once_clears_highlight_and_keeps_caret() {
        let text = "中文 e\u{301}\n\n  body";
        let mut s = TextSelection::default();
        draw(&mut s, text, 20, 4, Wrap::Clip, 0, 0);
        for (from, to, caret) in [((0, 0), (19, 2), text.len()), ((19, 2), (0, 0), 0)] {
            drag(&mut s, from, to);
            assert_eq!(s.selected_text(), Some(text));
            let action = s
                .mouse(&mouse(MouseEventKind::Up(MouseButton::Left), to.0, to.1))
                .unwrap();
            assert!(matches!(action, Action::Copy { text: copied, .. } if copied == text));
            assert!(s.selected_text().is_none());
            assert_eq!(s.head(), caret);
            let t = draw(&mut s, text, 20, 4, Wrap::Clip, 0, 0);
            assert_eq!(t.backend().buffer()[(0, 0)].bg, idea::PANEL);
            assert!(s
                .mouse(&mouse(MouseEventKind::Up(MouseButton::Left), to.0, to.1))
                .is_none());
        }
        s.mouse(&mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));
        assert!(matches!(
            s.mouse(&mouse(MouseEventKind::Up(MouseButton::Left), 0, 0)),
            Some(Action::None)
        ));
        s.select_all();
        assert!(s
            .mouse(&mouse(MouseEventKind::Up(MouseButton::Left), 0, 0))
            .is_none());
        assert_eq!(s.selected_text(), Some(text));
    }

    #[test]
    fn selection_click_after_copy_ignores_release_remapping_and_jitter() {
        let text = "first line\nsecond line\nthird line\nfourth line";
        for case in 0..4 {
            let mut s = TextSelection::default();
            draw(&mut s, text, 30, 4, Wrap::Clip, 0, 0);
            drag(&mut s, (0, 0), (5, 0));
            assert!(
                matches!(s.mouse(&mouse(MouseEventKind::Up(MouseButton::Left),5,0)), Some(Action::Copy { text, .. }) if text == "first")
            );
            assert!(s.selected_text().is_none());
            s.mouse(&mouse(MouseEventKind::Down(MouseButton::Left), 3, 1));
            let caret = s.head();
            if case == 0 || case == 3 {
                let mut t = Terminal::new(TestBackend::new(30, 5)).unwrap();
                t.draw(|f| {
                    s.render(
                        f,
                        Rect::new(0, 1, 30, 4),
                        TextLayout::plain(text, 30, Wrap::Clip),
                        0,
                        0,
                    )
                })
                .unwrap();
            } else if case == 1 {
                draw(&mut s, text, 30, 4, Wrap::Clip, 1, 0);
            }
            if case == 3 {
                s.mouse(&mouse(MouseEventKind::Drag(MouseButton::Left), 3, 1));
                assert!(
                    s.selected_text().is_none(),
                    "same-cell mouse motion is not a drag"
                );
            }
            let x = if case == 2 { 4 } else { 3 };
            assert!(
                matches!(
                    s.mouse(&mouse(MouseEventKind::Up(MouseButton::Left), x, 1)),
                    Some(Action::None)
                ),
                "click case {case} unexpectedly copied"
            );
            assert!(s.selected_text().is_none());
            assert_eq!(
                s.head(),
                caret,
                "a click keeps the caret placed on mouse down"
            );
        }
    }

    #[test]
    fn selection_forward_reverse_multiline_and_padding_preserve_original_bytes() {
        let text = "  中文 e\u{301} 👩‍💻\n\n\t+ tail  ";
        let mut s = TextSelection::default();
        draw(&mut s, text, 30, 4, Wrap::Clip, 0, 0);
        drag(&mut s, (0, 0), (29, 2));
        assert_eq!(s.selected_text(), Some(text));
        let t = draw(&mut s, text, 30, 4, Wrap::Clip, 0, 0);
        assert_eq!(t.backend().buffer()[(2, 0)].bg, idea::SELECT);
        drag(&mut s, (29, 2), (0, 0));
        assert_eq!(s.selected_text(), Some(text));
        assert_eq!(previous("e\u{301}👩‍💻", "e\u{301}👩‍💻".len()), "e\u{301}".len());
        assert_eq!(next("e\u{301}\n", 0), "e\u{301}".len());
    }

    #[test]
    fn selection_soft_wrap_scroll_resize_and_source_changes() {
        let text = "alpha beta 中文 gamma\n  +original";
        let mut s = TextSelection::default();
        draw(&mut s, text, 10, 8, Wrap::Word, 0, 0);
        drag(&mut s, (0, 0), (10, 7));
        assert_eq!(s.selected_text(), Some(text));
        draw(&mut s, text, 6, 3, Wrap::Word, 1, 0);
        assert_eq!(s.selected_text(), Some(text));
        draw(&mut s, "other", 6, 3, Wrap::Word, 0, 0);
        assert!(s.selected_text().is_none());
        draw(&mut s, "zero\n012中文xyz", 6, 1, Wrap::Clip, 1, 3);
        drag(&mut s, (0, 0), (4, 0));
        assert_eq!(s.selected_text(), Some("中文"));
    }

    #[test]
    fn selection_clamps_drag_edges_ignores_click_and_excludes_decorations() {
        let mut s = TextSelection::default();
        let mut t = Terminal::new(TestBackend::new(10, 5)).unwrap();
        t.draw(|f| {
            let mut l = TextLayout::default();
            l.push(Line::raw("head"), 6, Wrap::Clip);
            l.decoration(Line::raw("──────"));
            l.push(Line::raw("+body"), 6, Wrap::Clip);
            s.render(f, Rect::new(2, 1, 6, 3), l, 0, 0);
        })
        .unwrap();
        drag(&mut s, (2, 1), (2, 1));
        assert!(s.selected_text().is_none());
        drag(&mut s, (2, 1), (50, 50));
        assert_eq!(s.selected_text(), Some("head\n+body"));
        draw(&mut s, "中文", 1, 1, Wrap::Word, 0, 0);
        s.select_all();
        assert_eq!(s.selected_text(), Some("中文"));
    }

    #[test]
    fn selection_blank_lines_are_visible_and_partial_wide_glyphs_are_not_copied() {
        let mut s = TextSelection::default();
        draw(&mut s, "\n", 4, 2, Wrap::Clip, 0, 0);
        s.select_all();
        let t = draw(&mut s, "\n", 4, 2, Wrap::Clip, 0, 0);
        assert_eq!(s.selected_text(), Some("\n"));
        assert_eq!(t.backend().buffer()[(0, 0)].bg, idea::SELECT);
        assert_eq!(t.backend().buffer()[(0, 1)].bg, idea::PANEL);
        let t = draw(&mut s, "012中文xyz", 6, 1, Wrap::Clip, 0, 4);
        assert_eq!(t.backend().buffer()[(0, 0)].symbol(), " ");
        drag(&mut s, (0, 0), (3, 0));
        assert_eq!(s.selected_text(), Some("文"));
    }
}
