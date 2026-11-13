use super::*;
use crate::core::{
    git,
    loading::{Load, Scope},
};
use anyhow::{ensure, Context as _};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Range {
    pub repository: PathBuf,
    pub include: String,
    pub exclude: String,
    pub include_name: String,
    pub exclude_name: String,
}

impl Range {
    pub(super) fn banner(&self, width: u16) -> String {
        let text = format!(
            " Commits that exist in {} but don't exist in {}",
            self.include_name, self.exclude_name
        );
        if cells(&text) <= width {
            text
        } else {
            let budget = usize::from(width.saturating_sub(13)) / 2;
            format!(
                " {} only (not {})",
                clipped(&self.include_name, budget),
                clipped(&self.exclude_name, budget)
            )
        }
    }
}

#[derive(Clone)]
pub(crate) struct Target {
    pub repository: PathBuf,
    pub current: String,
    pub target: String,
}

impl Target {
    fn read(&self, context: &Context) -> Result<[Range; 2]> {
        ensure!(
            context
                .scope
                .iter()
                .any(|root| context.repos[*root].path == self.repository),
            "Comparison repository is outside Roots"
        );
        let refs = git(
            &self.repository,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)",
                &self.current,
                &self.target,
            ],
        )?;
        let resolve = |reference: &str| -> Result<String> {
            refs.lines()
                .filter_map(|line| line.split_once('\0'))
                .find_map(|(name, oid)| (name == reference).then(|| oid.to_owned()))
                .with_context(|| {
                    format!("Branch is unavailable: {}", filters::short_ref(reference))
                })
        };
        let current = resolve(&self.current)?;
        let target = resolve(&self.target)?;
        let current_name = filters::short_ref(&self.current).to_owned();
        let target_name = filters::short_ref(&self.target).to_owned();
        Ok([
            Range {
                repository: self.repository.clone(),
                include: target.clone(),
                exclude: current.clone(),
                include_name: target_name.clone(),
                exclude_name: current_name.clone(),
            },
            Range {
                repository: self.repository.clone(),
                include: current,
                exclude: target,
                include_name: current_name,
                exclude_name: target_name,
            },
        ])
    }
}

const PANES: [[PaneId; 3]; 2] = [
    [
        PaneId::CompareTargetGraph,
        PaneId::CompareTargetFiles,
        PaneId::CompareTargetInfo,
    ],
    [
        PaneId::CompareCurrentGraph,
        PaneId::CompareCurrentFiles,
        PaneId::CompareCurrentInfo,
    ],
];
const PARTS: [Pane; 3] = [Pane::Graph, Pane::Files, Pane::Info];

pub(crate) struct Comparison {
    target: Target,
    panels: [LogPanel; 2],
    active: usize,
    scope: Scope,
    loading: Load<Result<[Range; 2]>>,
    status: Option<String>,
    selected: [Option<(usize, String)>; 2],
    close: Rect,
    verify_current: bool,
    initial_focus: bool,
}

impl Comparison {
    pub(crate) fn new(target: Target, context: &Context, icons: IconMode) -> Self {
        let mut comparison = Self {
            target,
            panels: std::array::from_fn(|_| LogPanel {
                icons,
                show_refs: false,
                ..Default::default()
            }),
            active: 0,
            scope: Scope::new(context),
            loading: Load::default(),
            status: None,
            selected: Default::default(),
            close: Rect::default(),
            verify_current: true,
            initial_focus: true,
        };
        comparison.refresh(context);
        comparison
    }

    fn context(&self, context: &Context) -> Context {
        let mut scoped = context.clone();
        scoped
            .scope
            .retain(|root| context.repos[*root].path == self.target.repository);
        scoped
    }

    pub(crate) fn refresh(&mut self, context: &Context) {
        let same_scope = self.scope.matches(context);
        self.scope = Scope::new(context);
        self.status = Some("Loading comparison…".into());
        self.loading.request();
        for (panel, selected) in self.panels.iter_mut().zip(&mut self.selected) {
            if same_scope {
                *selected = panel.history_selection().or_else(|| selected.take());
            } else {
                *selected = None;
            }
            panel.enable_background_loading();
            panel.suspend_history();
            panel.visible.clear();
            panel.hist = History::default();
            panel.graph = Graph::default();
            panel.request_preview(Duration::ZERO);
            panel.hits.clear();
            panel.panes.begin();
            panel.cancel_key_sequence();
        }
    }

