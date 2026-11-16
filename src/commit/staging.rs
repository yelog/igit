//! File-local staged/unstaged inspection with separately reviewed index writes.
use super::*;
use crate::ui::{
    help::{HelpContent, HelpSection},
    idea::{button_style, clipped, diff_line},
};
use backend::staging::{self, Side};
use ratatui::layout::{Constraint, Layout, Margin};

#[derive(Clone, Copy)]
enum Control {
    Side(Side),
    Previous,
    Next,
    Review,
    Reload,
    Close,
    Range,
    Whole,
    Line(usize),
}

pub(super) struct Staging {
    snapshot: staging::Snapshot,
    cursor: usize,
    line: usize,
    anchor: Option<usize>,
    scroll: u16,
    body: Rect,
    dragging: bool,
    hits: Vec<(Rect, Control)>,
}

impl Staging {
    fn load(root: usize, repo: &Path, path: &Path, side: Side) -> Result<Self> {
        let snapshot = staging::read(root, repo, path, side)?;
        let line = first_change(&snapshot, 0);
        Ok(Self {
            snapshot,
            cursor: 0,
            line,
            anchor: None,
            scroll: 0,
            body: Rect::default(),
            dragging: false,
            hits: Vec::new(),
        })
    }

    fn reload(&mut self, side: Side) -> Result<()> {
        let next = staging::read(
            self.snapshot.root,
            &self.snapshot.repo,
            &self.snapshot.path,
            side,
        )?;
        self.snapshot = next;
        self.cursor = self.cursor.min(self.snapshot.hunks.len().saturating_sub(1));
        self.line = first_change(&self.snapshot, self.cursor);
        self.anchor = None;
        self.dragging = false;
        self.scroll = 0;
        Ok(())
    }

    fn navigate(&mut self, next: bool) {
        self.cursor = if next {
            (self.cursor + 1).min(self.snapshot.hunks.len().saturating_sub(1))
        } else {
            self.cursor.saturating_sub(1)
        };
        self.line = first_change(&self.snapshot, self.cursor);
        self.anchor = None;
        self.dragging = false;
        self.scroll = 0;
    }

    fn move_line(&mut self, down: bool) {
        let count = self
            .snapshot
            .hunks
            .get(self.cursor)
            .map_or(0, |hunk| hunk.text.lines().count());
        self.line = if down {
            self.line.saturating_add(1).min(count.saturating_sub(1))
        } else {
            self.line.saturating_sub(1)
        };
        self.scroll = usize::from(self.scroll)
            .min(self.line)
            .max((self.line + 1).saturating_sub(usize::from(self.body.height).max(1)))
            .min(u16::MAX as usize) as u16;
    }

