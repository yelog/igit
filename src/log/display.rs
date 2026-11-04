//! IDEA-style presentation options. These are shared by this session's Log tabs.
use super::*;
use crate::ui::idea::checkbox;
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
    /// Menu order is left-to-right log order; hidden columns keep their place.
    pub(super) columns: [(Column, bool); Column::ALL.len()],
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
            columns: Column::ALL.map(|col| (col, !matches!(col, Column::Hash | Column::Signature))),
            my_commits: true,
            merge_commits: true,
            current_branch: true,
            not_cherry_picked: false,
        }
    }
}

impl Options {
    fn column_index(&self, column: Column) -> usize {
        self.columns
            .iter()
            .position(|&(col, _)| col == column)
            .expect("column order is a permutation of Column::ALL")
    }

    pub(super) fn shows(&self, column: Column) -> bool {
        self.columns[self.column_index(column)].1
    }

    fn move_column(&mut self, column: Column, by: isize) {
        let from = self.column_index(column);
        let to = from.saturating_add_signed(by).min(self.columns.len() - 1);
        self.columns.swap(from, to);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    BranchOrTag,
    CommitDetails,
    ReferenceLabels,
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
    pub const ALL: [Self; 14] = [
        Self::BranchOrTag,
        Self::CommitDetails,
        Self::ReferenceLabels,
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
    const DIVIDER_ROW: usize = Self::MyCommits as usize + 1;
    const HIGHLIGHT_ROW: usize = Self::DIVIDER_ROW + 1;
    const ROWS: usize = Self::ALL.len() + 3; // Two headings and a divider.

    pub fn label(self) -> &'static str {
        match self {
            Self::BranchOrTag => "Branch or Tag",
            Self::CommitDetails => "Commit Details",
            Self::ReferenceLabels => "Reference Labels",
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
        let row = self as usize + 1;
        if row < Self::DIVIDER_ROW {
            row
        } else {
            row + 2
        }
    }

    fn value(self, panel: &LogPanel) -> bool {
        let o = &panel.options;
        match self {
            Self::BranchOrTag => panel.show_refs,
            Self::CommitDetails => panel.show_info,
            Self::ReferenceLabels => panel.show_labels,
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
    Graph,
    Message,
}

impl Column {
    pub const ALL: [Self; 6] = [
        Self::Author,
        Self::Hash,
        Self::Date,
        Self::Signature,
        Self::Graph,
        Self::Message,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Author => "Author",
            Self::Hash => "Hash",
            Self::Date => "Date",
            Self::Signature => "GPG Signature",
            Self::Graph => "Graph",
            Self::Message => "Message",
        }
    }
}

#[derive(Default, Clone, Copy)]
pub(super) struct Menu {
    row: usize,
    pub(super) column: Option<Column>,
}

#[derive(Clone, Copy)]
pub(super) enum MenuHit {
    Surface,
    Item(Item),
    Column(Column),
    MoveColumn(Column, isize),
}

impl LogPanel {
    pub(super) fn read_presentation(&mut self, c: &Context) {
        self.hist.read_presentation(
            c,
            &self.filter,
            self.options.shows(Column::Signature),
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
            MenuHit::MoveColumn(col, by) => {
                menu.column = Some(col);
                self.options.move_column(col, by);
            }
            MenuHit::Column(col) => {
                menu.column = Some(col);
                let i = self.options.column_index(col);
                let visible = &mut self.options.columns[i].1;
                *visible = !*visible;
                if col == Column::Signature {
                    self.reload(c, true)?;
                }
            }
            MenuHit::Item(item) => {
                menu.row = item as usize;
                menu.column = (item == Item::Columns).then_some(self.options.columns[0].0);
                self.toggle_view_item(item, c)?;
            }
        }
        Ok(())
    }

    pub(super) fn toggle_view_item(&mut self, item: Item, c: &Context) -> Result<()> {
        let v = match item {
            Item::BranchOrTag => &mut self.show_refs,
            Item::CommitDetails => &mut self.show_info,
            Item::ReferenceLabels => &mut self.show_labels,
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
        if !self.pane_visible(self.focus) {
            self.focus = Pane::Graph;
        }
        if item == Item::NotCherryPicked {
            self.reload(c, true)?;
        }
        Ok(())
    }

    pub(super) fn move_view_menu(&mut self, d: isize) {
        if let Some(m) = &mut self.view_menu {
            if let Some(col) = m.column {
                let i = self.options.column_index(col) as isize;
                let columns = &self.options.columns;
                m.column = Some(columns[(i + d).rem_euclid(columns.len() as isize) as usize].0);
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
                    m.column = (item == Item::Columns)
                        .then_some(m.column.unwrap_or(self.options.columns[0].0));
                }
                MenuHit::Column(col) | MenuHit::MoveColumn(col, _) => m.column = Some(col),
                MenuHit::Surface => (),
            }
        }
    }

    pub(super) fn view_menu_key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        let Some(m) = self.view_menu else {
            return Ok(());
        };
        if let Some(col) = m.column {
            let by = match (k.code, k.modifiers) {
                (K::Char('K'), Mods::NONE | Mods::SHIFT) | (K::Up | K::Char('k'), Mods::SHIFT) => {
                    -1
                }
                (K::Char('J'), Mods::NONE | Mods::SHIFT)
                | (K::Down | K::Char('j'), Mods::SHIFT) => 1,
                _ => 0,
            };
            if by != 0 {
                return self.view_menu_hit(MenuHit::MoveColumn(col, by), c);
            }
        }
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
                self.view_menu.as_mut().unwrap().column = Some(self.options.columns[0].0);
            }
            K::Enter | K::Char(' ') => {
                self.view_menu_hit(
                    m.column
                        .map_or(MenuHit::Item(Item::ALL[m.row]), MenuHit::Column),
                    c,
                )?;
            }
            K::Char('o') => self.view_menu = None,
            _ => (),
        }
        Ok(())
    }

