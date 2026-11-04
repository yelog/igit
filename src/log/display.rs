//! IDEA-style presentation options. These are shared by this session's Log tabs.
use super::*;
use crossterm::event::KeyEvent;
use ratatui::widgets::BorderType;

#[derive(Clone)]
pub(super) struct Options {
    pub root_names: bool,
    pub compact_refs: bool,
    pub tag_names: bool,
    pub long_edges: bool,
    pub commit_timestamp: bool,
    pub refs_left: bool,
    pub author: bool,
    pub hash: bool,
    pub date: bool,
    pub signature: bool,
    pub my_commits: bool,
    pub merge_commits: bool,
    pub current_branch: bool,
    pub not_cherry_picked: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root_names: false,
            compact_refs: false,
            tag_names: false,
            long_edges: false,
            commit_timestamp: false,
            refs_left: false,
            author: true,
            hash: false,
            date: true,
            signature: false,
            my_commits: true,
            merge_commits: true,
            current_branch: true,
            not_cherry_picked: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    RootNames,
    CompactRefs,
    TagNames,
    LongEdges,
    CommitTimestamp,
    RefsLeft,
    Columns,
    MyCommits,
    MergeCommits,
    CurrentBranch,
    NotCherryPicked,
}

impl Item {
    pub const ALL: [Self; 11] = [
        Self::RootNames,
        Self::CompactRefs,
        Self::TagNames,
        Self::LongEdges,
        Self::CommitTimestamp,
        Self::RefsLeft,
        Self::Columns,
        Self::MyCommits,
        Self::MergeCommits,
        Self::CurrentBranch,
        Self::NotCherryPicked,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RootNames => "Root Names",
            Self::CompactRefs => "Compact References View",
            Self::TagNames => "Tag Names",
            Self::LongEdges => "Long Edges",
            Self::CommitTimestamp => "Commit Timestamp",
            Self::RefsLeft => "References on the Left",
            Self::Columns => "Columns",
            Self::MyCommits => "My Commits",
            Self::MergeCommits => "Merge Commits",
            Self::CurrentBranch => "Current Branch",
            Self::NotCherryPicked => "Not Cherry-Picked Commits",
        }
    }

    fn position(self) -> usize {
        self as usize + if (self as usize) < 7 { 1 } else { 3 }
    }

    fn value(self, o: &Options) -> bool {
        match self {
            Self::RootNames => o.root_names,
            Self::CompactRefs => o.compact_refs,
            Self::TagNames => o.tag_names,
            Self::LongEdges => o.long_edges,
            Self::CommitTimestamp => o.commit_timestamp,
            Self::RefsLeft => o.refs_left,
            Self::Columns => false,
            Self::MyCommits => o.my_commits,
            Self::MergeCommits => o.merge_commits,
            Self::CurrentBranch => o.current_branch,
            Self::NotCherryPicked => o.not_cherry_picked,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Column {
    Author,
    Hash,
    Date,
    Signature,
}

impl Column {
    pub const ALL: [Self; 4] = [Self::Author, Self::Hash, Self::Date, Self::Signature];
    pub fn label(self) -> &'static str {
        match self {
            Self::Author => "Author",
            Self::Hash => "Hash",
            Self::Date => "Date",
            Self::Signature => "GPG Signature",
        }
    }
    fn value(self, o: &Options) -> bool {
        match self {
            Self::Author => o.author,
            Self::Hash => o.hash,
            Self::Date => o.date,
            Self::Signature => o.signature,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub(super) struct Menu {
    row: usize,
    column: Option<usize>,
}

#[derive(Clone, Copy)]
pub(super) enum MenuHit {
    Surface,
    Item(Item),
    Column(Column),
}

impl LogPanel {
    pub(super) fn read_presentation(&mut self, c: &Context) {
        self.hist.read_presentation(
            c,
            &self.filter,
            self.options.signature,
            self.options.not_cherry_picked,
        );
    }

    fn cherry_available(&self) -> bool {
        self.filter.single_branch().is_some_and(|b| {
            (b.starts_with("refs/heads/") || b.starts_with("refs/remotes/"))
                && self.hist.revs.iter().any(|r| {
                    self.filter.includes_root(r.root)
                        && self.hist.current.get(&r.root).map(String::as_str) != Some(b)
                })
        })
    }

    pub(super) fn toggle_view_menu(&mut self) {
        self.discard_editor();
        self.drag = None;
        self.last = None;
        self.view_menu = if self.view_menu.is_some() {
            None
        } else {
            Some(Menu::default())
        };
    }

    pub(super) fn view_menu_hit(&mut self, hit: MenuHit, c: &Context) -> Result<()> {
        let Some(menu) = &mut self.view_menu else {
            return Ok(());
        };
        match hit {
            MenuHit::Surface => (),
            MenuHit::Column(col) => {
                menu.column = Some(col as usize);
                let v = match col {
                    Column::Author => &mut self.options.author,
                    Column::Hash => &mut self.options.hash,
                    Column::Date => &mut self.options.date,
                    Column::Signature => &mut self.options.signature,
                };
                *v = !*v;
                if col == Column::Signature {
                    self.reload(c, true)?;
                }
            }
            MenuHit::Item(item) => {
                menu.row = item as usize;
                menu.column = (item == Item::Columns).then_some(0);
                let v = match item {
                    Item::RootNames => &mut self.options.root_names,
                    Item::CompactRefs => &mut self.options.compact_refs,
                    Item::TagNames => &mut self.options.tag_names,
                    Item::LongEdges => &mut self.options.long_edges,
                    Item::CommitTimestamp => &mut self.options.commit_timestamp,
                    Item::RefsLeft => &mut self.options.refs_left,
                    Item::MyCommits => &mut self.options.my_commits,
                    Item::MergeCommits => &mut self.options.merge_commits,
                    Item::CurrentBranch => &mut self.options.current_branch,
                    Item::NotCherryPicked => {
                        if !self.options.not_cherry_picked && !self.cherry_available() {
                            return Ok(());
                        }
                        &mut self.options.not_cherry_picked
                    }
                    Item::Columns => return Ok(()),
                };
                *v = !*v;
                if item == Item::NotCherryPicked {
                    self.reload(c, true)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn move_view_menu(&mut self, d: isize) {
        if let Some(m) = &mut self.view_menu {
            if let Some(i) = m.column {
                m.column = Some((i as isize + d).rem_euclid(Column::ALL.len() as isize) as usize);
            } else {
                m.row = (m.row as isize + d).rem_euclid(Item::ALL.len() as isize) as usize;
            }
        }
    }

    pub(super) fn hover_view_menu(&mut self, h: MenuHit) {
        if let Some(m) = &mut self.view_menu {
            match h {
                MenuHit::Item(item) => {
                    m.row = item as usize;
                    m.column = (item == Item::Columns).then_some(m.column.unwrap_or(0));
                }
                MenuHit::Column(col) => m.column = Some(col as usize),
                MenuHit::Surface => (),
            }
        }
    }

    pub(super) fn view_menu_key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        let Some(m) = self.view_menu else {
            return Ok(());
        };
        match k.code {
            K::Up | K::Char('k') => self.move_view_menu(-1),
            K::Down | K::Char('j') | K::Tab => self.move_view_menu(1),
            K::BackTab => self.move_view_menu(-1),
            K::Esc | K::Left | K::Char('h') => {
                if let Some(i) = &mut self.view_menu {
                    if i.column.is_some() {
                        i.column = None;
                    } else {
                        self.view_menu = None;
                    }
                }
            }
            K::Right | K::Char('l') if m.column.is_none() && Item::ALL[m.row] == Item::Columns => {
                self.view_menu.as_mut().unwrap().column = Some(0);
            }
            K::Enter | K::Char(' ') => {
                self.view_menu_hit(
                    m.column.map_or(MenuHit::Item(Item::ALL[m.row]), |i| {
                        MenuHit::Column(Column::ALL[i])
                    }),
                    c,
                )?;
            }
            K::Char('o') => self.view_menu = None,
            _ => (),
        }
        Ok(())
    }

    pub(super) fn view_button(&mut self, f: &mut Frame, a: &mut Rect) {
        let label = format!(
            " {}{} ",
            self.icons.glyph(Icon::Preview),
            self.icons.disclosure(true)
        );
        let w = cells(&label).min(a.width);
        let r = Rect::new(a.right() - w, a.y, w, 1.min(a.height));
        a.width -= w;
        self.button(f, r, &label, self.view_menu.is_some(), Hit::ViewOptions);
    }

    fn menu_box(&mut self, f: &mut Frame, p: Rect, title: &str) -> Rect {
        self.hit(p, Hit::ViewMenu(MenuHit::Surface));
        f.render_widget(Clear, p);
        let b = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(fg(EDGE))
            .style(fg(INK).bg(PANEL))
            .title(title.to_owned());
        let inner = b.inner(p);
        f.render_widget(b, p);
        inner
    }

    fn menu_row(
        &mut self,
        f: &mut Frame,
        r: Rect,
        label: &str,
        checked: bool,
        style: Style,
        hit: MenuHit,
    ) {
        let mark = if matches!(hit, MenuHit::Item(Item::Columns)) {
            " ".repeat(cells(self.icons.check(false)).into())
        } else {
            self.icons.check(checked).to_owned()
        };
        let label = format!(" {mark} {label}");
        f.render_widget(
            Paragraph::new(clipped(&label, r.width.into())).style(style),
            r,
        );
        self.hit(r, Hit::ViewMenu(hit));
    }

    pub(super) fn draw_view_menu(&mut self, f: &mut Frame, a: Rect) {
        let Some(m) = self.view_menu else { return };
        let anchor = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::ViewOptions).then_some(*r))
            .unwrap_or(Rect::new(a.right().saturating_sub(1), a.y, 1, 1));
        let w =
            (cells("Not Cherry-Picked Commits") + cells(self.icons.check(false)) + 5).min(a.width);
        let h = 17.min(a.height);
        let p = Rect::new(
            anchor.right().saturating_sub(w).clamp(a.x, a.right() - w),
            anchor.bottom().min(a.bottom() - h).max(a.y),
            w,
            h,
        );
        let child_w = (cells("GPG Signature") + cells(self.icons.check(false)) + 5).min(a.width);
        let right = p.right().saturating_sub(1);
        let side = if right + child_w <= a.right() {
            Some(right)
        } else if p.x >= a.x + child_w.saturating_sub(1) {
            Some(p.x + 1 - child_w)
        } else {
            None
        };
        let slots = p.height.saturating_sub(3) as usize;
        let start = window(Item::ALL[m.row].position(), slots, 14);
        if m.column.is_none() || side.is_some() {
            let inner = self.menu_box(f, p, "");
            for (dy, i) in (start..(start + slots).min(14)).enumerate() {
                let r = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
                match i {
                    0 | 9 => f.render_widget(
                        Paragraph::new(if i == 0 { " Show" } else { " Highlight" })
                            .style(fg(DIM).bold()),
                        r,
                    ),
                    8 => f.render_widget(
                        Paragraph::new("─".repeat(r.width.into())).style(fg(EDGE)),
                        r,
                    ),
                    _ => {
                        let item = Item::ALL[if i < 8 { i - 1 } else { i - 3 }];
                        let enabled = item != Item::NotCherryPicked
                            || self.cherry_available()
                            || self.options.not_cherry_picked;
                        let style =
                            fg(if enabled { INK } else { DIM }).bg(if m.row == item as usize {
                                SELECT
                            } else {
                                PANEL
                            });
                        self.menu_row(
                            f,
                            r,
                            item.label(),
                            item.value(&self.options),
                            style,
                            MenuHit::Item(item),
                        );
                        if item == Item::Columns && r.width > 2 {
                            let edge = Rect::new(r.right() - 2, r.y, 1, 1);
                            f.render_widget(
                                Paragraph::new(self.icons.glyph(Icon::Next)).style(fg(INK)),
                                edge,
                            );
                        }
                    }
                }
            }
            if inner.height > 0 {
                let hint = if Item::ALL[m.row] == Item::NotCherryPicked && !self.cherry_available()
                {
                    "Select another Branch to compare"
                } else {
                    "Space toggle · Esc close"
                };
                f.render_widget(
                    Paragraph::new(clipped(hint, inner.width.into())).style(fg(DIM)),
                    Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
                );
            }
        }
        if let Some(selected) = m.column {
            let h = 7.min(a.height);
            let y = if side.is_some() {
                p.y + 1 + Item::Columns.position().saturating_sub(start) as u16
            } else {
                p.y
            };
            let p = Rect::new(
                side.unwrap_or(p.x).min(a.right() - child_w),
                y.min(a.bottom() - h).max(a.y),
                child_w,
                h,
            );
            let inner = self.menu_box(f, p, "Columns");
            let slots = inner.height.saturating_sub(1) as usize;
            let start = window(selected, slots, Column::ALL.len());
            for (dy, i) in (start..(start + slots).min(Column::ALL.len())).enumerate() {
                let col = Column::ALL[i];
                self.menu_row(
                    f,
                    Rect::new(inner.x, inner.y + dy as u16, inner.width, 1),
                    col.label(),
                    col.value(&self.options),
                    fg(INK).bg(if selected == i { SELECT } else { PANEL }),
                    MenuHit::Column(col),
                );
            }
            if inner.height > 0 {
                f.render_widget(
                    Paragraph::new("Left / Esc back").style(fg(DIM)),
                    Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
                );
            }
        }
    }
}
