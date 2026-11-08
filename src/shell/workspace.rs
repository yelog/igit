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
        if self.pop.is_some() || self.key_picker.contains(x, y) {
            return false;
        }
        match self.tab {
            Commit => self.commit.hover_at(x, y),
            Log if self.log.editing() => self.log.hover_menu_at(x, y),
            Push => self.push.hover_group_at(x, y),
            _ => false,
        }
    }

    fn draw_header(&mut self, f: &mut Frame, mut row: Rect) -> Rect {
        let show_icons = row.width >= 72 && self.icons != IconMode::Ascii;
        let tab_width = Tab::DISPLAY_ORDER
            .into_iter()
            .filter(|&t| self.tab_available(t))
            .map(|t| t.name().width() + 2)
            .sum::<usize>();
        let context_width = match self.tab {
            Log => 11,
            Push => 14,
            _ => 0,
        };
        let compact = tab_width + context_width > usize::from(row.width);
        f.render_widget(Block::default().style(Style::new().bg(TAG)), row);
        for tab in Tab::DISPLAY_ORDER.into_iter().rev() {
            if !self.tab_available(tab) {
                continue;
            }
            let active = self.tab == tab;
            let mut style = if active {
                button_style(true)
            } else {
                Style::new().fg(MUTED)
            };
            if tab == Conflicts {
                style = style.fg(CONFLICT).bold();
            }
            let label = if show_icons {
                let icon = match tab {
                    Commit => Icon::Commit,
                    Log => Icon::Log,
                    Push => Icon::Push,
                    Conflicts => Icon::Conflict,
                    Branch => Icon::Branch,
                    Stash => Icon::Folder,
                };
                format!(" {} {} ", self.icons.glyph(icon), tab.name())
            } else if compact {
                format!(" {}", tab.name())
            } else {
                format!(" {} ", tab.name())
            };
            self.regions.tabs[tab as usize] = header_button(f, &mut row, &label, style);
        }
        row
    }

    pub(super) fn draw(&mut self, f: &mut Frame) {
        let a = f.area();
        self.regions = Regions::default();
        self.pop_hits.clear();
        self.key_picker.clear_geometry();
        f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), a);
        if a.width < 44 || a.height < 12 {
            self.cancel_drags();
            self.layout_mut(self.tab).begin();
            f.render_widget(
                Paragraph::new(format!(
                    "IGit · Terminal too small\nMinimum 44 × 12; recommended 120 × 36.\n{} Quit, y Confirm.",
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
                self.msg.clone()
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
        let context = self.draw_header(f, header);
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
            }
        }

        let (hint, color) = if self.maximize_pending {
            ("Synchronizing Kitty layout…".into(), ACCENT)
        } else if self.commit.add_running() {
            (
                self.commit
                    .add_progress(usize::from(foot.width.saturating_sub(18))),
                ACCENT,
            )
        } else if !self.msg.is_empty() {
            (self.msg.clone(), ACCENT)
        } else if self.pop.is_some() {
            ("F1 Help   Esc Back".into(), MUTED)
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
        } else if self.config.changed() {
            (self.configured_footer(), MUTED)
        } else if matches!(self.tab, Push | Conflicts | Branch | Stash) {
            (self.panel().help().into(), MUTED)
        } else {
            (
                if foot.width < 60 {
                    "F9 Updates  F1 Help"
                } else if foot.width < 100 {
                    "Tab Panes  p Pull  f Fetch  F1 Help"
                } else {
                    "Tab Panes  Ctrl-W Pane keys  p Pull  f Fetch  F9 Updates  F1 Help"
                }
                .into(),
                MUTED,
            )
        };
        self.draw_footer(f, foot, &hint, color);
        self.draw_workspace_overlays(f, work);
    }

    fn draw_footer(&mut self, f: &mut Frame, foot: Rect, hint: &str, color: ratatui::style::Color) {
        let brand = " IGit ";
        let version = format!(" v{}", env!("CARGO_PKG_VERSION"));
        let (status, status_style) = self.software_update.footer_status(self.icons);
        let start = foot.x + brand.width() as u16;
        let visible_width = foot.width.saturating_sub(brand.width() as u16);
        self.regions.software_update = Rect::new(
            start,
            foot.y,
            ((version.width() + status.width()) as u16).min(visible_width),
            1,
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(brand, button_style(true)),
                Span::styled(version, Style::new().fg(MUTED)),
                Span::styled(status, status_style),
                Span::styled(format!("  {hint}"), Style::new().fg(color)),
            ]))
            .style(Style::new().bg(TAG)),
            foot,
        );
    }

    fn draw_workspace_overlays(&mut self, f: &mut Frame, work: Rect) {
        if let Some(pending) = &self.pending_keys {
            self.key_picker
                .draw_items(f, work, &pending.prefix_label(), &pending.hints());
        } else if let Some(sequence) = self.key_sequence() {
            self.key_picker.draw(f, work, sequence);
        }
        self.draw_popup(f, f.area());
    }
}
