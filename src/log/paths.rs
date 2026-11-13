//! Immediate Roots checkboxes and two draft editors for the current Log's path filter.
mod tree;

use super::input::TextInput;
use super::*;
use crate::ui::idea::checkbox;
use crossterm::event::KeyEvent;
use model::PathFilter;
use std::collections::BTreeSet;
use tree::PathTree;
pub(super) use tree::TreeHit;

pub(super) const COMMANDS: [&str; 2] = ["Select...", "Select in Tree..."];
const RECENT_LIMIT: usize = 10;

pub(super) enum Dialog {
    Text(TextInput),
    Tree(PathTree),
}

impl Dialog {
    pub(super) fn scroll(&mut self, delta: isize) {
        match self {
            Self::Tree(tree) => tree.move_by(delta),
            Self::Text(input) => input.vertical(delta),
        }
    }
}

impl Filter {
    pub(super) fn includes_root(&self, id: usize) -> bool {
        self.roots.as_ref().is_none_or(|roots| roots.contains(&id))
    }

    fn menu_includes_root(&self, id: usize) -> bool {
        self.paths.is_empty() && self.includes_root(id)
    }

    pub(super) fn paths_label(&self, c: &Context) -> String {
        let paths = self.paths.label(c);
        let Some(roots) = &self.roots else {
            return paths;
        };
        let scope = match roots.len() {
            1 => c.repos[*roots.first().unwrap()].name.clone(),
            n => format!("{n} roots"),
        };
        if paths.is_empty() {
            scope
        } else {
            format!("{scope}: {paths}")
        }
    }

    fn set_roots(&mut self, roots: BTreeSet<usize>, c: &Context) {
        self.roots = (roots.len() != c.repos.len()).then_some(roots);
    }
}

impl LogPanel {
    pub(super) fn editing_paths(&self) -> bool {
        matches!(self.editor, Some((Field::Path, _)))
    }

    pub(super) fn choice_count(&self) -> usize {
        if self.editing_paths() {
            COMMANDS.len() + self.path_options.len() + self.path_recents.len()
        } else {
            self.suggestions.len()
        }
    }

    fn recent_choice_start(&self) -> usize {
        COMMANDS.len() + self.path_options.len()
    }

    pub(super) fn is_recent_choice(&self, index: usize) -> bool {
        index >= self.recent_choice_start() && index < self.choice_count()
    }

    fn path_menu_visual(&self, choice: usize) -> usize {
        let mut visual = choice + usize::from(choice >= COMMANDS.len());
        if self.is_recent_choice(choice) {
            visual += 1 + usize::from(self.path_options.is_empty());
        }
        visual
    }

    pub(super) fn remember_paths(&mut self, paths: &PathFilter) {
        if paths.is_empty() {
            return;
        }
        self.path_recents.retain(|recent| recent != paths);
        self.path_recents.insert(0, paths.clone());
        self.path_recents.truncate(RECENT_LIMIT);
    }

    pub(super) fn path_choice(&mut self, index: usize, c: &Context) -> Result<()> {
        match index {
            0 => self.path_dialog = Some(Dialog::Text(TextInput::new(self.filter.paths.text(c)?))),
            1 => self.path_dialog = Some(Dialog::Tree(PathTree::new(&self.filter, c))),
            _ if index < self.recent_choice_start() => self.toggle_root(index, c)?,
            _ => self.use_recent_paths(index, c)?,
        }
        Ok(())
    }

    fn toggle_root(&mut self, index: usize, c: &Context) -> Result<()> {
        let Some(&id) = self
            .path_options
            .get(index - COMMANDS.len())
            .filter(|id| c.scope.contains(id))
        else {
            return Ok(());
        };
        let path_mode = !self.filter.paths.is_empty();
        let mut roots = if path_mode {
            BTreeSet::new()
        } else {
            self.filter.roots.take().unwrap_or_else(|| c.scope.clone())
        };
        if !roots.remove(&id) {
            roots.insert(id);
        }
        self.filter.set_roots(roots, c);
        self.choice = Some(index);
        if path_mode {
            self.filter.paths = PathFilter::default();
            return self.reload(c, true);
        }
        self.refilter_roots(c)
    }

