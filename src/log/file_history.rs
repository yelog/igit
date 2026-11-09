//! A file-scoped history tab and its adjacent immutable revision preview.
use super::*;
use crate::ui::preview_scroll_key;
use anyhow::{ensure, Context as _};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub root: usize,
    pub path: PathBuf,
}

impl Target {
    fn resolve(c: &Context, file: &Path) -> Result<Self> {
        ensure!(!file.is_dir(), "File history requires a file path");
        let absolute = std::path::absolute(file)?;
        // Canonicalize an existing ancestor, so deleted files/directories still
        // work and a symlink file is viewed as a Git entry, not its target.
        let mut ancestor = absolute.parent().context("File path has no parent")?;
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .context("File has no existing parent directory")?;
        }
        let resolved = ancestor
            .canonicalize()?
            .join(absolute.strip_prefix(ancestor)?);
        let (root, repo) = c
            .repos
            .iter()
            .enumerate()
            .filter(|(_, repo)| resolved.starts_with(&repo.path))
            .max_by_key(|(_, repo)| repo.path.components().count())
            .context("File is outside the discovered Git worktrees")?;
        let path = resolved.strip_prefix(&repo.path)?.to_path_buf();
        ensure!(
            !path.as_os_str().is_empty() && !path.starts_with(".git"),
            "Choose a file inside the Git working tree"
        );
        ensure!(
            !path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir)),
            "File path escapes the Git working tree"
        );
        Ok(Self { root, path })
    }

    pub(super) fn filter(&self) -> Filter {
        Filter {
            file_history: Some(self.clone()),
            roots: Some([self.root].into()),
            paths: model::PathFilter([(Some(self.root), self.path.clone())].into()),
            branch: Some("HEAD".into()),
            ..Default::default()
        }
    }

    pub(super) fn title(&self) -> String {
        format!(
            "History: {}",
            model::plain(self.path.file_name().unwrap_or_default().as_encoded_bytes())
        )
    }
}

impl LogPanel {
    /// Set startup intent without reading history on the terminal thread.
    pub(crate) fn set_file_history(&mut self, c: &Context, file: &Path) -> Result<()> {
        let target = Target::resolve(c, file)?;
        self.views[self.view_index] = self.snapshot();
        self.filter = target.filter();
        self.views.push(ViewState {
            filter: self.filter.clone(),
            ..Default::default()
        });
        self.view_index = self.views.len() - 1;
        self.hist = History::default();
        self.visible.clear();
        self.ref_query.clear();
        self.ref_search = Default::default();
        self.file_search = Default::default();
        self.regex = false;
        self.case_sensitive = false;
        self.discard_editor();
        self.focus = Pane::Graph;
        self.request_preview(Duration::ZERO);
        Ok(())
    }

    pub(crate) fn file_history_preview_focused(&self) -> bool {
        self.filter.file_history.is_some() && self.focus == Pane::Info && !self.editing()
    }

    pub(crate) fn inline_diff_key(&self, event: &Event) -> bool {
        self.file_history_preview_focused()
            && matches!(event, Event::Key(k) if
                ((k.modifiers.is_empty() || k.modifiers == Mods::SHIFT)
                    && matches!(k.code, K::Char('n' | 'p' | 'f' | 'z' | 'j' | 'k' | 'h' | 'l' | 'g' | 'G' | 'y' | '[' | ']' | ' ')
                        | K::Enter | K::Esc | K::Up | K::Down | K::Left | K::Right | K::PageUp | K::PageDown | K::Home | K::End))
                || (k.modifiers == Mods::CONTROL && (matches!(k.code, K::Char('a' | 'f' | 'b' | 'd' | 'u') | K::Insert)
                    || (k.code == K::Char('c') && self.selected_text().is_some()))))
    }

