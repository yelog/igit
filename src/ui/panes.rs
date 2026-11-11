//! Geometry of the panes drawn in one surface. Size preferences outlive a frame;
//! hit targets and directional neighbours always come from the current layout.
use super::{inside, ACCENT, BG, FG, LINE};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind as M};
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, Borders},
    Frame,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneId {
    Stashes,
    BranchRepos,
    Branches,
    CompareTargetGraph,
    CompareTargetFiles,
    CompareTargetInfo,
    CompareCurrentGraph,
    CompareCurrentFiles,
    CompareCurrentInfo,
    Changes,
    Message,
    Preview,
    Refs,
    Graph,
    Files,
    Info,
    Outgoing,
    PushFiles,
    PushDetails,
    Conflicts,
    Current,
    Result,
    Incoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SplitId {
    StashFiles,
    BranchRepos,
    CompareRows,
    CompareColumns,
    CompareTargetDetails,
    CompareCurrentDetails,
    CommitPreview,
    CommitMessage,
    LogRefs,
    LogInspector,
    LogHistory,
    LogDetails,
    PushColumns,
    PushDetails,
    MergeCurrent,
    MergeIncoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    Width,
    Height,
}

impl Axis {
    fn interval(self, rect: Rect) -> (u16, u16) {
        match self {
            Self::Width => (rect.x, rect.right()),
            Self::Height => (rect.y, rect.bottom()),
        }
    }

    fn pointer(self, m: &MouseEvent) -> u16 {
        match self {
            Self::Width => m.column,
            Self::Height => m.row,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Width => Self::Height,
            Self::Height => Self::Width,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Left,
    Down,
    Up,
    Right,
}

// Used by both executable help and the Ctrl-W suffix picker. The last
// character of each binding is its suffix; the mask only lists applicable ones.
pub(crate) const WINDOW_KEYS: &[(&str, &str)] = &[
    ("Ctrl-w h", "Focus left pane"),
    ("Ctrl-w j", "Focus pane below"),
    ("Ctrl-w k", "Focus pane above"),
    ("Ctrl-w l", "Focus right pane"),
    ("Ctrl-w >", "Increase pane width"),
    ("Ctrl-w <", "Decrease pane width"),
    ("Ctrl-w +", "Increase pane height"),
    ("Ctrl-w -", "Decrease pane height"),
    ("Ctrl-w =", "Reset pane sizes"),
    ("Ctrl-w w", "Focus next pane"),
    ("Ctrl-w c", "Close current Log tab"),
    ("Ctrl-w f", "Maximize / restore focused pane"),
];

pub(crate) fn window_keys(
    mask: u16,
) -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    WINDOW_KEYS
        .iter()
        .enumerate()
        .filter_map(move |(i, b)| (mask & (1 << i) != 0).then_some(b))
}

/// Prefer the nearest aligned edge, then the pane sharing the most of that
/// edge. Ties retain registration order (e.g. Files above Info).
pub(crate) fn neighbor(nodes: &[(PaneId, Rect)], from: PaneId, dir: Direction) -> Option<PaneId> {
    let (_, origin) = nodes.iter().find(|(id, r)| *id == from && !r.is_empty())?;
    let (axis, forward) = match dir {
        Direction::Left => (Axis::Width, false),
        Direction::Right => (Axis::Width, true),
        Direction::Up => (Axis::Height, false),
        Direction::Down => (Axis::Height, true),
    };
    let (start, end) = axis.interval(*origin);
    let (near, far) = axis.other().interval(*origin);
    nodes
        .iter()
        .filter_map(|&(id, rect)| {
            if rect.is_empty() || id == from {
                return None;
            }
            let (a, b) = axis.interval(rect);
            let distance = if forward {
                a.checked_sub(end)?
            } else {
                start.checked_sub(b)?
            };
            let (c, d) = axis.other().interval(rect);
            let overlap = far.min(d).saturating_sub(near.max(c));
            (overlap > 0).then_some(((distance, std::cmp::Reverse(overlap)), id))
        })
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, id)| id)
}

