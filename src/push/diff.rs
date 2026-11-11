//! Open the shared revision viewer on a leaf of Push's current file tree.
use super::{preview::FileDiff, *};
use crate::merge::RevisionEvent;
use crate::ui::idea::{button_style, diff_line};
use crossterm::event::KeyEvent;
use unicode_width::UnicodeWidthStr;

pub(super) struct Diff {
    rows: Vec<usize>,
    index: usize,
    pub(super) content: Option<FileDiff>,
    patch: PatchView,
}

#[derive(Default)]
struct PatchView {
    selection: TextSelection,
    scroll: usize,
    page: usize,
    buttons: [Rect; 3],
}

impl PushPanel {
    pub(super) fn open_file(&mut self, c: &Context) {
        let tree = &self.preview.tree;
        let rows: Vec<_> = tree
            .visible
            .iter()
            .enumerate()
            .filter_map(|(row, &id)| matches!(tree.nodes[id].kind, Kind::File(_)).then_some(row))
            .collect();
        let Some(index) = rows.iter().position(|&row| row == tree.cursor) else {
            return;
        };
        self.diff = Some(Diff {
            rows,
            index,
            content: None,
            patch: PatchView::default(),
        });
        self.focus = 1;
        self.panes.cancel_drag();
        if self.show_details {
            self.update_details();
        }
        self.request_diff(c);
    }

    fn request_diff(&mut self, c: &Context) {
        let request = self
            .preview
            .tree
            .file_index()
            .and_then(|i| self.preview.files.get(i))
            .and_then(|file| self.preview_request(c, Some(file.clone())));
        if let Some(request) = request {
            self.preview_request = Some(request);
            self.preview_load.request();
        } else {
            self.close_diff();
        }
    }

    fn close_diff(&mut self) {
        self.diff = None;
        self.preview_load.cancel();
        self.preview_request = None;
    }

    pub(super) fn diff_event(&mut self, e: &Event, c: &Context) -> Action {
        let Some(diff) = &mut self.diff else {
            return Action::None;
        };
        // Preserve Push's plain d close shortcut without taking Ctrl-D from the viewer.
        let close = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let event = if matches!(e, Event::Key(k) if k.code == KeyCode::Char('d') && k.modifiers.is_empty())
        {
            &close
        } else {
            e
        };
        match diff.event(event) {
            RevisionEvent::None => (),
            RevisionEvent::Close => self.close_diff(),
            RevisionEvent::File(delta) => {
                let next = diff
                    .index
                    .saturating_add_signed(delta)
                    .min(diff.rows.len() - 1);
                if next != diff.index {
                    diff.index = next;
                    diff.content = None;
                    diff.patch = PatchView::default();
                    self.preview.tree.cursor = diff.rows[next];
                    self.request_diff(c);
                }
            }
            RevisionEvent::Copy(text) => {
                return Action::Copy {
                    text,
                    label: "revision diff",
                };
            }
            RevisionEvent::Apply(_) | RevisionEvent::ApplyAll => (),
        }
        Action::None
    }
}

impl Diff {
    pub(super) fn cancel_mouse(&mut self) {
        self.patch.selection.cancel_mouse();
        if let Some(FileDiff::Split(view)) = &mut self.content {
            view.cancel_mouse();
        }
    }

    pub(super) fn selected_text(&self) -> Option<&str> {
        match &self.content {
            Some(FileDiff::Split(view)) => view.selected_text(),
            Some(FileDiff::Patch(_)) => self.patch.selection.selected_text(),
            None => None,
        }
    }

    pub(super) fn draw(&mut self, f: &mut Frame, area: Rect, icons: IconMode, name: &str) {
        if let Some(FileDiff::Split(view)) = &mut self.content {
            view.draw(f, area, icons, self.index, self.rows.len());
            return;
        }
        self.patch.buttons = Default::default();
        let block = pane(format!("Repository Diff: {name}"), true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let [toolbar, body] = Layout::vertical([C::Length(1), C::Min(0)]).areas(inner);
        let counter = format!("{}/{} files", self.index + 1, self.rows.len());
        let [prev, count, next, _, close] = Layout::horizontal([
            C::Length(3),
            C::Length(counter.width() as u16),
            C::Length(3),
            C::Min(0),
            C::Length(7),
        ])
        .areas(toolbar);
        f.render_widget(
            Paragraph::new(counter).style(Style::new().fg(ACCENT)),
            count,
        );
        for (i, (r, label, enabled)) in [
            (prev, " < ", self.index > 0),
            (next, " > ", self.index + 1 < self.rows.len()),
            (close, " Close ", true),
        ]
        .into_iter()
        .enumerate()
        {
            let style = if enabled {
                button_style(false)
            } else {
                Style::new().fg(MUTED)
            };
            f.render_widget(Paragraph::new(label).style(style), r);
            if enabled {
                self.patch.buttons[i] = r;
            }
        }
        if let Some(FileDiff::Patch(text)) = &self.content {
            let mut layout = TextLayout::default();
            for line in text.split('\n') {
                layout.push(diff_line(line), body.width, TextWrap::Word);
            }
            self.patch.page = usize::from(body.height);
            self.patch.scroll = self
                .patch
                .scroll
                .min(layout.rows.len().saturating_sub(self.patch.page));
            self.patch
                .selection
                .render(f, body, layout, self.patch.scroll, 0);
        } else {
            f.render_widget(
                Paragraph::new("Loading diff…").style(Style::new().fg(MUTED)),
                body,
            );
        }
    }

    fn event(&mut self, e: &Event) -> RevisionEvent {
        if let Some(FileDiff::Split(view)) = &mut self.content {
            return view.event(e);
        }
        let patch = &mut self.patch;
        if matches!(e, Event::FocusLost) {
            patch.selection.clear();
        }
        if let Event::Mouse(m) = e {
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                if let Some(button) = patch
                    .buttons
                    .iter()
                    .position(|&r| inside(r, m.column, m.row))
                {
                    return match button {
                        0 => RevisionEvent::File(-1),
                        1 => RevisionEvent::File(1),
                        _ => RevisionEvent::Close,
                    };
                }
            }
        }
        if let Some(action) = patch.selection.key(e, true).or_else(|| match e {
            Event::Mouse(m) => patch.selection.mouse(m),
            _ => None,
        }) {
            return match action {
                Action::Copy { text, .. } => RevisionEvent::Copy(text),
                _ => RevisionEvent::None,
            };
        }
        let scroll = match e {
            Event::Key(k) => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return RevisionEvent::Close,
                KeyCode::Char('[') => return RevisionEvent::File(-1),
                KeyCode::Char(']') => return RevisionEvent::File(1),
                KeyCode::Char('j') | KeyCode::Down => 1,
                KeyCode::Char('k') | KeyCode::Up => -1,
                KeyCode::PageDown => patch.page as isize,
                KeyCode::PageUp => -(patch.page as isize),
                _ => 0,
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => 3,
                MouseEventKind::ScrollUp => -3,
                _ => 0,
            },
            _ => 0,
        };
        patch.scroll = patch.scroll.saturating_add_signed(scroll);
        RevisionEvent::None
    }
}
