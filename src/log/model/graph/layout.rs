//! Layout rules follow JetBrains' GraphLayoutBuilder and
//! GraphElementComparatorByLayoutIndex; see docs/plans/2026-11-07-idea-graph-topology.md.
// Copyright 2000-2024 JetBrains s.r.o. and contributors.
// Adapted to Rust and terminal row projections for IGit. Apache-2.0; see NOTICE.txt.
use super::{color::reference_color, Edge, Node, Rev, Row, Segment, Track};
use crate::log::model::RefKind;
use std::{cmp::Ordering, collections::HashMap};

fn reference(rev: &Rev) -> Option<(u8, &str)> {
    rev.decorations()
        .map(|(kind, name)| {
            let priority = match kind {
                RefKind::Remote if matches!(name, "origin/main" | "origin/master") => 0,
                RefKind::Remote => 1,
                RefKind::Local | RefKind::Current if matches!(name, "main" | "master") => 2,
                RefKind::Local | RefKind::Current => 3,
                RefKind::Tag => 4,
                RefKind::Head => 5,
            };
            (priority, name)
        })
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| natural_cmp(a.1, b.1)))
}

// Branch names use natural ordering (topic2 before topic10), as in IDEA.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek(), b.peek()) {
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(char::is_ascii_digit)).collect();
                let y: String = std::iter::from_fn(|| b.next_if(char::is_ascii_digit)).collect();
                let xn = x.trim_start_matches('0');
                let yn = y.trim_start_matches('0');
                let cmp = xn
                    .len()
                    .cmp(&yn.len())
                    .then_with(|| xn.cmp(yn))
                    .then(x.len().cmp(&y.len()));
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            (Some(_), Some(_)) => {
                let cmp = a
                    .next()
                    .unwrap()
                    .to_lowercase()
                    .cmp(b.next().unwrap().to_lowercase());
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
        }
    }
}

