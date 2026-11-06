use super::*;
use crate::ui::panes::{neighbor, window_keys, Axis, Direction, PaneId, PaneLayout, WINDOW_KEYS};
use crossterm::event::MouseEvent;

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
    fn layout(&self, tab: Tab) -> &PaneLayout {
        match tab {
            Commit => &self.commit.panes,
            Log => &self.log.panes,
            Push => &self.push.panes,
            Conflicts => &self.merge.panes,
            Branch => &self.branch.panes,
            Stash => &self.stash.panes,
        }
    }
    pub(super) fn layout_mut(&mut self, tab: Tab) -> &mut PaneLayout {
        match tab {
            Commit => &mut self.commit.panes,
            Log => &mut self.log.panes,
            Push => &mut self.push.panes,
            Conflicts => &mut self.merge.panes,
            Branch => &mut self.branch.panes,
            Stash => &mut self.stash.panes,
        }
    }
    fn focus_id(&self) -> PaneId {
        match self.tab {
            Commit => self.commit.focused_pane(),
            Log => self.log.focused_pane(),
            Push => self.push.focused_pane(),
            Conflicts => self.merge.focused_pane(),
            Branch => PaneId::Branches,
            Stash => self.stash.focused_pane(),
        }
    }
    fn focus_pane(&mut self, id: PaneId) {
        match self.tab {
            Commit => self.commit.focus_pane(id),
            Log => self.log.focus_pane(id),
            Push => self.push.focus_pane(id),
            Conflicts => self.merge.focus_pane(id),
            Branch => (),
            Stash => self.stash.focus_pane(id),
        }
    }

    fn reachable_panes(&self) -> &[(PaneId, Rect)] {
        self.layout(self.tab).nodes()
    }
    fn cycle_pane(&mut self, back: bool) -> bool {
        let nodes = self.reachable_panes();
        let Some(index) = nodes.iter().position(|(p, _)| *p == self.focus_id()) else {
            return false;
        };
        let len = nodes.len();
        let next = nodes[(index + if back { len - 1 } else { 1 }) % len].0;
        self.cancel_key_sequences();
        self.focus_pane(next);
        true
    }

    fn window_mask(&self) -> u16 {
        let nodes = self.reachable_panes();
        let id = self.focus_id();
        let layout = self.layout(self.tab);
        let Some(r) = layout.rect(id) else { return 0 };
        let maximized = self.pane_is_maximized();
        let resizable = |axis| !maximized && layout.can_resize(r, axis);
        WINDOW_KEYS
            .iter()
            .enumerate()
            .fold(0, |mask, (i, (key, _))| {
                let key = key.chars().last().unwrap();
                let available = if let Some(d) = direction(key) {
                    neighbor(nodes, id, d).is_some()
                } else if let Some((axis, _)) = size_change(key) {
                    resizable(axis)
                } else {
                    match key {
                        'w' => nodes.len() > 1,
                        'c' => self.tab == Log && self.log.can_close_tab(),
                        '=' => !maximized,
                        'f' => self.tab != Log,
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
        if let Some(next) = neighbor(self.reachable_panes(), self.focus_id(), direction) {
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
        if self.pane_is_maximized() {
            self.kitty.resize(direction);
            return;
        }
        let tab = self.tab;
        let moved = self.layout(tab).rect(self.focus_id()).and_then(|rect| {
            self.layout_mut(tab)
                .move_divider(rect, direction, kitty::RESIZE_STEP)
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

    pub(super) fn pane_is_maximized(&self) -> bool {
        self.tab != Log && self.pane_maximized
    }

    pub(super) fn toggle_pane_maximized(&mut self) {
        if self.tab != Log {
            self.set_pane_maximized(!self.pane_maximized);
        }
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
            self.toggle_pane_maximized();
        }
    }

    pub(super) fn run_window_key(&mut self, key: char) {
        let count = i32::from(self.window_prefix.and_then(|p| p.count).unwrap_or(1));
        self.cancel_key_sequences();
        let tab = self.tab;
        let id = self.focus_id();
        if let Some(dir) = direction(key) {
            if let Some(next) = neighbor(self.reachable_panes(), id, dir) {
                self.focus_pane(next);
            }
        } else if let Some((axis, step)) = size_change(key) {
            if self.pane_is_maximized() {
                return;
            }
            if let Some(rect) = self.layout(tab).rect(id) {
                self.layout_mut(tab).resize(rect, axis, count * step);
            }
        } else {
            match key {
                '=' if !self.pane_is_maximized() => {
                    self.layout_mut(tab).reset();
                }
                'w' => {
                    self.cycle_pane(false);
                }
                'f' => self.toggle_pane_maximized(),
                'c' if tab == Log => {
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
        self.layout_mut(self.tab).cancel_drag();
    }

    /// Capture the gesture where it starts, including a release in a different
    /// pane. A text drag that only crosses a divider can never start resizing.
    pub(super) fn resize_mouse(&mut self, m: &MouseEvent) -> bool {
        if self.pane_is_maximized() {
            return false;
        }
        if matches!(m.kind, MouseEventKind::Down(_)) {
            self.cancel_drags();
        } else if self.layout(self.tab).dragging() {
            self.layout_mut(self.tab).mouse(m);
            return true;
        }
        if m.kind != MouseEventKind::Down(MouseButton::Left) || self.panel().editing() {
            return false;
        }
        self.layout_mut(self.tab).mouse(m)
    }
}
