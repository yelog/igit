use super::*;
use crate::log::compare::{Comparison, Target};
use crate::ui::idea::{button_style, clipped};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

impl BranchPanel {
    fn comparison_target(&self) -> Option<Target> {
        if self.read_status().is_some() || self.syncing() {
            return None;
        }
        let (_, branch) = self.selection()?;
        let repository = self.repository()?;
        let snapshot = repository.state.as_ref().ok()?;
        let current = snapshot.head.as_ref()?;
        if branch.current
            || branch.reference == *current
            || branch.oid.is_empty()
            || snapshot.oid.is_none()
        {
            return None;
        }
        Some(Target {
            repository: repository.path.clone(),
            current: current.clone(),
            target: branch.reference,
        })
    }

    pub(super) fn comparison_label(&self) -> Option<String> {
        let target = self.comparison_target()?;
        Some(format!(
            "Compare with {}",
            model::short_ref(&target.current)
        ))
    }

    pub(super) fn open_comparison(&mut self, context: &Context) -> Result<Action> {
        self.compare_menu = None;
        let Some(target) = self.comparison_target() else {
            return Ok(Action::Notice(
                NoticeLevel::Warning,
                self.read_status().map(str::to_owned).unwrap_or_else(|| {
                    "Select a non-current branch; comparison requires a committed local current branch".into()
                }),
            ));
        };
        self.cancel_key_sequence();
        self.hits.clear();
        self.panes.begin();
        self.panes.cancel_drag();
        self.comparison = Some(Box::new(Comparison::new(target, context, self.icons)));
        Ok(Action::None)
    }

    pub(crate) fn comparing(&self) -> bool {
        self.comparison.is_some()
    }

    pub(crate) fn comparison_motion(&self, event: &Event) -> bool {
        self.comparison
            .as_ref()
            .is_some_and(|comparison| comparison.active().graph_motion(event))
    }

    pub(crate) fn hover_comparison(&mut self, column: u16, row: u16) -> bool {
        self.comparison
            .as_mut()
            .is_some_and(|comparison| comparison.active_mut().hover_menu_at(column, row))
    }

    pub(crate) fn close_comparison(&mut self) {
        self.comparison = None;
        self.panes.begin();
        self.panes.cancel_drag();
        self.hits.clear();
    }

    pub(crate) fn comparison_pending(&self) -> bool {
        self.comparison
            .as_ref()
            .is_some_and(|comparison| comparison.pending())
    }

    pub(crate) fn poll_comparison(&mut self, context: &Context) -> Result<bool> {
        self.comparison
            .as_mut()
            .map_or(Ok(false), |comparison| comparison.poll(context))
    }

    pub(crate) fn preview_wakeup(&self) -> Option<std::time::Duration> {
        self.comparison.as_ref()?.preview_wakeup()
    }

    pub(crate) fn path_copy_target(
        &self,
        context: &Context,
    ) -> Option<crate::ui::path_copy::Target> {
        self.comparison.as_ref()?.active().path_copy_target(context)
    }

    pub(super) fn compare_menu_event(
        &mut self,
        event: &Event,
        context: &Context,
    ) -> Result<Action> {
        match event {
            Event::Key(key) if key.modifiers.is_empty() => match key.code {
                K::Enter | K::Char('c' | ' ') => return self.open_comparison(context),
                K::Esc => self.compare_menu = None,
                _ => (),
            },
            Event::Mouse(mouse) if matches!(mouse.kind, M::Down(_)) => {
                if mouse.kind == M::Down(MouseButton::Left)
                    && matches!(self.hit_at(mouse.column, mouse.row), Some(Hit::Compare))
                {
                    return self.open_comparison(context);
                }
                self.compare_menu = None;
            }
            Event::FocusLost => self.compare_menu = None,
            _ => (),
        }
        Ok(Action::None)
    }

    pub(super) fn draw_compare_menu(&mut self, frame: &mut Frame, area: Rect) {
        let Some((column, row)) = self.compare_menu else {
            return;
        };
        let Some(label) = self.comparison_label() else {
            self.compare_menu = None;
            return;
        };
        let width = (label.width() + 4).min(usize::from(area.width)) as u16;
        let height = 3.min(area.height);
        let menu = Rect::new(
            column.clamp(area.x, area.right() - width),
            row.clamp(area.y, area.bottom() - height),
            width,
            height,
        );
        frame.render_widget(Clear, menu);
        let block = pane("Branch", true);
        let inner = block.inner(menu);
        frame.render_widget(block, menu);
        frame.render_widget(
            Paragraph::new(clipped(&format!(" {label} "), inner.width.into()))
                .style(button_style(true)),
            inner,
        );
        self.hits.push((menu, Hit::MenuSurface));
        self.hits.push((inner, Hit::Compare));
    }
}