    pub(super) fn history_event(&mut self, event: &Event) -> Option<Action> {
        if self.filter.file_history.is_none() || self.editing() {
            return None;
        }
        if matches!(event, Event::FocusLost) {
            self.history_mouse = false;
            if let Some(diff) = self.history_diff.as_mut() {
                diff.selection.clear();
                if let Some(comparison) = diff.comparison.as_mut() {
                    comparison.event(event);
                }
            }
            return None;
        }
        let mouse = if let Event::Mouse(m) = event {
            if self.panes.mouse(m) {
                self.history_mouse = false;
                return Some(Action::None);
            }
            let inside = self
                .panes
                .rect(PaneId::Info)
                .is_some_and(|r| inside(r, m.column, m.row));
            let capture = self.history_mouse;
            if m.kind == M::Down(MouseButton::Left) {
                self.history_mouse = inside;
                if inside {
                    self.focus_pane(PaneId::Info);
                }
            } else if m.kind == M::Up(MouseButton::Left) {
                self.history_mouse = false;
            }
            inside || capture
        } else {
            false
        };
        let scroll_key = preview_scroll_key(event);
        if !mouse && scroll_key.is_none() && !self.inline_diff_key(event) {
            return None;
        }
        self.cancel_key_sequence();
        let mut forwarded = event.clone();
        if let Event::Key(k) = &mut forwarded {
            k.code = scroll_key.unwrap_or(match k.code {
                K::Char('[') => K::BackTab,
                K::Char(']') => K::Tab,
                code => code,
            });
        }
        let Some(diff) = self.history_diff.as_mut() else {
            if matches!(event, Event::Key(k) if k.code == K::Esc) {
                self.focus = Pane::Graph;
            }
            return Some(Action::None);
        };
        if let Some(comparison) = diff.comparison.as_mut() {
            use crate::merge::RevisionEvent;
            return Some(match comparison.event(&forwarded) {
                RevisionEvent::Copy(text) => Action::Copy {
                    text,
                    label: "file revision",
                },
                RevisionEvent::Close => {
                    self.focus = Pane::Graph;
                    Action::None
                }
                RevisionEvent::None | RevisionEvent::File(_) => Action::None,
            });
        }
        if let Some(action) = diff.selection.key(event, true) {
            return Some(action);
        }
        if let Event::Mouse(m) = event {
            if let Some(action) = diff.selection.mouse(m) {
                return Some(action);
            }
            match m.kind {
                M::ScrollDown => diff.y = diff.y.saturating_add(3),
                M::ScrollUp => diff.y = diff.y.saturating_sub(3),
                _ => (),
            }
        }
        if let Event::Key(k) = &forwarded {
            match k.code {
                K::Esc => self.focus = Pane::Graph,
                K::Char('j') | K::Down => diff.y = diff.y.saturating_add(1),
                K::Char('k') | K::Up => diff.y = diff.y.saturating_sub(1),
                K::Char('h') | K::Left => diff.x = diff.x.saturating_sub(4),
                K::Char('l') | K::Right => diff.x = diff.x.saturating_add(4),
                K::PageDown => diff.y = diff.y.saturating_add(12),
                K::PageUp => diff.y = diff.y.saturating_sub(12),
                K::Home | K::Char('g') => diff.y = 0,
                K::End | K::Char('G') => diff.y = diff.lines.len().saturating_sub(1),
                _ => (),
            }
        }
        diff.y = diff.y.min(diff.lines.len().saturating_sub(1));
        Some(Action::None)
    }

