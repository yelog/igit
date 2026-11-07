use super::Graph;
use crate::ui::icons::IconMode;
use std::ops::Range;

#[derive(Clone, Copy)]
pub(in crate::log) struct Window {
    pub(in crate::log) first: usize,
    count: usize,
    clipped: bool,
}

impl Window {
    fn rail_width(count: usize) -> usize {
        // One column per rail and one separator only *between* adjacent rails.
        (2 * count).saturating_sub(1)
    }

    pub(in crate::log) fn width(self) -> usize {
        Self::rail_width(self.count) + 2 * usize::from(self.clipped)
    }
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::log) enum NodeGlyphs {
    Ascii,
    Unicode,
    Connected,
}

impl NodeGlyphs {
    pub(in crate::log) fn for_icons(self, icons: IconMode) -> Self {
        match icons {
            IconMode::NerdFont => self,
            IconMode::Unicode => Self::Unicode,
            IconMode::Ascii => Self::Ascii,
        }
    }

    fn edges(self) -> [(char, char); 2] {
        // Each side distinguishes an offscreen commit from continuing rails.
        match self {
            Self::Ascii => [('[', '<'), (']', '>')],
            _ => [('◀', '‹'), ('▶', '›')],
        }
    }

    pub(in crate::log) fn is_continuation(self, ch: char) -> bool {
        self.edges().iter().any(|&(_, rail)| rail == ch)
    }

    pub(in crate::log) fn detect() -> Self {
        let env = |key| std::env::var(key).unwrap_or_default();
        Self::for_terminal(
            &env("IGIT_GRAPH_GLYPHS"),
            &env("TERM"),
            &env("TERM_PROGRAM"),
            &env("TERM_PROGRAM_VERSION"),
        )
    }

    fn for_terminal(setting: &str, term: &str, program: &str, version: &str) -> Self {
        match setting {
            "unicode" => return Self::Unicode,
            "connected" => return Self::Connected,
            _ => {}
        }
        // Multiplexers can be reattached from a different terminal. Do not
        // infer glyph support from a stale outer session's environment.
        if term.starts_with("screen") || term.starts_with("tmux") || program == "tmux" {
            return Self::Unicode;
        }
        if term == "xterm-ghostty" || program == "ghostty" {
            return Self::Connected;
        }
        if term == "xterm-kitty" || program == "kitty" {
            if program == "kitty" && !version.is_empty() {
                let mut parts = version
                    .trim_start_matches('v')
                    .split(['.', '-', '+'])
                    .map_while(|s| s.parse::<u32>().ok());
                let reported = std::array::from_fn(|_| parts.next().unwrap_or(0));
                if reported < [0, 36, 2] {
                    return Self::Unicode;
                }
            }
            return Self::Connected;
        }
        Self::Unicode
    }

    fn node(self, dirs: u8, head: bool) -> char {
        if self == Self::Ascii {
            return if head { '@' } else { '*' };
        }
        if self == Self::Unicode {
            return if head { '◉' } else { '●' };
        }
        // Kitty branch drawing symbols: each even codepoint is a filled
        // circle with these exact ports; its successor is the hollow form.
        // https://github.com/kovidgoyal/kitty/blob/v0.36.2/kitty/fonts/box_drawing.py#L1334-L1365
        // Index bits: up=1, down=2, left=4, right=8.
        const SOLID: [u16; 16] = [
            0xf5ee, 0xf5f8, 0xf5f6, 0xf5fa, 0xf5f2, 0xf602, 0xf5fe, 0xf606, 0xf5f0, 0xf600, 0xf5fc,
            0xf604, 0xf5f4, 0xf60a, 0xf608, 0xf60c,
        ];
        char::from_u32(u32::from(SOLID[usize::from(dirs)]) + u32::from(head)).unwrap()
    }
}

#[derive(Clone, Copy, Default)]
struct Cell {
    dirs: u8,
    upper: u8,
    lower: u8,
    slopes: u8,
    color: usize,
    edge: Option<usize>,
    crossing: bool,
    shortcut: Option<(usize, bool)>,
}

