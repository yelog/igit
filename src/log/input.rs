use crate::ui::selection::{at_column, next, previous};
use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as Mods};
use ratatui::{layout::Rect, widgets::Paragraph, Frame};
use unicode_width::UnicodeWidthStr;

pub(super) struct TextInput {
    pub(super) text: String,
    caret: usize,
    area: Rect,
    top: usize,
    left: usize,
}

impl TextInput {
    pub(super) fn new(text: String) -> Self {
        Self {
            caret: text.len(),
            text,
            area: Rect::default(),
            top: 0,
            left: 0,
        }
    }

    fn insert(&mut self, value: &str) {
        self.text.insert_str(self.caret, value);
        self.caret += value.len();
    }

    pub(super) fn paste(&mut self, value: &str) {
        self.insert(&value.replace("\r\n", "\n").replace('\r', "\n"));
    }

    fn position(&self) -> (usize, usize) {
        let prefix = &self.text[..self.caret];
        let col = prefix.rsplit('\n').next().unwrap_or("").width();
        (prefix.bytes().filter(|&b| b == b'\n').count(), col)
    }

    fn go_to(&mut self, row: usize, col: usize) {
        let mut offset = 0;
        for (n, line) in self.text.split('\n').enumerate() {
            if n == row {
                self.caret = offset + at_column(line, col);
                return;
            }
            offset += line.len() + 1;
        }
        self.caret = self.text.len();
    }

    pub(super) fn vertical(&mut self, delta: isize) {
        let (row, col) = self.position();
        let last = self.text.bytes().filter(|&b| b == b'\n').count();
        self.go_to(row.saturating_add_signed(delta).min(last), col);
    }

    pub(super) fn place(&mut self, x: u16, y: u16) {
        self.go_to(
            self.top + usize::from(y.saturating_sub(self.area.y)),
            self.left + usize::from(x.saturating_sub(self.area.x)),
        );
    }

    pub(super) fn key(&mut self, k: &KeyEvent) {
        let pos = self.caret;
        match k.code {
            K::Enter => self.insert("\n"),
            K::Char('u') if k.modifiers.contains(Mods::CONTROL) => {
                self.text.clear();
                self.caret = 0;
            }
            K::Char(ch)
                if !k
                    .modifiers
                    .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER) =>
            {
                self.text.insert(pos, ch);
                self.caret += ch.len_utf8();
            }
            K::Backspace => {
                self.caret = previous(&self.text, pos);
                self.text.replace_range(self.caret..pos, "");
            }
            K::Delete => self.text.replace_range(pos..next(&self.text, pos), ""),
            K::Left => self.caret = previous(&self.text, pos),
            K::Right => self.caret = next(&self.text, pos),
            K::Home => self.caret = self.text[..pos].rfind('\n').map_or(0, |i| i + 1),
            K::End => {
                self.caret = self.text[pos..]
                    .find('\n')
                    .map_or(self.text.len(), |i| pos + i)
            }
            K::Up => self.vertical(-1),
            K::Down => self.vertical(1),
            _ => (),
        }
    }

    pub(super) fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.area = area;
        if !self.area.is_empty() {
            let (row, col) = self.position();
            self.top = self
                .top
                .min(row)
                .max(row.saturating_sub(usize::from(self.area.height - 1)));
            self.left = self
                .left
                .min(col)
                .max(col.saturating_sub(usize::from(self.area.width - 1)));
            for (dy, line) in self
                .text
                .split('\n')
                .skip(self.top)
                .take(self.area.height.into())
                .enumerate()
            {
                let r = Rect::new(self.area.x, self.area.y + dy as u16, self.area.width, 1);
                f.render_widget(
                    Paragraph::new(line).scroll((0, self.left.min(u16::MAX.into()) as u16)),
                    r,
                );
            }
            f.set_cursor_position((
                self.area.x + (col - self.left) as u16,
                self.area.y + (row - self.top) as u16,
            ));
        }
    }
}