pub(super) fn order(revs: &[&Rev], parents: &[Vec<usize>], has_child: &[bool]) -> Vec<Node> {
    let refs: Vec<_> = revs.iter().map(|r| reference(r)).collect();
    let mut heads: Vec<_> = (0..revs.len())
        .filter(|&i| !has_child[i] || revs[i].decorations().any(|(kind, _)| kind != RefKind::Tag))
        .collect();
    heads.sort_by(|&a, &b| match (refs[a], refs[b]) {
        (Some(a_ref), Some(b_ref)) => a_ref
            .0
            .cmp(&b_ref.0)
            .then_with(|| natural_cmp(a_ref.1, b_ref.1))
            .then(revs[a].root.cmp(&revs[b].root))
            .then(a.cmp(&b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(&b),
    });
    let mut nodes = vec![Node::default(); revs.len()];
    let mut next = 1;
    for head in heads {
        if nodes[head].order != 0 {
            continue;
        }
        let first = next;
        let main_color = refs[head].map_or(0, |(_, name)| reference_color(name));
        // An explicit stack keeps very deep histories off the call stack.
        let mut stack = vec![(head, 0)];
        nodes[head] = Node {
            order: next,
            color: main_color,
        };
        while let Some((row, parent_index)) = stack.last_mut() {
            if let Some(&parent) = parents[*row].get(*parent_index) {
                *parent_index += 1;
                if nodes[parent].order == 0 {
                    nodes[parent] = Node {
                        order: next,
                        color: if next == first { main_color } else { next },
                    };
                    stack.push((parent, 0));
                }
            } else {
                // A fragment ends only at its newly visited leaf, not every
                // ancestor encountered while unwinding the DFS stack.
                if nodes[*row].order == next {
                    next += 1;
                }
                stack.pop();
            }
        }
    }
    nodes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Element {
    Node(usize),
    Edge(usize),
}

fn edge_node(edge: Edge, node: usize, nodes: &[Node]) -> Ordering {
    nodes[edge.child]
        .order
        .max(nodes[edge.parent].order)
        .cmp(&nodes[node].order)
        .then(edge.child.cmp(&node))
}

fn compare(a: Element, b: Element, nodes: &[Node], edges: &[Edge]) -> Ordering {
    match (a, b) {
        (Element::Node(a), Element::Node(b)) => a.cmp(&b),
        (Element::Edge(e), Element::Node(n)) => edge_node(edges[e], n, nodes),
        (Element::Node(n), Element::Edge(e)) => edge_node(edges[e], n, nodes).reverse(),
        (Element::Edge(a), Element::Edge(b)) => {
            if a == b {
                return Ordering::Equal;
            }
            let (a, b) = (edges[a], edges[b]);
            if a.child == b.child {
                if a.parent < b.parent {
                    edge_node(b, a.parent, nodes).reverse()
                } else {
                    edge_node(a, b.parent, nodes)
                }
            } else if a.child < b.child {
                edge_node(a, b.child, nodes)
            } else {
                edge_node(b, a.child, nodes).reverse()
            }
        }
    }
}

pub(super) fn project(nodes: &[Node], edges: &[Edge], long_edges: bool) -> Vec<Row> {
    let (limit, visible) = if long_edges { (1000, 250) } else { (30, 1) };
    let count = nodes.len();
    let mut starts = vec![Vec::new(); count + 1];
    let mut ends = vec![Vec::new(); count + 1];
    let mut adjacent = vec![Vec::new(); count];
    for (id, edge) in edges.iter().enumerate() {
        adjacent[edge.child].push(id);
        adjacent[edge.parent].push(id);
        let mut band = |start: usize, end: usize| {
            if start < end {
                starts[start].push(id);
                ends[end].push(id);
            }
        };
        if edge.parent - edge.child < limit {
            band(edge.child + 1, edge.parent);
        } else {
            band(edge.child + 1, edge.child + visible + 1);
            band(edge.parent - visible, edge.parent);
        }
    }
    let mut positions = Vec::with_capacity(count);
    let mut active = Vec::new();
    let mut rows = Vec::with_capacity(count);
    for row in 0..count {
        active.retain(|id| !ends[row].contains(id));
        active.extend(starts[row].iter().copied());
        let mut elements: Vec<_> = active.iter().copied().map(Element::Edge).collect();
        elements.push(Element::Node(row));
        elements.sort_by(|&a, &b| compare(a, b, nodes, edges));
        let position: HashMap<_, _> = elements.iter().enumerate().map(|(i, &e)| (e, i)).collect();
        rows.push(Row {
            node: Track {
                lane: position[&Element::Node(row)],
                color: nodes[row].color,
            },
            last: elements.len() - 1,
            ..Row::default()
        });
        positions.push(position);
    }
    for row in 0..count {
        let mut elements: Vec<_> = positions[row].iter().map(|(&e, &pos)| (e, pos)).collect();
        elements.sort_by_key(|&(_, pos)| pos);
        for (element, from) in elements {
            let ids: &[usize] = match &element {
                Element::Node(i) => &adjacent[*i],
                Element::Edge(id) => std::slice::from_ref(id),
            };
            for &id in ids {
                let edge = edges[id];
                for up in [true, false] {
                    if (up && row <= edge.child) || (!up && row >= edge.parent) {
                        continue;
                    }
                    let neighbor = if up { row - 1 } else { row + 1 };
                    let to = positions[neighbor]
                        .get(&Element::Edge(id))
                        .or_else(|| {
                            positions[neighbor].get(&Element::Node(if up {
                                edge.child
                            } else {
                                edge.parent
                            }))
                        })
                        .copied();
                    let distance = edge.parent - edge.child;
                    let offset = if up {
                        edge.parent - row
                    } else {
                        row - edge.child
                    };
                    let arrow = (distance >= limit && offset == visible)
                        || (long_edges && distance >= 30 && offset == 1);
                    if let Some(to) = to.or_else(|| arrow.then_some(from)) {
                        rows[row].last = rows[row].last.max(from.max(to));
                        rows[row].segments.push(Segment {
                            edge: id,
                            from,
                            to,
                            up,
                            arrow,
                        });
                    }
                }
            }
        }
    }
    rows
}
