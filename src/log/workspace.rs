mod backend;

use super::*;
use crate::{
    core::{
        loading::{Load, Scope},
        Repo,
    },
    ui::help::{HelpContent, HelpSection},
};
use anyhow::{anyhow, ensure, Context as _};

pub(super) struct Comparison {
    target: operations::Target,
    repo: Repo,
    pub(super) panel: LogPanel,
    scope: Scope,
    files: Load<Result<Vec<model::File>>>,
    preview: Load<Result<DiffView>>,
    selected: Option<Vec<u8>>,
    requested: Option<Vec<u8>>,
    due: Option<Instant>,
    status: Option<String>,
    close: Rect,
    refresh: Rect,
    changed: bool,
}

impl Comparison {
    fn new(target: operations::Target, context: &Context, icons: IconMode) -> Result<Self> {
        let repo = context
            .repos
            .get(target.root)
            .filter(|repo| repo.path == target.path)
            .context("Comparison repository is unavailable")?
            .clone();
        ensure!(
            context.scope.contains(&target.root),
            "Comparison repository is outside Roots"
        );
        let mut panel = LogPanel {
            icons,
            focus: Pane::Files,
            ..Default::default()
        };
        panel.hist.revs.push(model::Rev {
            root: target.root,
            oid: target.oid.clone(),
            subject: target.subject.clone(),
            ..Default::default()
        });
        panel.visible.push(0);
        let mut comparison = Self {
            target,
            repo,
            panel,
            scope: Scope::new(context),
            files: Load::default(),
            preview: Load::default(),
            selected: None,
            requested: None,
            due: None,
            status: None,
            close: Rect::default(),
            refresh: Rect::default(),
            changed: false,
        };
        comparison.refresh(context);
        Ok(comparison)
    }

    fn available(&self, context: &Context) -> bool {
        context.scope.contains(&self.target.root)
            && context
                .repos
                .get(self.target.root)
                .is_some_and(|repo| repo.path == self.repo.path)
    }

    pub(super) fn refresh(&mut self, context: &Context) {
        self.selected = self.path().or_else(|| self.selected.take());
        self.scope = Scope::new(context);
        self.panel.details = Details::default();
        self.panel.inspector = inspector::Inspector::default();
        self.panel.file_search.invalidate();
        self.panel.diff = None;
        self.panel.hits.clear();
        self.requested = None;
        self.due = None;
        self.preview.cancel();
        self.files.cancel();
        self.status = Some(if self.available(context) {
            self.files.request();
            "Loading workspace comparison…".into()
        } else {
            "Comparison repository is outside Roots or unavailable".into()
        });
    }

    fn path(&self) -> Option<Vec<u8>> {
        self.panel
            .file_index()
            .and_then(|index| self.panel.details.files.get(index))
            .map(|file| file.raw.clone())
    }

    fn request_preview(&mut self, delay: Duration) {
        let path = self.path();
        self.selected = path.clone();
        if path != self.requested {
            self.preview.cancel();
            self.panel.diff = None;
            self.requested = path;
            self.due = self.requested.as_ref().map(|_| Instant::now() + delay);
            if self.due.is_some() {
                self.preview.request();
            }
        }
    }

    pub(super) fn wakeup(&self) -> Option<Duration> {
        if self.files.pending() || self.preview.pending() {
            Some(
                self.preview
                    .needs_start()
                    .then_some(self.due)
                    .flatten()
                    .map(|due| due.saturating_duration_since(Instant::now()))
                    .unwrap_or(Duration::from_millis(16))
                    .min(Duration::from_millis(16)),
            )
        } else {
            None
        }
    }

