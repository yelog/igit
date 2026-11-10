//! Mouse and keyboard entry points for the selected commit's operations.
use super::{
    input::TextInput,
    operations::{Kind, Review, Target},
    *,
};
use crate::ui::{help::HelpSection, idea::button_style};

const SELECT_ROW: usize = Kind::ALL.len();
use crossterm::event::{KeyEvent, KeyEventKind};

pub(super) struct Dialog {
    target: Target,
    anchor: Option<(u16, u16)>,
    mode: Mode,
    hits: Vec<(Rect, Hit)>,
}

enum Mode {
    Menu(usize),
    Input(Kind, TextInput),
}

#[derive(Clone, Copy)]
enum Hit {
    Choice(usize),
    Text,
    Review,
    Cancel,
}

enum Input {
    None,
    Close,
    Choose(Kind),
    SelectCommits,
    Review(Kind, String),
}

impl Dialog {
    fn new(target: Target, anchor: Option<(u16, u16)>, mode: Mode) -> Self {
        Self {
            target,
            anchor,
            mode,
            hits: vec![],
        }
    }

    pub fn help(&self) -> HelpSection {
        match self.mode {
            Mode::Menu(_) => HelpSection::new(
                "Log / Commit Actions",
                &[
                    ("j", "Select next action"),
                    ("k", "Select previous action"),
                    ("Enter", "Open selected action"),
                    ("Esc", "Close commit actions"),
                ],
            ),
            Mode::Input(kind, _) => HelpSection::new(
                if kind == Kind::Edit {
                    "Log / Edit Commit Message"
                } else {
                    "Log / Name"
                },
                &[
                    ("F2", "Review changes"),
                    ("Ctrl-s", "Review changes"),
                    ("Ctrl-u", "Clear input"),
                    ("Esc", "Cancel"),
                ],
            ),
        }
    }

    fn choose(row: usize) -> Input {
        Kind::ALL
            .get(row)
            .map_or(Input::SelectCommits, |kind| Input::Choose(*kind))
    }

