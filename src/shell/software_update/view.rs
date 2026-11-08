use super::{Action, SoftwareUpdate, State};
use crate::{
    installation::Status,
    ui::{
        help::{HelpContent, HelpSection},
        icons::{Icon, IconMode},
        idea::{button_style, CONFLICT, GREEN, RED, TAG},
        inside, pane, ACCENT, BG, FG, MUTED,
    },
};
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as M, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

impl SoftwareUpdate {
    pub(in crate::shell) fn footer_status(&self, icons: IconMode) -> (String, Style) {
        let (label, color, bold) = match &self.state {
            State::Checking => (" Checking", MUTED, false),
            State::Installing(_) => (" Updating", ACCENT, false),
            State::Ready(_) => (" Restart", GREEN, true),
            State::Failed(_) => (" ! Update failed", RED, false),
            State::Report(report)
                if matches!(
                    report.status,
                    Status::UpdateAvailable | Status::ManagerActionRequired
                ) =>
            {
                (
                    if icons == IconMode::Ascii {
                        " ^ Update"
                    } else {
                        " ↑ Update"
                    },
                    CONFLICT,
                    true,
                )
            }
            _ => ("", MUTED, false),
        };
        let style = Style::new().fg(color);
        (label.into(), if bold { style.bold() } else { style })
    }

    fn text(&self) -> String {
        let running = format!("Running v{}", env!("CARGO_PKG_VERSION"));
        match &self.state {
            State::Idle => format!("{running}\n\nCheck for a new IGit release."),
            State::Checking => format!("{running}\n\nChecking for updates…"),
            State::Failed(message) => format!("{running}\n\nUpdate failed\n{message}\n\nPress r to retry."),
            State::Report(report) => {
                let details = match report.status {
                    Status::UpToDate => "IGit is up to date.".into(),
                    Status::UpdateAvailable => format!("Available v{}\n\nInstall this update? Restart when you are ready.", report.target_version),
                    Status::ManagerActionRequired => format!("Available v{}\n\n{}", report.target_version, report.action.as_deref().unwrap_or("Update IGit with its original installation method.")),
                    Status::Updated => "Restart IGit to use the update.".into(),
                };
                format!("{running}\nChannel {}\n\n{details}", report.channel)
            }
            State::Installing(report) => format!("{running}\nChannel {}\n\nUpdating to v{}…\nYou can close this window. The update will continue.", report.channel, report.target_version),
            State::Ready(report) => format!("{running}\nInstalled v{}\n\nRestart IGit to use the update. Finish any editing before restarting.", report.target_version),
        }
    }

    pub(in crate::shell) fn keymap(&self) -> HelpContent {
        HelpContent::new(
            HelpSection::new(
                "Update Center",
                &[
                    ("Tab", "Switch focused button"),
                    ("Enter", "Activate selected button"),
                    ("r", "Check for updates again"),
                    ("j", "Scroll details down"),
                    ("k", "Scroll details up"),
                    ("Esc", "Close Update Center"),
                ],
            ),
            vec![],
        )
    }

    pub(in crate::shell) fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        icons: IconMode,
        hits: &mut Vec<(Rect, usize)>,
    ) {
        let width = 74.min(area.width.saturating_sub(4));
        let height = 18.min(area.height.saturating_sub(2));
        if width < 20 || height < 6 {
            return;
        }
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, popup);
        let block = pane("Update Center", true);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let close = format!(" {} ", icons.glyph(Icon::Close));
        let close_area = Rect::new(
            popup.right() - close.width() as u16 - 1,
            popup.y,
            close.width() as u16,
            1,
        );
        f.render_widget(
            Paragraph::new(close).style(Style::new().fg(ACCENT)),
            close_area,
        );
        hits.push((close_area, usize::MAX));
        let body = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(3),
        );
        let text = self.text();
        let rows: usize = text
            .lines()
            .map(|line| line.width().max(1).div_ceil(usize::from(body.width.max(1))))
            .sum();
        self.scroll = self.scroll.min(
            rows.saturating_sub(body.height as usize)
                .min(u16::MAX as usize) as u16,
        );
        f.render_widget(
            Paragraph::new(text)
                .style(Style::new().fg(FG).bg(BG))
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            body,
        );
        let actions = self.actions();
        let mut x = inner.x + 1;
        for (index, action) in actions.iter().enumerate() {
            let label = match action {
                Action::Check => " Check again ",
                Action::Install => " Install update ",
                Action::Restart => " Restart IGit ",
                Action::Close => " Close ",
            };
            let button = Rect::new(
                x,
                inner.bottom() - 2,
                (label.width() as u16).min(inner.right().saturating_sub(x)),
                1,
            );
            let selected = if self.primary {
                index == 0
            } else {
                index == actions.len() - 1
            };
            f.render_widget(Paragraph::new(label).style(button_style(selected)), button);
            hits.push((button, index));
            x = button.right() + 2;
        }
        let hint = if matches!(
            self.state,
            State::Checking | State::Installing(_) | State::Ready(_)
        ) {
            "Tab Select  Enter Run  Esc Close"
        } else if inner.width < 44 {
            "Tab Next  Enter Run  r Check  Esc"
        } else {
            "Tab Select  Enter Run  r Check  Esc Close"
        };
        f.render_widget(
            Paragraph::new(hint).style(Style::new().fg(MUTED).bg(TAG)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}

impl super::super::Shell {
    pub(in crate::shell) fn software_update_event(&mut self, event: &Event) {
        let actions = self.software_update.actions();
        let action = match event {
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => self
                .pop_hits
                .iter()
                .rev()
                .find(|(area, _)| inside(*area, mouse.column, mouse.row))
                .and_then(|(_, index)| {
                    if *index == usize::MAX {
                        Some(Action::Close)
                    } else {
                        actions.get(*index).copied()
                    }
                }),
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
                self.software_update.scroll = self.software_update.scroll.saturating_add(3);
                None
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
                self.software_update.scroll = self.software_update.scroll.saturating_sub(3);
                None
            }
            Event::Key(key) if !key.modifiers.intersects(M::CONTROL | M::ALT | M::SUPER) => {
                match key.code {
                    K::Esc | K::Char('q') => Some(Action::Close),
                    K::Tab | K::BackTab | K::Left | K::Right if key.kind == KeyEventKind::Press => {
                        self.software_update.primary = !self.software_update.primary;
                        None
                    }
                    K::Enter if key.kind == KeyEventKind::Press => actions
                        .get(if self.software_update.primary {
                            0
                        } else {
                            actions.len() - 1
                        })
                        .copied(),
                    K::Char('r') if key.kind == KeyEventKind::Press => Some(Action::Check),
                    K::Char('j') | K::Down => {
                        self.software_update.scroll = self.software_update.scroll.saturating_add(1);
                        None
                    }
                    K::Char('k') | K::Up => {
                        self.software_update.scroll = self.software_update.scroll.saturating_sub(1);
                        None
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        match action {
            Some(Action::Close) => self.pop = None,
            Some(Action::Check) => self.software_update.check(false),
            Some(Action::Install) => self.software_update.install(),
            Some(Action::Restart) => {
                if self.git_busy() || self.panel().editing() {
                    self.notice(
                        "Finish the current input or remote operation before restarting".into(),
                    );
                } else if let State::Ready(report) = &self.software_update.state {
                    match crate::installation::restart_path(report) {
                        Ok(path) => {
                            self.restart = Some(path);
                            self.quit = true;
                        }
                        Err(error) => self.notice(format!("Restart: {error:#}")),
                    }
                }
            }
            None => (),
        }
    }
}