    fn button(
        &mut self,
        f: &mut Frame,
        row: Rect,
        x: &mut u16,
        label: &str,
        control: Control,
        selected: bool,
    ) {
        use unicode_width::UnicodeWidthStr;
        let width = (label.width() as u16).min(row.right().saturating_sub(*x));
        if width == 0 {
            return;
        }
        let rect = Rect::new(*x, row.y, width, row.height.min(1));
        f.render_widget(
            Paragraph::new(clipped(label, width.into())).style(button_style(selected)),
            rect,
        );
        self.hits.push((rect, control));
        *x = rect.right().saturating_add(1);
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let width = area.width.saturating_sub(2).min(110);
        let height = area.height.saturating_sub(2).min(34);
        let modal = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, modal);
        f.render_widget(crate::ui::pane("Stage / Unstage hunks", true), modal);
        let inner = modal.inner(Margin::new(1, 1));
        let [name, sources, status, body, navigation, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        self.hits.clear();
        self.body = body;
        let repo = &c.repos[self.snapshot.root];
        f.render_widget(
            Paragraph::new(clipped(
                &format!("{} / {}", repo.name, display_path(&self.snapshot.path)),
                name.width.into(),
            )),
            name,
        );
        let mut x = sources.x;
        self.button(
            f,
            sources,
            &mut x,
            " Unstaged (u) ",
            Control::Side(Side::Unstaged),
            self.snapshot.side == Side::Unstaged,
        );
        self.button(
            f,
            sources,
            &mut x,
            " Staged (s) ",
            Control::Side(Side::Staged),
            self.snapshot.side == Side::Staged,
        );
        let count = self.snapshot.hunks.len();
        let selection = self.anchor.map_or_else(
            || "whole hunk".to_owned(),
            |anchor| format!("lines {}–{}", anchor.min(self.line), anchor.max(self.line)),
        );
        let label = format!(
            "{} · hunk {}/{} · {selection}",
            self.snapshot.side.label(),
            usize::from(count > 0) + self.cursor,
            count
        );
        f.render_widget(
            Paragraph::new(clipped(&label, status.width.into())).style(Style::new().fg(MUTED)),
            status,
        );
        let text = self.snapshot.hunks.get(self.cursor).map_or(
            "No hunks on this side. Tab switches staged / unstaged.",
            |hunk| hunk.text.as_str(),
        );
        let lines: Vec<_> = text
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let mut rendered = diff_line(line);
                rendered.spans.insert(
                    0,
                    ratatui::text::Span::styled(
                        if index == self.line { "› " } else { "  " },
                        Style::new().fg(BLUE),
                    ),
                );
                if self.anchor.is_some_and(|anchor| {
                    (anchor.min(self.line)..=anchor.max(self.line)).contains(&index)
                }) {
                    rendered = rendered.style(Style::new().bg(SEL));
                }
                rendered
            })
            .collect();
        let max_scroll = lines
            .len()
            .saturating_sub(body.height.into())
            .min(u16::MAX as usize) as u16;
        self.scroll = self.scroll.min(max_scroll);
        f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), body);
        for row in 0..body.height {
            let line = usize::from(self.scroll) + usize::from(row);
            if line < text.lines().count() {
                self.hits.push((
                    Rect::new(body.x, body.y + row, body.width, 1),
                    Control::Line(line),
                ));
            }
        }
        x = navigation.x;
        self.button(
            f,
            navigation,
            &mut x,
            " p Previous ",
            Control::Previous,
            false,
        );
        self.button(f, navigation, &mut x, " n Next ", Control::Next, false);
        self.button(f, navigation, &mut x, " r Reload ", Control::Reload, false);
        self.button(
            f,
            navigation,
            &mut x,
            " v Lines ",
            Control::Range,
            self.anchor.is_some(),
        );
        self.button(
            f,
            navigation,
            &mut x,
            " a Whole ",
            Control::Whole,
            self.anchor.is_none(),
        );
        x = footer.x;
        self.button(f, footer, &mut x, " Esc Close ", Control::Close, false);
        if count > 0 {
            let label = format!(
                " Space {} {} ",
                self.snapshot.side.action(),
                if self.anchor.is_some() {
                    "lines"
                } else {
                    "hunk"
                }
            );
            self.button(f, footer, &mut x, &label, Control::Review, true);
        }
    }
}

fn first_change(snapshot: &staging::Snapshot, cursor: usize) -> usize {
    snapshot.hunks.get(cursor).map_or(0, |hunk| {
        hunk.text
            .lines()
            .position(|line| line.starts_with(['+', '-']))
            .unwrap_or(0)
    })
}

