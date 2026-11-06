use super::*;
use crate::ui::panes::{neighbor, window_keys, Axis, Direction, PaneId, PaneLayout, WINDOW_KEYS};
use crossterm::event::MouseEvent;

fn owner(id: PaneId) -> Surface {
    use PaneId::*;
    match id {
        Changes | Message | Preview => Split(Commit),
        Refs | Graph | Files | Info => Split(Log),
        Outgoing | PushFiles | PushDetails => Dialog(Push),
        Conflicts | Current | Result | Incoming => Dialog(Merge),
    }
}

fn direction(key: char) -> Option<Direction> {
    Some(match key {
        'h' => Direction::Left,
        'j' => Direction::Down,
        'k' => Direction::Up,
        'l' => Direction::Right,
        _ => return None,
    })
}

fn size_change(key: char) -> Option<(Axis, i32)> {
    Some(match key {
        '>' => (Axis::Width, 1),
        '<' => (Axis::Width, -1),
        '+' => (Axis::Height, 1),
        '-' => (Axis::Height, -1),
        _ => return None,
    })
}

impl Shell {
    fn layout(&self, surface: Surface) -> &PaneLayout {
        match surface {
            Split(Commit) => &self.commit.panes,
            Split(Log) => &self.log.panes,
            Dialog(Push) => &self.push.panes,
            Dialog(Merge) => &self.merge.panes,
        }
    }
    fn layout_mut(&mut self, surface: Surface) -> &mut PaneLayout {
        match surface {
            Split(Commit) => &mut self.commit.panes,
            Split(Log) => &mut self.log.panes,
            Dialog(Push) => &mut self.push.panes,
            Dialog(Merge) => &mut self.merge.panes,
        }
    }
    fn focus_id(&self) -> PaneId {
        match self.surface() {
            Split(Commit) => self.commit.focused_pane(),
            Split(Log) => self.log.focused_pane(),
            Dialog(Push) => self.push.focused_pane(),
            Dialog(Merge) => self.merge.focused_pane(),
        }
    }
    fn focus_pane(&mut self, id: PaneId) {
        if owner(id) != self.surface() {
            self.go(owner(id));
        }
        match owner(id) {
            Split(Commit) => self.commit.focus_pane(id),
            Split(Log) => self.log.focus_pane(id),
            Dialog(Push) => self.push.focus_pane(id),
            Dialog(Merge) => self.merge.focus_pane(id),
        }
    }

    fn reachable_panes(&self) -> Vec<(PaneId, Rect)> {
        if let Some(tool) = self.tool {
            self.layout(Dialog(tool)).nodes().to_vec()
        } else {
            [Split(Commit), Split(Log)]
                .into_iter()
                .flat_map(|s| self.layout(s).nodes().iter().copied())
                .collect()
        }
    }
    fn cycle_pane(&mut self, back: bool) -> bool {
        let nodes = self.reachable_panes();
        let Some(index) = nodes.iter().position(|(p, _)| *p == self.focus_id()) else {
            return false;
        };
        let len = nodes.len();
        let next = (index + if back { len - 1 } else { 1 }) % len;
        self.cancel_key_sequences();
        self.focus_pane(nodes[next].0);
        true
    }

    fn window_mask(&self) -> u16 {
        let nodes = self.reachable_panes();
        let id = self.focus_id();
        let layout = self.layout(self.surface());
        let Some(r) = layout.rect(id) else { return 0 };
        let resizable = |axis| {
            !self.pane_maximized
                && (layout.can_resize(r, axis)
                    || self.tool.is_none() && self.work_panes.can_resize(r, axis))
        };
        WINDOW_KEYS
            .iter()
            .enumerate()
            .fold(0, |mask, (i, (key, _))| {
                let key = key.chars().last().unwrap();
                let available = if let Some(d) = direction(key) {
                    neighbor(&nodes, id, d).is_some()
                } else if let Some((axis, _)) = size_change(key) {
                    resizable(axis)
                } else {
                    match key {
                        'w' => nodes.len() > 1,
                        'c' => self.surface() == Split(Log) && self.log.can_close_tab(),
                        '=' => !self.pane_maximized,
                        'f' => true,
                        _ => false,
                    }
                };
                if available {
                    mask | (1 << i)
                } else {
                    mask
                }
            })
    }

    pub(super) fn pane_help(&self, content: &mut HelpContent) {
        for binding in window_keys(self.window_mask()) {
            content
                .sections
                .push(HelpSection::new("Panes", std::slice::from_ref(binding)));
        }
    }

    pub(super) fn smart_focus(&mut self, direction: Direction) {
        if self.pop.is_some() || self.panel().editing() || self.kitty.busy() {
            return;
        }
        self.cancel_key_sequences();
        self.cancel_drags();
        if let Some(next) = neighbor(&self.reachable_panes(), self.focus_id(), direction) {
            self.focus_pane(next);
        } else {
            self.kitty.focus(direction);
        }
    }

