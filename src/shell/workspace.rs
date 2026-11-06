use super::*;
use crate::ui::panes::{Axis, SplitId};
use crossterm::event::MouseEvent;
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::Stylize;

#[derive(Default)]
pub(super) struct Regions {
    pub(super) commit: Rect,
    pub(super) log: Rect,
    tool_body: Rect,
    pub(super) close: Rect,
    tool_help: Rect,
}

impl Regions {
    fn at(&self, x: u16, y: u16) -> Option<WorkspacePane> {
        [(self.commit, Commit), (self.log, Log)]
            .into_iter()
            .find_map(|(r, pane)| inside(r, x, y).then_some(pane))
    }
}

impl Shell {
    fn overlay_at(&self, x: u16, y: u16) -> bool {
        match self.focus {
            Log => self.log.overlay_at(x, y),
            Commit => self.commit.overlay_at(x, y),
        }
    }

    pub(super) fn tool_at_root(&self) -> bool {
        match self.tool {
            Some(Push) => self.push.at_root(),
            Some(Merge) => self.merge.at_root(),
            None => false,
        }
    }

    pub(super) fn workspace_mouse(&mut self, m: &MouseEvent) {
        let (x, y) = (m.column, m.row);
        let press = m.kind == MouseEventKind::Down(MouseButton::Left);
        let wheel = matches!(
            m.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        );
        let release_or_drag = matches!(m.kind, MouseEventKind::Up(_) | MouseEventKind::Drag(_));
        let ev = Event::Mouse(*m);
        if press && self.key_sequence().is_some() {
            self.cancel_key_sequences();
        }
        if self.resize_mouse(m) {
            return;
        }
        if let Some(tool) = self.tool {
            if press && inside(self.regions.close, x, y) {
                if self.can_leave_panel() {
                    self.go(Split(self.focus));
                } else {
                    self.notice("Save or cancel the current edit first (Esc)".into());
                }
            } else if press && inside(self.regions.tool_help, x, y) {
                self.open_help();
            } else if inside(self.regions.tool_body, x, y) || release_or_drag {
                self.panel_event(Dialog(tool), &ev);
            }
            return;
        }
        if press {
            if let Some(&(_, n)) = self.tools.iter().find(|(r, _)| inside(*r, x, y)) {
                if n == 1 {
                    self.open_help();
                } else if self.can_leave_panel() {
                    self.global(n);
                }
                return;
            }
        }
        // Editors and menus may extend beyond their half. A drag/release still
        // belongs to the pane that received the press, even across the divider.
        if self.overlay_at(x, y) || release_or_drag {
            self.panel_event(Split(self.focus), &ev);
            return;
        }
        let Some(pane) = self.regions.at(x, y) else {
            return;
        };
        if pane != self.focus {
            if press {
                if !self.can_leave_panel() {
                    self.notice("Finish or cancel the current input first (Esc)".into());
                    return;
                }
                self.go(Split(pane));
            } else if wheel && !self.panel().editing() {
                self.cancel_key_sequences();
            } else {
                return;
            }
        }
        self.panel_event(Split(pane), &ev);
    }

    pub(super) fn hover(&mut self, x: u16, y: u16) -> bool {
        if self.pop.is_some() || self.key_picker.contains(x, y) {
            return false;
        }
        match self.surface() {
            Dialog(Push) => self.push.hover_group_at(x, y),
            Dialog(Merge) => false,
            Split(Log) if self.log.editing() => self.log.hover_menu_at(x, y),
            // Commit's toolbar can be hovered without taking keyboard focus.
            Split(_) => self.commit.hover_at(x, y),
        }
    }

