use super::{Center, Notification};
use crate::{
    config::keys::normalize,
    ui::{
        help::{HelpContent, HelpSection},
        inside, selection, Action,
    },
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::Rect;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HistoryEvent {
    Handled,
    Close,
    Copy(String),
}

#[derive(Clone, Default)]
pub(crate) struct History {
    pub(super) selected_id: Option<u64>,
    pub(super) selected: usize,
    pub(super) offset: usize,
    pub(super) query: String,
    pub(super) input: Option<SearchInput>,
    pub(super) clear_confirm: bool,
    pub(super) detail: Option<Detail>,
    pub(super) preview: Preview,
    pub(super) selection: Box<selection::TextSelection>,
    pub(super) rows: Vec<(Rect, u64)>,
    pub(super) list_area: Rect,
    pub(super) content_area: Rect,
    pub(super) close_area: Rect,
    pub(super) page_size: usize,
}

#[derive(Clone, Default)]
pub(super) struct Preview {
    pub(super) id: Option<u64>,
    pub(super) scroll: usize,
    pub(super) max_scroll: usize,
    pub(super) page_size: usize,
}

#[derive(Clone)]
pub(super) struct Detail {
    pub entry: Notification,
    pub scroll: usize,
    pub max_scroll: usize,
    pub page_size: usize,
}

#[derive(Clone, Default)]
struct Edit {
    text: String,
    caret: usize,
}

#[derive(Clone, Default)]
pub(super) struct SearchInput {
    edit: Edit,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl SearchInput {
    pub(super) fn text(&self) -> &str {
        &self.edit.text
    }

    pub(super) fn caret(&self) -> usize {
        self.edit.caret
    }

    fn replace(&mut self, start: usize, end: usize, text: &str) {
        if start == end && text.is_empty() {
            return;
        }
        self.undo.push(self.edit.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.edit.text.replace_range(start..end, text);
        self.edit.caret = start + text.len();
    }

    fn insert(&mut self, text: &str) {
        let safe: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        self.replace(self.edit.caret, self.edit.caret, &safe);
    }

    fn event(&mut self, key: KeyEvent) {
        let position = self.edit.caret;
        let length = self.edit.text.len();
        match (key.code, key.modifiers) {
            (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                if let Some(previous) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.edit, previous));
                }
            }
            (KeyCode::Char('y' | 'Z'), KeyModifiers::CONTROL) => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.edit, next));
                }
            }
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                self.replace(
                    selection::previous_word(&self.edit.text, position),
                    position,
                    "",
                );
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => self.replace(0, length, ""),
            (KeyCode::Char('a'), KeyModifiers::CONTROL) | (KeyCode::Home, KeyModifiers::NONE) => {
                self.edit.caret = 0;
            }
            (KeyCode::Char('e'), KeyModifiers::CONTROL) | (KeyCode::End, KeyModifiers::NONE) => {
                self.edit.caret = length;
            }
            (KeyCode::Left, KeyModifiers::NONE) => {
                self.edit.caret = selection::previous(&self.edit.text, position);
            }
            (KeyCode::Right, KeyModifiers::NONE) => {
                self.edit.caret = selection::next(&self.edit.text, position);
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.replace(selection::previous(&self.edit.text, position), position, "");
            }
            (KeyCode::Delete, KeyModifiers::NONE) => {
                self.replace(position, selection::next(&self.edit.text, position), "");
            }
            (KeyCode::Char(character), KeyModifiers::NONE) if !character.is_control() => {
                self.insert(character.encode_utf8(&mut [0; 4]));
            }
            _ => {}
        }
    }
}