    fn use_recent_paths(&mut self, index: usize, c: &Context) -> Result<()> {
        let Some(paths) = self
            .path_recents
            .get(index - self.recent_choice_start())
            .cloned()
        else {
            return Ok(());
        };
        let roots_changed = self.filter.roots.take().is_some();
        let paths_changed = self.filter.paths != paths;
        self.filter.paths = paths.clone();
        self.remember_paths(&paths);
        self.choice = Some(self.recent_choice_start());
        if paths_changed || roots_changed {
            self.reload(c, true)
        } else {
            Ok(())
        }
    }

    pub(super) fn toggle_listed_roots(&mut self, c: &Context) -> Result<()> {
        if self.path_options.is_empty() {
            return Ok(());
        }
        let path_mode = !self.filter.paths.is_empty();
        let all = !path_mode
            && self
                .path_options
                .iter()
                .all(|&id| self.filter.menu_includes_root(id));
        let mut roots = if path_mode {
            BTreeSet::new()
        } else {
            self.filter.roots.take().unwrap_or_else(|| c.scope.clone())
        };
        for &id in &self.path_options {
            if all {
                roots.remove(&id);
            } else {
                roots.insert(id);
            }
        }
        self.filter.set_roots(roots, c);
        self.choice = Some(self.choice.unwrap_or(COMMANDS.len()).max(COMMANDS.len()));
        if path_mode {
            self.filter.paths = PathFilter::default();
            return self.reload(c, true);
        }
        self.refilter_roots(c)
    }

    fn refilter_roots(&mut self, c: &Context) -> Result<()> {
        let keep = self.rev().map(|r| (r.root, r.oid.clone()));
        self.rebuild_refs(c);
        self.rebuild_visible(keep)
    }

    fn finish_paths(&mut self, c: &Context, paths: PathFilter) -> Result<()> {
        self.discard_editor();
        let mut roots_changed = false;
        if !paths.is_empty() {
            roots_changed = self.filter.roots.is_some();
            self.filter.roots = None;
            self.remember_paths(&paths);
        }
        if self.filter.paths == paths && !roots_changed {
            return Ok(());
        }
        self.filter.paths = paths;
        self.reload(c, true)
    }

    // Type-to-find in the menu still accepts an exact repository name as a shortcut.
    // The explicit multiline Select dialog never interprets a bare name as a root.
    pub(super) fn apply_path_input(&mut self, c: &Context, input: String) -> Result<()> {
        if input.is_empty() {
            self.discard_editor();
            return Ok(());
        }
        if let Some(id) = c.repos.iter().position(|r| r.name == input) {
            self.filter.set_roots([id].into(), c);
            if self.filter.paths.is_empty() {
                self.discard_editor();
                return self.refilter_roots(c);
            }
            self.finish_paths(c, PathFilter::default())
        } else {
            self.finish_paths(c, PathFilter::parse(&input, c)?)
        }
    }

    pub(super) fn clear_paths(&mut self, c: &Context) -> Result<()> {
        self.filter.roots = None;
        if self.filter.paths.is_empty() {
            self.discard_editor();
            return self.refilter_roots(c);
        }
        self.finish_paths(c, PathFilter::default())
    }

    pub(super) fn open_path_text_for_paste(&mut self, paste: &str) {
        let mut input = TextInput::new(
            self.editor
                .as_ref()
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
        );
        input.paste(paste);
        self.path_dialog = Some(Dialog::Text(input));
    }

    pub(super) fn apply_path_dialog(&mut self, c: &Context) -> Result<()> {
        let paths = match &self.path_dialog {
            Some(Dialog::Text(input)) => PathFilter::parse(&input.text, c)?,
            Some(Dialog::Tree(tree)) if tree.can_apply() => tree.result(&self.filter.paths),
            _ => return Ok(()),
        };
        self.finish_paths(c, paths)
    }