    pub(super) fn smart_resize(&mut self, direction: Direction) {
        if self.pop.is_some() || self.panel().editing() || self.kitty.busy() {
            return;
        }
        self.cancel_key_sequences();
        self.cancel_drags();
        if self.pane_maximized {
            self.kitty.resize(direction);
            return;
        }
        let surface = self.surface();
        let moved = self.layout(surface).rect(self.focus_id()).and_then(|rect| {
            self.layout_mut(surface)
                .move_divider(rect, direction, kitty::RESIZE_STEP)
                .or_else(|| {
                    if self.tool.is_none() {
                        self.work_panes
                            .move_divider(rect, direction, kitty::RESIZE_STEP)
                    } else {
                        None
                    }
                })
        });
        if moved != Some(true) {
            self.kitty.resize(direction);
        }
    }

    pub(super) fn set_pane_maximized(&mut self, maximized: bool) {
        self.cancel_key_sequences();
        self.cancel_drags();
        self.pane_maximized = maximized;
    }

    pub(super) fn smart_maximize(&mut self) {
        if self.pop.is_some()
            || self.panel().editing()
            || self.kitty.busy()
            || self.ctx.repos.is_empty()
        {
            return;
        }
        self.cancel_key_sequences();
        self.cancel_drags();
        if self.kitty.available() {
            self.maximize_pending = true;
            self.kitty.toggle_maximized();
        } else {
            self.set_pane_maximized(!self.pane_maximized);
        }
    }

    pub(super) fn run_window_key(&mut self, key: char) {
        let count = i32::from(self.window_prefix.and_then(|p| p.count).unwrap_or(1));
        self.cancel_key_sequences();
        let surface = self.surface();
        let id = self.focus_id();
        if let Some(dir) = direction(key) {
            if let Some(next) = neighbor(&self.reachable_panes(), id, dir) {
                self.focus_pane(next);
            }
        } else if let Some((axis, step)) = size_change(key) {
            if self.pane_maximized {
                return;
            }
            if let Some(rect) = self.layout(surface).rect(id) {
                if !self.layout_mut(surface).resize(rect, axis, count * step) && self.tool.is_none()
                {
                    self.work_panes.resize(rect, axis, count * step);
                }
            }
        } else {
            match key {
                '=' if !self.pane_maximized => {
                    if self.tool.is_some() {
                        self.layout_mut(surface).reset();
                    } else {
                        self.work_panes.reset();
                        self.commit.panes.reset();
                        self.log.panes.reset();
                    }
                }
                'w' => {
                    self.cycle_pane(false);
                }
                'f' => self.set_pane_maximized(!self.pane_maximized),
                'c' if surface == Split(Log) => {
                    if let Err(error) = self.log.close_tab(&self.ctx) {
                        self.notice(format!("Log: {error:#}"));
                    }
                }
                _ => (),
            }
        }
    }

    /// Only called in normal mode, after overlays have had first refusal.
    pub(super) fn pane_key(&mut self, key: &KeyEvent) -> bool {
        let prefix = key.code == KeyCode::Char('w') && key.modifiers == KeyModifiers::CONTROL;
        if let Some(pending) = &mut self.window_prefix {
            if prefix {
                self.run_window_key('w');
                return true;
            }
            if let KeyCode::Char(c) = key.code {
                if let Some(n) = c.to_digit(10).filter(|_| key.modifiers.is_empty()) {
                    let n = pending
                        .count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(n as u16);
                    pending.count = Some(n);
                    return true;
                }
                if key.modifiers.is_empty()
                    || key.modifiers == KeyModifiers::SHIFT
                    || key.modifiers == KeyModifiers::CONTROL && direction(c).is_some()
                {
                    self.run_window_key(c);
                    // Normal global shortcuts still switch context and cancel the prefix.
                    return !matches!(c, '?' | ':' | 'r' | 'q');
                }
            }
            self.cancel_key_sequences();
        }
        if prefix {
            self.cancel_key_sequences();
            let mask = self.window_mask();
            if mask != 0 {
                self.window_prefix = Some(WindowPrefix { mask, count: None });
                self.key_picker =
                    KeySequencePicker::pending_with_delay(Instant::now(), self.config.hint_delay);
            }
            return true;
        }
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab | KeyCode::F(6))
            && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
        {
            return self
                .cycle_pane(key.code == KeyCode::BackTab || key.modifiers == KeyModifiers::SHIFT);
        }
        false
    }

    pub(super) fn cancel_drags(&mut self) {
        self.work_panes.cancel_drag();
        for s in SHORTCUTS {
            self.layout_mut(s).cancel_drag();
        }
    }

    /// Capture the gesture where it starts, including a release in a different
    /// pane. A text drag that only crosses a divider can never start resizing.
    pub(super) fn resize_mouse(&mut self, m: &MouseEvent) -> bool {
        if self.pane_maximized {
            return false;
        }
        if matches!(m.kind, MouseEventKind::Down(_)) {
            self.cancel_drags();
        } else {
            if self.work_panes.dragging() {
                self.work_panes.mouse(m);
                return true;
            }
            for s in SHORTCUTS {
                if self.layout(s).dragging() {
                    self.layout_mut(s).mouse(m);
                    return true;
                }
            }
        }
        if m.kind != MouseEventKind::Down(MouseButton::Left) || self.panel().editing() {
            return false;
        }
        if let Some(tool) = self.tool {
            return self.layout_mut(Dialog(tool)).mouse(m);
        }
        self.work_panes.mouse(m) || self.commit.panes.mouse(m) || self.log.panes.mouse(m)
    }
}
