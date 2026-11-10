//! Literal find in a pane's loaded model. Row keys come from the pane, and
//! byte ranges always refer to the original text, including for Unicode.
use super::{help::Binding, idea, selection};
use crate::config::keys::normalize;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers as M};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::{
    borrow::{Borrow, Cow},
    cmp::Ordering,
    ops::Range,
};
use unicode_width::UnicodeWidthStr;

pub(crate) const BINDINGS: &[Binding] = &[
    ("/", "Find text in this pane"),
    ("n", "Jump to next match"),
    ("N", "Jump to previous match"),
    ("Esc", "Clear pane search"),
];
pub(crate) const INPUT_BINDINGS: &[Binding] = &[
    ("Enter", "Find and jump to the first match"),
    ("Esc", "Cancel editing the search"),
    ("Ctrl-w", "Delete word before cursor"),
    ("Ctrl-u", "Clear search input"),
];

pub(crate) const MATCH: Style = Style::new()
    .fg(Color::Rgb(255, 219, 150))
    .bg(Color::Rgb(87, 63, 36));
pub(crate) const CURRENT: Style = Style::new()
    .fg(idea::PANEL)
    .bg(Color::Rgb(255, 193, 98))
    .add_modifier(Modifier::BOLD);

fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

#[derive(Clone, Default)]
struct Pattern {
    text: String,
    folded: String,
}

impl Pattern {
    fn visit(&self, text: &str, mut hit: impl FnMut(Range<usize>)) {
        if self.folded.is_empty() {
            return;
        }
        if text.is_ascii() {
            if !self.folded.is_ascii() {
                return;
            }
            let folded = if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
                Cow::Owned(text.to_ascii_lowercase())
            } else {
                Cow::Borrowed(text)
            };
            for (start, term) in folded.match_indices(&self.folded) {
                hit(start..start + term.len());
            }
            return;
        }
        let mut folded = String::with_capacity(text.len());
        let mut offsets = Vec::new();
        for (byte, ch) in text.char_indices() {
            offsets.push((folded.len(), byte));
            folded.extend(ch.to_lowercase());
        }
        offsets.push((folded.len(), text.len()));
        let mut previous = None;
        for (start, term) in folded.match_indices(&self.folded) {
            let from = offsets.partition_point(|&(i, _)| i <= start) - 1;
            let to = offsets.partition_point(|&(i, _)| i < start + term.len());
            let range = offsets[from].1..offsets[to].1;
            // A lowercased scalar can occupy more than one scalar in the search text.
            if previous.as_ref() != Some(&range) {
                hit(range.clone());
                previous = Some(range);
            }
        }
    }

    fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut hits = Vec::new();
        self.visit(text, |hit| hits.push(hit));
        hits
    }
}

#[derive(Clone, Default)]
struct Draft {
    pattern: Pattern,
    caret: usize,
}

impl Draft {
    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.caret = range.start + text.len();
        self.pattern.text.replace_range(range, text);
        self.pattern.folded = fold(&self.pattern.text);
    }
}

#[derive(Clone, Default)]
pub(crate) struct Prompt {
    query: Pattern,
    draft: Option<Draft>,
    position: Option<(usize, usize)>,
}

