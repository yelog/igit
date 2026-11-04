//! Lay out the visible DAG once. A rail has two endpoints regardless of how
//! many rows it crosses; terminal width and the cursor do not enter the layout.
mod render;
pub(in crate::log) use render::{NodeGlyphs, Window as GraphWindow};

use super::Rev;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Default)]
struct Track {
    lane: usize,
    color: usize,
}

#[derive(Default)]
struct Row {
    node: Track,
    // Horizontal connections between the node and these rails on this row.
    links: Vec<Track>,
    first: usize,
    last: usize,
}

struct Rail {
    child: usize,
    parent: usize,
    color: usize,
    joined: bool,
}

#[derive(Default)]
pub(in crate::log) struct Graph {
    rows: Vec<Row>,
    // Each list is chronological; adjacent intervals may meet at a commit.
    lanes: Vec<Vec<Rail>>,
}

#[derive(Default)]
struct Slots {
    live: BTreeSet<usize>,
    free: BTreeSet<usize>,
    count: usize,
}

impl Slots {
    fn vacant(&self, from: usize) -> usize {
        self.free
            .range(from..)
            .next()
            .copied()
            .unwrap_or(self.count.max(from))
    }

    fn claim(&mut self, lane: usize) {
        if lane >= self.count {
            self.free.extend(self.count..lane);
            self.count = lane + 1;
        }
        self.free.remove(&lane);
        self.live.insert(lane);
    }

    fn release(&mut self, lane: usize) {
        self.live.remove(&lane);
        self.free.insert(lane);
    }
}

impl Graph {
    pub(in crate::log) fn new<'a>(revs: impl IntoIterator<Item = &'a Rev>) -> Self {
        let revs: Vec<_> = revs.into_iter().collect();
        let mut index = HashMap::new();
        let mut tips = HashMap::new();
        for (i, r) in revs.iter().enumerate() {
            index.insert((r.root, r.oid.as_str()), i);
            if r.is_head_tip() {
                tips.insert(r.root, i);
            } else {
                tips.entry(r.root).or_insert(i);
            }
        }

        // A repository's HEAD (or its first visible tip) owns its first-parent
        // line. Stop at a gap; adjacent search results need not be related.
        let mut primary = vec![false; revs.len()];
        for &tip in tips.values() {
            let mut row = tip;
            loop {
                primary[row] = true;
                let r = revs[row];
                match r
                    .parents
                    .first()
                    .and_then(|p| index.get(&(r.root, p.as_str())))
                {
                    Some(&next) if next > row => row = next,
                    _ => break,
                }
            }
        }

        let mut graph = Self::default();
        let mut arrivals = vec![Vec::<Track>::new(); revs.len()];
        let mut slots = Slots::default();
        let mut next_color = 1;
        let mut fork_color = || {
            let c = next_color;
            next_color += 1;
            c
        };
        for (i, r) in revs.into_iter().enumerate() {
            let incoming = std::mem::take(&mut arrivals[i]);
            let preferred = incoming
                .iter()
                .find(|t| primary[i] && t.color == 0)
                .or(incoming.first());
            // Protect lane zero for a HEAD that has not appeared yet. A lane can
            // move left at a commit only with an explicit link to its old column.
            let first_slot = usize::from(i < tips[&r.root]);
            let vacant = slots.vacant(first_slot);
            let node = Track {
                lane: preferred.map_or(vacant, |t| t.lane.min(vacant)),
                color: if primary[i] {
                    0
                } else {
                    preferred.map_or_else(&mut fork_color, |t| t.color)
                },
            };
            slots.claim(node.lane);
            let mut links: Vec<_> = incoming
                .iter()
                .copied()
                .filter(|t| t.lane != node.lane)
                .collect();
            let mut continues_below_node = false;
            for (n, parent) in r.parents.iter().enumerate() {
                let Some(&end) = index
                    .get(&(r.root, parent.as_str()))
                    .filter(|&&end| end > i)
                else {
                    continue;
                };
                let first = n == 0;
                // Share the remaining run to the same parent. Keep the primary
                // line straight if a side branch reached for its parent first:
                // those two real edges then meet at the parent, not at this child.
                let shared = arrivals[end]
                    .iter()
                    .filter(|t| !first || !primary[i] || t.lane == node.lane)
                    .min_by_key(|t| t.lane.abs_diff(node.lane));
                let track = if let Some(&t) = shared {
                    // A shared run has a real incoming connection in its middle.
                    // Keep it continuous even when empty long edges are shortened.
                    if let Some(rail) = graph.lanes.get_mut(t.lane).and_then(|v| v.last_mut()) {
                        rail.joined = true;
                    }
                    t
                } else {
                    // If the first parent joined an existing rail, the next
                    // parent can leave directly below the dot without a detour.
                    let t = Track {
                        lane: if continues_below_node {
                            slots.vacant(first_slot)
                        } else {
                            node.lane
                        },
                        color: if first { node.color } else { fork_color() },
                    };
                    slots.claim(t.lane);
                    if graph.lanes.len() <= t.lane {
                        graph.lanes.resize_with(t.lane + 1, Vec::new);
                    }
                    graph.lanes[t.lane].push(Rail {
                        child: i,
                        parent: end,
                        color: t.color,
                        joined: false,
                    });
                    arrivals[end].push(t);
                    t
                };
                if track.lane == node.lane {
                    continues_below_node = true;
                } else {
                    links.push(Track {
                        lane: track.lane,
                        color: if first || track.color == 0 {
                            node.color
                        } else {
                            track.color
                        },
                    });
                }
            }
            graph.rows.push(Row {
                node,
                links,
                first: *slots.live.first().unwrap(),
                last: *slots.live.last().unwrap(),
            });
            for t in incoming.into_iter().chain([node]) {
                if graph
                    .lanes
                    .get(t.lane)
                    .and_then(|v| v.last())
                    .is_none_or(|rail| rail.parent <= i)
                {
                    slots.release(t.lane);
                }
            }
        }
        graph
    }
}