impl Cell {
    fn add(&mut self, edge: usize, dirs: u8, color: usize, up: bool, slope: u8) {
        self.crossing |= self.edge.is_some_and(|old| old != edge);
        if self.dirs & (UP | DOWN) == 0 {
            self.color = color;
        }
        self.dirs |= dirs;
        self.slopes |= slope;
        if up {
            self.upper |= dirs;
        } else {
            self.lower |= dirs;
        }
        self.edge = Some(edge);
    }

    fn glyph(self, glyphs: NodeGlyphs) -> char {
        if let Some((_, up)) = self.shortcut {
            return match (glyphs, up) {
                (NodeGlyphs::Ascii, true) => '^',
                (NodeGlyphs::Ascii, false) => 'v',
                (_, true) => '↑',
                (_, false) => '↓',
            };
        }
        // Upper and lower half-row paths can share a cell without crossing.
        // Preserve the two diagonal paths rather than turning them into a '+'.
        let diagonal = match (self.upper, self.lower) {
            (4, 8) | (9, 6) => Some(false),
            (8, 4) | (5, 10) => Some(true),
            _ => None,
        };
        if let Some(rising) = diagonal {
            return match (glyphs, rising, self.dirs == 15) {
                (NodeGlyphs::Ascii, true, _) => '/',
                (NodeGlyphs::Ascii, false, _) => '\\',
                (NodeGlyphs::Connected, true, true) => '\u{1fba8}',
                (NodeGlyphs::Connected, false, true) => '\u{1fba9}',
                (_, true, _) => '╱',
                (_, false, _) => '╲',
            };
        }
        if self.crossing {
            // Only a commit dot joins different edges. Do not invent junctions
            // at rasterized crossings between independent parent connections.
            return match (glyphs, self.slopes) {
                (NodeGlyphs::Ascii, 1) => '/',
                (NodeGlyphs::Ascii, 2) => '\\',
                (NodeGlyphs::Ascii, _) => 'x',
                (_, 1) => '╱',
                (_, 2) => '╲',
                (_, _) => '╳',
            };
        }
        if glyphs == NodeGlyphs::Ascii {
            return match self.dirs {
                0 => ' ',
                1..=3 => '|',
                4 | 8 | 12 => '-',
                5 | 10 => '/',
                6 | 9 => '\\',
                _ => '+',
            };
        }
        if glyphs == NodeGlyphs::Connected {
            // Standard Unicode diagonal mid-lines, drawn natively by the same
            // terminals as connected dots. Endpoints meet at cell boundaries.
            match self.dirs {
                5 => return '\u{1fba0}',  // upper center to middle left
                9 => return '\u{1fba1}',  // upper center to middle right
                6 => return '\u{1fba2}',  // middle left to lower center
                10 => return '\u{1fba3}', // lower center to middle right
                _ => (),
            }
        }
        match self.dirs {
            0 => ' ',
            1 => '╵',
            2 => '╷',
            3 => '│',
            4 => '╴',
            8 => '╶',
            12 => '─',
            5 => '╯',
            6 => '╮',
            9 => '╰',
            10 => '╭',
            _ => match self.slopes {
                1 => '╱',
                2 => '╲',
                _ => '╳',
            },
        }
    }
}

impl Graph {
    #[cfg(test)]
    pub(in crate::log) fn window(
        &self,
        rows: Range<usize>,
        selected: usize,
        budget: usize,
        previous: usize,
    ) -> Window {
        self.window_with_edges(rows, selected, budget, previous, true)
    }

    // Pan the entire frame only when the selected node leaves the old window.
    pub(in crate::log) fn window_with_edges(
        &self,
        rows: Range<usize>,
        selected: usize,
        budget: usize,
        previous: usize,
        long_edges: bool,
    ) -> Window {
        let layout = self.rows(long_edges);
        let total = layout[rows].iter().map(|r| r.last + 1).max().unwrap_or(0);
        let count = if Window::rail_width(total) <= budget {
            total
        } else {
            budget.saturating_sub(2).div_ceil(2).max(1)
        };
        let node = layout.get(selected).map_or(0, |r| r.node.lane);
        let first = previous
            .min(node)
            .max((node + 1).saturating_sub(count))
            .min(total.saturating_sub(count));
        Window {
            first,
            count,
            clipped: count < total,
        }
    }