    pub(super) fn view_button(&mut self, f: &mut Frame, a: &mut Rect) {
        let label = format!(" {} ", self.icons.glyph(Icon::Preview));
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
            Span::raw(" ".repeat(cells(self.icons.check(false)).into()))
        } else {
            checkbox(self.icons, checked)
        };
        let width = usize::from(r.width).saturating_sub(mark.width() + 2);
        let line = Line::from(vec![
            Span::raw(" "),
            mark,
            Span::raw(format!(" {}", clipped(label, width))),
        ]);
        f.render_widget(Paragraph::new(line).style(style), r);
        self.hit(r, Hit::ViewMenu(hit));
    }

    pub(super) fn draw_view_menu(&mut self, f: &mut Frame, a: Rect) {
        let Some(m) = self.view_menu else { return };
        const COLUMN_HINTS: [&str; 2] = ["K/J or Shift+Up/Down: move", "Space toggle · Esc back"];
        let anchor = self
            .hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::ViewOptions).then_some(*r))
            .unwrap_or(Rect::new(a.right().saturating_sub(1), a.y, 1, 1));
        let w =
            (cells("Not Cherry-Picked Commits") + cells(self.icons.check(false)) + 5).min(a.width);
        let h = (Item::ROWS as u16 + 3).min(a.height);
        let p = Rect::new(
            anchor.right().saturating_sub(w).clamp(a.x, a.right() - w),
            anchor.bottom().min(a.bottom() - h).max(a.y),
            w,
            h,
        );
        let move_width =
            cells(self.icons.glyph(Icon::Up)) + cells(self.icons.glyph(Icon::Down)) + 4;
        let child_w =
            (cells(Column::Signature.label()) + cells(self.icons.check(false)) + 5 + move_width)
                .max(cells(COLUMN_HINTS[0]) + 2)
                .min(a.width);
        let right = p.right().saturating_sub(1);
        let side = if right + child_w <= a.right() {
            Some(right)
        } else if p.x >= a.x + child_w.saturating_sub(1) {
            Some(p.x + 1 - child_w)
        } else {
            None
        };
        let slots = p.height.saturating_sub(3) as usize;
        let start = window(Item::ALL[m.row].position(), slots, Item::ROWS);
        if m.column.is_none() || side.is_some() {
            let inner = self.menu_box(f, p, "");
            for (dy, i) in (start..(start + slots).min(Item::ROWS)).enumerate() {
                let r = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
                match i {
                    0 | Item::HIGHLIGHT_ROW => f.render_widget(
                        Paragraph::new(if i == 0 { " Show" } else { " Highlight" })
                            .style(fg(DIM).bold()),
                        r,
                    ),
                    Item::DIVIDER_ROW => f.render_widget(
                        Paragraph::new("─".repeat(r.width.into())).style(fg(EDGE)),
                        r,
                    ),
                    _ => {
                        let item = Item::ALL[if i < Item::DIVIDER_ROW { i - 1 } else { i - 3 }];
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
                            item.value(self),
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
            let columns = self.options.columns;
            let h = (columns.len() + COLUMN_HINTS.len() + 2) as u16;
            let h = h.min(a.height);
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
            let slots = columns.len().min(inner.height.into());
            let start = window(self.options.column_index(selected), slots, columns.len());
            for (dy, i) in (start..start + slots).enumerate() {
                let (col, visible) = columns[i];
                let style = fg(INK).bg(if selected == col { SELECT } else { PANEL });
                let mut row = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
                // At very small widths keep the checkbox and name reachable by mouse.
                let label_width = if row.width >= move_width + cells(self.icons.check(false)) + 4 {
                    row.width - move_width
                } else {
                    row.width
                };
                self.menu_row(
                    f,
                    take(&mut row, label_width),
                    col.label(),
                    visible,
                    style,
                    MenuHit::Column(col),
                );
                for (icon, by, enabled) in [
                    (Icon::Up, -1, i > 0),
                    (Icon::Down, 1, i + 1 < columns.len()),
                ] {
                    let glyph = self.icons.glyph(icon);
                    let button = take(&mut row, cells(glyph) + 2);
                    f.render_widget(
                        Paragraph::new(glyph).centered().style(style.fg(if enabled {
                            BLUE
                        } else {
                            DIM
                        })),
                        button,
                    );
                    self.hit(button, Hit::ViewMenu(MenuHit::MoveColumn(col, by)));
                }
            }
            for (dy, hint) in COLUMN_HINTS
                .iter()
                .take(usize::from(inner.height) - slots)
                .enumerate()
            {
                f.render_widget(
                    Paragraph::new(*hint).style(fg(DIM)),
                    Rect::new(inner.x, inner.y + (slots + dy) as u16, inner.width, 1),
                );
            }
        }
    }
}
