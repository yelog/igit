use super::file_history::LocalCompare;
use crate::{
    core::{revision, Context},
    merge::{RevisionDiff, RevisionEvent},
    ui::{
        clipboard,
        help::{HelpContent, HelpEvent, HelpSection, HelpState},
        icons::IconMode,
        inside, review,
        selection::{TextLayout, Wrap},
        BG, FG,
    },
};
use anyhow::{ensure, Context as _, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Paragraph},
    Frame,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

struct BufferSnapshot {
    path: PathBuf,
    text: String,
    permissions: fs::Permissions,
}

#[derive(Clone)]
struct UndoEntry {
    text: String,
    change_position: usize,
}

impl BufferSnapshot {
    fn read(path: &Path) -> Result<(String, fs::Permissions)> {
        let metadata = fs::symlink_metadata(path).context("Cannot inspect the buffer snapshot")?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "The buffer snapshot must be a regular file"
        );
        ensure!(
            metadata.len() <= revision::LIMIT as u64,
            "The buffer snapshot is larger than 512 KiB"
        );
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take((revision::LIMIT + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= revision::LIMIT && !bytes.contains(&0),
            "The buffer snapshot must be UTF-8 text of at most 512 KiB"
        );
        Ok((
            String::from_utf8(bytes).context("The buffer snapshot must be UTF-8 text")?,
            metadata.permissions(),
        ))
    }

    fn open(path: &Path, context: &Context) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "The buffer snapshot path must be absolute"
        );
        let (text, permissions) = Self::read(path)?;
        let path = path.canonicalize()?;
        ensure!(
            !context
                .repos
                .iter()
                .any(|repo| path.starts_with(&repo.path)),
            "The buffer snapshot must be outside Git working trees"
        );
        Ok(Self {
            path,
            text,
            permissions,
        })
    }

    fn ensure_current(&self) -> Result<()> {
        let (text, permissions) = Self::read(&self.path)?;
        ensure!(
            text == self.text && permissions == self.permissions,
            "The buffer snapshot changed externally; reopen File Diff"
        );
        Ok(())
    }

    fn write(&mut self, text: &str) -> Result<()> {
        self.ensure_current()?;
        let mut temporary = NamedTempFile::new_in(
            self.path
                .parent()
                .context("Buffer snapshot has no parent")?,
        )?;
        temporary.write_all(text.as_bytes())?;
        temporary
            .as_file()
            .set_permissions(self.permissions.clone())?;
        temporary.as_file().sync_all()?;
        self.ensure_current()?;
        temporary.persist(&self.path).map_err(|error| error.error)?;
        self.text = text.to_owned();
        Ok(())
    }
}

pub(crate) struct FileDiff {
    local: LocalCompare,
    comparison: RevisionDiff,
    buffer: Option<BufferSnapshot>,
    notice: Option<String>,
    reviewing: bool,
    confirm_focused: bool,
    review_scroll: u16,
    review_hits: Vec<(Rect, usize)>,
    help: Option<HelpState>,
    available: bool,
    undo: Vec<UndoEntry>,
}

impl FileDiff {
    pub(crate) fn open(
        context: &Context,
        file: &Path,
        line: usize,
        buffer: Option<&Path>,
    ) -> Result<Self> {
        let local = LocalCompare::prepare_head(context, file)?;
        let buffer = buffer
            .map(|path| BufferSnapshot::open(path, context))
            .transpose()?;
        let mut comparison =
            local.head_comparison(buffer.as_ref().map(|snapshot| snapshot.text.as_str()))?;
        comparison.reveal_local_line(line);
        Ok(Self {
            local,
            comparison,
            buffer,
            notice: None,
            reviewing: false,
            confirm_focused: false,
            review_scroll: 0,
            review_hits: Vec::new(),
            help: None,
            available: false,
            undo: Vec::new(),
        })
    }