    #[cfg(test)]
    pub(in crate::log) fn cells(
        &self,
        index: usize,
        w: Window,
        glyphs: NodeGlyphs,
        head: bool,
    ) -> Vec<(char, usize)> {
        self.cells_with_edges(index, w, glyphs, head, true)
            .into_iter()
            .map(|(ch, color, _)| (ch, color))
            .collect()
    }

    pub(in crate::log) fn cells_with_edges(
        &self,
        index: usize,
        w: Window,
        glyphs: NodeGlyphs,
        head: bool,
        long_edges: bool,
    ) -> Vec<(char, usize, Option<usize>)> {
        let row = &self.rows(long_edges)[index];
        let origin = 2 * w.first;
        let node_x = 2 * row.node.lane;
        let mut node_dirs = 0;
        let mut cells = vec![Cell::default(); Window::rail_width(w.count)];
        for segment in &row.segments {
            let edge = self.edges[segment.edge];
            let from = 2 * segment.from;
            // Both halves of a connection meet at exactly the same boundary
            // column; moving/scrolling a viewport never changes this geometry.
            let boundary = segment.from + segment.to;
            let slope = if boundary == from {
                0
            } else if (boundary > from) == segment.up {
                1
            } else {
                2
            };
            let lo = from.min(boundary);
            let hi = from.max(boundary);
            for x in lo.max(origin)..=hi.min(origin + cells.len().saturating_sub(1)) {
                let mut dirs = 0;
                if x > lo {
                    dirs |= LEFT;
                }
                if x < hi {
                    dirs |= RIGHT;
                }
                if x == boundary {
                    dirs |= if segment.up { UP } else { DOWN };
                }
                if let Some(c) = cells.get_mut(x - origin) {
                    c.add(segment.edge, dirs, edge.color, segment.up, slope);
                }
                if x == node_x && (edge.child == index || edge.parent == index) {
                    node_dirs |= dirs;
                }
            }
            if segment.arrow {
                if let Some(c) = from.checked_sub(origin).and_then(|x| cells.get_mut(x)) {
                    c.shortcut = Some((
                        if segment.up { edge.child } else { edge.parent },
                        segment.up,
                    ));
                    c.color = edge.color;
                }
            }
        }
        let mut out = Vec::with_capacity(w.width());
        let edge = |hidden_node, hidden_rail, node_arrow, arrow| {
            if hidden_node {
                (node_arrow, row.node.color, None)
            } else if hidden_rail {
                (arrow, 0, None)
            } else {
                (' ', 0, None)
            }
        };
        let [(left_node, left_rail), (right_node, right_rail)] = glyphs.edges();
        if w.clipped {
            out.push(edge(
                row.node.lane < w.first,
                row.first < w.first,
                left_node,
                left_rail,
            ));
        }
        out.extend(cells.into_iter().enumerate().map(|(x, c)| {
            if origin + x == node_x {
                (glyphs.node(node_dirs, head), row.node.color, None)
            } else {
                (
                    c.glyph(glyphs),
                    c.color,
                    c.shortcut.map(|(target, _)| target),
                )
            }
        }));
        if w.clipped {
            let end = w.first + w.count;
            out.push(edge(
                row.node.lane >= end,
                row.last >= end,
                right_node,
                right_rail,
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::NodeGlyphs::{self, Connected, Unicode};

    #[test]
    fn private_glyphs_require_a_known_terminal_or_an_explicit_override() {
        for (setting, term, program, version, want) in [
            ("", "xterm-kitty", "", "", Connected),
            ("", "xterm-256color", "kitty", "0.36.1", Unicode),
            ("", "xterm-256color", "kitty", "0.36.2-dev", Connected),
            ("", "xterm-256color", "ghostty", "1.0.0", Connected),
            ("", "xterm-256color", "Apple_Terminal", "", Unicode),
            ("", "tmux-256color", "ghostty", "1.0.0", Unicode),
            ("connected", "tmux-256color", "tmux", "", Connected),
            ("unicode", "xterm-kitty", "", "", Unicode),
        ] {
            assert_eq!(
                NodeGlyphs::for_terminal(setting, term, program, version),
                want
            );
        }
    }
}
