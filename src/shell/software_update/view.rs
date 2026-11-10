use super::{Action, SoftwareUpdate, State, FADE_STEPS};
use crate::{
    installation::{Inspection, Progress, Status},
    ui::{
        help::{HelpContent, HelpSection},
        icons::IconMode,
        idea::{button_style, CONFLICT, GREEN, RED},
        inside,
        selection::{TextLayout, Wrap},
        ACCENT, BG, FG, MUTED,
    },
};
use crossterm::event::{
    Event, KeyCode as K, KeyEventKind, KeyModifiers as M, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

const CONFIRM: usize = usize::MAX;
const NEXT: usize = usize::MAX - 1;
const RECHECK: usize = usize::MAX - 2;
const DISMISS: usize = usize::MAX - 3;

impl SoftwareUpdate {
    pub(in crate::shell) fn footer_status(&self, icons: IconMode) -> (String, Style) {
        let (label, color, bold) = match &self.state {
            State::Checking => (" Checking", MUTED, false),
            State::Installing(..) => (" Updating", ACCENT, false),
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

    fn body(&self, width: u16, ascii: bool) -> Vec<Line<'static>> {
        let success = if ascii { "OK" } else { "✓" };
        let up = if ascii { "^" } else { "↑" };
        let error = if ascii { "x" } else { "×" };
        let dot = if ascii { "/" } else { "·" };
        let arrow = if ascii { "->" } else { "→" };
        let versions = |report: &Inspection| {
            format!(
                "Current {} {arrow} Latest {} {dot} {}",
                report.current_version,
                report.target_version,
                channel(report)
            )
        };
        let (title, color, details) = match &self.state {
            State::Idle => (
                "Ready to check for updates".into(),
                FG,
                vec!["No update check has run yet.".into()],
            ),
            State::Checking => (
                "Checking for updates…".into(),
                ACCENT,
                vec!["This only takes a moment.".into()],
            ),
            State::Report(report) if report.status == Status::UpToDate => {
                let ahead = report.current_version != report.target_version;
                let mut details = vec![format!(
                    "Current {} {dot} {}",
                    report.current_version,
                    channel(report)
                )];
                if ahead {
                    details.push(format!("Channel latest {}", report.target_version));
                }
                let title = if ahead {
                    "No update needed"
                } else {
                    "You're up to date"
                };
                (format!("{success} {title}"), GREEN, details)
            }
            State::Report(report) => (
                format!("{up} Update available"),
                CONFLICT,
                vec![versions(report)],
            ),
            State::Installing(report, progress) => (
                "Updating IGit".into(),
                ACCENT,
                vec![
                    format!(
                        "Installing {} {dot} {}",
                        report.target_version,
                        channel(report)
                    ),
                    progress.label().into(),
                ],
            ),
            State::Ready(report) => (
                format!("{success} Update installed"),
                GREEN,
                vec![format!("Restart to use {}", report.target_version)],
            ),
            State::Failed(message) => (
                format!("{error} Update failed"),
                RED,
                vec![terminal_text(message)],
            ),
        };

        let mut body = TextLayout::default();
        body.push(
            Line::styled(title, Style::new().fg(color).bold()),
            width,
            Wrap::Word,
        );
        for detail in &details {
            for line in detail.split('\n') {
                body.push(
                    Line::styled(line, Style::new().fg(MUTED)),
                    width,
                    Wrap::Word,
                );
            }
        }
        if self.instructions {
            if let State::Report(report) = &self.state {
                body.push(Line::default(), width, Wrap::Word);
                let message = terminal_text(
                    report
                        .action
                        .as_deref()
                        .unwrap_or("Update IGit with its original installation method."),
                );
                for line in message.split('\n') {
                    body.push(Line::styled(line, Style::new().fg(FG)), width, Wrap::Word);
                }
            }
        }
        body.rows.into_iter().map(|row| row.line).collect()
    }

    fn action_label(&self, action: Action) -> &'static str {
        match action {
            Action::Check if matches!(self.state, State::Failed(_)) => "Check again",
            Action::Check => "Check now",
            Action::Install => "Update",
            Action::Instructions => "How to update",
            Action::Restart => "Restart now",
            Action::Close if self.actions().len() > 1 => "Not now",
            Action::Close => match self.state {
                State::Installing(..) => "Run in background",
                State::Checking => "Close",
                _ => "OK",
            },
        }
    }

    pub(in crate::shell) fn keymap(&self) -> HelpContent {
        HelpContent::new(
            HelpSection::new(
                "Update Center",
                &[
                    ("Tab", "Switch focused button"),
                    ("Enter", "Activate selected button"),
                    ("r", "Check again after a check result or failure"),
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
        let max_width = if self.instructions { 76 } else { 64 };
        let width = max_width.min(area.width.saturating_sub(4));
        if width < 20 || area.height < 6 {
            return;
        }
        let ascii = icons == IconMode::Ascii;
        let mut body = self.body(width - 6, ascii);
        let progress = match self.state {
            State::Installing(_, progress) => Some(progress),
            _ => None,
        };
        let labels: Vec<_> = self
            .actions()
            .iter()
            .map(|a| self.action_label(*a))
            .collect();
        let buttons_width = labels.iter().map(|s| s.width() as u16 + 6).sum::<u16>()
            + 2 * (labels.len() as u16 - 1);
        let stacked = buttons_width > width - 2;
        let action_rows = if stacked { labels.len() as u16 } else { 1 };
        let desired_height =
            body.len() + usize::from(progress.is_some()) * 2 + usize::from(action_rows) + 4; // borders, footer, and a spacer before buttons
        let height = desired_height.min(usize::from(area.height)) as u16;
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.buffer_mut()
            .set_style(area, Style::new().add_modifier(Modifier::DIM));
        f.render_widget(Clear, popup);
        let block = Block::new()
            .title(" UPDATE CENTER ")
            .title_style(Style::new().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(ACCENT))
            .style(Style::new().fg(FG).bg(BG));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        let footer_y = inner.bottom() - 1;
        let buttons_y = footer_y - action_rows;
        let mut text = Rect::new(inner.x + 2, inner.y, inner.width - 4, buttons_y - inner.y);
        if let Some(progress) = progress.filter(|_| text.height > 0) {
            let offset = (body.len() + 1).min(usize::from(text.height - 1)) as u16;
            f.render_widget(
                Paragraph::new(progress_line(progress, text.width, self.sweep, ascii)),
                Rect::new(text.x, text.y + offset, text.width, 1),
            );
            text.height = offset;
        }
        self.scroll = self.scroll.min(
            body.len()
                .saturating_sub(usize::from(text.height))
                .min(usize::from(u16::MAX)) as u16,
        );
        body.drain(..usize::from(self.scroll));
        f.render_widget(Paragraph::new(body), text);

        let mut x = inner.x + inner.width.saturating_sub(buttons_width) / 2;
        for (i, label) in labels.into_iter().enumerate() {
            let focused = i == self.focused_button();
            let label = format!("[ {} {label} ]", if focused { ">" } else { " " });
            let width = (label.width() as u16).min(inner.width);
            let button = if stacked {
                Rect::new(
                    inner.x + (inner.width - width) / 2,
                    buttons_y + i as u16,
                    width,
                    1,
                )
            } else {
                Rect::new(x, buttons_y, width, 1)
            };
            f.render_widget(
                Paragraph::new(Span::styled(label, button_style(focused))),
                button,
            );
            hits.push((button, i));
            x += width + 2;
        }
        self.draw_hints(f, Rect::new(inner.x, footer_y, inner.width, 1), hits);
        if self.opened_at.is_some() && self.fade < FADE_STEPS {
            fade(f, popup, self.fade);
        }
    }

    fn draw_hints(&self, f: &mut Frame, area: Rect, hits: &mut Vec<(Rect, usize)>) {
        let mut hints = Vec::with_capacity(4);
        if self.actions().len() > 1 {
            hints.push(("Tab", "select next", "next", NEXT));
        }
        hints.push(("Enter", "confirm", "run", CONFIRM));
        if self.can_check_again() {
            hints.push(("r", "check again", "check", RECHECK));
        }
        hints.push(("Esc", "close", "close", DISMISS));

        let required = hints
            .iter()
            .map(|(key, long, _, _)| key.len() + long.len() + 1)
            .sum::<usize>()
            + 3 * (hints.len() - 1);
        let compact = required > usize::from(area.width);
        let gap = if compact { 2 } else { 3 };
        let compact_width = hints
            .iter()
            .map(|(key, _, short, _)| key.len() + short.len() + 1)
            .sum::<usize>()
            + gap * (hints.len() - 1);
        let mut spans = Vec::new();
        let mut x = area.x;
        for (key, long, short, hit) in hints {
            let description = if !compact {
                long
            } else if compact_width <= usize::from(area.width) {
                short
            } else {
                ""
            };
            let len = key.len()
                + if description.is_empty() {
                    0
                } else {
                    1 + description.len()
                };
            if x + len as u16 > area.right() {
                break;
            }
            spans.push(Span::styled(key, Style::new().fg(FG).bold()));
            if !description.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(description, Style::new().fg(MUTED)));
            }
            spans.push(Span::raw(if gap == 2 { "  " } else { "   " }));
            hits.push((Rect::new(x, area.y, len as u16, 1), hit));
            x += (len + gap) as u16;
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

fn channel(report: &Inspection) -> &str {
    match report.channel.as_str() {
        "stable" => "Stable",
        "beta" => "Beta",
        other => other,
    }
}

fn terminal_text(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

fn progress_line(progress: Progress, width: u16, frame: usize, ascii: bool) -> Line<'static> {
    let ratio = progress.ratio();
    let suffix = match (ratio, progress) {
        (Some(ratio), _) => format!(" {:>3}%", (ratio * 100.0).round() as u16),
        (None, Progress::Downloading { bytes, .. }) => format!(" {}", format_bytes(bytes)),
        _ => String::new(),
    };
    let width = usize::from(width);
    let bar_width = width
        .saturating_sub(8)
        .min(width.saturating_sub(2 + suffix.width()));
    let segment = (bar_width / 4).max(2);
    let (filled, empty) = if ascii { ('#', '-') } else { ('█', '░') };
    let bar: String = (0..bar_width)
        .map(|cell| {
            let active = match ratio {
                Some(ratio) => cell < (ratio * bar_width as f64).round() as usize,
                None => (cell + bar_width - frame % bar_width) % bar_width < segment,
            };
            if active {
                filled
            } else {
                empty
            }
        })
        .collect();
    Line::from(vec![
        Span::styled("[", Style::new().fg(MUTED)),
        Span::styled(bar, Style::new().fg(ACCENT)),
        Span::styled(format!("]{suffix}"), Style::new().fg(FG)),
    ])
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn fade(f: &mut Frame, popup: Rect, frame: u8) {
    let remaining = u16::from(FADE_STEPS - frame);
    let total = u16::from(FADE_STEPS).pow(2);
    let strength = total - remaining.pow(2); // quadratic ease-out over 160 ms
    let scale = |c| (u16::from(c) * strength / total) as u8;
    let buffer = f.buffer_mut();
    for y in popup.y..popup.bottom() {
        for x in popup.x..popup.right() {
            let cell = &mut buffer[(x, y)];
            let (r, g, b) = match cell.fg {
                Color::Rgb(r, g, b) => (r, g, b),
                Color::White => (255, 255, 255),
                _ => continue,
            };
            cell.set_fg(Color::Rgb(scale(r), scale(g), scale(b)));
        }
    }
}

impl super::super::Shell {
    pub(in crate::shell) fn software_update_event(&mut self, event: &Event) {
        let update = &mut self.software_update;
        let action = match event {
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => self
                .pop_hits
                .iter()
                .find(|(area, _)| inside(*area, mouse.column, mouse.row))
                .and_then(|(_, index)| match *index {
                    CONFIRM => update.actions().get(update.focused_button()).copied(),
                    NEXT => {
                        update.primary = !update.primary;
                        None
                    }
                    RECHECK => Some(Action::Check),
                    DISMISS => Some(Action::Close),
                    index => update.actions().get(index).copied(),
                }),
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
                update.scroll = update.scroll.saturating_add(3);
                None
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
                update.scroll = update.scroll.saturating_sub(3);
                None
            }
            Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && !key.modifiers.intersects(M::CONTROL | M::ALT | M::SUPER) =>
            {
                match key.code {
                    K::Esc | K::Char('q') => Some(Action::Close),
                    K::Tab | K::BackTab | K::Left | K::Right if key.kind == KeyEventKind::Press => {
                        update.primary = !update.primary;
                        None
                    }
                    K::Enter if key.kind == KeyEventKind::Press => {
                        update.actions().get(update.focused_button()).copied()
                    }
                    K::Char('r') if key.kind == KeyEventKind::Press => Some(Action::Check),
                    K::Char('j') | K::Down => {
                        update.scroll = update.scroll.saturating_add(1);
                        None
                    }
                    K::Char('k') | K::Up => {
                        update.scroll = update.scroll.saturating_sub(1);
                        None
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        match action {
            Some(Action::Close) => self.pop = None,
            Some(Action::Check) => update.check(false),
            Some(Action::Install) => update.install(),
            Some(Action::Instructions) => {
                update.instructions = true;
                update.primary = false;
                update.scroll = 0;
            }
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
                        Err(error) => self.notify(
                            crate::ui::NoticeLevel::Error,
                            "IGit Update",
                            format!("Restart: {error:#}"),
                        ),
                    }
                }
            }
            None => (),
        }
    }
}