    pub(super) fn poll(&mut self, context: &Context) -> Result<bool> {
        let mut changed = false;
        if !self.scope.matches(context) {
            self.refresh(context);
            changed = true;
        }
        if let Some(result) = self.files.take_result() {
            match result.and_then(|result| result) {
                Ok(files) => {
                    self.panel.details.files = files;
                    self.panel.inspector = inspector::Inspector::new(&self.panel.details);
                    if let Some(index) = self.selected.as_ref().and_then(|path| {
                        self.panel
                            .details
                            .files
                            .iter()
                            .position(|file| &file.raw == path)
                    }) {
                        self.panel.select_file(index);
                    }
                    self.panel.file_search.invalidate();
                    self.status = None;
                    self.request_preview(Duration::ZERO);
                }
                Err(error) => self.status = Some(format!("{error:#} · F5 retry")),
            }
            changed = true;
        }
        if self.files.needs_start() {
            let repo = self.repo.path.clone();
            let oid = self.target.oid.clone();
            if let Err(error) = self
                .files
                .start("igit-workspace-files", move |_| backend::files(&repo, &oid))
            {
                self.status = Some(format!("Cannot load comparison: {error} · F5 retry"));
                return Err(error);
            }
        }
        if let Some(result) = self.preview.take_result() {
            self.panel.diff =
                Some(
                    result
                        .and_then(|result| result)
                        .unwrap_or_else(|error| DiffView {
                            lines: vec![format!("Cannot preview file: {error:#}")],
                            ..Default::default()
                        }),
                );
            changed = true;
        }
        if self.preview.needs_start() && self.due.is_some_and(|due| due <= Instant::now()) {
            self.due = None;
            if let Some(file) = self
                .panel
                .file_index()
                .and_then(|index| self.panel.details.files.get(index))
                .cloned()
            {
                let repo = self.repo.clone();
                let root = self.target.root;
                let oid = self.target.oid.clone();
                if let Err(error) = self.preview.start("igit-workspace-preview", move |_| {
                    backend::diff(&repo, root, &oid, &file)
                }) {
                    self.panel.diff = Some(DiffView {
                        lines: vec![format!("Cannot load preview: {error} · F5 retry")],
                        ..Default::default()
                    });
                    return Err(error);
                }
            }
        }
        Ok(changed)
    }

    pub(super) fn focused_search(&self) -> Option<&crate::ui::search::Prompt> {
        if self.panel.focus == Pane::Files {
            Some(self.panel.file_search.prompt())
        } else {
            self.panel
                .diff
                .as_ref()?
                .comparison
                .as_ref()
                .map(crate::merge::RevisionDiff::search_prompt)
        }
    }

    pub(super) fn editing(&self) -> bool {
        self.focused_search().is_some_and(|prompt| prompt.editing())
    }

    pub(super) fn selected_text(&self) -> Option<&str> {
        (self.panel.focus == Pane::Info)
            .then(|| self.panel.selected_text())
            .flatten()
    }

    pub(super) fn focus_pane(&mut self, id: PaneId) {
        if matches!(id, PaneId::Files | PaneId::Info) {
            self.panel.cancel_mouse();
            self.panel.focus = if id == PaneId::Files {
                Pane::Files
            } else {
                Pane::Info
            };
        }
    }

    pub(super) fn changed(&self) -> bool {
        self.changed
    }

    fn apply_change(&mut self, context: &Context, change: usize) -> Result<()> {
        ensure!(
            self.available(context),
            "Comparison repository is outside Roots or unavailable"
        );
        let path = self.path().context("Select a file first")?;
        let target = file_history::Target {
            root: self.target.root,
            path: model::path_from_bytes(&path)?.to_path_buf(),
        };
        let diff = self
            .panel
            .diff
            .as_ref()
            .context("Wait for the file comparison")?;
        let local = diff
            .local
            .as_ref()
            .context("This file comparison is read-only")?;
        let view = diff
            .comparison
            .as_ref()
            .context("File comparison is unavailable")?;
        let revision = self
            .panel
            .rev()
            .context("Comparison commit is unavailable")?;
        local.ensure_matches(context, revision, &target)?;
        let text = view
            .merged_local_text(Some(change))
            .context("Select a changed block first")?;
        local.write(&text)?;
        self.changed = true;
        self.selected = Some(path);
        self.files.request();
        self.panel.hits.clear();
        let next = local.refresh().and_then(|local| local.diff());
        match next {
            Ok(mut next) => {
                if let Some(next_view) = next.comparison.as_mut() {
                    next_view.inherit_local_view(view);
                }
                self.panel.diff = Some(next);
                Ok(())
            }
            Err(error) => {
                self.panel.diff = None;
                self.requested = None;
                self.request_preview(Duration::ZERO);
                Err(anyhow!(
                    "Hunk applied, but the comparison could not refresh: {error:#}. Press F5 to refresh."
                ))
            }
        }
    }