#[derive(Clone, Copy)]
struct Divider {
    id: SplitId,
    axis: Axis,
    sides: [Rect; 2],
    handle: Rect,
    first: u16,
    min: u16,
    max: u16,
}

#[derive(Clone, Copy)]
struct Drag {
    id: SplitId,
    pointer: u16,
    first: u16,
    moved: bool,
}

pub(crate) struct PaneLayout {
    pub active: bool,
    pub maximized: bool,
    nodes: Vec<(PaneId, Rect)>,
    splits: Vec<Divider>,
    sizes: BTreeMap<SplitId, u16>,
    drag: Option<Drag>,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            active: true,
            maximized: false,
            nodes: vec![],
            splits: vec![],
            sizes: BTreeMap::new(),
            drag: None,
        }
    }
}

impl PaneLayout {
    pub fn begin(&mut self) {
        self.nodes.clear();
        self.splits.clear();
    }

    pub fn add(&mut self, id: PaneId, rect: Rect) {
        self.nodes.push((id, rect));
    }
    pub fn nodes(&self) -> &[(PaneId, Rect)] {
        &self.nodes
    }
    pub fn rect(&self, id: PaneId) -> Option<Rect> {
        self.nodes.iter().find(|(p, _)| *p == id).map(|(_, r)| *r)
    }
    pub fn at(&self, x: u16, y: u16) -> Option<PaneId> {
        self.nodes
            .iter()
            .find_map(|&(p, r)| inside(r, x, y).then_some(p))
    }

    /// Divide into contiguous rectangles. The two touching borders make one
    /// drag handle. Compact layouts can restrict it to their actual border.
    /// If neither side has spare space use the responsive
    /// default without exposing a divider that cannot move.
    pub fn divide(
        &mut self,
        id: SplitId,
        area: Rect,
        axis: Axis,
        default_first: u16,
        min: [u16; 2],
    ) -> [Rect; 2] {
        let (start, end) = axis.interval(area);
        let total = end - start;
        let max = total.saturating_sub(min[1]);
        let first = if total < min[0].saturating_add(min[1]) {
            default_first.min(total)
        } else {
            self.sizes
                .get(&id)
                .copied()
                .unwrap_or(default_first)
                .clamp(min[0], max)
        };
        let mut sides = [area; 2];
        let mut handle = area;
        match axis {
            Axis::Width => {
                sides[0].width = first;
                sides[1].x += first;
                sides[1].width -= first;
                handle.x = sides[0].right().saturating_sub(1);
                handle.width = (area.right() - handle.x).min(2);
            }
            Axis::Height => {
                sides[0].height = first;
                sides[1].y += first;
                sides[1].height -= first;
                handle.y = sides[0].bottom().saturating_sub(1);
                handle.height = (area.bottom() - handle.y).min(2);
            }
        }
        if sides.iter().all(|side| !side.is_empty()) {
            // Keep fixed visible boundaries for smart resize: reaching one must
            // hand off instead of selecting a different, outer divider.
            let (min, max) = if min[0] < max {
                (min[0], max)
            } else {
                (first, first)
            };
            self.splits.push(Divider {
                id,
                axis,
                sides,
                handle,
                first,
                min,
                max,
            });
        }
        sides
    }

    pub fn handle(&mut self, id: SplitId, rect: Rect) {
        if let Some(s) = self.splits.iter_mut().find(|s| s.id == id) {
            s.handle = rect;
        }
    }

    fn split_for(&self, rect: Rect, axis: Axis) -> Option<(Divider, i32)> {
        if rect.is_empty() {
            return None;
        }
        // Nested dividers are registered after their parent.
        self.splits
            .iter()
            .rev()
            .filter(|s| s.axis == axis && s.min < s.max)
            .find_map(|s| {
                s.sides
                    .iter()
                    .position(|&side| side.intersection(rect) == rect)
                    .map(|side| (*s, if side == 0 { 1 } else { -1 }))
            })
    }

    pub fn can_resize(&self, rect: Rect, axis: Axis) -> bool {
        self.split_for(rect, axis).is_some()
    }

