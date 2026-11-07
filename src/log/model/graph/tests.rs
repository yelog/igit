use super::*;

fn rev(id: &str, parents: &[&str]) -> Rev {
    Rev {
        oid: id.into(),
        parents: parents.iter().map(|s| (*s).into()).collect(),
        ..Default::default()
    }
}

fn long_edge(distance: usize) -> Vec<Rev> {
    let mut rows = vec![rev("child", &["parent"])];
    rows.extend((1..distance).map(|i| rev(&format!("side{i}"), &[])));
    rows.push(rev("parent", &[]));
    rows
}

#[test]
fn idea_shortens_at_thirty_rows_and_places_arrows_next_to_endpoints() {
    for distance in [12, 13, 29, 30, 31] {
        let rows = long_edge(distance);
        let graph = Graph::new(&rows);
        let w = graph.window(0..rows.len(), 0, 100, 0);
        let arrows: Vec<_> = (0..rows.len())
            .flat_map(|i| {
                graph
                    .cells_with_edges(i, w, NodeGlyphs::Unicode, false, false)
                    .into_iter()
                    .filter_map(move |(ch, _, target)| target.map(|target| (i, ch, target)))
            })
            .collect();
        let expected = if distance >= 30 {
            vec![(1, '↓', distance), (distance - 1, '↑', 0)]
        } else {
            vec![]
        };
        assert_eq!(arrows, expected, "edge distance {distance}");
    }
}

#[test]
fn independent_children_only_join_at_their_actual_parent() {
    let rows = [
        rev("a", &["base"]),
        rev("b", &["base"]),
        rev("c", &["base"]),
        rev("base", &[]),
    ];
    let graph = Graph::new(&rows);
    let w = graph.window(0..4, 0, 100, 0);
    assert_eq!(
        w.width(),
        5,
        "three independent edges need three positions before base"
    );
    for i in 0..3 {
        let cells = graph.cells(i, w, NodeGlyphs::Unicode, false);
        assert!(
            !cells
                .iter()
                .any(|(ch, _)| matches!(ch, '├' | '┤' | '┬' | '┴' | '┼')),
            "an edge crossing another edge is not a commit: {cells:?}"
        );
    }
}

#[test]
fn another_child_does_not_disable_shortening_a_long_edge() {
    let mut rows = long_edge(40);
    rows[5].parents = vec!["parent".into()];
    let graph = Graph::new(&rows);
    let w = graph.window(0..rows.len(), 0, 100, 0);
    for child in [0, 5] {
        assert!(graph
            .cells_with_edges(child + 1, w, NodeGlyphs::Unicode, false, false)
            .iter()
            .any(|&(ch, _, target)| ch == '↓' && target == Some(40)));
    }
}

#[test]
fn idea_layout_preserves_separate_merge_parents_and_compacts_between_rows() {
    let rows = [
        rev("head", &["base"]),
        rev("merge", &["base", "topic"]),
        rev("topic", &["base"]),
        rev("base", &[]),
    ];
    let graph = Graph::new(&rows);
    // The first-parent edge of merge cannot join head's edge before base.
    assert_eq!(
        graph
            .compact
            .iter()
            .map(|r| r.node.lane)
            .collect::<Vec<_>>(),
        [0, 1, 2, 0]
    );
    assert_eq!(graph.edges.len(), 4);
    assert_eq!(
        graph.nodes.iter().map(|n| n.order).collect::<Vec<_>>(),
        [1, 2, 2, 1]
    );
    let w = graph.window(0..4, 0, 100, 0);
    for glyphs in [NodeGlyphs::Unicode, NodeGlyphs::Connected] {
        for i in 0..4 {
            let cells = graph.cells(i, w, glyphs, false);
            assert!(
                !cells
                    .iter()
                    .any(|(ch, _)| matches!(ch, '├' | '┤' | '┬' | '┴' | '┼' | '╪')),
                "only commit dots connect edges: {cells:?}"
            );
        }
    }
}

#[test]
fn remote_mainline_precedes_local_head_and_named_branches_keep_their_color() {
    let mut rows = [
        rev("local", &["base"]),
        rev("remote", &["base"]),
        rev("base", &[]),
    ];
    rows[0].refs = "HEAD -> refs/heads/work".into();
    rows[1].refs = "refs/remotes/origin/master".into();
    let graph = Graph::new(&rows);
    assert_eq!(
        graph.nodes.iter().map(|n| n.order).collect::<Vec<_>>(),
        [2, 1, 1]
    );
    assert_eq!(
        graph.nodes[1].color,
        color::reference_color("origin/master")
    );
    assert_eq!(graph.nodes[2].color, graph.nodes[1].color);
    let filtered = Graph::filtered(&rows, &[0, 2]);
    assert_eq!(filtered.nodes[0].color, graph.nodes[0].color);
    assert_eq!(filtered.nodes[1].color, graph.nodes[2].color);
}