impl History {
    pub(crate) fn keymap(&self) -> HelpContent {
        let section = if self.clear_confirm {
            HelpSection::new(
                "Notifications / Clear confirmation",
                &[
                    ("y", "Confirm clearing all notifications"),
                    ("n", "Cancel clearing notifications"),
                    ("Esc", "Cancel clearing notifications"),
                ],
            )
        } else if self.input.is_some() {
            HelpSection::new(
                "Notifications / Search",
                &[
                    ("Enter", "Confirm search and select first match"),
                    ("Esc", "Cancel search editing"),
                    ("Left", "Move cursor left"),
                    ("Right", "Move cursor right"),
                    ("Home", "Move cursor to start"),
                    ("End", "Move cursor to end"),
                    ("Backspace", "Delete previous character"),
                    ("Delete", "Delete next character"),
                    ("Ctrl-a", "Move cursor to start"),
                    ("Ctrl-e", "Move cursor to end"),
                    ("Ctrl-w", "Delete word before cursor"),
                    ("Ctrl-u", "Clear search input"),
                    ("Ctrl-z", "Undo search edit"),
                    ("Ctrl-y", "Redo search edit"),
                ],
            )
        } else if self.detail.is_some() {
            HelpSection::new(
                "Notifications / Detail",
                &[
                    ("j", "Scroll detail down"),
                    ("k", "Scroll detail up"),
                    ("J", "Scroll detail down"),
                    ("K", "Scroll detail up"),
                    ("Down", "Scroll detail down"),
                    ("Up", "Scroll detail up"),
                    ("PgDn", "Scroll detail down one page"),
                    ("PgUp", "Scroll detail up one page"),
                    ("Home", "Scroll to start of detail"),
                    ("End", "Scroll to end of detail"),
                    ("y", "Copy complete notification or selection"),
                    ("Ctrl-a", "Select all detail text"),
                    ("Ctrl-c", "Copy selected text"),
                    ("Ctrl-Insert", "Copy selected text"),
                    ("Esc", "Back to notification history"),
                    ("q", "Back to notification history"),
                ],
            )
        } else {
            HelpSection::new(
                "Notifications / History",
                &[
                    ("j", "Select next notification"),
                    ("k", "Select previous notification"),
                    ("J", "Scroll preview down"),
                    ("K", "Scroll preview up"),
                    ("Down", "Select next notification"),
                    ("Up", "Select previous notification"),
                    ("PgDn", "Select next page"),
                    ("PgUp", "Select previous page"),
                    ("Home", "Select first notification"),
                    ("End", "Select last notification"),
                    ("/", "Search notifications"),
                    ("n", "Next search match"),
                    ("N", "Previous search match"),
                    ("Enter", "Open complete notification"),
                    ("y", "Copy complete notification or selection"),
                    ("Ctrl-a", "Select all preview text"),
                    ("Ctrl-c", "Copy selected text"),
                    ("Ctrl-Insert", "Copy selected text"),
                    ("c", "Clear notifications with confirmation"),
                    ("Esc", "Close notification history"),
                    ("q", "Close notification history"),
                ],
            )
        };
        HelpContent::new(section, vec![])
    }

    pub(crate) fn clear_hits(&mut self) {
        self.rows.clear();
        self.list_area = Rect::default();
        self.content_area = Rect::default();
        self.close_area = Rect::default();
        self.page_size = 0;
        self.preview.page_size = 0;
        if let Some(detail) = &mut self.detail {
            detail.page_size = 0;
        }
    }

    pub(super) fn query(&self) -> &str {
        self.input
            .as_ref()
            .map_or(self.query.as_str(), SearchInput::text)
    }

    pub(super) fn reconcile(&mut self, center: &Center) {
        let previous = self.selected_id;
        if center.history.is_empty() {
            self.selected = 0;
            self.selected_id = None;
            self.offset = 0;
        } else {
            self.selected = self
                .selected_id
                .and_then(|id| center.history.iter().position(|entry| entry.id == id))
                .unwrap_or(self.selected.min(center.history.len() - 1));
            self.selected_id = Some(center.history[self.selected].id);
            if self.page_size != 0 {
                self.offset = self
                    .offset
                    .min(center.history.len().saturating_sub(self.page_size));
                if self.selected < self.offset {
                    self.offset = self.selected;
                } else if self.selected >= self.offset + self.page_size.max(1) {
                    self.offset = self.selected + 1 - self.page_size.max(1);
                }
            }
        }
        if self.selected_id != previous && self.detail.is_none() {
            self.preview = Preview::default();
            self.selection.reset();
        }
    }

    fn select(&mut self, center: &Center, index: usize) {
        self.selected = index.min(center.history.len().saturating_sub(1));
        self.selected_id = center.history.get(self.selected).map(|entry| entry.id);
        self.reconcile(center);
    }

    fn move_by(&mut self, center: &Center, delta: isize) {
        if let Some(detail) = &mut self.detail {
            detail.scroll = detail
                .scroll
                .saturating_add_signed(delta)
                .min(detail.max_scroll);
        } else {
            self.select(center, self.selected.saturating_add_signed(delta));
        }
    }

    fn scroll_preview(&mut self, delta: isize) {
        if self.detail.is_none() && !self.content_area.is_empty() {
            self.preview.scroll = self
                .preview
                .scroll
                .saturating_add_signed(delta)
                .min(self.preview.max_scroll);
        }
    }

    fn move_match(&mut self, center: &Center, forward: bool, first: bool) {
        let matches: Vec<_> = center
            .history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.matches(&self.query))
            .map(|(index, _)| index)
            .collect();
        let index = if first {
            matches.first().copied()
        } else if forward {
            matches
                .iter()
                .copied()
                .find(|index| *index > self.selected)
                .or(matches.first().copied())
        } else {
            matches
                .iter()
                .rev()
                .copied()
                .find(|index| *index < self.selected)
                .or(matches.last().copied())
        };
        if let Some(index) = index {
            self.select(center, index);
        }
    }

    fn open_detail(&mut self, center: &Center) {
        if let Some(entry) = self.selected_id.and_then(|id| center.get(id)) {
            self.detail = Some(Detail {
                entry: entry.clone(),
                scroll: 0,
                max_scroll: 0,
                page_size: self.page_size.max(1),
            });
            self.selection.clear();
        }
    }
}

fn copy_event(action: Action) -> HistoryEvent {
    match action {
        Action::Copy { text, .. } => HistoryEvent::Copy(text),
        _ => HistoryEvent::Handled,
    }
}

impl Center {
    pub(crate) fn open_history(&self, id: Option<u64>) -> History {
        let mut state = History {
            selected_id: id,
            ..History::default()
        };
        state.reconcile(self);
        state
    }