    /// Move the nearest right/bottom divider, or the left/top one at an edge.
    /// None means absent; Some(false) means present but clamped. A clamped
    /// divider hands off to Kitty instead of unexpectedly moving an ancestor.
    pub fn move_divider(&mut self, rect: Rect, direction: Direction, amount: u16) -> Option<bool> {
        if rect.is_empty() {
            return None;
        }
        let (axis, sign) = match direction {
            Direction::Left => (Axis::Width, -1),
            Direction::Right => (Axis::Width, 1),
            Direction::Up => (Axis::Height, -1),
            Direction::Down => (Axis::Height, 1),
        };
        let split = [0, 1].into_iter().find_map(|side| {
            self.splits
                .iter()
                .rev()
                .find(|s| s.axis == axis && s.sides[side].intersection(rect) == rect)
                .copied()
        })?;
        let current = self
            .sizes
            .get(&split.id)
            .copied()
            .unwrap_or(split.first)
            .clamp(split.min, split.max);
        let next = (i32::from(current) + sign * i32::from(amount))
            .clamp(i32::from(split.min), i32::from(split.max));
        if next == i32::from(current) {
            return Some(false);
        }
        self.set(split, next);
        Some(true)
    }

    pub fn resize(&mut self, rect: Rect, axis: Axis, step: i32) -> bool {
        let Some((split, sign)) = self.split_for(rect, axis) else {
            return false;
        };
        let current = self
            .sizes
            .get(&split.id)
            .copied()
            .unwrap_or(split.first)
            .clamp(split.min, split.max);
        let value =
            (i32::from(current) + sign * step).clamp(i32::from(split.min), i32::from(split.max));
        if value != i32::from(current) {
            self.set(split, value);
        }
        true
    }

    fn set(&mut self, s: Divider, value: i32) {
        let value = value.clamp(i32::from(s.min), i32::from(s.max)) as u16;
        self.sizes.insert(s.id, value);
    }

    pub fn reset(&mut self) {
        self.sizes.clear();
        self.cancel_drag();
    }
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn mouse(&mut self, m: &MouseEvent) -> bool {
        match m.kind {
            M::Down(MouseButton::Left) => {
                self.drag = self
                    .splits
                    .iter()
                    .rev()
                    .find(|s| s.min < s.max && inside(s.handle, m.column, m.row))
                    .map(|s| Drag {
                        id: s.id,
                        pointer: s.axis.pointer(m),
                        first: s.first,
                        moved: false,
                    });
                self.drag.is_some()
            }
            M::Drag(MouseButton::Left) | M::Up(MouseButton::Left) => {
                let Some(drag) = self.drag else { return false };
                if let Some(s) = self.splits.iter().find(|s| s.id == drag.id).copied() {
                    let delta = i32::from(s.axis.pointer(m)) - i32::from(drag.pointer);
                    let value = i32::from(drag.first) + delta;
                    // A press/release in place must not pin an automatic size.
                    if delta != 0 || drag.moved {
                        self.set(s, value);
                        self.drag = Some(Drag {
                            moved: true,
                            ..drag
                        });
                    }
                }
                if matches!(m.kind, M::Up(_)) {
                    self.cancel_drag();
                }
                true
            }
            _ => false,
        }
    }
}