impl CommitPanel {
    pub(super) fn open_staging(&mut self, c: &Context) -> Result<Action> {
        if self.add_running() {
            bail!(Constraint(
                "Adding files to Git; wait before inspecting staged hunks"
            ));
        }
        if self.execution_running() {
            bail!(Constraint("Wait for the current Git write to finish"));
        }
        let file = self
            .active()
            .and_then(Row::file)
            .map(|i| &self.files[i])
            .ok_or(Constraint(
                "Select one tracked file to inspect its staged and unstaged hunks",
            ))?;
        if !c.scope.contains(&file.root) {
            bail!(Constraint("File is outside Roots"));
        }
        let side = if file.change.xy[1] != b' ' {
            Side::Unstaged
        } else {
            Side::Staged
        };
        self.staging = Some(Staging::load(
            file.root,
            &c.repos[file.root].path,
            &file.change.path,
            side,
        )?);
        Ok(Action::None)
    }

    pub(super) fn staging_event(&mut self, e: &Event, c: &Context) -> Option<Action> {
        let stage = self.staging.as_mut()?;
        let control = match e {
            Event::Key(k) if k.modifiers.is_empty() || k.modifiers == Mod::SHIFT => match k.code {
                K::Esc => Some(Control::Close),
                K::Tab | K::BackTab => {
                    Some(Control::Side(if stage.snapshot.side == Side::Unstaged {
                        Side::Staged
                    } else {
                        Side::Unstaged
                    }))
                }
                K::Char('u') => Some(Control::Side(Side::Unstaged)),
                K::Char('s') => Some(Control::Side(Side::Staged)),
                K::Char('n') | K::Right => Some(Control::Next),
                K::Char('p') | K::Left => Some(Control::Previous),
                K::Char('r') => Some(Control::Reload),
                K::Char('v') => Some(Control::Range),
                K::Char('a') => Some(Control::Whole),
                K::Char(' ') | K::Enter => Some(Control::Review),
                K::Char('j') | K::Down => {
                    stage.move_line(true);
                    None
                }
                K::Char('k') | K::Up => {
                    stage.move_line(false);
                    None
                }
                K::PageDown => {
                    stage.scroll = stage.scroll.saturating_add(10);
                    None
                }
                K::PageUp => {
                    stage.scroll = stage.scroll.saturating_sub(10);
                    None
                }
                _ => None,
            },
            Event::Mouse(m) => match m.kind {
                M::Down(MouseButton::Left) => {
                    let control = stage
                        .hits
                        .iter()
                        .rev()
                        .find(|(rect, _)| inside(*rect, m.column, m.row))
                        .map(|(_, control)| *control);
                    stage.dragging = matches!(control, Some(Control::Line(_)));
                    if let Some(Control::Line(line)) = control {
                        if m.modifiers.contains(Mod::SHIFT) {
                            stage.anchor.get_or_insert(stage.line);
                        } else {
                            stage.anchor = Some(line);
                        }
                    }
                    control
                }
                M::Drag(MouseButton::Left) if stage.dragging => {
                    if !stage.body.is_empty() {
                        let row = m.row.clamp(stage.body.y, stage.body.bottom() - 1);
                        Some(Control::Line(
                            usize::from(stage.scroll) + usize::from(row - stage.body.y),
                        ))
                    } else {
                        None
                    }
                }
                M::Up(MouseButton::Left) => {
                    stage.dragging = false;
                    None
                }
                M::ScrollDown => {
                    stage.scroll = stage.scroll.saturating_add(3);
                    None
                }
                M::ScrollUp => {
                    stage.scroll = stage.scroll.saturating_sub(3);
                    None
                }
                _ => None,
            },
            _ => None,
        };
        let action = match control {
            Some(Control::Close) => {
                self.staging = None;
                self.staging_plan = None;
                Action::None
            }
            Some(Control::Side(side)) => {
                stage.reload(side).map(|_| Action::None).unwrap_or_else(err)
            }
            Some(Control::Reload) => stage
                .reload(stage.snapshot.side)
                .map(|_| Action::None)
                .unwrap_or_else(err),
            Some(Control::Previous) => {
                stage.navigate(false);
                Action::None
            }
            Some(Control::Next) => {
                stage.navigate(true);
                Action::None
            }
            Some(Control::Range) => {
                stage.anchor = if stage.anchor.is_some() {
                    None
                } else {
                    Some(stage.line)
                };
                Action::None
            }
            Some(Control::Whole) => {
                stage.anchor = None;
                Action::None
            }
            Some(Control::Line(line)) => {
                let count = stage
                    .snapshot
                    .hunks
                    .get(stage.cursor)
                    .map_or(0, |hunk| hunk.text.lines().count());
                stage.line = line.min(count.saturating_sub(1));
                Action::None
            }
            Some(Control::Review) => {
                let snapshot = &stage.snapshot;
                if !c.scope.contains(&snapshot.root)
                    || c.repos
                        .get(snapshot.root)
                        .is_none_or(|repo| repo.path != snapshot.repo)
                {
                    return Some(warning("Roots or workspace changed; reopen the hunk view"));
                }
                let selected = stage.anchor.map_or_else(
                    || staging::plan(snapshot, stage.cursor),
                    |anchor| staging::plan_range(snapshot, stage.cursor, anchor, stage.line),
                );
                match selected {
                    Ok(plan) => {
                        let noun = if stage.anchor.is_some() {
                            "lines"
                        } else {
                            "hunk"
                        };
                        let action = Action::Confirm {
                            title: format!("Review {} {noun}", plan.side.action()),
                            body: format!("Repository: {}\nFile: {}\n{} only the displayed patch in the index.\nWorking files and unrelated staging are kept.\n\n{}", c.repos[plan.root].name, display_path(&plan.path), plan.side.action(), plan.patch),
                            token: "commit-hunk".into(),
                        };
                        self.staging_plan = Some(plan);
                        action
                    }
                    Err(error) => err(error),
                }
            }
            None => Action::None,
        };
        Some(action)
    }

