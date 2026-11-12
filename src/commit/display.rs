//! Session-local Commit presentation; none of these options changes the index.
use super::*;
use crate::ui::idea::{checkbox, clipped, DIM, EDGE, INK, SELECT};
use ratatui::{style::Stylize, text::Span, widgets::BorderType};
use unicode_width::UnicodeWidthStr;

pub(super) struct Options {
    pub directories: bool,
    pub repositories: bool,
    pub modules: bool,
    pub ignored: bool,
    pub status: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            directories: true,
            repositories: true,
            modules: false,
            ignored: false,
            status: false,
        }
    }
}

impl Options {
    pub fn repository_grouping(&self) -> bool {
        self.repositories || self.modules
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    Directory,
    Repository,
    Module,
    Ignored,
    Status,
}

impl Item {
    const ALL: [Self; 5] = [
        Self::Directory,
        Self::Repository,
        Self::Module,
        Self::Ignored,
        Self::Status,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Directory => "Directory",
            Self::Repository => "Repository",
            Self::Module => "Module",
            Self::Ignored => "Ignored Files",
            Self::Status => "Git Status",
        }
    }

    fn position(self) -> usize {
        [1, 2, 3, 6, 7][self as usize]
    }

    fn checked(self, options: &Options) -> bool {
        match self {
            Self::Directory => options.directories,
            Self::Repository => options.repositories,
            Self::Module => options.modules,
            Self::Ignored => options.ignored,
            Self::Status => options.status,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Menu {
    pub row: usize,
    pub area: Rect,
}

impl CommitPanel {
    pub(crate) fn dismiss_view_menu(&mut self) {
        self.view_menu = None;
        self.hover = None;
    }

    pub(super) fn toggle_view_menu(&mut self) {
        self.last_click = None;
        self.dragged = None;
        self.hover = None;
        self.view_menu = if self.view_menu.is_some() {
            None
        } else {
            Some(Menu::default())
        };
    }

    pub(super) fn toggle_view_item(&mut self, item: Item, c: &Context) -> Result<()> {
        if let Some(menu) = &mut self.view_menu {
            menu.row = item as usize;
        }
        match item {
            Item::Directory => self.set_directory_grouping(!self.options.directories, c),
            Item::Repository => self.set_repository_grouping(!self.options.repositories, c),
            Item::Module => self.set_module_grouping(!self.options.modules, c),
            Item::Status => self.options.status = !self.options.status,
            Item::Ignored => {
                if !self.options.ignored {
                    // Fetch every root before publishing the option, so a failed
                    // read leaves the current view and selection intact.
                    let mut ignored = vec![];
                    for (root, repo) in c.repos.iter().enumerate() {
                        for change in backend::ignored(&repo.path)? {
                            ignored.push(File {
                                key: format!("{}:{}", rid(c, root), keypart(&change.path)),
                                root,
                                change,
                                group: IGNORED.into(),
                            });
                        }
                    }
                    self.files.extend(ignored);
                } else {
                    if self.active().is_some_and(|r| {
                        !r.files.is_empty()
                            && r.files.iter().all(|&i| self.files[i].change.ignored())
                    }) {
                        self.cursor = 0;
                    }
                    self.files.retain(|f| !f.change.ignored());
                }
                self.options.ignored = !self.options.ignored;
                self.request_refresh();
                self.diff_key.clear();
                self.sync(c);
            }
        }
        Ok(())
    }

    fn move_view_menu(&mut self, delta: isize) {
        if let Some(menu) = &mut self.view_menu {
            menu.row = (menu.row as isize + delta).rem_euclid(Item::ALL.len() as isize) as usize;
        }
    }

    pub(super) fn view_menu_event(&mut self, event: &Event, c: &Context) -> Option<Action> {
        let menu = self.view_menu?;
        match event {
            Event::Key(key) => match key.code {
                K::Up | K::Char('k') | K::BackTab => self.move_view_menu(-1),
                K::Down | K::Char('j') | K::Tab => self.move_view_menu(1),
                K::Home | K::Char('g') => self.view_menu.as_mut().unwrap().row = 0,
                K::End | K::Char('G') => self.view_menu.as_mut().unwrap().row = Item::ALL.len() - 1,
                K::Enter | K::Char(' ') => {
                    return Some(
                        self.toggle_view_item(Item::ALL[menu.row], c)
                            .map(|_| Action::None)
                            .unwrap_or_else(err),
                    )
                }
                K::Esc | K::Char('o') | K::Left | K::Char('h') => self.dismiss_view_menu(),
                _ => (),
            },
            Event::Mouse(mouse) => {
                let inside_menu = inside(menu.area, mouse.column, mouse.row);
                match mouse.kind {
                    M::Down(MouseButton::Left) if inside_menu => {
                        let item = self.hits.iter().rev().find_map(|(r, h)| {
                            if inside(*r, mouse.column, mouse.row) {
                                if let Hit::ViewItem(item) = h {
                                    return Some(*item);
                                }
                            }
                            None
                        });
                        if let Some(item) = item {
                            return Some(
                                self.toggle_view_item(item, c)
                                    .map(|_| Action::None)
                                    .unwrap_or_else(err),
                            );
                        }
                    }
                    M::Down(_) if inside_menu => (),
                    M::Down(_) => {
                        let trigger = self.hits.iter().any(|(r, h)| {
                            matches!(h, Hit::ViewOptions) && inside(*r, mouse.column, mouse.row)
                        });
                        self.dismiss_view_menu();
                        if !trigger {
                            return None;
                        }
                    }
                    M::ScrollUp if inside_menu => self.move_view_menu(-1),
                    M::ScrollDown if inside_menu => self.move_view_menu(1),
                    M::Moved => {
                        self.hover_at(mouse.column, mouse.row);
                    }
                    _ => (),
                }
            }
            Event::FocusLost => self.dismiss_view_menu(),
            _ => (),
        }
        Some(Action::None)
    }

    pub(super) fn draw_view_menu(&mut self, f: &mut Frame, area: Rect) {
        let Some(menu) = self.view_menu else { return };
        if area.width < 2 || area.height < 3 {
            return;
        }
        let anchor = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::ViewOptions).then_some(*r))
            .unwrap_or(Rect::new(area.x, area.y, 1, 1));
        let width = (self.icons.check(false).width() + "Ignored Files".width() + 6)
            .max("Space toggle · Esc close".width() + 2)
            .min(usize::from(area.width)) as u16;
        let height = area.height.min(11);
        let popup = Rect::new(
            anchor.x.clamp(area.x, area.right() - width),
            anchor.bottom().min(area.bottom() - height).max(area.y),
            width,
            height,
        );
        self.view_menu.as_mut().unwrap().area = popup;
        f.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(EDGE))
            .style(Style::new().fg(INK).bg(PANEL));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let slots = usize::from(inner.height.saturating_sub(1));
        let start = Item::ALL[menu.row]
            .position()
            .saturating_sub(slots.saturating_sub(1))
            .min(8usize.saturating_sub(slots));
        for (dy, position) in (start..(start + slots).min(8)).enumerate() {
            let row = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
            match position {
                0 | 5 => word(
                    f,
                    row,
                    if position == 0 { " Group By" } else { " Show" },
                    Style::new().fg(DIM).bold(),
                ),
                4 => word(f, row, &"─".repeat(row.width.into()), Style::new().fg(EDGE)),
                _ => {
                    let item = Item::ALL[if position < 4 {
                        position - 1
                    } else {
                        position - 3
                    }];
                    let style = Style::new().fg(INK).bg(if menu.row == item as usize {
                        SELECT
                    } else {
                        PANEL
                    });
                    let mark = checkbox(self.icons, item.checked(&self.options));
                    let label_width = usize::from(row.width).saturating_sub(mark.width() + 2);
                    let line = Line::from(vec![
                        Span::raw(" "),
                        mark,
                        Span::raw(format!(" {}", clipped(item.label(), label_width))),
                    ]);
                    f.render_widget(Paragraph::new(line).style(style), row);
                    self.hits.push((row, Hit::ViewItem(item)));
                }
            }
        }
        if inner.height > 0 {
            word(
                f,
                slice(inner, inner.height - 1),
                &clipped("Space toggle · Esc close", inner.width.into()),
                Style::new().fg(DIM),
            );
        }
    }
}