    pub(crate) fn history_event(&mut self, event: &Event, state: &mut History) -> HistoryEvent {
        state.reconcile(self);
        if matches!(event, Event::FocusLost) {
            state.selection.clear();
        }
        if let Event::Mouse(mouse) = event {
            if let MouseEventKind::Down(button) = mouse.kind {
                self.captured_button = Some(button);
            }
        }
        if let Event::Resize(..) = event {
            state.clear_hits();
            state.selection.cancel_mouse();
        }
        if matches!(event, Event::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && inside(state.close_area, mouse.column, mouse.row))
        {
            if state.clear_confirm {
                state.clear_confirm = false;
            } else if state.detail.take().is_none() {
                return HistoryEvent::Close;
            } else {
                state.selection.clear();
            }
            return HistoryEvent::Handled;
        }
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                return HistoryEvent::Handled;
            }
            let key = normalize(*key);
            if state.clear_confirm {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('y'), KeyModifiers::NONE) => {
                        self.history.clear();
                        self.dismiss_all_live();
                        self.hits.clear();
                        *state = History::default();
                    }
                    (KeyCode::Char('n') | KeyCode::Esc, KeyModifiers::NONE) => {
                        state.clear_confirm = false;
                    }
                    _ => {}
                }
                return HistoryEvent::Handled;
            }
            if let Some(input) = &mut state.input {
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, KeyModifiers::NONE) => state.input = None,
                    (KeyCode::Enter, KeyModifiers::NONE) => {
                        state.query = input.text().to_owned();
                        state.input = None;
                        state.move_match(self, true, true);
                    }
                    _ => input.event(key),
                }
                return HistoryEvent::Handled;
            }
            if !state.content_area.is_empty() {
                if let Some(action) = state.selection.key(event, true) {
                    return copy_event(action);
                }
            }
            if key.modifiers != KeyModifiers::NONE {
                return HistoryEvent::Handled;
            }
            let page = state
                .detail
                .as_ref()
                .map_or(state.page_size, |detail| detail.page_size)
                .max(1) as isize;
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    if state.detail.take().is_none() {
                        return HistoryEvent::Close;
                    }
                    state.selection.clear();
                }
                KeyCode::Char('y') => {
                    if let Some(entry) = state
                        .detail
                        .as_ref()
                        .map(|detail| &detail.entry)
                        .or_else(|| state.selected_id.and_then(|id| self.get(id)))
                    {
                        return HistoryEvent::Copy(entry.body.clone());
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => state.move_by(self, 1),
                KeyCode::Up | KeyCode::Char('k') => state.move_by(self, -1),
                KeyCode::Char('J') if state.detail.is_some() => state.move_by(self, 1),
                KeyCode::Char('K') if state.detail.is_some() => state.move_by(self, -1),
                KeyCode::Char('J') => state.scroll_preview(1),
                KeyCode::Char('K') => state.scroll_preview(-1),
                KeyCode::PageDown => state.move_by(self, page),
                KeyCode::PageUp => state.move_by(self, -page),
                KeyCode::Home => state.move_by(self, isize::MIN),
                KeyCode::End => state.move_by(self, isize::MAX),
                KeyCode::Enter if state.detail.is_none() => state.open_detail(self),
                KeyCode::Char('/') if state.detail.is_none() => {
                    state.input = Some(SearchInput {
                        edit: Edit {
                            text: state.query.clone(),
                            caret: state.query.len(),
                        },
                        ..SearchInput::default()
                    });
                }
                KeyCode::Char('n') if state.detail.is_none() => state.move_match(self, true, false),
                KeyCode::Char('N') if state.detail.is_none() => {
                    state.move_match(self, false, false)
                }
                KeyCode::Char('c') if state.detail.is_none() => state.clear_confirm = true,
                _ => {}
            }
        } else if !state.clear_confirm {
            match event {
                Event::Paste(text) => {
                    if let Some(input) = &mut state.input {
                        input.insert(text);
                    }
                }
                Event::Mouse(mouse) if state.input.is_none() => {
                    if let Some(action) = state.selection.mouse(mouse) {
                        return copy_event(action);
                    }
                    match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            if state.detail.is_some()
                                || !inside(state.content_area, mouse.column, mouse.row)
                            {
                                state.move_by(self, 3);
                            } else {
                                state.scroll_preview(3);
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if state.detail.is_some()
                                || !inside(state.content_area, mouse.column, mouse.row)
                            {
                                state.move_by(self, -3);
                            } else {
                                state.scroll_preview(-3);
                            }
                        }
                        MouseEventKind::Down(MouseButton::Left) if state.detail.is_none() => {
                            let target = state
                                .rows
                                .iter()
                                .find(|(area, _)| inside(*area, mouse.column, mouse.row))
                                .map(|(_, id)| *id);
                            if let Some(id) = target {
                                if let Some(index) =
                                    self.history.iter().position(|entry| entry.id == id)
                                {
                                    if state.selected_id == Some(id) {
                                        state.open_detail(self);
                                    } else {
                                        state.select(self, index);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        HistoryEvent::Handled
    }
}