    fn apply(&mut self, change: Option<usize>) -> Result<()> {
        if let Some(change) = change {
            ensure!(
                self.comparison.change_visible(change),
                "Use n/p to select and review a changed block before restoring it"
            );
        }
        self.local.ensure_head()?;
        let previous_text = match &self.buffer {
            Some(buffer) => buffer.text.clone(),
            None => self.local.text()?,
        };
        let change_position = self.comparison.selected_change_position().unwrap_or(0);
        let text = self
            .comparison
            .merged_local_text(change)
            .context("Select a changed block first")?;
        if let Some(buffer) = &mut self.buffer {
            buffer.write(&text)?;
        } else {
            self.local.write(&text)?;
            self.local = self.local.refresh()?;
        }
        self.undo.push(UndoEntry {
            text: previous_text,
            change_position,
        });
        let mut comparison = self
            .local
            .head_comparison(self.buffer.as_ref().map(|buffer| buffer.text.as_str()))?;
        comparison.inherit_local_view(&self.comparison);
        self.comparison = comparison;
        self.notice = None;
        Ok(())
    }

    fn undo(&mut self) -> Result<()> {
        let Some(entry) = self.undo.last().cloned() else {
            self.notice = Some("Nothing to undo in this File Diff session".into());
            return Ok(());
        };
        self.local.ensure_head()?;
        if let Some(buffer) = &mut self.buffer {
            buffer.write(&entry.text)?;
        } else {
            self.local.write(&entry.text)?;
            self.local = self.local.refresh()?;
        }
        let mut comparison = self
            .local
            .head_comparison(self.buffer.as_ref().map(|buffer| buffer.text.as_str()))?;
        comparison.inherit_local_view(&self.comparison);
        comparison.select_change_position(entry.change_position);
        self.comparison = comparison;
        self.undo.pop();
        self.notice = None;
        Ok(())
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame, area: Rect, icons: IconMode) {
        frame.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), area);
        self.available = area.width >= 44 && area.height >= 12;
        if !self.available {
            frame.render_widget(
                Paragraph::new("File Diff · Terminal too small\nMinimum 44 × 12. Esc / q Close.")
                    .style(Style::new().fg(FG).bg(BG)),
                area,
            );
            return;
        }
        let notice_height = if self.notice.is_some() {
            area.height.saturating_sub(6).min(4)
        } else {
            0
        };
        let comparison = Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(notice_height),
        );
        self.comparison.draw(frame, comparison, icons, 0, 1);
        if let Some(notice) = &self.notice {
            frame.render_widget(
                Paragraph::new(notice.as_str())
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(Style::new().fg(FG).bg(BG)),
                Rect::new(area.x, comparison.bottom(), area.width, notice_height),
            );
        }
        self.review_hits.clear();
        if self.reviewing {
            let destination = if self.buffer.is_some() {
                "Neovim buffer. Save normally after closing the viewer."
            } else {
                "working file."
            };
            let body = format!(
                "Restore all {} changed blocks from HEAD to the {destination}\n\nThe index and HEAD remain unchanged.",
                self.comparison.change_count(),
            );
            review::Dialog {
                title: "Restore all changes from HEAD",
                icons,
                letter_keys: true,
            }
            .draw(
                frame,
                area,
                |width| TextLayout::plain(&body, width, Wrap::Word),
                &mut self.review_scroll,
                self.confirm_focused,
                &mut self.review_hits,
            );
        }
        if let Some(help) = &mut self.help {
            help.draw(frame, area, icons);
        }
    }

    fn reviewed_event(&mut self, event: &Event) -> Result<()> {
        let choice = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc | KeyCode::Char('n' | 'q') => Some(false),
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => Some(false),
                KeyCode::Char('y') => Some(true),
                KeyCode::Enter => Some(self.confirm_focused),
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    self.confirm_focused = !self.confirm_focused;
                    None
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.review_scroll = self.review_scroll.saturating_add(1);
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.review_scroll = self.review_scroll.saturating_sub(1);
                    None
                }
                _ => None,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => self
                .review_hits
                .iter()
                .find(|(area, _)| inside(*area, mouse.column, mouse.row))
                .map(|(_, choice)| *choice == 1),
            _ => None,
        };
        if let Some(confirm) = choice {
            self.reviewing = false;
            if confirm {
                self.apply(None)?;
            }
        }
        Ok(())
    }

    pub(crate) fn event(&mut self, event: &Event) -> bool {
        if let Some(help) = &mut self.help {
            let action = help.event(event);
            match action {
                HelpEvent::Keep => {}
                HelpEvent::Close => self.help = None,
                HelpEvent::Run(command) => {
                    self.help = None;
                    if let Some(events) = command.events() {
                        for event in events {
                            if self.event(&event) {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }
        if self.reviewing {
            if !self.available {
                if matches!(event, Event::Key(key)
                    if key.kind != KeyEventKind::Release
                        && (matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
                            || key.code == KeyCode::Char('c')
                                && key.modifiers == KeyModifiers::CONTROL))
                {
                    self.reviewing = false;
                }
                return false;
            }
            if let Err(error) = self.reviewed_event(event) {
                self.notice = Some(format!("{error:#}"));
            }
            return false;
        }
        if matches!(event, Event::Key(key)
            if key.kind != KeyEventKind::Release
                && key.code == KeyCode::Char('c')
                && key.modifiers == KeyModifiers::CONTROL)
            && self.comparison.selected_text().is_none()
        {
            return true;
        }
        if !self.available {
            return matches!(event, Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && matches!(key.code, KeyCode::Esc | KeyCode::Char('q')));
        }
        if !self.comparison.search_prompt().editing()
            && matches!(event, Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && matches!(key.code, KeyCode::F(1) | KeyCode::Char('?')))
        {
            self.help = Some(HelpState::new(HelpContent::new(
                HelpSection::new(
                    "File Diff / HEAD and Local",
                    &[
                        (
                            "n",
                            "Next changed block (next search match when finding text)",
                        ),
                        ("p", "Previous changed block"),
                        ("L", "Restore the selected block from HEAD"),
                        ("u / U", "Undo the last restore in this viewer"),
                        ("A", "Review and restore all changes from HEAD"),
                        ("Tab", "Focus the next revision pane"),
                        ("Shift-Tab", "Focus the previous revision pane"),
                        ("j", "Scroll both revisions down"),
                        ("k", "Scroll both revisions up"),
                        ("h", "Scroll the focused revision left"),
                        ("l", "Scroll the focused revision right"),
                        ("f", "Fold or unfold unchanged regions"),
                        ("z", "Zoom the focused pane / restore split"),
                        ("/", "Find text in the focused pane"),
                        ("N", "Previous search match"),
                        ("Ctrl-a", "Select the focused revision"),
                        ("y", "Copy the selection or focused revision"),
                        ("Esc", "Clear search or selection; otherwise close"),
                        ("q", "Close File Diff"),
                    ],
                ),
                Vec::new(),
            )));
            return false;
        }
        if self.comparison.search_event(event) {
            return false;
        }
        if matches!(event, Event::Key(key)
            if key.kind != KeyEventKind::Release
                && matches!(key.code, KeyCode::Char('u' | 'U'))
                && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT))
        {
            if let Err(error) = self.undo() {
                self.notice = Some(format!("{error:#}"));
            }
            return false;
        }
        let result = match self.comparison.event(event) {
            RevisionEvent::Close => return true,
            RevisionEvent::Apply(change) => self.apply(Some(change)),
            RevisionEvent::ApplyAll => {
                self.reviewing = true;
                self.confirm_focused = false;
                self.review_scroll = 0;
                Ok(())
            }
            RevisionEvent::Copy(text) => clipboard::copy(&text),
            RevisionEvent::None | RevisionEvent::File(_) => Ok(()),
        };
        if let Err(error) = result {
            self.notice = Some(format!("{error:#}"));
        }
        false
    }
}

#[cfg(test)]
mod tests;