    pub(super) fn path_dialog_key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        if k.code == K::Esc {
            self.discard_editor();
        } else if k.code == K::F(2)
            || (k.code == K::Char('s') && k.modifiers.contains(Mods::CONTROL))
            || (k.code == K::Enter
                && (k.modifiers.intersects(Mods::CONTROL | Mods::SUPER)
                    || matches!(self.path_dialog, Some(Dialog::Tree(_)))))
        {
            self.apply_path_dialog(c)?;
        } else {
            match &mut self.path_dialog {
                Some(Dialog::Text(input)) => input.key(k),
                Some(Dialog::Tree(tree)) => tree.key(k, c)?,
                None => (),
            }
        }
        Ok(())
    }

    pub(super) fn draw_paths(
        &mut self,
        f: &mut Frame,
        a: Rect,
        anchor: Rect,
        c: &Context,
        query: &str,
    ) {
        let (p, title) = match &self.path_dialog {
            Some(Dialog::Text(input)) => {
                let w = a.width.min(66);
                let h = a
                    .height
                    .min((input.text.lines().count().clamp(3, 8) + 4) as u16);
                (
                    Rect::new(
                        anchor.x.min(a.right() - w),
                        anchor.bottom().min(a.bottom() - h).max(a.y),
                        w,
                        h,
                    ),
                    "Select Paths",
                )
            }
            Some(Dialog::Tree(_)) => (
                popup(a, 84, a.height.saturating_sub(2).clamp(6, 32)),
                "Select Paths to Filter by",
            ),
            None => {
                let w = a.width.min(38);
                let x = anchor.x.min(a.right() - w);
                let y = anchor.bottom().min(a.bottom().saturating_sub(6)).max(a.y);
                let recents = self.path_recents.len() + usize::from(!self.path_recents.is_empty());
                let items = (self.path_options.len().max(1)
                    + 5
                    + recents
                    + usize::from(!query.is_empty() || self.choice.is_none()))
                .min(28) as u16;
                (Rect::new(x, y, w, items.min(a.bottom() - y)), "Paths")
            }
        };
        if self.path_dialog.is_some() {
            self.hits.clear(); // A modal's blank space cannot select a commit behind it.
        }
        self.hit(p, Hit::Dropdown(DropdownHit::Surface));
        f.render_widget(Clear, p);
        let block = boxof(title.into(), true);
        let inner = block.inner(p);
        f.render_widget(block, p);
        if inner.is_empty() {
            return;
        }
        let foot = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
        let body = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let (apply, enabled) = match &mut self.path_dialog {
            Some(Dialog::Text(input)) => {
                let area = input_area(body);
                input.draw(f, area);
                self.hit(area, Hit::Dropdown(DropdownHit::PathText));
                if !body.is_empty() {
                    f.render_widget(
                        Paragraph::new(
                            "One relative path per line · Enter: new line · Ctrl-S: apply",
                        )
                        .style(fg(DIM)),
                        Rect::new(body.x, body.bottom() - 1, body.width, 1),
                    );
                }
                ("Apply F2", true)
            }
            Some(Dialog::Tree(tree)) => {
                tree.draw(f, body, self.icons, c, &mut self.hits);
                ("OK Enter", tree.can_apply())
            }
            None => {
                self.draw_path_menu(f, inner, c, query);
                return;
            }
        };
        let mut bar = foot;
        let cancel = "Cancel Esc";
        let padding = bar.width.saturating_sub(cells(cancel) + cells(apply) + 5);
        take(&mut bar, padding);
        for (label, hit, active) in [
            (cancel, DropdownHit::Cancel, true),
            (apply, DropdownHit::Apply, enabled),
        ] {
            let r = take(&mut bar, cells(label) + 2);
            let style = if active {
                crate::ui::idea::button_style(matches!(hit, DropdownHit::Apply))
            } else {
                fg(DIM).bg(crate::ui::idea::TAG)
            };
            f.render_widget(Paragraph::new(format!(" {label} ")).style(style), r);
            if active {
                self.hit(r, Hit::Dropdown(hit));
            }
            take(&mut bar, 1);
        }
    }

    fn draw_path_menu(&mut self, f: &mut Frame, inner: Rect, c: &Context, query: &str) {
        let n = self.path_options.len();
        let mark_width = cells(self.icons.check(true)).max(cells(self.icons.check(false)));
        let recent_width = usize::from(inner.width.saturating_sub(mark_width + 1));
        let recents: Vec<_> = self
            .path_recents
            .iter()
            .map(|paths| {
                (
                    paths == &self.filter.paths,
                    paths.recent_label(c, recent_width),
                )
            })
            .collect();
        let selected = self
            .path_options
            .iter()
            .filter(|&&id| self.filter.menu_includes_root(id))
            .count();
        let commands = COMMANDS.len();
        let row = |y| Rect::new(inner.x, y, inner.width, 1);
        let searching = !query.is_empty() || self.choice.is_none();
        let h = usize::from(inner.height) - usize::from(searching);
        let recent_rows = recents.len() + usize::from(!recents.is_empty());
        let len = commands + 1 + n.max(1) + recent_rows;
        let top = window(self.path_menu_visual(self.choice.unwrap_or(0)), h, len);
        let recent_header = commands + 1 + n.max(1);
        for (dy, row_index) in (top..(top + h).min(len)).enumerate() {
            let r = row(inner.y + dy as u16);
            let (line, hit) = if row_index < commands {
                (
                    Line::raw(format!("  {}", COMMANDS[row_index])),
                    DropdownHit::Choice(row_index),
                )
            } else if row_index == commands {
                let mark = if selected > 0 && selected < n {
                    self.icons.glyph(Icon::Partial)
                } else {
                    self.icons.check(n > 0 && selected == n)
                };
                (
                    Line::styled(format!("{mark} Roots  {selected}/{n}   Ctrl-A"), fg(DIM)),
                    DropdownHit::AllRoots,
                )
            } else if let Some(&id) = self
                .path_options
                .get(row_index - commands - 1)
                .filter(|_| row_index < commands + 1 + n)
            {
                let mark = self.icons.check(self.filter.menu_includes_root(id));
                (
                    Line::from(vec![
                        Span::styled(format!("{mark} "), fg(repo_tint(id))),
                        Span::raw(clipped(
                            &c.repos[id].name,
                            usize::from(r.width.saturating_sub(cells(mark) + 1)),
                        )),
                    ]),
                    DropdownHit::Choice(row_index - 1),
                )
            } else if n == 0 && row_index == commands + 1 {
                (
                    Line::styled(
                        if query.is_empty() {
                            "No roots in scope"
                        } else {
                            "Enter to use this path"
                        },
                        fg(DIM),
                    ),
                    DropdownHit::Surface,
                )
            } else if row_index == recent_header {
                (Line::styled("Recent", fg(DIM)), DropdownHit::Surface)
            } else if let Some((index, (current, label))) = row_index
                .checked_sub(recent_header + 1)
                .and_then(|index| recents.get(index).map(|recent| (index, recent)))
            {
                let mark = self.icons.check(*current);
                (
                    Line::from(vec![
                        checkbox(self.icons, *current),
                        Span::raw(" "),
                        Span::raw(clipped(
                            label,
                            usize::from(r.width.saturating_sub(cells(mark) + 1)),
                        )),
                    ]),
                    DropdownHit::Choice(self.recent_choice_start() + index),
                )
            } else {
                (Line::raw(""), DropdownHit::Surface)
            };
            let active = matches!(hit, DropdownHit::Choice(i) if self.choice == Some(i));
            f.render_widget(
                Paragraph::new(line).style(if active { fg(INK).bg(SELECT) } else { fg(INK) }),
                r,
            );
            if !matches!(hit, DropdownHit::Surface) {
                self.hit(r, Hit::Dropdown(hit));
            }
        }
        if searching {
            let r = row(inner.bottom() - 1);
            toolbar_field(f, r, query, self.choice.is_none(), self.icons, false);
            self.hit(r, Hit::Dropdown(DropdownHit::PathInput));
        }
    }
}

fn input_area(body: Rect) -> Rect {
    Rect::new(body.x, body.y, body.width, body.height.saturating_sub(1))
}
