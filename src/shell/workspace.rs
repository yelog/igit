use super::*;
use crate::ui::idea::CONFLICT;
use crossterm::event::MouseEvent;
use ratatui::{
    layout::{Constraint, Layout},
    style::Stylize,
};

#[derive(Default)]
pub(super) struct Regions {
    pub(super) body: Rect,
    pub(super) log_tabs: Rect,
    pub(super) software_update: Rect,
    tabs: [Rect; Tab::DISPLAY_ORDER.len()],
}

impl Regions {
    pub(super) fn tab(&self, tab: Tab) -> Rect {
        self.tabs[tab as usize]
    }
}

fn header_button(f: &mut Frame, row: &mut Rect, label: &str, style: Style) -> Rect {
    let width = (label.width() as u16).min(row.width);
    row.width -= width;
    let r = Rect::new(row.right(), row.y, width, 1);
    f.render_widget(Paragraph::new(label).style(style), r);
    r
}

pub(super) fn footer_identity() -> [Span<'static>; 2] {
    [
        Span::styled(" IGit ", button_style(true)),
        Span::styled(
            format!(" v{}", env!("CARGO_PKG_VERSION")),
            Style::new().fg(MUTED),
        ),
    ]
}

impl Shell {
    pub(super) fn workspace_mouse(&mut self, m: &MouseEvent) {
        let (x, y) = (m.column, m.row);
        let press = m.kind == MouseEventKind::Down(MouseButton::Left);
        if press && self.key_sequence().is_some() {
            self.cancel_key_sequences();
        }
        if self.resize_mouse(m) {
            return;
        }
        if press {
            if inside(self.regions.software_update, x, y) {
                self.open_software_update();
                return;
            }
            if let Some(tab) = Tab::DISPLAY_ORDER
                .into_iter()
                .find(|&tab| inside(self.regions.tab(tab), x, y))
            {
                self.go(tab);
                return;
            }
        }
        // Keep routing drags and releases to the tab that received the press
        // even after the pointer leaves the pane or crosses the header.
        if inside(self.regions.body, x, y)
            || inside(self.regions.log_tabs, x, y)
            || matches!(m.kind, MouseEventKind::Up(_) | MouseEventKind::Drag(_))
        {
            self.panel_event(&Event::Mouse(*m));
        }
    }

    pub(super) fn hover(&mut self, x: u16, y: u16) -> bool {
        if self.pop.is_some() || self.key_picker.contains(x, y) || self.notifications.contains(x, y)
        {
            return false;
        }
        match self.tab {
            Commit => self.commit.hover_at(x, y),
            Log if self.log.editing() => self.log.hover_menu_at(x, y),
            Branch => self.branch.hover_comparison(x, y),
            Push => self.push.hover_group_at(x, y),
            _ => false,
        }
    }
}

impl Regions {
    pub(super) fn draw_header(
        &mut self,
        f: &mut Frame,
        mut row: Rect,
        selected: Tab,
        icons: IconMode,
        conflicts: bool,
        enabled: bool,
    ) -> Rect {
        let tab_width = Tab::DISPLAY_ORDER
            .into_iter()
            .filter(|&tab| tab != Conflicts || conflicts)
            .map(|t| t.name().width() + 2)
            .sum::<usize>();
        let context_width = match selected {
            Log => 15,
            Push => 14,
            _ => 0,
        };
        let show_icons = row.width >= 72
            && icons != IconMode::Ascii
            && tab_width
                + Tab::DISPLAY_ORDER
                    .into_iter()
                    .filter(|&tab| tab != Conflicts || conflicts)
                    .map(|_| 3)
                    .sum::<usize>()
                + context_width
                <= usize::from(row.width);
        let compact = tab_width + context_width > usize::from(row.width);
        let abbreviate = Tab::DISPLAY_ORDER
            .into_iter()
            .filter(|&tab| tab != Conflicts || conflicts)
            .map(|tab| tab.name().width() + 1)
            .sum::<usize>()
            + context_width
            > usize::from(row.width);
        f.render_widget(Block::default().style(Style::new().bg(TAG)), row);
        for tab in Tab::DISPLAY_ORDER.into_iter().rev() {
            if tab == Conflicts && !conflicts {
                continue;
            }
            let active = selected == tab;
            let mut style = if active {
                button_style(true)
            } else {
                Style::new().fg(MUTED)
            };
            if tab == Conflicts {
                style = style.fg(CONFLICT).bold();
            }
            if !enabled {
                style = style.fg(MUTED);
            }
            let label = if show_icons {
                let icon = match tab {
                    Commit => Icon::Commit,
                    Log => Icon::Log,
                    Push => Icon::Push,
                    Conflicts => Icon::Conflict,
                    Branch => Icon::Branch,
                    Stash => Icon::Folder,
                    Remote => Icon::Remote,
                };
                format!(" {} {} ", icons.glyph(icon), tab.name())
            } else if compact {
                let name = if abbreviate {
                    match tab {
                        Commit => "Cmt",
                        Branch => "Br",
                        Remote => "Rem",
                        _ => tab.name(),
                    }
                } else {
                    tab.name()
                };
                format!(" {name}")
            } else {
                format!(" {} ", tab.name())
            };
            self.tabs[tab as usize] = header_button(f, &mut row, &label, style);
        }
        row
    }
}

