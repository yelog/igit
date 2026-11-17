use super::*;
use crate::core::{activity::Record, loading::Load};

impl Shell {
    pub(super) fn notify_unfinished_activity(&mut self) {
        if let Ok(journal) = crate::core::activity::load(&self.ctx.state_dir) {
            let count = journal
                .records
                .iter()
                .filter(|record| {
                    matches!(
                        record.status.as_str(),
                        "Unfinished" | "Unknown" | "Needs verification"
                    )
                })
                .count();
            if count > 0 {
                self.notify(NoticeLevel::Warning, "Recovery", format!("{count} recorded operations need verification. Open Activity / Recovery (F8) before retrying writes."));
            }
        }
    }

    pub(super) fn open_activity(&mut self) {
        if self.operation.is_none() {
            self.cancel_key_sequences();
            self.pop = Some(Popup::Activity(Dialog::new(self.ctx.state_dir.clone())));
        }
    }
}

pub(super) struct Dialog {
    records: Vec<Record>,
    loading: Load<Result<crate::core::activity::Journal>>,
    error: Option<String>,
    row: usize,
    scroll: u16,
}

impl Dialog {
    pub(super) fn new(state: PathBuf) -> Self {
        let mut loading = Load::default();
        loading.request();
        let error = loading
            .start("igit-activity", move |_| {
                crate::core::activity::load(&state)
            })
            .err()
            .map(|error| error.to_string());
        Self {
            records: Vec::new(),
            loading,
            error,
            row: 0,
            scroll: 0,
        }
    }

    pub(super) fn poll(&mut self) -> bool {
        let Some(result) = self.loading.take_result() else {
            return false;
        };
        match result.and_then(|result| result) {
            Ok(journal) => {
                self.records = journal.records;
                if journal.unreadable > 0 {
                    self.error = Some(format!(
                        "{} activity records are unreadable; their files were retained.",
                        journal.unreadable
                    ));
                }
            }
            Err(error) => self.error = Some(format!("Could not read Activity: {error:#}")),
        }
        true
    }

    fn text(&self) -> String {
        let Some(record) = self.records.get(self.row) else {
            return self.error.clone().unwrap_or_else(|| if self.loading.pending() {
                "Loading Activity…".into()
            } else {
                "No recorded operations yet.\n\nRollback backups and Merge drafts remain available through their panels.".into()
            });
        };
        let repository = record
            .repository
            .path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "Unavailable path".into());
        let note = if matches!(
            record.status.as_str(),
            "Unfinished" | "Unknown" | "Needs verification"
        ) {
            "The result needs verification; another instance may still be running.\nInspect Git state before another write. IGit never replays this record."
        } else {
            "Results describe one recorded step or repository action. An action can have several entries.\nA successful step does not imply a whole batch or remote publication succeeded."
        };
        format!("{}{} · {}\n{}\n\nRepository: {}\nBefore HEAD: {}\nAfter HEAD: {}\n\n{}\n\nRecovery: Commit provides reviewed Rollback restore; Conflicts provides saved drafts and native Continue / Abort. History rewrites retain backup refs.\nNo universal undo or remote rollback is implied.",
            self.error.as_ref().map_or(String::new(), |error| format!("{error}\n\n")),
            record.operation, record.status, record.id, repository,
            record.before.as_deref().unwrap_or("Unavailable / unborn"),
            record.after.as_deref().unwrap_or("Unavailable / unfinished"), note)
    }

    pub(super) fn event(&mut self, event: &Event) -> Option<Action> {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => return Some(Action::CloseDialog),
                KeyCode::Down | KeyCode::Char('j') => {
                    self.row = (self.row + 1).min(self.records.len().saturating_sub(1));
                    self.scroll = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.row = self.row.saturating_sub(1);
                    self.scroll = 0;
                }
                KeyCode::Char('J') => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Char('K') => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Char('y') => {
                    return Some(Action::Copy {
                        text: self.text(),
                        label: "operation details",
                    })
                }
                _ => (),
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
                self.scroll = self.scroll.saturating_add(1)
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_sub(1)
            }
            _ => (),
        }
        None
    }

    pub(super) fn draw(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(2).min(96);
        let height = area.height.saturating_sub(2).min(28);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = pane(
            format!(
                "Activity / Recovery · {}/{}",
                if self.records.is_empty() {
                    0
                } else {
                    self.row + 1
                },
                self.records.len()
            ),
            true,
        );
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let body = Rect {
            height: inner.height.saturating_sub(2),
            ..inner
        };
        frame.render_widget(
            Paragraph::new(self.text())
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            body,
        );
        let footer = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(1),
            inner.width,
            u16::from(inner.height > 0),
        );
        frame.render_widget(
            Paragraph::new(
                "j/k Entries · J/K Scroll · y Copy · 1 Commit · 7 Conflicts · Esc Close",
            ),
            footer,
        );
    }
}