    pub(crate) fn pending(&self) -> bool {
        self.loading.pending() || self.panels.iter().any(LogPanel::history_pending)
    }

    pub(crate) fn preview_wakeup(&self) -> Option<Duration> {
        self.panels
            .iter()
            .filter_map(LogPanel::preview_wakeup)
            .min()
    }

    pub(crate) fn poll(&mut self, context: &Context) -> Result<bool> {
        let mut changed = false;
        if !self.scope.matches(context) {
            self.refresh(context);
            changed = true;
        }
        if let Some(result) = self.loading.take_result() {
            match result.and_then(|ranges| ranges) {
                Ok(ranges) => {
                    self.verify_current = false;
                    let scoped = self.context(context);
                    for ((panel, range), selected) in
                        self.panels.iter_mut().zip(ranges).zip(&mut self.selected)
                    {
                        panel.filter.comparison = Some(range);
                        panel.reload_history(&scoped, selected.take())?;
                    }
                    self.status = None;
                }
                Err(error) => self.status = Some(format!("{error:#} · F5 retry")),
            }
            changed = true;
        }
        if self.loading.needs_start() {
            let target = self.target.clone();
            let context = context.clone();
            let verify_current = self.verify_current;
            if let Err(error) = self.loading.start("igit-compare-refs", move |_| {
                let ranges = target.read(&context)?;
                if verify_current {
                    ensure!(
                        git(&target.repository, &["symbolic-ref", "-q", "HEAD"])
                            .ok()
                            .as_deref()
                            == Some(target.current.as_str()),
                        "Current branch changed; return to Branch and refresh before comparing"
                    );
                }
                Ok(ranges)
            }) {
                self.status = Some(format!("Could not load comparison: {error} · F5 retry"));
                return Err(error);
            }
        }
        let scoped = self.context(context);
        for panel in &mut self.panels {
            if self.status.is_none() {
                changed |= panel.poll_history(&scoped)?;
            } else {
                panel.drain_suspended_history();
            }
            changed |= panel.poll_preview(&scoped)?;
        }
        if self.status.is_none() {
            changed |= self.panels[self.active].poll_more(&scoped)?;
        }
        if self.initial_focus
            && self.status.is_none()
            && !self.panels.iter().any(LogPanel::history_pending)
        {
            if self.panels[0].visible.is_empty() && !self.panels[1].visible.is_empty() {
                self.active = 1;
                changed = true;
            }
            self.initial_focus = false;
        }
        Ok(changed)
    }

    pub(crate) fn active(&self) -> &LogPanel {
        &self.panels[self.active]
    }

    pub(crate) fn active_mut(&mut self) -> &mut LogPanel {
        &mut self.panels[self.active]
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        let part = PARTS
            .iter()
            .position(|pane| *pane == self.active().focus)
            .unwrap_or(0);
        PANES[self.active][part]
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        self.initial_focus = false;
        for (index, panes) in PANES.iter().enumerate() {
            if let Some(part) = panes.iter().position(|pane| *pane == id) {
                self.active_mut().cancel_key_sequence();
                self.active = index;
                self.active_mut().focus_pane(PARTS[part].id());
                break;
            }
        }
    }

    pub(crate) fn wants_close(&self, event: &Event) -> bool {
        (self.status.is_some()
            || !self.active().editing()
                && self.active().selected_text().is_none()
                && self
                    .active()
                    .focused_search()
                    .is_none_or(|prompt| !prompt.visible()))
            && (matches!(event, Event::Key(key) if key.kind != crossterm::event::KeyEventKind::Release
                && key.code == K::Esc && key.modifiers.is_empty())
                || matches!(event, Event::Mouse(mouse) if mouse.kind == M::Down(MouseButton::Left)
                    && inside(self.close, mouse.column, mouse.row)))
    }

    pub(crate) fn event(&mut self, event: &Event, context: &Context, panes: &PaneLayout) -> Action {
        if !self.scope.matches(context) {
            self.refresh(context);
        }
        if self.status.is_some() {
            return Action::None;
        }
        if !self.active().editing() {
            if let Event::Mouse(mouse) = event {
                if matches!(mouse.kind, M::Down(_) | M::ScrollDown | M::ScrollUp) {
                    if let Some(id) = panes.at(mouse.column, mouse.row) {
                        self.focus_pane(id);
                    }
                }
                if mouse.kind == M::Down(MouseButton::Left)
                    && self.active().hits.iter().any(|(area, hit)| {
                        matches!(hit, Hit::Refresh) && inside(*area, mouse.column, mouse.row)
                    })
                {
                    self.refresh(context);
                    return Action::None;
                }
            }
            if let Event::Key(key) = event {
                if key.modifiers.is_empty() && matches!(key.code, K::Char('[' | ']')) {
                    self.active_mut().cancel_key_sequence();
                    self.active = 1 - self.active;
                    return Action::None;
                }
            }
        }
        let scoped = self.context(context);
        self.active_mut().event(event, &scoped)
    }