    fn draw_header(&mut self, f: &mut Frame, mut row: Rect) {
        let roots = format!(
            " {} Roots {}/{} ",
            self.icons.glyph(Icon::Repo),
            self.ctx.scope.len(),
            self.ctx.repos.len()
        );
        let menu = if row.width >= 64 {
            format!(" {} Actions ", self.icons.glyph(Icon::More))
        } else {
            format!(" {} ", self.icons.glyph(Icon::More))
        };
        let help = format!(" {} ", self.icons.glyph(Icon::Help));
        let console = format!(" {} Console ", self.icons.glyph(Icon::Console));
        for (label, n) in [(&help, 1), (&menu, 2), (&roots, 0), (&console, 4)] {
            if n == 4 && row.width < 70 {
                break;
            }
            let w = (label.width() as u16).min(row.width);
            row.width -= w;
            let r = Rect::new(row.right(), row.y, w, 1);
            f.render_widget(
                Paragraph::new(label.as_str()).style(Style::new().fg(MUTED)),
                r,
            );
            self.tools.push((r, n));
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" IGit  ", Style::new().fg(FG).bold()),
                Span::styled(
                    self.ctx
                        .workspace
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    Style::new().fg(MUTED),
                ),
            ])),
            row,
        );
    }

    fn pane_header(&self, f: &mut Frame, row: Rect, pane: WorkspacePane) -> Rect {
        let active = self.surface() == Split(pane) && self.pop.is_none();
        let (icon, name, action) = match pane {
            Commit => (Icon::Commit, "Commit", "focus-commit"),
            Log => (Icon::Log, "Git Log", "focus-log"),
        };
        let label = format!(
            "{} {} {name} [{}] ",
            if active { ">" } else { " " },
            self.icons.glyph(icon),
            self.config.label("global", action)
        );
        let [title, rest] = Layout::horizontal([
            Constraint::Length(label.width() as u16),
            Constraint::Fill(1),
        ])
        .areas(row);
        f.render_widget(Block::default().style(Style::new().bg(TAG)), row);
        f.render_widget(
            Paragraph::new(label).style(Style::new().bold().fg(if active {
                ACCENT
            } else {
                MUTED
            })),
            title,
        );
        rest
    }

    fn draw_split(&mut self, f: &mut Frame, work: Rect) {
        let [commit, log] = if self.pane_maximized {
            match self.focus {
                Commit => [work, Rect::default()],
                Log => [Rect::default(), work],
            }
        } else {
            self.work_panes.divide(
                SplitId::Workspace,
                work,
                Axis::Height,
                work.height.div_ceil(2),
                [6, 5],
            )
        };
        self.regions.commit = commit;
        self.regions.log = log;
        let pane = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]);
        // Give the workspace divider its own cells; it must never steal a
        // press on the commit buttons or on the Log tab strip.
        let mut above = commit;
        if !self.pane_maximized && work.height > 10 {
            above.height = above.height.saturating_sub(1);
            let boundary = Rect::new(commit.x, above.bottom(), commit.width, 1);
            self.work_panes.handle(SplitId::Workspace, boundary);
            f.render_widget(
                Block::default()
                    .borders(ratatui::widgets::Borders::TOP)
                    .border_style(Style::new().fg(LINE)),
                boundary,
            );
        }
        let [commit_header, changes] = pane.areas(above);
        let [log_header, history] = pane.areas(log);
        self.pane_header(f, commit_header, Commit);
        let tabs = self.pane_header(f, log_header, Log);
        self.commit.panes.active = self.surface() == Split(Commit) && self.pop.is_none();
        self.log.panes.active = self.surface() == Split(Log) && self.pop.is_none();
        self.commit.draw_content(f, changes, &self.ctx);
        self.log.draw_content(f, history, &self.ctx);
        if !tabs.is_empty() {
            self.log.draw_tabs(f, tabs, &self.ctx);
        }
        match self.focus {
            Commit => self.commit.draw_overlays(
                f,
                work,
                &self.ctx,
                self.pop.is_none() && self.tool.is_none(),
            ),
            Log => self.log.draw_overlays(f, work, &self.ctx),
        }
    }

    fn draw_tool(&mut self, f: &mut Frame, work: Rect, tool: ToolWindow) {
        let window = if self.pane_maximized {
            work
        } else {
            work.inner(Margin::new(
                (work.width / 24).clamp(1, 6),
                (work.height / 16).min(3),
            ))
        };
        let (name, icon) = match tool {
            Push => (self.push.window_title(&self.ctx), Icon::Push),
            Merge => ("Merge".into(), Icon::Merge),
        };
        // Only the receiving pane (or its editor) owns the accent border.
        let close = format!(" Esc {} ", self.icons.glyph(Icon::Close));
        let help = format!(" {} ", self.icons.glyph(Icon::Help));
        let title = crate::ui::idea::clipped(
            &format!("{} {name}", self.icons.glyph(icon)),
            usize::from(window.width).saturating_sub(close.width() + help.width() + 4),
        );
        let frame = pane(title, false);
        let inner = frame.inner(window);
        f.render_widget(Clear, window);
        f.render_widget(frame, window);
        self.regions.tool_body = inner;
        let mut right = inner.right();
        for (label, r) in [
            (close, &mut self.regions.close),
            (help, &mut self.regions.tool_help),
        ] {
            let w = label.width() as u16;
            right -= w;
            *r = Rect::new(right, window.y, w, 1);
            f.render_widget(
                Paragraph::new(label).style(Style::new().fg(ACCENT).bg(TAG)),
                *r,
            );
        }
        self.push.panes.active = tool == Push && self.pop.is_none();
        self.merge.panes.active = tool == Merge && self.pop.is_none();
        match tool {
            Push => self.push.draw(f, inner, &self.ctx),
            Merge => self.merge.draw(f, inner, &self.ctx),
        }
    }

    pub(super) fn draw(&mut self, f: &mut Frame) {
        let a = f.area();
        self.regions = Regions::default();
        self.work_panes.begin();
        self.commit.panes.maximized = self.pane_maximized;
        self.log.panes.maximized = self.pane_maximized;
        self.push.panes.maximized = self.pane_maximized;
        self.merge.panes.maximized = self.pane_maximized;
        self.tools.clear();
        self.pop_hits.clear();
        self.key_picker.clear_geometry();
        f.render_widget(Block::default().style(Style::new().bg(BG).fg(FG)), a);
        if a.width < 44 || a.height < 12 {
            self.cancel_drags();
            self.commit.panes.begin();
            self.log.panes.begin();
            self.push.panes.begin();
            self.merge.panes.begin();
            f.render_widget(
                Paragraph::new(format!(
                    "IGit · Terminal too small\nMinimum 44 × 12; recommended 120 × 36.\n{} Quit, y Confirm.", self.config.label("global", "quit")
                ))
                .wrap(Wrap { trim: false }),
                a,
            );
            return;
        }
        let [header, work, foot] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(a);
        self.draw_header(f, header);
        if self.ctx.repos.is_empty() {
            let empty = format!("No Git working trees found.\n\nUse {} Actions → Open / rescan workspace, or start igit /path/to/workspace.", self.config.label("global", "actions"));
            f.render_widget(
                Paragraph::new(empty)
                    .wrap(Wrap { trim: true })
                    .block(pane("Workspace", true)),
                work,
            );
        } else if !(self.pane_maximized && self.tool.is_some()) {
            self.draw_split(f, work);
        }
        if let Some(tool) = self.tool {
            f.buffer_mut()
                .set_style(header.union(work), Style::new().fg(MUTED).bg(BG).dim());
            self.draw_tool(f, work, tool);
        }
        let mode = if self.pop.is_some() {
            "DIALOG"
        } else if self.panel().editing() {
            "INPUT"
        } else if self.pane_maximized {
            "MAX"
        } else {
            "NORMAL"
        };
        let configured_hint = self.configured_footer();
        let (hint, color) = if self.maximize_pending {
            ("Synchronizing Kitty layout…", ACCENT)
        } else if !self.msg.is_empty() {
            (self.msg.as_str(), ACCENT)
        } else if self.config.changed() && self.pop.is_some() {
            ("F1 Help   Esc Back", MUTED)
        } else if self.config.changed() && self.pop.is_none() && !self.panel().editing() {
            (configured_hint.as_str(), MUTED)
        } else if self.tool.is_some() {
            (self.panel().help(), MUTED)
        } else if self.panel().editing() {
            ("F1 Help   Esc Back", MUTED)
        } else if self.ctx.repos.is_empty() {
            (": Actions   ·   Open / rescan workspace", MUTED)
        } else if foot.width < 84 {
            ("Tab/Ctrl-W Panes  3 Push 4 Merge ?", MUTED)
        } else {
            (
                "Tab Panes   Ctrl-W Pane keys   3 Push   4 Merge   ·   : Actions   ? Help",
                MUTED,
            )
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {mode} "), button_style(true)),
                Span::styled(format!("  {hint}"), Style::new().fg(color)),
            ]))
            .style(Style::new().bg(TAG)),
            foot,
        );
        if let Some(pending) = &self.pending_keys {
            let bounds = if self.tool.is_some() {
                self.regions.tool_body
            } else {
                work
            };
            self.key_picker
                .draw_items(f, bounds, &pending.prefix_label(), &pending.hints());
        } else if let Some(sequence) = self.key_sequence() {
            let bounds = if self.tool.is_some() {
                self.regions.tool_body
            } else if matches!(sequence, KeySequence::Window(_)) {
                work
            } else {
                self.regions.log
            };
            self.key_picker.draw(f, bounds, sequence);
        }
        self.draw_popup(f, a);
    }
}