    pub(super) fn confirm_staging(&mut self, c: &Context) -> Action {
        let Some(plan) = self.staging_plan.take() else {
            return warning("Hunk review expired");
        };
        if self.execution_running()
            || self.add_running()
            || !c.scope.contains(&plan.root)
            || c.repos
                .get(plan.root)
                .is_none_or(|repo| repo.path != plan.repo)
        {
            return warning("Workspace or Roots changed; review the hunk again");
        }
        match staging::execute(&plan) {
            Ok(()) => {
                let refresh_error = self
                    .staging
                    .as_mut()
                    .and_then(|stage| stage.reload(plan.side).err());
                let mut message = format!(
                    "{} hunk: {} / {}",
                    plan.side.action(),
                    c.repos[plan.root].name,
                    display_path(&plan.path)
                );
                let level = if let Some(error) = refresh_error {
                    self.staging = None;
                    message.push_str(&format!("\nCould not refresh hunks: {error:#}"));
                    NoticeLevel::Warning
                } else {
                    NoticeLevel::Success
                };
                Action::Refresh(level, message)
            }
            Err(error) => err(error),
        }
    }

    pub(super) fn draw_staging(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if let Some(stage) = self.staging.as_mut() {
            stage.draw(f, area, c);
        }
    }

    pub(super) fn staging_help(&self) -> HelpContent {
        HelpContent::new(
            HelpSection::new(
                "Commit / Stage Hunks",
                &[
                    ("Tab", "Switch staged / unstaged"),
                    ("u", "Inspect unstaged hunks"),
                    ("s", "Inspect staged hunks"),
                    ("n", "Next hunk"),
                    ("p", "Previous hunk"),
                    ("j", "Move to the next patch line"),
                    ("k", "Move to the previous patch line"),
                    ("v", "Start or stop a line range"),
                    ("a", "Select the complete hunk"),
                    (
                        "Space",
                        "Review selected hunk or lines for staging / unstaging",
                    ),
                    ("r", "Reload this file"),
                    ("Esc", "Close without changing the index"),
                ],
            ),
            vec![],
        )
    }
}

#[cfg(test)]
mod tests;