impl Shell {
    pub(super) fn draw(&mut self, f: &mut Frame) {
        let a = f.area();
        self.regions = Regions::default();
        self.notifications.clear_hits();
        if let Some(popup) = &mut self.pop {
            popup.clear_notification_hits();
        }
        self.pop_hits.clear();
        self.key_picker.clear_geometry();
        f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), a);
        if a.width < 44 || a.height < 12 {
            self.cancel_drags();
            self.layout_mut(self.tab).begin();
            f.render_widget(
                Paragraph::new(format!(
                    "IGit · Terminal too small\nMinimum 44 × 12; recommended 120 × 36.\n{} Quit.",
                    self.config.label("global", "quit")
                ))
                .wrap(Wrap { trim: false }),
                a,
            );
            return;
        }
        if self.tab == Conflicts && self.merge.has_document() {
            let editor = Rect::new(a.x, a.y, a.width, a.height - 1);
            self.regions.body = editor;
            self.merge.panes.active = self.pop.is_none();
            self.merge.panes.maximized = self.pane_is_maximized();
            let notice = if self.maximize_pending {
                "Synchronizing Kitty layout…".to_owned()
            } else {
                String::new()
            };
            self.merge.draw_editor(f, editor, &self.ctx, &notice);
            let hint = format!("{} Updates  F1 Help", self.config.label("global", "update"));
            self.draw_footer(f, Rect::new(a.x, a.bottom() - 1, a.width, 1), &hint, MUTED);
            let hints = Rect::new(
                a.x,
                a.y,
                a.width,
                editor.height - 1 - u16::from(!notice.is_empty()),
            );
            self.draw_workspace_overlays(f, hints);
            return;
        }
        let [header, work, foot] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(a);
        let conflicts = self.tab_available(Conflicts);
        let context = self
            .regions
            .draw_header(f, header, self.tab, self.icons, conflicts, true);
        let maximized = self.pane_is_maximized();
        if self.ctx.repos.is_empty() {
            let empty = format!("No Git working trees found.\n\nUse {} Actions → Open / rescan workspace, or start igit /path/to/workspace.", self.config.label("global", "actions"));
            f.render_widget(
                Paragraph::new(empty)
                    .wrap(Wrap { trim: true })
                    .block(pane("Workspace", true)),
                work,
            );
        } else {
            self.regions.body = work;
            let active = self.pop.is_none();
            let layout = self.layout_mut(self.tab);
            layout.active = active;
            layout.maximized = maximized;
            match self.tab {
                Commit => {
                    self.commit.draw_content(f, work, &self.ctx);
                    self.commit.draw_overlays(f, work, &self.ctx, active);
                }
                Log => {
                    if self.log_load.ready() {
                        self.log.draw(f, work, &self.ctx);
                        self.log.draw_tabs(f, context, &self.ctx);
                        self.regions.log_tabs = context;
                    } else {
                        f.render_widget(
                            Paragraph::new("Loading history...").block(pane("Log", true)),
                            work,
                        );
                    }
                }
                Push => {
                    f.render_widget(
                        Paragraph::new(format!(" {}", self.push.window_title(&self.ctx)))
                            .style(Style::new().fg(FG).bold()),
                        context,
                    );
                    self.push.draw(f, work, &self.ctx);
                }
                Conflicts => self.merge.draw(f, work, &self.ctx),
                Branch => self.branch.draw(f, work, &self.ctx),
                Stash => self.stash.draw(f, work, &self.ctx),
                Remote => self.remote.draw(f, work, &self.ctx),
            }
        }

        let pick_hint = (self.tab == Log)
            .then(|| {
                self.log.pick_hint(
                    &self.ctx,
                    &self.config.label("log", "show-file-at-revision"),
                    &self.config.label("log", "compare-with-local"),
                )
            })
            .flatten();
        let (hint, color) = if self.maximize_pending {
            ("Synchronizing Kitty layout…".into(), ACCENT)
        } else if self.commit.add_running() {
            (
                self.commit
                    .add_progress(usize::from(foot.width.saturating_sub(18))),
                ACCENT,
            )
        } else if self.pop.is_some() {
            ("F1 Help   Esc Back".into(), MUTED)
        } else if self.key_sequence() == Some(KeySequence::LogFilters) {
            (
                if foot.width < 80 {
                    "b/u/d/p Filters  Esc"
                } else {
                    "Filters: b/u/d/p open  Arrows select  Enter open  Esc cancel"
                }
                .into(),
                ACCENT,
            )
        } else if let Some(hint) = self.notification_prefix_hint() {
            (hint, ACCENT)
        } else if self.panel().editing() {
            (
                if matches!(self.tab, Push | Conflicts | Branch)
                    || self.tab == Commit && self.commit.message_editing()
                {
                    self.panel().help()
                } else {
                    "F1 Help   Esc Back"
                }
                .into(),
                MUTED,
            )
        } else if self.ctx.repos.is_empty() {
            (
                format!(
                    "{} Actions   ·   Open / rescan workspace",
                    self.config.label("global", "actions")
                ),
                MUTED,
            )
        } else if let Some(hint) = pick_hint {
            (hint, MUTED)
        } else if self.config.changed()
            || self.tab == Log && self.log.comparing_workspace()
            || self.tab == Commit && self.commit.file_comparison_focused()
        {
            (self.configured_footer(), MUTED)
        } else if matches!(self.tab, Push | Conflicts | Branch | Stash | Remote) {
            (self.panel().help().into(), MUTED)
        } else {
            (
                if foot.width < 60 {
                    "g n Notices  F1 Help".into()
                } else if foot.width < 100 {
                    "Tab Panes  p Pull  f Fetch  F1 Help".to_string()
                } else {
                    "Tab Panes  Ctrl-W Pane keys  p Pull  f Fetch  g n Notices  F9 Updates  F1 Help"
                        .into()
                },
                MUTED,
            )
        };
        self.draw_footer(f, foot, &hint, color);
        self.draw_workspace_overlays(f, work);
    }

    fn draw_footer(&mut self, f: &mut Frame, foot: Rect, hint: &str, color: ratatui::style::Color) {
        let [brand, version] = footer_identity();
        let (status, status_style) = self.software_update.footer_status(self.icons);
        let start = foot.x + brand.width() as u16;
        let visible_width = foot.width.saturating_sub(brand.width() as u16);
        self.regions.software_update = Rect::new(
            start,
            foot.y,
            ((version.width() + status.width()) as u16).min(visible_width),
            1,
        );
        let search = self.focused_search().filter(|prompt| {
            self.pop.is_none()
                && self.key_sequence() != Some(KeySequence::LogFilters)
                && prompt.visible()
                && (prompt.editing() || !self.commit.add_running())
        });
        let hint = if search.is_some() {
            String::new()
        } else {
            format!("  {hint}")
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                brand,
                version,
                Span::styled(status, status_style),
                Span::styled(hint, Style::new().fg(color)),
            ]))
            .style(Style::new().bg(TAG)),
            foot,
        );
        if let Some(search) = search {
            let x = self.regions.software_update.right().min(foot.right());
            search.draw(f, Rect::new(x, foot.y, foot.right() - x, 1));
        }
    }

    fn draw_workspace_overlays(&mut self, f: &mut Frame, work: Rect) {
        self.draw_popup(f, f.area());
        if let Some(pending) = &self.pending_keys {
            self.key_picker
                .draw_items(f, work, &pending.prefix_label(), &pending.hints());
        } else if let Some(sequence) = self.key_sequence() {
            if let Some(targets) = (sequence == KeySequence::LogFilters)
                .then(|| self.log.filter_shortcut_targets())
                .flatten()
            {
                self.key_picker.draw_inline(f, &targets);
            } else {
                self.key_picker.draw(f, work, sequence);
            }
        }
        if !matches!(self.pop, Some(Popup::Notifications(_))) {
            let mut viewport = f.area();
            viewport.height = viewport.height.saturating_sub(1);
            self.notifications.draw_live(f, viewport, self.icons);
        }
    }
}