    pub(crate) fn draw(
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
        self.close = Rect::default();
        if area.is_empty() {
            return;
        }
        let title = format!(
            " Compare: {} and {} · {}",
            filters::short_ref(&self.target.target),
            filters::short_ref(&self.target.current),
            context
                .repos
                .iter()
                .find(|repo| repo.path == self.target.repository)
                .map_or("Repository unavailable", |repo| repo.name.as_str())
        );
        self.close = Rect::new(area.right().saturating_sub(5), area.y, 5.min(area.width), 1);
        frame.render_widget(
            Paragraph::new(clipped(&title, usize::from(area.width.saturating_sub(5))))
                .style(fg(INK).bg(crate::ui::idea::TAG).bold()),
            Rect::new(area.x, area.y, area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(" Esc ").style(fg(BLUE).bg(crate::ui::idea::TAG)),
            self.close,
        );
        let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
        if let Some(status) = &self.status {
            frame.render_widget(Paragraph::new(status.as_str()).style(fg(DIM)), body);
            return;
        }
        let rows = panes.divide(
            SplitId::CompareRows,
            body,
            Axis::Height,
            body.height / 2,
            [4, 4],
        );
        let columns = panes.divide(
            SplitId::CompareColumns,
            body,
            Axis::Width,
            (u32::from(body.width) * 70 / 100) as u16,
            [24, 12],
        );
        let focused = self.focused_pane();
        let scoped = self.context(context);
        for (index, row) in rows.into_iter().enumerate() {
            let panel = &mut self.panels[index];
            panel.bounds = body;
            panel.hits.clear();
            panel.panes.begin();
            panel.panes.active = panes.active && self.active == index;
            panel.graph_height = 0;
            let graph = if panel.show_info {
                row.intersection(columns[0])
            } else {
                row
            };
            let inspector = row.intersection(columns[1]);
            let details = if index == 0 {
                SplitId::CompareTargetDetails
            } else {
                SplitId::CompareCurrentDetails
            };
            let [files, info] = if panel.show_info {
                panes.divide(
                    details,
                    inspector,
                    Axis::Height,
                    inspector.height * 3 / 5,
                    [2, 2],
                )
            } else {
                [Rect::default(); 2]
            };
            for ((part, id), mut outer) in PARTS
                .into_iter()
                .zip(PANES[index])
                .zip([graph, files, info])
            {
                if !panel.pane_visible(part) {
                    continue;
                }
                if panes.maximized {
                    outer = if focused == id { body } else { Rect::default() };
                }
                panes.add(id, outer);
                panel.panes.add(part.id(), outer);
                if outer.is_empty() {
                    continue;
                }
                let inner = pane_frame(frame, outer, panes.active && focused == id);
                match part {
                    Pane::Graph => panel.draw_graph(frame, inner, &scoped),
                    Pane::Files => panel.draw_files(frame, inner, &scoped),
                    Pane::Info => panel.draw_info(frame, inner, &scoped),
                    Pane::Refs => (),
                }
                if panel.rev().is_none()
                    && matches!(part, Pane::Files | Pane::Info)
                    && !inner.is_empty()
                {
                    let text = if part == Pane::Files {
                        "Select commit to view changes"
                    } else {
                        "Commit details"
                    };
                    frame.render_widget(
                        Paragraph::new(clipped(text, inner.width.into()))
                            .centered()
                            .style(fg(DIM)),
                        Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
                    );
                }
            }
            if panel
                .panes
                .rect(PaneId::Info)
                .is_none_or(|rect| rect.is_empty())
            {
                panel.info_selection.reset();
                panel.info_click = None;
            }
            if !panes.maximized {
                compact_split_handle(frame, panes, details, files, info);
            }
        }
        if !panes.maximized {
            compact_split_handle(
                frame,
                panes,
                SplitId::CompareRows,
                rows[0].intersection(columns[0]),
                rows[1].intersection(columns[0]),
            );
        }
        self.active_mut().draw_overlays(frame, body, &scoped);
    }
}

#[cfg(test)]
mod tests;
