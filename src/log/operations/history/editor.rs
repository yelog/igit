use super::{Draft, Step};
use crate::{
    log::{input::TextInput, *},
    ui::{help::HelpSection, idea::button_style},
};
use crossterm::event::KeyEventKind;

pub(in crate::log) enum Response {
    None,
    Close,
    Review,
}

#[derive(Clone, Copy)]
enum Hit {
    Row(usize),
    Step(Step),
    Move(isize),
    Message,
    Text,
    Save,
    Review,
    Cancel,
}

pub(in crate::log) struct Editor {
    pub draft: Draft,
    row: usize,
    message: Option<TextInput>,
    hits: Vec<(Rect, Hit)>,
}

impl Editor {
    pub fn new(draft: Draft) -> Self {
        Self {
            draft,
            row: 0,
            message: None,
            hits: Vec::new(),
        }
    }

    pub fn help(&self) -> HelpSection {
        if self.message.is_some() {
            HelpSection::new(
                "Log / Squash Message",
                &[
                    ("Enter", "Insert a newline"),
                    ("F2", "Save squash message in plan"),
                    ("Ctrl-s", "Save squash message in plan"),
                    ("Ctrl-u", "Clear squash message"),
                    ("Esc", "Discard message edit; return to plan"),
                ],
            )
        } else {
            HelpSection::new(
                "Log / Linear History Plan",
                &[
                    ("j", "Select next commit in execution order"),
                    ("k", "Select previous commit"),
                    ("J", "Move selected commit later"),
                    ("K", "Move selected commit earlier"),
                    ("p", "Keep selected commit separate"),
                    ("s", "Squash into preceding group and edit message"),
                    ("f", "Fixup into preceding group; retain its message"),
                    ("m", "Edit selected squash message"),
                    ("F2", "Open separate final review"),
                    ("Ctrl-s", "Open separate final review"),
                    ("Esc", "Cancel history plan without writing"),
                ],
            )
        }
    }

    fn action(&mut self, hit: Hit) -> Result<Response> {
        match hit {
            Hit::Row(row) => self.row = row,
            Hit::Step(step) => {
                self.draft.set_step(self.row, step)?;
                if step == Step::Squash {
                    self.message =
                        Some(TextInput::new(self.draft.entries[self.row].message.clone()));
                }
            }
            Hit::Move(delta) => self.row = self.draft.move_by(self.row, delta)?,
            Hit::Message => {
                anyhow::ensure!(
                    self.draft.entries[self.row].step == Step::Squash,
                    "Select a squash row to edit its combined message"
                );
                self.message = Some(TextInput::new(self.draft.entries[self.row].message.clone()));
            }
            Hit::Text => (),
            Hit::Save => {
                if let Some(input) = &self.message {
                    anyhow::ensure!(
                        !input.text.trim().is_empty() && !input.text.contains('\0'),
                        "Enter a non-empty squash message without NUL bytes"
                    );
                    self.draft.entries[self.row].message = input.text.clone();
                    self.message = None;
                }
            }
            Hit::Review => return Ok(Response::Review),
            Hit::Cancel => {
                if self.message.take().is_none() {
                    return Ok(Response::Close);
                }
            }
        }
        Ok(Response::None)
    }

