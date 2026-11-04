//! Paths combines a per-Log set of repository IDs with an optional literal Git path.
use super::*;
use std::collections::BTreeSet;

impl Filter {
    pub(super) fn includes_root(&self, id: usize) -> bool {
        self.roots.as_ref().is_none_or(|roots| roots.contains(&id))
    }

    pub(super) fn paths_label(&self, c: &Context) -> String {
        let Some(roots) = &self.roots else {
            return self.path.clone();
        };
        let scope = match roots.len() {
            1 => c.repos[*roots.first().unwrap()].name.clone(),
            n => format!("{n} roots"),
        };
        if self.path.is_empty() {
            scope
        } else {
            format!("{scope}: {}", self.path)
        }
    }

    fn set_roots(&mut self, roots: BTreeSet<usize>, c: &Context) {
        self.roots = (roots.len() != c.repos.len()).then_some(roots);
    }
}

impl LogPanel {
    pub(super) fn editing_paths(&self) -> bool {
        matches!(self.editor, Some((control, _)) if control.field() == Field::Path)
    }

    pub(super) fn choice_count(&self) -> usize {
        if self.editing_paths() {
            self.path_options.len()
        } else {
            self.suggestions.len()
        }
    }

    pub(super) fn toggle_path_root(&mut self, index: usize, c: &Context) -> Result<()> {
        let Some(&id) = self
            .path_options
            .get(index)
            .filter(|id| c.scope.contains(id))
        else {
            return Ok(());
        };
        let mut roots = self.filter.roots.take().unwrap_or_else(|| c.scope.clone());
        if !roots.remove(&id) {
            roots.insert(id);
        }
        self.filter.set_roots(roots, c);
        self.choice = Some(index);
        self.refilter_roots()
    }

    pub(super) fn toggle_listed_roots(&mut self, c: &Context) -> Result<()> {
        if self.path_options.is_empty() {
            return Ok(());
        }
        let all = self
            .path_options
            .iter()
            .all(|&id| self.filter.includes_root(id));
        let mut roots = self.filter.roots.take().unwrap_or_else(|| c.scope.clone());
        for &id in &self.path_options {
            if all {
                roots.remove(&id);
            } else {
                roots.insert(id);
            }
        }
        self.filter.set_roots(roots, c);
        self.choice = Some(self.choice.unwrap_or(0));
        self.refilter_roots()
    }

    fn refilter_roots(&mut self) -> Result<()> {
        let keep = self.rev().map(|r| (r.root, r.oid.clone()));
        self.rebuild_visible(keep)
    }

    pub(super) fn apply_path_input(&mut self, c: &Context, input: String) -> Result<()> {
        self.discard_editor();
        if input == self.filter.path {
            return Ok(());
        }
        // A typed repository name is the existing shortcut for selecting just that root.
        // Resolve it across the workspace, so hiding that root cannot turn it into a filename.
        if let Some(id) = c.repos.iter().position(|r| r.name == input) {
            self.filter.set_roots([id].into(), c);
            if self.filter.path.is_empty() {
                return self.refilter_roots();
            }
            self.filter.path.clear();
        } else {
            self.filter.path = input;
        }
        self.reload(c, true)
    }

    pub(super) fn clear_paths(&mut self, c: &Context) -> Result<()> {
        self.discard_editor();
        self.filter.roots = None;
        if self.filter.path.is_empty() {
            return self.refilter_roots();
        }
        self.filter.path.clear();
        self.reload(c, true)
    }

    pub(super) fn draw_path_choices(
        &mut self,
        f: &mut Frame,
        inner: Rect,
        c: &Context,
        input: &str,
    ) {
        let row = |y| Rect::new(inner.x, y, inner.width, 1);
        let n = self.path_options.len();
        let selected = self
            .path_options
            .iter()
            .filter(|&&id| self.filter.includes_root(id))
            .count();
        let mark = if selected > 0 && selected < n {
            self.icons.glyph(Icon::Partial)
        } else {
            self.icons.check(n > 0 && selected == n)
        };
        let header = row(inner.y);
        f.render_widget(
            Paragraph::new(format!("{mark} Roots  {selected}/{n}   Ctrl-A")).style(fg(DIM)),
            header,
        );
        self.hit(header, Hit::Dropdown(DropdownHit::AllRoots));
        let rows = usize::from(inner.height.saturating_sub(3));
        let start = window(self.choice.unwrap_or(0), rows, n);
        for (dy, i) in (start..(start + rows).min(n)).enumerate() {
            let id = self.path_options[i];
            let mark = self.icons.check(self.filter.includes_root(id));
            let r = row(inner.y + 1 + dy as u16);
            let line = Line::from(vec![
                Span::styled(format!("{mark} "), fg(repo_tint(id))),
                Span::raw(clipped(
                    &c.repos[id].name,
                    usize::from(r.width.saturating_sub(cells(mark) + 1)),
                )),
            ]);
            let style = if self.choice == Some(i) {
                fg(INK).bg(SELECT)
            } else {
                fg(INK)
            };
            f.render_widget(Paragraph::new(line).style(style), r);
            self.hit(r, Hit::Dropdown(DropdownHit::Choice(i)));
        }
        if rows > 0 && n == 0 {
            f.render_widget(
                Paragraph::new(if input.is_empty() {
                    "No roots in scope"
                } else {
                    "Enter to use this literal path"
                })
                .style(fg(DIM)),
                row(inner.y + 1),
            );
        }
        if inner.height >= 3 {
            let input_row = row(inner.bottom() - 2);
            let mut field = input_row;
            let label = take(&mut field, 5);
            f.render_widget(Paragraph::new("Path ").style(fg(DIM)), label);
            toolbar_field(f, field, input, self.choice.is_none(), self.icons, false);
            self.hit(input_row, Hit::Dropdown(DropdownHit::PathInput));
        }
        if inner.height >= 2 {
            let mut bar = row(inner.bottom() - 1);
            let hint = take(&mut bar, 13);
            f.render_widget(Paragraph::new("Space toggle").style(fg(DIM)), hint);
            self.bar(
                f,
                bar,
                &[
                    (
                        format!(
                            "{} {}",
                            if self.choice.is_some() {
                                "Done"
                            } else {
                                "Apply"
                            },
                            self.icons.glyph(Icon::Enter)
                        ),
                        Hit::Dropdown(DropdownHit::Apply),
                    ),
                    ("Close Esc".into(), Hit::Dropdown(DropdownHit::Cancel)),
                ],
            );
        }
    }
}