    fn diff_event(&mut self, event: &Event, context: &Context) -> (bool, Action) {
        if let Some(view) = self
            .panel
            .diff
            .as_mut()
            .and_then(|diff| diff.comparison.as_mut())
        {
            if view.search_event(event) {
                return (false, Action::None);
            }
            let mut forwarded = event.clone();
            if let Event::Key(key) = &mut forwarded {
                key.code = match key.code {
                    K::Char('[') => K::BackTab,
                    K::Char(']') => K::Tab,
                    code => code,
                };
            }
            return match view.event(&forwarded) {
                crate::merge::RevisionEvent::Close => (true, Action::None),
                crate::merge::RevisionEvent::Copy(text) => (
                    false,
                    Action::Copy {
                        text,
                        label: "revision",
                    },
                ),
                crate::merge::RevisionEvent::Apply(change)
                    if !matches!(
                        event,
                        Event::Key(key)
                            if key.kind != crossterm::event::KeyEventKind::Press
                                || key.modifiers.intersects(Mods::ALT | Mods::CONTROL | Mods::SUPER)
                    ) =>
                {
                    (
                        false,
                        self.apply_change(context, change).map_or_else(
                            |error| Action::Notice(NoticeLevel::Error, format!("Log: {error:#}")),
                            |()| Action::None,
                        ),
                    )
                }
                _ => (false, Action::None),
            };
        }
        let Some(diff) = self.panel.diff.as_mut() else {
            return (
                matches!(event, Event::Key(key) if key.code == K::Esc),
                Action::None,
            );
        };
        if let Some(action) = diff.selection.key(event, true) {
            return (false, action);
        }
        if let Event::Mouse(mouse) = event {
            if let Some(action) = diff.selection.mouse(mouse) {
                return (false, action);
            }
            match mouse.kind {
                M::ScrollDown => diff.y = diff.y.saturating_add(3),
                M::ScrollUp => diff.y = diff.y.saturating_sub(3),
                _ => (),
            }
        }
        if let Event::Key(key) = event {
            match key.code {
                K::Esc => return (true, Action::None),
                K::Char('j') | K::Down => diff.y = diff.y.saturating_add(1),
                K::Char('k') | K::Up => diff.y = diff.y.saturating_sub(1),
                K::Char('h') | K::Left => diff.x = diff.x.saturating_sub(4),
                K::Char('l') | K::Right => diff.x = diff.x.saturating_add(4),
                K::PageDown => diff.y = diff.y.saturating_add(12),
                K::PageUp => diff.y = diff.y.saturating_sub(12),
                K::Char('g') if key.modifiers.is_empty() => {
                    if std::mem::take(&mut self.panel.key_prefix) == Some('g') {
                        diff.y = 0;
                    } else {
                        self.panel.key_prefix = Some('g');
                    }
                }
                K::Char('G') => diff.y = diff.lines.len().saturating_sub(1),
                _ => (),
            }
        }
        (false, Action::None)
    }

    fn scroll_preview(&mut self, event: &Event) -> bool {
        let Some(code) = crate::ui::preview_scroll_key(event) else {
            return false;
        };
        let mut forwarded = event.clone();
        if let Event::Key(key) = &mut forwarded {
            key.code = code;
        }
        if let Some(view) = self
            .panel
            .diff
            .as_mut()
            .and_then(|diff| diff.comparison.as_mut())
        {
            let _ = view.event(&forwarded);
        } else if let Some(diff) = self.panel.diff.as_mut() {
            match code {
                K::Down => diff.y = diff.y.saturating_add(1),
                K::Up => diff.y = diff.y.saturating_sub(1),
                _ => (),
            }
            diff.y = diff.y.min(diff.lines.len().saturating_sub(1));
        }
        true
    }