    fn event(&mut self, event: &Event) -> Input {
        if matches!(event, Event::Key(k) if k.kind == KeyEventKind::Release) {
            return Input::None;
        }
        if matches!(event, Event::Key(KeyEvent { code: K::Esc, .. })) {
            return Input::Close;
        }
        match &mut self.mode {
            Mode::Menu(row) => {
                let delta = match event {
                    Event::Key(k) => match k.code {
                        K::Down | K::Char('j') => 1,
                        K::Up | K::Char('k') => -1,
                        K::Home => -(Kind::ALL.len() as isize),
                        K::End => Kind::ALL.len() as isize,
                        K::Enter => return Self::choose(*row),
                        _ => 0,
                    },
                    Event::Mouse(m) => {
                        let hit = self.hits.iter().find(|(r, _)| inside(*r, m.column, m.row));
                        match (m.kind, hit.map(|(_, h)| *h)) {
                            (M::Down(MouseButton::Left), Some(Hit::Choice(kind))) => {
                                return Self::choose(kind);
                            }
                            (M::Down(_), None) => return Input::Close,
                            (M::Moved, Some(Hit::Choice(kind))) => {
                                *row = kind;
                                0
                            }
                            (M::ScrollDown, _) => 1,
                            (M::ScrollUp, _) => -1,
                            _ => 0,
                        }
                    }
                    _ => 0,
                };
                *row = row.saturating_add_signed(delta).min(SELECT_ROW);
            }
            Mode::Input(kind, input) => {
                let submit = match event {
                    Event::Key(k) => {
                        if matches!(k.code, K::F(2))
                            || k.modifiers.contains(Mods::CONTROL)
                                && matches!(k.code, K::Char('s') | K::Enter)
                            || *kind != Kind::Edit && k.code == K::Enter
                        {
                            true
                        } else {
                            input.key(k);
                            false
                        }
                    }
                    Event::Paste(text) => {
                        if *kind == Kind::Edit {
                            input.paste(&text.replace('\0', ""));
                        } else if !text.trim().chars().any(char::is_control) {
                            input.paste(text.trim());
                        }
                        false
                    }
                    Event::Mouse(m) if m.kind == M::Down(MouseButton::Left) => {
                        match self.hits.iter().find(|(r, _)| inside(*r, m.column, m.row)) {
                            Some((_, Hit::Cancel)) => return Input::Close,
                            Some((_, Hit::Review)) => true,
                            Some((_, Hit::Text)) => {
                                input.place(m.column, m.row);
                                false
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                };
                if submit {
                    return Input::Review(*kind, input.text.clone());
                }
            }
        }
        Input::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.hits.clear();
        let (title, width, height) = match self.mode {
            Mode::Menu(_) => ("Commit Actions", 42, Kind::ALL.len() as u16 + 4),
            Mode::Input(kind, _) => (kind.title(), 68, if kind == Kind::Edit { 15 } else { 7 }),
        };
        let mut r = popup(area, width, height);
        if let (Mode::Menu(_), Some((x, y))) = (&self.mode, self.anchor) {
            r.x = x.clamp(area.x, area.right() - r.width);
            r.y = y.saturating_add(1).clamp(area.y, area.bottom() - r.height);
        }
        f.render_widget(Clear, r);
        let frame = boxof(title.to_owned(), true);
        let inner = frame.inner(r);
        f.render_widget(frame, r);
        if inner.is_empty() {
            return;
        }
        let repo = c
            .repos
            .get(self.target.root)
            .map_or("Repository", |repo| &repo.name);
        let header = format!("{} · {repo} · {}", self.target.short(), self.target.subject);
        let line = |y| Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(
            Paragraph::new(clipped(&header, inner.width.into())).style(fg(DIM)),
            line(inner.y),
        );
        match &mut self.mode {
            Mode::Menu(row) => {
                let h = usize::from(inner.height - 1);
                let start = window(*row, h, SELECT_ROW + 1);
                for i in (start..=SELECT_ROW).take(h) {
                    let label = match Kind::ALL.get(i) {
                        Some(Kind::CherryPick) => "Cherry-pick Commit  C",
                        Some(kind) => kind.title(),
                        None => "Select Commits  v",
                    };
                    let r = line(inner.y + 1 + (i - start) as u16);
                    f.render_widget(
                        Paragraph::new(format!(" {label}")).style(fg(INK).bg(if i == *row {
                            SELECT
                        } else {
                            PANEL
                        })),
                        r,
                    );
                    self.hits.push((r, Hit::Choice(i)));
                }
            }
            Mode::Input(kind, input) if inner.height > 2 => {
                let r = Rect::new(
                    inner.x,
                    inner.y + 1,
                    inner.width,
                    if *kind == Kind::Edit {
                        inner.height - 2
                    } else {
                        1
                    },
                );
                f.render_widget(Block::default().style(fg(INK).bg(EDGE)), r);
                input.draw(f, r);
                self.hits.push((r, Hit::Text));
                let mut footer = line(inner.bottom() - 1);
                for (label, hit) in [(" Cancel Esc ", Hit::Cancel), (" Review F2 ", Hit::Review)] {
                    let r = take(&mut footer, cells(label));
                    f.render_widget(
                        Paragraph::new(label).style(button_style(matches!(hit, Hit::Review))),
                        r,
                    );
                    self.hits.push((r, hit));
                }
                if *kind == Kind::Edit {
                    f.render_widget(Paragraph::new(" Enter: new line").style(fg(DIM)), footer);
                }
            }
            _ => (),
        }
    }
}

impl LogPanel {
    pub(super) fn set_pick_mode(&mut self, enabled: bool) {
        self.cancel_key_sequence();
        self.pick_mode = enabled;
        if !enabled {
            self.picks.clear();
        }
    }

    pub(super) fn toggle_pick(&mut self, c: &Context) -> Result<()> {
        self.cancel_key_sequence();
        self.require_current_history(c)?;
        let rev = self
            .rev()
            .ok_or_else(|| anyhow::anyhow!("No commit selected"))?;
        let target = Target::new(c, rev)?;
        if let Some(i) = self
            .picks
            .iter()
            .position(|pick| pick.root == target.root && pick.oid == target.oid)
        {
            self.picks.remove(i);
        } else {
            if let Some(first) = self.picks.first() {
                anyhow::ensure!(
                    first.root == target.root,
                    "Select commits from {}",
                    c.repos
                        .get(first.root)
                        .map_or("the original repository", |repo| repo.name.as_str())
                );
            }
            anyhow::ensure!(
                rev.parents.len() <= 1,
                "Cherry-picking a merge requires choosing a mainline parent"
            );
            self.picks.push(target);
        }
        Ok(())
    }

    pub(super) fn review_picks(&mut self, c: &Context, target: Option<Target>) -> Result<Action> {
        self.require_current_history(c)?;
        let picks = if self.pick_mode {
            self.picks.clone()
        } else if let Some(target) = target {
            vec![target]
        } else {
            vec![Target::new(
                c,
                self.rev()
                    .ok_or_else(|| anyhow::anyhow!("No commit selected"))?,
            )?]
        };
        let count = picks.len();
        let (review, body) = Review::prepare_picks(c, picks)?;
        self.commit_review = Some(review);
        self.commit_dialog = None;
        let noun = if count == 1 { "commit" } else { "commits" };
        Ok(Action::Confirm {
            title: format!("Cherry-pick {count} {noun}"),
            body,
            token: REVIEW_TOKEN.into(),
        })
    }

    pub(crate) fn pick_hint(&self, c: &Context) -> Option<String> {
        if !self.pick_mode {
            return (self.focus == Pane::Graph && !self.editing())
                .then(|| "C Cherry-pick  v Select commits  F1 Help".to_owned());
        }
        let visible = self
            .visible
            .iter()
            .filter(|&&i| {
                let rev = &self.hist.revs[i];
                self.picks
                    .iter()
                    .any(|p| p.root == rev.root && p.oid == rev.oid)
            })
            .count();
        let hidden = self.picks.len().saturating_sub(visible);
        let mut hint = format!("{} selected", self.picks.len());
        if hidden > 0 {
            hint.push_str(&format!(" · {hidden} hidden"));
        }
        if let Some(repo) = self.picks.first().and_then(|p| c.repos.get(p.root)) {
            hint.push_str(&format!(" · {}", repo.name));
        }
        Some(format!("{hint}  Space Toggle  C Cherry-pick  Esc Clear"))
    }

    pub(super) fn open_commit_actions(
        &mut self,
        c: &Context,
        at: Option<(u16, u16)>,
    ) -> Result<()> {
        self.open_commit_dialog(c, at, Mode::Menu(0))
    }

    fn open_commit_dialog(
        &mut self,
        c: &Context,
        at: Option<(u16, u16)>,
        mode: Mode,
    ) -> Result<()> {
        let rev = self
            .rev()
            .ok_or_else(|| anyhow::anyhow!("No commit selected"))?;
        let target = Target::new(c, rev)?;
        self.commit_dialog = Some(Dialog::new(target, at, mode));
        self.commit_review = None;
        self.cancel_key_sequence();
        self.panes.cancel_drag();
        self.info_selection.clear();
        self.info_click = None;
        self.last = None;
        Ok(())
    }

    pub(super) fn edit_commit_message(&mut self, c: &Context) -> Result<()> {
        let rev = self
            .rev()
            .ok_or_else(|| anyhow::anyhow!("No commit selected"))?;
        let text = Target::new(c, rev)?.message(c)?;
        self.open_commit_dialog(c, None, Mode::Input(Kind::Edit, TextInput::new(text)))
    }

    pub(super) fn draw_commit_dialog(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        if let Some(dialog) = &mut self.commit_dialog {
            dialog.draw(f, a, c);
        }
    }

    pub(super) fn commit_dialog_event(&mut self, e: &Event, c: &Context) -> Action {
        self.handle_commit_dialog(e, c)
            .unwrap_or_else(|err| Action::Notice(format!("Log: {err:#}")))
    }

    fn handle_commit_dialog(&mut self, e: &Event, c: &Context) -> Result<Action> {
        let Some(dialog) = &mut self.commit_dialog else {
            return Ok(Action::None);
        };
        let (kind, input) = match dialog.event(e) {
            Input::None => return Ok(Action::None),
            Input::Close => {
                self.commit_dialog = None;
                return Ok(Action::None);
            }
            Input::SelectCommits => {
                self.commit_dialog = None;
                self.set_pick_mode(true);
                return Ok(Action::None);
            }
            Input::Choose(Kind::CherryPick) => {
                let target = dialog.target.clone();
                return self.review_picks(c, Some(target));
            }
            Input::Choose(kind @ (Kind::Branch | Kind::Tag | Kind::Edit)) => {
                let text = if kind == Kind::Edit {
                    dialog.target.message(c)?
                } else {
                    String::new()
                };
                dialog.mode = Mode::Input(kind, TextInput::new(text));
                dialog.hits.clear();
                return Ok(Action::None);
            }
            Input::Choose(kind) => (kind, String::new()),
            Input::Review(kind, input) => (kind, input),
        };
        let (review, body) = Review::prepare(c, dialog.target.clone(), kind, input)?;
        self.commit_review = Some(review);
        self.commit_dialog = None;
        Ok(Action::Confirm {
            title: kind.title().into(),
            body,
            token: REVIEW_TOKEN.into(),
        })
    }

    pub(super) fn confirm_commit_action(&mut self, token: &str, c: &Context) -> Action {
        if token != REVIEW_TOKEN {
            return Action::None;
        }
        let Some(review) = self.commit_review.take() else {
            return Action::None;
        };
        let is_pick = review.is_cherry_pick();
        let message = match review.execute(c) {
            Ok(message) => {
                if is_pick {
                    self.set_pick_mode(false);
                }
                message
            }
            Err(error) => format!("Log: {error:#}"),
        };
        // Git can leave a sequencer or conflicted index even when it reports failure.
        Action::Refresh(message)
    }
}