    pub(super) fn draw_history(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let [graph, preview] = self.panes.divide(
            SplitId::LogHistory,
            area,
            Axis::Width,
            area.width / 2,
            [20, 16],
        );
        self.panes.add(PaneId::Graph, graph);
        self.panes.add(PaneId::Info, preview);
        let graph_body = Rect::new(
            graph.x,
            graph.y,
            graph.width.saturating_sub(1),
            graph.height,
        );
        self.draw_graph(f, graph_body, c);
        if graph.width > 0 {
            let divider = Rect::new(graph.right() - 1, graph.y, 1, graph.height);
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(fg(if self.focus == Pane::Info { BLUE } else { EDGE })),
                divider,
            );
            self.panes.handle(SplitId::LogHistory, divider);
        }
        if preview.is_empty() {
            return;
        }
        if let Some(diff) = self.history_diff.as_mut() {
            if let Some(comparison) = diff.comparison.as_mut() {
                comparison.draw_inline(f, preview, self.icons);
            } else {
                let mut layout = TextLayout::default();
                for line in &diff.lines {
                    let color = if line.starts_with('+') {
                        Color::LightGreen
                    } else if line.starts_with('-') {
                        Color::LightRed
                    } else if line.starts_with("@@") {
                        BLUE
                    } else {
                        INK
                    };
                    layout.push(
                        Line::styled(line.clone(), fg(color)),
                        preview.width,
                        Wrap::Clip,
                    );
                }
                diff.selection
                    .render(f, preview, layout, diff.y, usize::from(diff.x));
            }
        } else {
            f.render_widget(
                Paragraph::new(
                    self.preview
                        .message()
                        .unwrap_or("Select a commit to preview this file"),
                )
                .style(fg(DIM)),
                preview,
            );
        }
    }

    pub(super) fn history_toolbar(&mut self, f: &mut Frame, area: Rect) -> u16 {
        let mut bar = Rect::new(area.x, area.y, area.width, 1);
        let label = format!(
            "Branch: {}",
            self.filter
                .branch
                .as_ref()
                .map_or_else(|| "All".into(), |b| b.text(true))
        );
        let branch = take(&mut bar, cells(&label).saturating_add(1).min(30));
        self.button(f, branch, &label, false, Hit::Filter(Field::Branch));
        if self.filter.branch.is_some() {
            let close = take(&mut bar, 2);
            self.button(
                f,
                close,
                self.icons.glyph(Icon::Close),
                false,
                Hit::Clear(Field::Branch),
            );
        }
        for (icon, hit) in [(Icon::Refresh, Hit::Refresh), (Icon::Add, Hit::More)] {
            if matches!(hit, Hit::More) && (self.hist.more.is_empty() || self.limit >= 3200) {
                continue;
            }
            let label = format!(" {} ", self.icons.glyph(icon));
            let target = take(&mut bar, cells(&label));
            self.button(f, target, &label, false, hit);
        }
        self.view_button(f, &mut bar);
        if !self.filter.query.is_empty() || matches!(self.editor, Some((Field::Text, _))) {
            let text = self
                .editor
                .as_ref()
                .filter(|(k, _)| *k == Field::Text)
                .map_or(self.filter.query.as_str(), |(_, value)| value);
            let clear = toolbar_field(
                f,
                bar,
                text,
                self.editor.is_some(),
                self.icons,
                !self.filter.query.is_empty(),
            );
            self.hit(bar, Hit::Filter(Field::Text));
            if let Some(clear) = clear {
                self.hit(clear, Hit::Clear(Field::Text));
            }
        }
        1
    }
}

pub(super) fn comparison(
    repo: &crate::core::Repo,
    r: &model::Rev,
    file: &model::File,
    index: usize,
) -> Result<DiffView> {
    let mut diff = DiffView {
        file: Some(index),
        name: format!(
            "{} · {} · {}",
            repo.name,
            &r.oid[..8],
            model::plain(&file.raw)
        ),
        ..Default::default()
    };
    if r.parents.len() > 1 {
        match model::merge_diff::sources(&repo.path, r, file) {
            Ok(sources) => {
                let annotations = sources.each_ref().map(|s| s.description.clone());
                let texts = sources.map(|s| s.text);
                diff.comparison = Some(crate::merge::RevisionDiff::new(
                    model::plain(&file.raw),
                    [r.parents[0].clone(), r.oid.clone(), r.parents[1].clone()],
                    texts,
                    annotations,
                ));
            }
            Err(error) => {
                diff.lines.push(error.to_string());
                diff.lines.push(String::new());
            }
        }
    }
    if r.parents.len() <= 1 {
        match model::merge_diff::pair(&repo.path, r, file) {
            Ok(sources) => {
                let annotations = sources.each_ref().map(|s| s.description.clone());
                let texts = sources.map(|s| s.text);
                diff.comparison = Some(crate::merge::RevisionDiff::two_way(
                    model::plain(&file.raw),
                    [
                        r.parents
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "Empty tree".into()),
                        r.oid.clone(),
                    ],
                    texts,
                    annotations,
                ));
            }
            Err(error) => {
                diff.lines.push(error.to_string());
                diff.lines.push(String::new());
            }
        }
    }
    if diff.comparison.is_none() {
        diff.lines.extend(model::patch(&repo.path, r, file)?);
        if let Some(parent) = file.parent.and_then(|p| r.parents.get(p)) {
            diff.name
                .push_str(&format!(" · Changes to {}", &parent[..8]));
        }
    }
    Ok(diff)
}

#[cfg(test)]
mod tests;