    pub(super) fn event(
        &mut self,
        event: &Event,
        context: &Context,
        panes: &mut PaneLayout,
    ) -> (bool, Action) {
        if !self.scope.matches(context) {
            self.refresh(context);
        }
        if matches!(event, Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Release) {
            return (false, Action::None);
        }
        if !matches!(
            event,
            Event::Key(key) if key.code == K::Char('g') && key.modifiers.is_empty()
        ) {
            self.panel.cancel_key_sequence();
        }
        if let Event::Mouse(mouse) = event {
            if mouse.kind == M::Down(MouseButton::Left) {
                if inside(self.close, mouse.column, mouse.row) {
                    return (true, Action::None);
                }
                if inside(self.refresh, mouse.column, mouse.row) {
                    self.refresh(context);
                    return (false, Action::None);
                }
            }
            if !self.editing() && panes.mouse(mouse) {
                return (false, Action::None);
            }
            if matches!(mouse.kind, M::Down(_) | M::ScrollDown | M::ScrollUp) && !self.editing() {
                if let Some(id) = panes.at(mouse.column, mouse.row) {
                    self.focus_pane(id);
                }
            }
        }
        if self.panel.focus == Pane::Files && self.panel.file_search_event(event, context) {
            self.request_preview(preview::NAV_SETTLE);
            return (false, Action::None);
        }
        if matches!(event, Event::Key(key) if key.code == K::F(5) && !self.editing()) {
            self.refresh(context);
            return (false, Action::None);
        }
        if self.status.is_some() {
            return (
                matches!(event, Event::Key(key) if key.code == K::Esc),
                Action::None,
            );
        }
        if !self.editing() && self.scroll_preview(event) {
            return (false, Action::None);
        }
        if matches!(event, Event::Key(key) if matches!(key.code, K::Tab | K::BackTab | K::F(6)) && !self.editing())
        {
            self.panel.focus = if self.panel.focus == Pane::Files {
                Pane::Info
            } else {
                Pane::Files
            };
            return (false, Action::None);
        }
        if self.panel.focus == Pane::Info {
            return self.diff_event(event, context);
        }
        match event {
            Event::Key(key)
                if !key
                    .modifiers
                    .intersects(Mods::ALT | Mods::CONTROL | Mods::SUPER) =>
            {
                match key.code {
                    K::Esc => return (true, Action::None),
                    K::Char('j') | K::Down => self.panel.file_move(1),
                    K::Char('k') | K::Up => self.panel.file_move(-1),
                    K::Char('g') if key.modifiers.is_empty() => {
                        if std::mem::take(&mut self.panel.key_prefix) == Some('g') {
                            self.panel.file_move(-1_000_000);
                        } else {
                            self.panel.key_prefix = Some('g');
                        }
                    }
                    K::Home => self.panel.file_move(-1_000_000),
                    K::Char('G') | K::End => self.panel.file_move(1_000_000),
                    K::PageDown => self.panel.file_move(12),
                    K::PageUp => self.panel.file_move(-12),
                    K::Char('z' | 'Z') => self.panel.file_fold_all(key.code == K::Char('z')),
                    K::Char('h') | K::Left => {
                        self.panel.file_horizontal(true);
                    }
                    K::Char('l') | K::Right => {
                        if !self.panel.file_horizontal(false) {
                            self.panel.focus = Pane::Info;
                        }
                    }
                    K::Enter => {
                        if self.panel.file_index().is_some() {
                            self.panel.focus = Pane::Info;
                        } else {
                            self.panel.file_activate(self.panel.file_cursor(), true);
                        }
                    }
                    _ => (),
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                M::Down(MouseButton::Left) => {
                    if let Some(hit) = self
                        .panel
                        .hits
                        .iter()
                        .rev()
                        .find(|(rect, _)| inside(*rect, mouse.column, mouse.row))
                        .map(|(_, hit)| hit.clone())
                    {
                        match hit {
                            Hit::File(row) => self.panel.file_activate(row, true),
                            Hit::FileFold(collapse) => self.panel.file_fold_all(collapse),
                            Hit::Diff => self.panel.focus = Pane::Info,
                            _ => (),
                        }
                    }
                }
                M::ScrollDown => self.panel.file_move(3),
                M::ScrollUp => self.panel.file_move(-3),
                _ => (),
            },
            Event::FocusLost => self.panel.cancel_mouse(),
            _ => (),
        }
        self.request_preview(preview::NAV_SETTLE);
        (false, Action::None)
    }

    pub(super) fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        context: &Context,
        panes: &mut PaneLayout,
    ) {
        if !self.scope.matches(context) {
            self.refresh(context);
        }
        panes.begin();
        self.panel.hits.clear();
        self.close = Rect::default();
        self.refresh = Rect::default();
        if area.is_empty() {
            return;
        }
        let title = format!(
            " Compare: {} and Working tree · {}",
            self.target.short(),
            model::plain(self.repo.name.as_bytes())
        );
        let mut title_area = Rect::new(area.x, area.y, area.width, 1);
        self.close = Rect::new(
            area.right().saturating_sub(5).max(area.x),
            area.y,
            area.width.min(5),
            1,
        );
        title_area.width = title_area.width.saturating_sub(self.close.width);
        frame.render_widget(
            Paragraph::new(clipped(&title, title_area.width.into()))
                .style(fg(INK).bg(crate::ui::idea::TAG).bold()),
            title_area,
        );
        frame.render_widget(
            Paragraph::new(" Esc ").style(fg(BLUE).bg(crate::ui::idea::TAG)),
            self.close,
        );
        if area.height < 2 {
            return;
        }
        let banner = Rect::new(area.x, area.y + 1, area.width, 1);
        self.refresh = Rect::new(
            banner.right().saturating_sub(4).max(banner.x),
            banner.y,
            banner.width.min(4),
            1,
        );
        let mut direction = format!(" {} → Working tree", self.target.short());
        if self.status.is_none() {
            let count = self.panel.details.files.len();
            direction.push_str(&format!(
                " · {count} file{}",
                if count == 1 { "" } else { "s" }
            ));
        }
        frame.render_widget(
            Paragraph::new(clipped(&direction, banner.width.saturating_sub(4).into()))
                .style(fg(GOLD)),
            banner,
        );
        frame.render_widget(Paragraph::new(" F5 ").style(fg(BLUE)), self.refresh);
        let body = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        if let Some(status) = &self.status {
            frame.render_widget(Paragraph::new(status.as_str()).style(fg(DIM)), body);
            return;
        }
        let [files, diff] = panes.divide(
            SplitId::LogWorkspace,
            body,
            Axis::Width,
            body.width / 3,
            [16, 20],
        );
        panes.add(PaneId::Files, files);
        panes.add(PaneId::Info, diff);
        let files = pane_frame(
            frame,
            files,
            panes.active && self.panel.focus == Pane::Files,
        );
        self.panel.draw_files(frame, files, context);
        let mut diff = pane_frame(frame, diff, panes.active && self.panel.focus == Pane::Info);
        if let Some(view) = self.panel.diff.as_mut() {
            if let Some(comparison) = view.comparison.as_mut() {
                if let Some(explanation) = view.lines.first() {
                    let height = cells(explanation)
                        .div_ceil(diff.width.max(1))
                        .min(3)
                        .min(diff.height);
                    frame.render_widget(
                        Paragraph::new(explanation.as_str())
                            .style(fg(GOLD))
                            .wrap(ratatui::widgets::Wrap { trim: false }),
                        Rect::new(diff.x, diff.y, diff.width, height),
                    );
                    diff.y += height;
                    diff.height -= height;
                }
                comparison.draw_inline(frame, diff, self.panel.icons);
            } else {
                let mut layout = TextLayout::default();
                for line in &view.lines {
                    layout.push(Line::styled(line.as_str(), fg(DIM)), diff.width, Wrap::Word);
                }
                view.selection
                    .render(frame, diff, layout, view.y, usize::from(view.x));
            }
        } else {
            let text = if self.panel.details.files.is_empty() {
                "No differences between this commit and the working tree"
            } else if self.requested.is_some() {
                "Loading file comparison…"
            } else {
                "Select a file to compare"
            };
            frame.render_widget(
                Paragraph::new(text)
                    .style(fg(DIM))
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                diff,
            );
        }
    }

