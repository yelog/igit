//! IDEA-style permanent branch order, projected into compact visible rows.
//! Parent edges keep their own identity all the way to the parent commit.
mod color;
mod layout;
mod load;
mod render;
#[cfg(test)]
mod tests;
pub(in crate::log) use color::graph_color;
pub(in crate::log) use render::{NodeGlyphs, Window as GraphWindow};

use super::Rev;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Default, Debug)]
struct Track {
    lane: usize,
    color: usize,
}

#[derive(Clone, Copy, Default)]
struct Node {
    order: usize,
    color: usize,
}

#[derive(Default)]
pub(in crate::log) struct PermanentLayout {
    nodes: HashMap<(usize, String), Node>,
}

impl PermanentLayout {
    fn new(revs: &[Rev]) -> Self {
        let refs: Vec<_> = revs.iter().collect();
        let graph = Graph::topology(&refs);
        Self {
            nodes: revs
                .iter()
                .zip(graph.nodes)
                .map(|(rev, node)| ((rev.root, rev.oid.clone()), node))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    child: usize,
    parent: usize,
    color: usize,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    edge: usize,
    from: usize,
    to: usize,
    up: bool,
    arrow: bool,
}

#[derive(Default)]
struct Row {
    node: Track,
    segments: Vec<Segment>,
    first: usize,
    last: usize,
}

#[derive(Default)]
pub(in crate::log) struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    // Long-edge visibility changes lane positions, not just the painted glyphs.
    compact: Vec<Row>,
    expanded: OnceLock<Vec<Row>>,
}

impl Graph {
    #[cfg(test)]
    pub(in crate::log) fn new<'a>(revs: impl IntoIterator<Item = &'a Rev>) -> Self {
        let revs: Vec<_> = revs.into_iter().collect();
        let mut graph = Self::topology(&revs);
        graph.project();
        graph
    }

    #[cfg(test)]
    pub(in crate::log) fn filtered(revs: &[Rev], visible: &[usize]) -> Self {
        Self::using_layout(revs, visible, &PermanentLayout::default())
    }

    pub(in crate::log) fn using_layout(
        revs: &[Rev],
        visible: &[usize],
        layout: &PermanentLayout,
    ) -> Self {
        let refs: Vec<_> = revs.iter().collect();
        let mut graph = Self::topology(&refs);
        let known: Option<Vec<_>> = revs
            .iter()
            .map(|r| layout.nodes.get(&(r.root, r.oid.clone())).copied())
            .collect();
        // Refs may move between the metadata read and the history read. Never
        // mix two incompatible DFS orders; the loaded DAG is the safe fallback.
        if let Some(nodes) = known {
            graph.nodes = nodes;
            for edge in &mut graph.edges {
                let owner = if graph.nodes[edge.child].order >= graph.nodes[edge.parent].order {
                    edge.child
                } else {
                    edge.parent
                };
                edge.color = graph.nodes[owner].color;
            }
        }
        let mut positions = vec![None; revs.len()];
        for (row, &index) in visible.iter().enumerate() {
            positions[index] = Some(row);
        }
        let mut filtered = Self {
            nodes: visible.iter().map(|&i| graph.nodes[i]).collect(),
            edges: graph
                .edges
                .into_iter()
                .filter_map(|edge| {
                    Some(Edge {
                        child: positions[edge.child]?,
                        parent: positions[edge.parent]?,
                        color: edge.color,
                    })
                })
                .collect(),
            ..Self::default()
        };
        filtered.project();
        filtered
    }

    fn topology(revs: &[&Rev]) -> Self {
        let index: HashMap<_, _> = revs
            .iter()
            .enumerate()
            .map(|(i, r)| ((r.root, r.oid.as_str()), i))
            .collect();
        let mut parents = vec![Vec::new(); revs.len()];
        let mut has_child = vec![false; revs.len()];
        for (i, r) in revs.iter().enumerate() {
            for oid in &r.parents {
                if let Some(&parent) = index.get(&(r.root, oid.as_str())) {
                    if parent > i && !parents[i].contains(&parent) {
                        parents[i].push(parent);
                        has_child[parent] = true;
                    }
                }
            }
        }
        let nodes = layout::order(revs, &parents, &has_child);
        let mut edges = Vec::new();
        for (child, parents) in parents.into_iter().enumerate() {
            for parent in parents {
                let owner = if nodes[child].order >= nodes[parent].order {
                    child
                } else {
                    parent
                };
                edges.push(Edge {
                    child,
                    parent,
                    color: nodes[owner].color,
                });
            }
        }
        Self {
            nodes,
            edges,
            ..Self::default()
        }
    }

    fn project(&mut self) {
        self.compact = layout::project(&self.nodes, &self.edges, false);
    }

    fn rows(&self, long_edges: bool) -> &[Row] {
        if long_edges {
            self.expanded
                .get_or_init(|| layout::project(&self.nodes, &self.edges, true))
        } else {
            &self.compact
        }
    }
}
