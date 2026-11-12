use super::{model::Remote, *};
use crate::ui::{
    idea::{button_style, clipped, TAG},
    selection,
};
use ratatui::{
    style::Style,
    widgets::{Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(super) enum Event {
    Keep,
    Cancel,
    Review,
}

struct Field {
    text: String,
    caret: usize,
    area: Rect,
    top: usize,
    left: usize,
}

impl Field {
    fn new(text: String) -> Self {
        Self {
            caret: text.len(),
            text,
            area: Rect::default(),
            top: 0,
            left: 0,
        }
    }

    fn line_start(&self) -> usize {
        self.text[..self.caret]
            .rfind('\n')
            .map_or(0, |position| position + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.caret..]
            .find('\n')
            .map_or(self.text.len(), |position| self.caret + position)
    }

    fn vertical(&mut self, down: bool) {
        let start = self.line_start();
        let column = self.text[start..self.caret].width();
        let next = if down {
            let end = self.line_end();
            if end == self.text.len() {
                return;
            }
            end + 1
        } else {
            if start == 0 {
                return;
            }
            self.text[..start - 1]
                .rfind('\n')
                .map_or(0, |position| position + 1)
        };
        let line = self.text[next..].split('\n').next().unwrap_or_default();
        self.caret = next + selection::at_column(line, column);
    }

    fn insert(&mut self, text: &str, multiline: bool) {
        let text = text.replace("\r\n", "\n");
        let text: String = text
            .chars()
            .filter(|character| !character.is_control() || multiline && *character == '\n')
            .collect();
        self.text.insert_str(self.caret, &text);
        self.caret += text.len();
    }

    fn key(&mut self, key: &crossterm::event::KeyEvent, multiline: bool) {
        if key.modifiers == Mods::CONTROL {
            match key.code {
                K::Char('u') => {
                    self.text.clear();
                    self.caret = 0;
                }
                K::Char('a') => self.caret = self.line_start(),
                K::Char('e') => self.caret = self.line_end(),
                K::Char('w') => {
                    let previous = selection::previous_word(&self.text, self.caret);
                    self.text.drain(previous..self.caret);
                    self.caret = previous;
                }
                _ => (),
            }
            return;
        }
        if key
            .modifiers
            .intersects(Mods::CONTROL | Mods::ALT | Mods::SUPER)
        {
            return;
        }
        match key.code {
            K::Char(character) => self.insert(&character.to_string(), multiline),
            K::Enter if multiline => self.insert("\n", true),
            K::Left => self.caret = selection::previous(&self.text, self.caret),
            K::Right => self.caret = selection::next(&self.text, self.caret),
            K::Home => self.caret = self.line_start(),
            K::End => self.caret = self.line_end(),
            K::Up => self.vertical(false),
            K::Down => self.vertical(true),
            K::Backspace => {
                let previous = selection::previous(&self.text, self.caret);
                self.text.drain(previous..self.caret);
                self.caret = previous;
            }
            K::Delete => {
                let next = selection::next(&self.text, self.caret);
                self.text.drain(self.caret..next);
            }
            _ => (),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, focused: bool, active: bool) {
        self.area = area;
        if area.is_empty() {
            return;
        }
        let line_index = self.text[..self.caret].matches('\n').count();
        let column = self.text[self.line_start()..self.caret].width();
        let height = usize::from(area.height);
        let width = usize::from(area.width);
        if line_index < self.top {
            self.top = line_index;
        } else if line_index >= self.top + height {
            self.top = line_index + 1 - height;
        }
        self.left = if column >= width {
            column + 1 - width
        } else {
            0
        };
        let lines: Vec<_> = self
            .text
            .split('\n')
            .skip(self.top)
            .take(height)
            .map(|line| {
                let start = selection::at_column(line, self.left);
                clipped(&line[start..], width)
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines.join("\n")).style(Style::new().fg(FG).bg(if focused {
                TAG
            } else {
                BG
            })),
            area,
        );
        if focused && active {
            let line = self.text[self.line_start()..]
                .split('\n')
                .next()
                .unwrap_or_default();
            let start = selection::at_column(line, self.left);
            let visible_column = line[start..self.caret - self.line_start()].width();
            f.set_cursor_position((
                area.x + (visible_column as u16).min(area.width - 1),
                area.y + (line_index - self.top) as u16,
            ));
        }
    }

    fn click(&mut self, x: u16, y: u16) {
        let row = self.top + usize::from(y.saturating_sub(self.area.y));
        let mut start = 0;
        for (index, line) in self.text.split('\n').enumerate() {
            if index == row {
                self.caret = start
                    + selection::at_column(
                        line,
                        self.left + usize::from(x.saturating_sub(self.area.x)),
                    );
                return;
            }
            start += line.len() + 1;
        }
        self.caret = self.text.len();
    }
}

pub(super) struct Editor {
    pub root: usize,
    pub source: Option<Remote>,
    fields: [Field; 3],
    focus: usize,
    buttons: [Rect; 2],
}

impl Editor {
    pub fn new(root: usize, source: Option<Remote>, default_name: &str) -> Self {
        let values = source.as_ref().map_or_else(
            || [default_name.into(), String::new(), String::new()],
            |remote| {
                [
                    remote.name.clone(),
                    remote.urls.join("\n"),
                    remote.push_urls.join("\n"),
                ]
            },
        );
        Self {
            root,
            source,
            fields: values.map(Field::new),
            focus: 0,
            buttons: [Rect::default(); 2],
        }
    }

    pub fn target(&self) -> Remote {
        let urls = |index: usize| {
            self.fields[index]
                .text
                .split('\n')
                .filter(|line| !line.is_empty())
                .map(String::from)
                .collect()
        };
        Remote {
            name: self.fields[0].text.clone(),
            urls: urls(1),
            push_urls: urls(2),
        }
    }

    pub fn event(&mut self, event: &crossterm::event::Event) -> Event {
        match event {
            crossterm::event::Event::Key(key) => match (key.code, key.modifiers) {
                (K::Esc, _) => return Event::Cancel,
                (K::Char('s'), Mods::CONTROL) => return Event::Review,
                (K::Tab, Mods::NONE) => self.focus = (self.focus + 1) % 5,
                (K::BackTab, _) | (K::Tab, Mods::SHIFT) => {
                    self.focus = (self.focus + 4) % 5;
                }
                (K::Enter, _) if self.focus == 3 => return Event::Review,
                (K::Enter, _) if self.focus == 4 => return Event::Cancel,
                (K::Enter, _) if self.focus == 0 => self.focus = 1,
                (K::Left | K::Right, _) if self.focus >= 3 => {
                    self.focus = 7 - self.focus;
                }
                _ if self.focus < 3 => {
                    self.fields[self.focus].key(key, self.focus != 0);
                }
                _ => (),
            },
            crossterm::event::Event::Paste(text) if self.focus < 3 => {
                self.fields[self.focus].insert(text, self.focus != 0);
            }
            crossterm::event::Event::Mouse(mouse) if mouse.kind == M::Down(MouseButton::Left) => {
                for (index, rect) in self.buttons.iter().enumerate() {
                    if inside(*rect, mouse.column, mouse.row) {
                        self.focus = index + 3;
                        return if index == 0 {
                            Event::Review
                        } else {
                            Event::Cancel
                        };
                    }
                }
                for (index, field) in self.fields.iter_mut().enumerate() {
                    if inside(field.area, mouse.column, mouse.row) {
                        self.focus = index;
                        field.click(mouse.column, mouse.row);
                        break;
                    }
                }
            }
            _ => (),
        }
        Event::Keep
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context, active: bool) {
        let width = area.width.saturating_sub(4).min(88);
        let height = area.height.min(18);
        let frame = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, frame);
        let block = pane(
            if self.source.is_some() {
                "Edit remote"
            } else {
                "Add remote"
            },
            true,
        );
        let inner = block.inner(frame);
        f.render_widget(block, frame);
        self.buttons = [Rect::default(); 2];
        if inner.width < 2 || inner.height < 8 {
            return;
        }
        let content = Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height);
        let repository = c.repos.get(self.root).map_or_else(
            || "Repository unavailable".into(),
            |repo| format!("Repository: {}", model::label(&repo.name)),
        );
        f.render_widget(
            Paragraph::new(clipped(&repository, usize::from(content.width)))
                .style(Style::new().fg(MUTED)),
            Rect::new(content.x, content.y, content.width, 1),
        );
        let url_height = content.height.saturating_sub(6) / 2;
        let mut y = content.y + 1;
        for (index, label) in [
            "Name",
            "Fetch URLs (one per line)",
            "Push URLs (empty = fetch URLs)",
        ]
        .into_iter()
        .enumerate()
        {
            f.render_widget(
                Paragraph::new(clipped(label, usize::from(content.width)))
                    .style(Style::new().fg(if self.focus == index { ACCENT } else { MUTED })),
                Rect::new(content.x, y, content.width, 1),
            );
            y += 1;
            let field_height = if index == 0 { 1 } else { url_height.max(1) };
            self.fields[index].draw(
                f,
                Rect::new(content.x, y, content.width, field_height),
                self.focus == index,
                active,
            );
            y += field_height;
        }
        let button_y = content.bottom() - 1;
        let mut right = content.right();
        for (index, label) in [(1, " Cancel "), (0, " Review ")] {
            let width = label.width() as u16;
            right = right.saturating_sub(width);
            let rect = Rect::new(right, button_y, width, 1);
            self.buttons[index] = rect;
            f.render_widget(
                Paragraph::new(label).style(button_style(self.focus == index + 3)),
                rect,
            );
            right = right.saturating_sub(1);
        }
    }
}
