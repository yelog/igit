//! The Group By menu only rearranges the current outgoing preview.
use super::*;
use crate::ui::idea::TAG;
use ratatui::widgets::{Block, BorderType, Borders};
use unicode_width::UnicodeWidthStr;

impl PushPanel {
    pub(crate) fn dismiss_group_menu(&mut self) {
        self.group_menu = None;
    }

    pub(super) fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, h)| *h)
    }

    fn toggle_group(&mut self, group: Group, c: &Context) {
        self.group_menu = Some(group);
        let tree = &mut self.preview.tree;
        let selected = tree.visible.get(tree.cursor).copied();
        self.grouping.toggle(group);
        tree.regroup(&self.preview.files, self.grouping);
        // A retained file keeps its already loaded patch and scroll position.
        if self.show_details && selected != tree.visible.get(tree.cursor).copied() {
            self.inspect_file(c);
        }
    }

    pub(super) fn group_menu_event(&mut self, event: &Event, c: &Context) -> bool {
        let Some(current) = self.group_menu else {
            return false;
        };
        let step = |delta: isize| {
            Group::ALL[(current as isize + delta).rem_euclid(Group::ALL.len() as isize) as usize]
        };
        match event {
            Event::Key(k) => match k.code {
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                    self.group_menu = Some(step(1))
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                    self.group_menu = Some(step(-1))
                }
                KeyCode::Home | KeyCode::Char('g') => self.group_menu = Some(Group::Directory),
                KeyCode::End | KeyCode::Char('G') => self.group_menu = Some(Group::Module),
                KeyCode::Char(' ') | KeyCode::Enter => self.toggle_group(current, c),
                KeyCode::Esc | KeyCode::Char('o' | 'h') | KeyCode::Left => {
                    self.dismiss_group_menu()
                }
                _ => (),
            },
            Event::Mouse(m) => match (m.kind, self.hit_at(m.column, m.row)) {
                (MouseEventKind::Down(MouseButton::Left), Some(Hit::Group(item))) => {
                    self.toggle_group(item, c)
                }
                (MouseEventKind::Moved, Some(Hit::Group(item))) => self.group_menu = Some(item),
                (MouseEventKind::ScrollDown, Some(Hit::Group(_) | Hit::MenuSurface)) => {
                    self.group_menu = Some(step(1))
                }
                (MouseEventKind::ScrollUp, Some(Hit::Group(_) | Hit::MenuSurface)) => {
                    self.group_menu = Some(step(-1))
                }
                (MouseEventKind::Down(_), Some(Hit::MenuSurface | Hit::Group(_))) => (),
                (MouseEventKind::Down(_), Some(Hit::GroupBy)) => self.dismiss_group_menu(),
                (MouseEventKind::Down(_), _) => {
                    self.dismiss_group_menu();
                    return false; // Handle an outside click in its own pane.
                }
                _ => (),
            },
            Event::FocusLost => self.dismiss_group_menu(),
            _ => (),
        }
        true
    }

    pub(super) fn draw_group_menu(&mut self, f: &mut Frame, area: Rect) {
        let Some(current) = self.group_menu else {
            return;
        };
        if area.height < 3 || area.width < 2 {
            return;
        }
        let anchor = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::GroupBy).then_some(*r))
            .unwrap_or(self.zones[1]);
        let width = (self.icons.check(true).width() + "Repository".width() + 6)
            .min(area.width.into()) as u16;
        let preferred = Group::ALL.len() as u16 + 3;
        let below = area.bottom().saturating_sub(anchor.bottom());
        let height = if below >= preferred - 1 {
            preferred.min(below) // At minimum height, put the heading in the border.
        } else {
            preferred.min(area.height)
        };
        let popup = Rect::new(
            anchor.x.clamp(area.x, area.right() - width),
            anchor.bottom().clamp(area.y, area.bottom() - height),
            width,
            height,
        );
        f.render_widget(Clear, popup);
        self.hits.push((popup, Hit::MenuSurface));
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(LINE))
            .style(Style::new().fg(FG).bg(TAG));
        let heading = height == preferred;
        if !heading {
            block = block.title(" Group By ");
        }
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        if heading {
            f.render_widget(
                Paragraph::new(" Group By")
                    .style(Style::new().fg(MUTED).add_modifier(Modifier::BOLD)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
        }
        let top = inner.y + u16::from(heading);
        let slots = (inner.bottom() - top) as usize;
        let start = (current as usize + 1).saturating_sub(slots);
        let monochrome = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        for (i, &group) in Group::ALL.iter().skip(start).take(slots).enumerate() {
            let r = Rect::new(inner.x, top + i as u16, inner.width, 1);
            let text = format!(
                "{}{} {}",
                if monochrome && group == current {
                    ">"
                } else {
                    " "
                },
                self.icons.check(self.grouping.enabled(group)),
                group.label()
            );
            let bg = if group == current { SEL } else { TAG };
            f.render_widget(Paragraph::new(text).style(Style::new().fg(FG).bg(bg)), r);
            self.hits.push((r, Hit::Group(group)));
        }
    }
}