pub(crate) fn pane_frame(f: &mut Frame, area: Rect, focused: bool) -> Rect {
    // At the minimum terminal size, keep toolbar and data rows available.
    let edges = if area.height > 4 {
        Borders::ALL
    } else {
        Borders::LEFT | Borders::RIGHT
    };
    let block = Block::default()
        .borders(edges)
        .border_type(BorderType::Rounded)
        .style(Style::new().fg(FG).bg(BG))
        .border_style(Style::new().fg(if focused { ACCENT } else { LINE }));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_resize_moves_the_divider_in_the_same_direction_from_both_sides() {
        for (axis, negative, positive) in [
            (Axis::Width, Direction::Left, Direction::Right),
            (Axis::Height, Direction::Up, Direction::Down),
        ] {
            for side in 0..2 {
                let mut layout = PaneLayout::default();
                let draw = |layout: &mut PaneLayout| {
                    layout.begin();
                    layout.divide(
                        SplitId::LogInspector,
                        Rect::new(0, 0, 120, 120),
                        axis,
                        40,
                        [12, 24],
                    )
                };
                let rect = draw(&mut layout)[side];
                assert_eq!(layout.move_divider(rect, positive, 3), Some(true));
                // Repeated input before a new frame accumulates from the saved size.
                assert_eq!(layout.move_divider(rect, positive, 3), Some(true));
                let panes = draw(&mut layout);
                assert_eq!(axis.interval(panes[0]).1, 46);
                assert_eq!(layout.move_divider(panes[side], negative, 3), Some(true));
                assert_eq!(axis.interval(draw(&mut layout)[0]).1, 43);
            }
        }
    }

    #[test]
    fn smart_resize_keeps_a_fixed_inner_boundary_instead_of_moving_its_parent() {
        let mut layout = PaneLayout::default();
        let [main, _] = layout.divide(
            SplitId::LogInspector,
            Rect::new(0, 0, 120, 20),
            Axis::Width,
            36,
            [36, 16],
        );
        let [refs, _] = layout.divide(SplitId::LogRefs, main, Axis::Width, 12, [12, 24]);
        assert_eq!(layout.move_divider(refs, Direction::Right, 3), Some(false));
        assert!(layout.sizes.is_empty());
        assert!(!layout.mouse(&MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: 11,
            row: 4,
            modifiers: crossterm::event::KeyModifiers::NONE
        }));
    }

    #[test]
    fn smart_resize_prefers_the_right_divider_and_hands_off_at_its_limit() {
        let mut layout = PaneLayout::default();
        let draw = |layout: &mut PaneLayout| {
            layout.begin();
            let [main, inspector] = layout.divide(
                SplitId::LogInspector,
                Rect::new(0, 0, 120, 20),
                Axis::Width,
                80,
                [36, 16],
            );
            let [refs, graph] = layout.divide(SplitId::LogRefs, main, Axis::Width, 20, [12, 24]);
            [refs, graph, inspector]
        };
        let [_, graph, _] = draw(&mut layout);
        assert_eq!(layout.move_divider(graph, Direction::Right, 3), Some(true));
        let [refs, graph, inspector] = draw(&mut layout);
        assert_eq!((refs.width, graph.width, inspector.width), (20, 63, 37));
        assert_eq!(
            layout.move_divider(graph, Direction::Right, 100),
            Some(true)
        );
        let [_, graph, inspector] = draw(&mut layout);
        assert_eq!(inspector.width, 16);
        assert_eq!(layout.move_divider(graph, Direction::Right, 3), Some(false));
        assert_eq!(layout.move_divider(graph, Direction::Left, 3), Some(true));
        assert_eq!(draw(&mut layout)[2].width, 19);
        layout.begin();
        assert_eq!(layout.move_divider(graph, Direction::Left, 3), None);
        assert_eq!(
            layout.move_divider(Rect::default(), Direction::Down, 3),
            None
        );
    }

    #[test]
    fn resizing_from_a_clamped_width_has_no_hidden_steps() {
        let mut layout = PaneLayout::default();
        let draw = |layout: &mut PaneLayout, width: u16| {
            layout.begin();
            layout.divide(
                SplitId::LogRefs,
                Rect::new(4, 6, width, 8),
                Axis::Width,
                width / 3,
                [12, 24],
            )
        };
        let [left, _] = draw(&mut layout, 120);
        layout.resize(left, Axis::Width, 55);
        assert_eq!(draw(&mut layout, 120)[0].width, 95);
        let [_, right] = draw(&mut layout, 60);
        assert_eq!(right.width, 24);
        layout.resize(right, Axis::Width, 1);
        assert_eq!(draw(&mut layout, 60)[1].width, 25);
        layout.reset();
        assert_eq!(draw(&mut layout, 60)[0].width, 20);
        assert_eq!(draw(&mut layout, 120)[0].width, 40);
    }
}