    pub(super) fn keymap(&self) -> HelpContent {
        let current = if self.panel.focus == Pane::Files {
            HelpSection::new(
                "Log / Workspace Files",
                &[
                    ("j", "Select next file"),
                    ("k", "Select previous file"),
                    ("h", "Collapse directory"),
                    ("l", "Expand directory or focus Diff"),
                    ("Enter", "Focus Diff or toggle directory"),
                    ("/", "Find file or directory"),
                    ("n", "Next matching file"),
                    ("N", "Previous matching file"),
                    ("gg", "Select first item"),
                    ("G", "Select last item"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("z", "Collapse all directories"),
                    ("Z", "Expand all directories"),
                    ("yy", "Copy absolute path"),
                    ("yf", "Copy name"),
                    ("yn", "Copy name without extension"),
                    ("yr", "Copy repository-relative path"),
                ],
            )
        } else {
            HelpSection::new(
                "Log / Workspace Diff",
                &[
                    ("[", "Focus previous revision"),
                    ("]", "Focus next revision"),
                    ("n", "Next change"),
                    ("p", "Previous change"),
                    ("L", "Apply current commit hunk to the working file"),
                    ("/", "Find text in the focused revision"),
                    ("N", "Previous search match"),
                    ("j", "Scroll revisions down"),
                    ("k", "Scroll revisions up"),
                    ("gg", "Go to diff start"),
                    ("G", "Go to diff end"),
                    ("h", "Scroll focused revision left"),
                    ("l", "Scroll focused revision right"),
                    ("f", "Fold or unfold unchanged regions"),
                    ("z", "Zoom focused revision"),
                    ("Ctrl-a", "Select complete focused revision"),
                    ("y", "Copy selection or focused revision"),
                ],
            )
        };
        HelpContent::new(
            current,
            vec![HelpSection::new(
                "Workspace Comparison",
                &[
                    ("Tab", "Focus next comparison pane"),
                    ("Shift-Tab", "Focus previous comparison pane"),
                    ("J", "Scroll the right preview down"),
                    ("K", "Scroll the right preview up"),
                    ("F5", "Refresh working tree comparison"),
                    ("Esc", "Clear search or selection; otherwise return to Log"),
                ],
            )],
        )
    }
}

impl LogPanel {
    pub(super) fn open_workspace_compare(
        &mut self,
        context: &Context,
        target: operations::Target,
    ) -> Result<()> {
        self.require_current_history(context)?;
        if self.filter.comparison.is_some() {
            return Ok(());
        }
        let comparison = Comparison::new(target, context, self.icons)?;
        self.commit_dialog = None;
        self.commit_review = None;
        self.cancel_key_sequence();
        self.panes.begin();
        self.panes.cancel_drag();
        self.hits.clear();
        self.workspace_comparison = Some(Box::new(comparison));
        Ok(())
    }

    pub(crate) fn comparing_workspace(&self) -> bool {
        self.workspace_comparison.is_some()
    }

    pub(crate) fn workspace_apply_available(&self) -> bool {
        self.workspace_comparison
            .as_ref()
            .is_some_and(|comparison| {
                comparison.panel.focus == Pane::Info
                    && comparison
                        .panel
                        .diff
                        .as_ref()
                        .and_then(|diff| diff.comparison.as_ref())
                        .is_some_and(|view| view.local_merge() && view.selected_change().is_some())
            })
    }
}

#[cfg(test)]
mod tests;