    pub fn event(&mut self, event: &Event) -> Result<Response> {
        let hit = match event {
            Event::Key(key) if key.kind == KeyEventKind::Release => None,
            Event::Key(key) if key.code == K::Esc => Some(Hit::Cancel),
            Event::Key(key)
                if key.code == K::F(2)
                    || key.modifiers.contains(Mods::CONTROL)
                        && matches!(key.code, K::Char('s') | K::Enter) =>
            {
                Some(if self.message.is_some() {
                    Hit::Save
                } else {
                    Hit::Review
                })
            }
            Event::Key(key) if self.message.is_some() => {
                self.message.as_mut().unwrap().key(key);
                None
            }
            Event::Paste(text) if self.message.is_some() => {
                self.message
                    .as_mut()
                    .unwrap()
                    .paste(&text.replace('\0', ""));
                None
            }
            Event::Key(key) if matches!(key.modifiers, Mods::NONE | Mods::SHIFT) => {
                match key.code {
                    K::Down | K::Char('j') => {
                        Some(Hit::Row((self.row + 1).min(self.draft.entries.len() - 1)))
                    }
                    K::Up | K::Char('k') => Some(Hit::Row(self.row.saturating_sub(1))),
                    K::Home => Some(Hit::Row(0)),
                    K::End => Some(Hit::Row(self.draft.entries.len() - 1)),
                    K::Char('J') => Some(Hit::Move(1)),
                    K::Char('K') => Some(Hit::Move(-1)),
                    K::Char('p') => Some(Hit::Step(Step::Pick)),
                    K::Char('s') => Some(Hit::Step(Step::Squash)),
                    K::Char('f') => Some(Hit::Step(Step::Fixup)),
                    K::Char('m') => Some(Hit::Message),
                    _ => None,
                }
            }
            Event::Mouse(mouse) if mouse.kind == M::Down(MouseButton::Left) => {
                let hit = self
                    .hits
                    .iter()
                    .find(|(area, _)| inside(*area, mouse.column, mouse.row))
                    .map(|(_, hit)| *hit);
                if matches!(hit, Some(Hit::Text)) {
                    if let Some(message) = &mut self.message {
                        message.place(mouse.column, mouse.row);
                    }
                }
                hit
            }
            Event::Mouse(mouse) if self.message.is_none() => match mouse.kind {
                M::ScrollDown => Some(Hit::Row((self.row + 1).min(self.draft.entries.len() - 1))),
                M::ScrollUp => Some(Hit::Row(self.row.saturating_sub(1))),
                _ => None,
            },
            _ => None,
        };
        hit.map_or(Ok(Response::None), |hit| self.action(hit))
    }

    fn buttons(&mut self, f: &mut Frame, mut row: Rect, items: &[(&str, Hit)]) {
        for (label, hit) in items {
            let area = take(&mut row, cells(label));
            if area.width < cells(label) {
                break;
            }
            f.render_widget(
                Paragraph::new(*label).style(button_style(matches!(hit, Hit::Review | Hit::Save))),
                area,
            );
            self.hits.push((area, *hit));
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.hits.clear();
        let area = popup(area, 88, 24);
        f.render_widget(Clear, area);
        let frame = boxof(
            if self.message.is_some() {
                "Squash Message".into()
            } else {
                "Linear History Plan".into()
            },
            true,
        );
        let inner = frame.inner(area);
        f.render_widget(frame, area);
        if inner.height < 5 || inner.width < 12 {
            return;
        }
        let line = |y| Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(
            Paragraph::new(clipped(
                &format!(
                    "Branch: {} · selected commit through HEAD",
                    self.draft.branch()
                ),
                inner.width.into(),
            ))
            .style(fg(DIM)),
            line(inner.y),
        );
        if let Some(message) = &mut self.message {
            let text = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2);
            f.render_widget(Block::default().style(fg(INK).bg(EDGE)), text);
            message.draw(f, text);
            self.hits.push((text, Hit::Text));
            self.buttons(
                f,
                line(inner.bottom() - 1),
                &[(" Back Esc ", Hit::Cancel), (" Save F2 ", Hit::Save)],
            );
            return;
        }
        f.render_widget(
            Paragraph::new("Old IDs · execution order · combined IDs removed").style(fg(DIM)),
            line(inner.y + 1),
        );
        let height = usize::from(inner.height - 5);
        let start = window(self.row, height, self.draft.entries.len());
        for (index, entry) in self
            .draft
            .entries
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
        {
            let row = line(inner.y + 2 + (index - start) as u16);
            let text = format!(
                "{}. {:6} {} {}",
                index + 1,
                entry.step.label(),
                &entry.oid[..8],
                model::plain(entry.subject.as_bytes()),
            );
            f.render_widget(
                Paragraph::new(clipped(&text, row.width.into()))
                    .style(fg(INK).bg(if index == self.row { SELECT } else { PANEL })),
                row,
            );
            self.hits.push((row, Hit::Row(index)));
        }
        self.buttons(
            f,
            line(inner.bottom() - 3),
            &[
                (" p Pick ", Hit::Step(Step::Pick)),
                (" s Squash ", Hit::Step(Step::Squash)),
                (" f Fixup ", Hit::Step(Step::Fixup)),
                (" K Up ", Hit::Move(-1)),
                (" J Down ", Hit::Move(1)),
            ],
        );
        f.render_widget(
            Paragraph::new(if inner.width >= 65 {
                "J/K Reorder · m Message · squash/fixup combine into the preceding group"
            } else {
                "J/K Move · m Message · into prior group"
            })
            .style(fg(DIM)),
            line(inner.bottom() - 2),
        );
        self.buttons(
            f,
            line(inner.bottom() - 1),
            &[(" Cancel Esc ", Hit::Cancel), (" Review F2 ", Hit::Review)],
        );
    }
}