#[test]
fn folding_reclaims_columns_and_expansion_keeps_clickable_long_edge_arrows() {
    let rows = long_edge(40);
    let graph = Graph::new(&rows);
    assert_eq!(graph.compact[10].node.lane, 0);
    assert_eq!(graph.rows(true)[10].node.lane, 1);
    for long in [false, true] {
        let w = graph.window_with_edges(0..41, 0, 100, 0, long);
        let down = graph.cells_with_edges(1, w, NodeGlyphs::Unicode, false, long);
        assert!(down
            .iter()
            .any(|&(ch, _, target)| ch == '↓' && target == Some(40)));
    }
    for distance in [999, 1000] {
        let graph = Graph::new(&long_edge(distance));
        let w = graph.window(0..distance + 1, 0, 100, 0);
        let middle = graph.cells_with_edges(500, w, NodeGlyphs::Unicode, false, true);
        assert_eq!(middle.iter().any(|&(ch, _, _)| ch == '│'), distance < 1000);
        if distance == 1000 {
            for (at, target) in [(250, 1000), (750, 0)] {
                assert!(graph
                    .cells_with_edges(at, w, NodeGlyphs::Unicode, false, true)
                    .iter()
                    .any(|&(_, _, jump)| jump == Some(target)));
            }
        }
    }
}

#[test]
fn every_visible_edge_half_meets_its_other_half_without_changing_identity() {
    // Deterministic DAGs with octopus merges, close joins, distant parents,
    // independent tips, and duplicate parent IDs.
    for seed in 0..12 {
        let mut rows: Vec<_> = (0..80).map(|i| rev(&i.to_string(), &[])).collect();
        for (i, row) in rows.iter_mut().enumerate().take(79) {
            for offset in [1, 2 + seed, 31 + seed] {
                if i + offset < 80 {
                    row.parents.push((i + offset).to_string());
                }
            }
        }
        let graph = Graph::new(&rows);
        for long in [false, true] {
            let projected = graph.rows(long);
            for (i, row) in projected.iter().enumerate() {
                for half in &row.segments {
                    let next = if half.up { i - 1 } else { i + 1 };
                    let other = projected[next]
                        .segments
                        .iter()
                        .find(|s| s.edge == half.edge && s.up != half.up);
                    if let Some(other) = other {
                        assert_eq!((half.from, half.to), (other.to, other.from));
                    } else {
                        assert!(half.arrow, "only an endpoint arrow may terminate an edge");
                    }
                }
            }
        }
    }
}

#[test]
fn paging_uses_full_history_fragment_colors_and_stale_metadata_falls_back() {
    let mut full = vec![
        rev("head", &["main", "topic"]),
        rev("main", &["base", "old-side"]),
        rev("topic", &["base"]),
        rev("old-side", &["older"]),
        rev("older", &["base"]),
        rev("base", &[]),
    ];
    full[0].refs = "refs/remotes/origin/master".into();
    let layout = PermanentLayout::new(&full);
    let page: Vec<_> = [0, 1, 2, 5].map(|i| full[i].clone()).into();
    let paged = Graph::using_layout(&page, &[0, 1, 2, 3], &layout);
    let complete = Graph::new(&full);
    assert_eq!(paged.nodes[2].color, complete.nodes[2].color);
    assert_ne!(Graph::new(&page).nodes[2].color, complete.nodes[2].color);
    let filtered = Graph::using_layout(&page, &[2, 3], &layout);
    assert_eq!(filtered.nodes[0].color, complete.nodes[2].color);
    let changed = [rev("new-head", &["topic"]), page[2].clone()];
    let stale = Graph::using_layout(&changed, &[0, 1], &layout);
    let fallback = Graph::new(&changed);
    assert_eq!(
        stale
            .nodes
            .iter()
            .map(|n| (n.order, n.color))
            .collect::<Vec<_>>(),
        fallback
            .nodes
            .iter()
            .map(|n| (n.order, n.color))
            .collect::<Vec<_>>()
    );
}