impl Prompt {
    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }

    pub fn visible(&self) -> bool {
        self.editing() || !self.query.text.is_empty()
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        if area.width < 3 || area.is_empty() {
            return;
        }
        let (text, caret, hint) = if let Some(draft) = &self.draft {
            (
                draft.pattern.text.as_str(),
                Some(draft.caret),
                "  Enter Find · Esc Cancel".into(),
            )
        } else {
            (
                self.query.text.as_str(),
                None,
                match self.position {
                    Some((0, _)) => "  No matches".into(),
                    Some((index, total)) => format!("  {index}/{total}"),
                    None => String::new(),
                },
            )
        };
        let available = usize::from(area.width) - 2;
        let hint = if hint.width() + 8 <= available {
            &hint
        } else {
            ""
        };
        let width = available - hint.width();
        let column = caret.map_or(0, |caret| text[..caret].width());
        let start = if caret.is_some() && column >= width {
            // Keep the insertion point visible, advancing by complete graphemes.
            selection::next(text, selection::at_column(text, column - width))
        } else {
            0
        };
        let visible = &text[start..];
        let shown = &visible[..selection::at_column(visible, width)];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " /",
                    Style::new().fg(idea::BLUE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(shown, Style::new().fg(idea::INK)),
                Span::styled(hint, Style::new().fg(idea::DIM)),
            ]))
            .style(Style::new().bg(idea::TAG)),
            area,
        );
        if let Some(caret) = caret {
            let x = text[start.min(caret)..caret].width().min(width - 1) as u16;
            f.set_cursor_position((area.x + 2 + x, area.y));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Jump {
    First,
    Next,
    Previous,
}

pub(crate) enum Input {
    Pass,
    Handled,
    Jump(Jump),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Hit<K> {
    pub row: K,
    pub field: usize,
    pub bytes: Range<usize>,
}

impl<K> Hit<K> {
    /// Pan to complete terminal glyphs, even for a match inside a grapheme.
    pub fn cells(&self, text: &str) -> Range<usize> {
        let mut bytes = 0;
        let mut column = 0;
        let mut first = None;
        for glyph in Span::raw(text).styled_graphemes(Style::default()) {
            bytes += glyph.symbol.len();
            if bytes > self.bytes.start && first.is_none() {
                first = Some(column);
            }
            column += glyph.symbol.width();
            if bytes >= self.bytes.end {
                break;
            }
        }
        first.unwrap_or(column)..column
    }
}

#[derive(Clone)]
pub(crate) struct Search<K = usize> {
    prompt: Prompt,
    current: Option<Hit<K>>,
}

impl<K> Default for Search<K> {
    fn default() -> Self {
        Self {
            prompt: Prompt::default(),
            current: None,
        }
    }
}

impl<K: Eq> Search<K> {
    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    pub fn invalidate(&mut self) {
        self.current = None;
        self.prompt.position = None;
    }

    /// Retain session input when replacing a model or changing search row keys.
    /// Hit positions belong to the destination's current rows.
    pub fn inherit_query(&mut self, prompt: &Prompt) {
        self.prompt.clone_from(prompt);
        self.invalidate();
    }

    pub fn event(&mut self, event: &Event) -> Input {
        if matches!(event, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return Input::Pass;
        }
        if let Some(draft) = &mut self.prompt.draft {
            match event {
                Event::Paste(text) => {
                    let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                    draft.replace(draft.caret..draft.caret, &text);
                }
                Event::Key(key) => {
                    let key = normalize(*key);
                    let pos = draft.caret;
                    let text = &draft.pattern.text;
                    match (key.code, key.modifiers) {
                        (KeyCode::Enter, M::NONE) => {
                            self.prompt.query = self.prompt.draft.take().unwrap().pattern;
                            self.invalidate();
                            if !self.prompt.query.text.is_empty() {
                                return Input::Jump(Jump::First);
                            }
                        }
                        (KeyCode::Esc, M::NONE) => self.prompt.draft = None,
                        (KeyCode::Char('w'), M::CONTROL) => {
                            draft.replace(selection::previous_word(text, pos)..pos, "");
                        }
                        (KeyCode::Char('u'), M::CONTROL) => draft.replace(0..text.len(), ""),
                        (KeyCode::Char(ch), M::NONE) if !ch.is_control() => {
                            draft.replace(pos..pos, ch.encode_utf8(&mut [0; 4]))
                        }
                        (KeyCode::Left, M::NONE) => draft.caret = selection::previous(text, pos),
                        (KeyCode::Right, M::NONE) => draft.caret = selection::next(text, pos),
                        (KeyCode::Home, M::NONE) => draft.caret = 0,
                        (KeyCode::End, M::NONE) => draft.caret = text.len(),
                        (KeyCode::Backspace, M::NONE) => {
                            draft.replace(selection::previous(text, pos)..pos, "");
                        }
                        (KeyCode::Delete, M::NONE) => {
                            draft.replace(pos..selection::next(text, pos), "");
                        }
                        _ => (),
                    }
                }
                Event::Mouse(_) => (),
                _ => return Input::Pass,
            }
            return Input::Handled;
        }
        let Event::Key(key) = event else {
            return Input::Pass;
        };
        let key = normalize(*key);
        match (key.code, key.modifiers) {
            (KeyCode::Char('/'), M::NONE) => {
                self.prompt.draft = Some(Draft::default());
                Input::Handled
            }
            (KeyCode::Char('n'), M::NONE) if self.prompt.visible() => Input::Jump(Jump::Next),
            (KeyCode::Char('N'), M::NONE) if self.prompt.visible() => Input::Jump(Jump::Previous),
            (KeyCode::Esc, M::NONE) if self.prompt.visible() => {
                *self = Self::default();
                Input::Handled
            }
            _ => Input::Pass,
        }
    }

    pub fn field(&self, row: &K, field: usize) -> Highlight<'_> {
        let pattern = &self.prompt.query;
        if let Some(draft) = &self.prompt.draft {
            return Highlight {
                pattern: &draft.pattern,
                current: None,
            };
        }
        Highlight {
            pattern,
            current: self
                .current
                .as_ref()
                .filter(|hit| &hit.row == row && hit.field == field)
                .map(|hit| &hit.bytes),
        }
    }

    /// Rows are in display order before folding. The row key survives scrolling
    /// and lets a second match on the same row participate in n/N navigation.
    pub fn jump<Q, R, T>(
        &mut self,
        direction: Jump,
        anchor: Option<&K>,
        rows: impl IntoIterator<Item = (Q, R)>,
    ) -> Option<Hit<K>>
    where
        K: Clone,
        Q: Borrow<K>,
        R: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let mut count = 0;
        let mut anchor_seen = false;
        let mut chosen = None;
        let mut wrap = None;
        let reverse = direction == Jump::Previous;
        for (key, fields) in rows {
            let key = key.borrow();
            let at_anchor = Some(key) == anchor;
            let row_order = if at_anchor {
                Ordering::Equal
            } else if anchor_seen {
                Ordering::Greater
            } else {
                Ordering::Less
            };
            anchor_seen |= at_anchor;
            for (field, text) in fields.into_iter().enumerate() {
                self.prompt.query.visit(text.as_ref(), |bytes| {
                    count += 1;
                    let candidate = || {
                        (
                            count,
                            Hit {
                                row: key.clone(),
                                field,
                                bytes: bytes.clone(),
                            },
                        )
                    };
                    if reverse || wrap.is_none() {
                        wrap = Some(candidate());
                    }
                    let order = self
                        .current
                        .as_ref()
                        .filter(|hit| at_anchor && &hit.row == key)
                        .map_or(row_order, |hit| {
                            (field, bytes.start).cmp(&(hit.field, hit.bytes.start))
                        });
                    let eligible = match direction {
                        Jump::First => row_order != Ordering::Less,
                        Jump::Next => order == Ordering::Greater,
                        Jump::Previous => order == Ordering::Less,
                    };
                    if eligible && (reverse || chosen.is_none()) {
                        chosen = Some(candidate());
                    }
                });
            }
        }
        let (index, hit) = chosen.or(wrap).map_or((0, None), |(i, hit)| (i, Some(hit)));
        self.prompt.position = Some((index, count));
        self.current = hit;
        self.current.clone()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Highlight<'a> {
    pattern: &'a Pattern,
    current: Option<&'a Range<usize>>,
}

impl Default for Highlight<'_> {
    fn default() -> Self {
        static EMPTY: Pattern = Pattern {
            text: String::new(),
            folded: String::new(),
        };
        Self {
            pattern: &EMPTY,
            current: None,
        }
    }
}

