//! Read-only alignment projected back onto the original revision bytes.
use super::super::{
    highlight::{pair_lines, revision_equal},
    Chunk, Kind,
};
use similar::Algorithm;
use std::time::Duration;

fn empty_chunk(kind: Kind) -> Chunk {
    Chunk {
        base: String::new(),
        left: String::new(),
        result: String::new(),
        right: String::new(),
        kind,
        resolution: Default::default(),
    }
}

pub(in crate::merge::view) fn chunks(texts: [&str; 3]) -> Vec<Chunk> {
    let lines = texts.map(|text| text.split_inclusive('\n').collect::<Vec<_>>());
    let keys = lines.each_ref().map(|lines| {
        lines
            .iter()
            .map(|line| line.trim_matches([' ', '\t', '\r', '\n']))
            .collect::<Vec<_>>()
    });
    // Prefer distinctive content to repeated blank and structural lines. Keys
    // guide pairing only. The original lines decide the colors and line numbers.
    let matches = [0, 2].map(|pane| {
        let [_, paired] = pair_lines(
            &keys[pane],
            &keys[1],
            Algorithm::Patience,
            Duration::from_millis(100),
        );
        paired
    });
    let mut chunks = Vec::new();
    let mut previous = None;
    let mut append = |row: [Option<&str>; 3]| {
        let context = row[1].is_some_and(|center| {
            [0, 2]
                .into_iter()
                .all(|pane| row[pane].is_some_and(|source| revision_equal(source, center)))
        });
        // Each section has the same set of present panes on every row. Its
        // ordinary row offset can then address the paired line in each pane.
        let shape = (row.map(|line| line.is_some()), context);
        if previous != Some(shape) {
            chunks.push(empty_chunk(if context {
                Kind::Context
            } else {
                Kind::Safe
            }));
            previous = Some(shape);
        }
        let chunk = chunks.last_mut().expect("alignment section");
        for (text, line) in [&mut chunk.left, &mut chunk.result, &mut chunk.right]
            .into_iter()
            .zip(row)
        {
            if let Some(line) = line {
                text.push_str(line);
            }
        }
    };

    let sources = [&lines[0], &lines[2]];
    let mut next = [0; 2];
    for center in 0..=lines[1].len() {
        let paired = matches
            .each_ref()
            .map(|rows| rows.get(center).copied().flatten());
        let stop = if center == lines[1].len() {
            sources.map(Vec::len)
        } else {
            std::array::from_fn(|slot| paired[slot].unwrap_or(next[slot]))
        };
        // Emit any remaining deletions before the next paired line. Two
        // parents can each have their own deletions at this result boundary.
        while (0..2).any(|slot| next[slot] < stop[slot]) {
            let [left, right] = std::array::from_fn(|slot| {
                (next[slot] < stop[slot]).then(|| {
                    let line = sources[slot][next[slot]];
                    next[slot] += 1;
                    line
                })
            });
            append([left, None, right]);
        }
        if let Some(&line) = lines[1].get(center) {
            let [left, right] = std::array::from_fn(|slot| {
                paired[slot].map(|index| {
                    next[slot] = index + 1;
                    sources[slot][index]
                })
            });
            append([left, Some(line), right]);
        }
    }

    if chunks.is_empty() {
        chunks.push(empty_chunk(Kind::Context));
    }
    for chunk in &mut chunks {
        chunk.base.clone_from(&chunk.result);
    }
    chunks
}