impl Highlight<'_> {
    pub fn spans(self, text: &str, style: Style) -> Vec<Span<'static>> {
        self.clipped_with(text, usize::MAX, style, idea::clipped)
    }

    pub fn clipped(self, text: &str, width: usize, style: Style) -> Vec<Span<'static>> {
        self.clipped_with(text, width, style, idea::clipped)
    }

    pub fn clipped_with(
        self,
        text: &str,
        width: usize,
        style: Style,
        normal: impl FnOnce(&str, usize) -> String,
    ) -> Vec<Span<'static>> {
        let hits = self.pattern.ranges(text);
        let Some(first) = hits.first() else {
            return vec![Span::styled(normal(text, width), style)];
        };
        if width == 0 {
            return vec![];
        }
        let current = self.current.filter(|r| hits.contains(r));
        let target = current.unwrap_or(first);
        let overflow = width != usize::MAX && text.width() > width;
        let start = if overflow && text[..target.end].width() >= width {
            let mut start = 0;
            for glyph in Span::raw(text).styled_graphemes(Style::default()) {
                let end = start + glyph.symbol.len();
                if end > target.start {
                    break;
                }
                start = end;
            }
            start
        } else {
            0
        };
        let prefix = start > 0 && width >= 3;
        let mut budget = width - usize::from(prefix);
        let tail = &text[start..];
        let suffix = overflow
            && tail.width() > budget
            && budget > 1
            && text[start..target.end].width() < budget;
        budget -= usize::from(suffix);
        let tail = if overflow {
            &tail[..selection::at_column(tail, budget)]
        } else {
            tail
        };
        let mono = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let (matched, focused) = if mono {
            (
                Style::new().add_modifier(Modifier::UNDERLINED),
                Style::new().add_modifier(Modifier::REVERSED),
            )
        } else {
            (MATCH, CURRENT)
        };
        let mut out = Vec::<Span<'static>>::new();
        if prefix {
            out.push(Span::styled("…", style));
        }
        let mut byte = start;
        let mut hits = hits.iter().peekable();
        for glyph in Span::raw(tail).styled_graphemes(style) {
            let end = byte + glyph.symbol.len();
            while hits.peek().is_some_and(|hit| hit.end <= byte) {
                hits.next();
            }
            let overlaps = |r: &Range<usize>| byte < r.end && r.start < end;
            let ink = if current.is_some_and(overlaps) {
                style.patch(focused)
            } else if hits.peek().is_some_and(|r| overlaps(r)) {
                style.patch(matched)
            } else {
                style
            };
            if let Some(last) = out.last_mut().filter(|s| s.style == ink) {
                last.content.to_mut().push_str(glyph.symbol);
            } else {
                out.push(Span::styled(glyph.symbol.to_owned(), ink));
            }
            byte = end;
        }
        if suffix {
            out.push(Span::styled("…", style));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, M::NONE))
    }

    fn begin(search: &mut Search, text: &str) {
        assert!(matches!(
            search.event(&key(KeyCode::Char('/'))),
            Input::Handled
        ));
        search.event(&Event::Paste(text.into()));
    }

    #[test]
    fn search_ctrl_w_deletes_the_previous_word_and_preserves_the_suffix() {
        for (prefix, suffix, remaining) in [
            ("", "", ""),
            ("   ", "", ""),
            ("alpha beta   ", "", "alpha "),
            ("alpha/path.rs", "", ""),
            ("前缀\u{3000}中文\u{2003}", " suffix", "前缀\u{3000}"),
            ("alpha e\u{301}👩‍💻 ", "界", "alpha "),
            ("alpha be", "ta suffix", "alpha "),
            ("", "untouched", ""),
        ] {
            let mut search = Search::default();
            begin(&mut search, &format!("{prefix}{suffix}"));
            search.prompt.draft.as_mut().unwrap().caret = prefix.len();
            assert!(matches!(
                search.event(&Event::Key(KeyEvent::new(KeyCode::Char('w'), M::CONTROL))),
                Input::Handled
            ));
            let draft = search.prompt.draft.as_ref().unwrap();
            let expected = format!("{remaining}{suffix}");
            assert_eq!(draft.pattern.text, expected);
            assert_eq!(draft.pattern.folded, fold(&expected));
            assert_eq!(draft.caret, remaining.len());
            search.event(&key(KeyCode::Enter));
            assert_eq!(search.prompt.query.text, expected);
        }
    }

    #[test]
    fn pane_search_uses_literal_ranges_and_preserves_unicode_graphemes() {
        let mut search = Search::default();
        begin(&mut search, "é[.]");
        assert!(matches!(
            search.event(&key(KeyCode::Enter)),
            Input::Jump(Jump::First)
        ));
        let text = "目录 É[.] é[x] é[.]";
        let first = search.jump(Jump::First, None, [(0, [text])]).unwrap();
        assert_eq!(&text[first.bytes], "É[.]");
        assert_eq!(search.prompt.position, Some((1, 2)));
        let spans = search.field(&0, 0).spans(text, Style::default());
        assert_eq!(Line::from(spans.clone()).to_string(), text);
        assert_eq!(spans.len(), 4);

        begin(&mut search, "i");
        search.event(&key(KeyCode::Enter));
        assert_eq!(
            search.jump(Jump::First, None, [(0, ["İ"])]).unwrap().bytes,
            0..2
        );

        begin(&mut search, "\u{301}");
        search.event(&key(KeyCode::Enter));
        let text = "some long text a\u{301} more";
        search.jump(Jump::First, None, [(0, [text])]).unwrap();
        let spans = search.field(&0, 0).clipped(text, 3, Style::default());
        assert!(Line::from(spans.clone()).to_string().contains("a\u{301}"));
        assert!(spans
            .iter()
            .any(|span| span.content.contains("a\u{301}") && span.style != Style::default()));
    }

    #[test]
    fn pane_search_steps_through_occurrences_then_wraps_and_cancel_restores_query() {
        let rows = [(1, ["a z", ""]), (2, ["a a", "A"]), (3, ["no hit", ""])];
        let mut search = Search::default();
        begin(&mut search, "a");
        search.event(&key(KeyCode::Enter));
        let mut jump = |direction, anchor| search.jump(direction, Some(&anchor), rows).unwrap();
        let first = jump(Jump::First, 2);
        assert_eq!((first.row, first.field, first.bytes), (2, 0, 0..1));
        assert_eq!(jump(Jump::Next, 2).bytes, 2..3);
        assert_eq!(jump(Jump::Next, 2).field, 1);
        assert_eq!(jump(Jump::Next, 2).row, 1);
        assert_eq!(jump(Jump::Previous, 1).field, 1);
        assert_eq!(jump(Jump::Previous, 3).row, 2);

        begin(&mut search, "no hit");
        assert_eq!(search.field(&3, 0).pattern.text, "no hit");
        search.event(&key(KeyCode::Esc));
        assert_eq!(search.field(&3, 0).pattern.text, "a");
        assert!(!search.prompt.editing());
        search.event(&key(KeyCode::Esc));
        assert!(!search.prompt.visible());
    }

    #[test]
    fn pane_search_inherits_query_across_row_keys_without_old_hit_positions() {
        let mut original: Search<(usize, usize)> = Search::default();
        original.event(&key(KeyCode::Char('/')));
        original.event(&Event::Paste("名".into()));
        original.event(&key(KeyCode::Enter));
        original.jump(Jump::First, None, [((1, 80), ["old 名 名"])]);
        assert!(original.current.is_some());
        assert_eq!(original.prompt.position, Some((1, 2)));

        let mut patch: Search<usize> = Search::default();
        patch.inherit_query(original.prompt());
        assert!(patch.prompt.visible());
        assert!(patch.current.is_none());
        assert!(patch.prompt.position.is_none());
        let first = patch.jump(Jump::First, None, [(0, ["名 new"]), (1, ["no match"])]);
        assert_eq!(first.unwrap().row, 0);
        assert_eq!(patch.prompt.position, Some((1, 1)));
        assert_eq!(
            original.current.as_ref().unwrap().row,
            (1, 80),
            "transferring input leaves the source search untouched"
        );

        original.event(&key(KeyCode::Char('/')));
        original.event(&Event::Paste("new".into()));
        patch.inherit_query(original.prompt());
        assert!(patch.prompt.editing());
        assert!(patch.current.is_none());
        assert!(patch.prompt.position.is_none());
        let spans = patch.field(&0, 0).spans("名 new", Style::default());
        assert!(spans
            .iter()
            .any(|span| span.content == "new" && span.style != Style::default()));
        patch.event(&key(KeyCode::Esc));
        assert_eq!(patch.prompt.query.text, "名");
        assert!(!patch.prompt.editing());
        assert!(original.prompt.editing());

        let mut empty: Search<String> = Search::default();
        empty.inherit_query(patch.prompt());
        patch.event(&key(KeyCode::Esc));
        empty.inherit_query(patch.prompt());
        assert!(!empty.prompt.visible());
        assert!(empty.current.is_none());
    }
}
