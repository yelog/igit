//! Display-only changes. Merge decisions and the aligned source rows stay intact.
use super::*;
use similar::{Algorithm, DiffTag, TextDiff};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthChar;

const INNER_LIMIT: usize = 2048;
const PAIR_LIMIT: usize = 4096;
const EXACT_WORK_LIMIT: usize = 65_536;

pub(super) type Pairing = [Vec<Option<usize>>; 2];

#[derive(Default)]
pub(super) struct HighlightCache {
    pairs: RefCell<HashMap<(usize, usize), CachedPair>>,
}

struct CachedPair {
    old: String,
    new: String,
    pairs: Pairing,
}

#[derive(Clone, Copy)]
pub(super) struct Tint {
    pub line: Color,
    pub gutter: Color,
    pub word: Color,
    pub marker: Color,
}

const MODIFIED: Tint = Tint {
    line: Color::Rgb(31, 40, 50),
    gutter: Color::Rgb(43, 66, 93),
    word: Color::Rgb(43, 66, 93),
    marker: Color::Rgb(85, 132, 165),
};
const CONFLICTED: Tint = Tint {
    line: Color::Rgb(35, 28, 28),
    gutter: Color::Rgb(53, 36, 32),
    word: Color::Rgb(64, 43, 39),
    marker: Color::Rgb(166, 90, 73),
};
const INSERTED: Tint = Tint {
    line: Color::Rgb(38, 54, 45),
    gutter: Color::Rgb(46, 73, 56),
    word: Color::Rgb(57, 92, 66),
    marker: Color::Rgb(83, 139, 99),
};
const DELETED: Tint = Tint {
    line: Color::Rgb(42, 43, 48),
    gutter: Color::Rgb(62, 63, 72),
    word: Color::Rgb(76, 78, 88),
    marker: Color::Rgb(116, 117, 131),
};

pub(super) fn side_tint(chunk: &Chunk, side: FileSide) -> Option<Tint> {
    chunk.pending(side).then(|| {
        if chunk.kind == Kind::Conflict {
            CONFLICTED
        } else {
            change_tint(&chunk.base, side.text(chunk))
        }
    })
}

fn change_tint(base: &str, result: &str) -> Tint {
    if base.is_empty() {
        INSERTED
    } else if result.is_empty() {
        DELETED
    } else {
        MODIFIED
    }
}

pub(super) fn padding_tint(chunk: &Chunk, pane: usize, read_only: bool) -> Option<Tint> {
    if read_only {
        revision_range_tint(chunk, pane)
    } else if let Some(side) = FileSide::ALL.into_iter().find(|side| side.pane() == pane) {
        side_tint(chunk, side)
    } else {
        FileSide::ALL
            .into_iter()
            .find_map(|side| side_tint(chunk, side))
            .or_else(|| {
                (chunk.result != chunk.base).then(|| change_tint(&chunk.base, &chunk.result))
            })
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Change {
    #[default]
    Equal,
    Inserted,
    Deleted,
    Modified,
    Conflict,
}

impl Change {
    fn tint(self) -> Option<Tint> {
        match self {
            Self::Equal => None,
            Self::Inserted => Some(INSERTED),
            Self::Deleted => Some(DELETED),
            Self::Modified => Some(MODIFIED),
            Self::Conflict => Some(CONFLICTED),
        }
    }
}

fn visual_indent(line: &str) -> (usize, &str) {
    let content = line.trim_start_matches([' ', '\t']);
    (
        rows::cell_width(&line[..line.len() - content.len()]),
        content,
    )
}

pub(super) fn revision_equal(old: &str, new: &str) -> bool {
    old == new || visual_indent(old) == visual_indent(new)
}

pub(super) fn revision_range_tint(chunk: &Chunk, pane: usize) -> Option<Tint> {
    if chunk.kind == Kind::Context {
        return None;
    }
    let compare = |source: &str| match (source.is_empty(), chunk.result.is_empty()) {
        (true, true) => Change::Equal,
        (true, false) => Change::Inserted,
        (false, true) => Change::Deleted,
        // A section with lines on both sides contains only paired rows.
        (false, false) if source == chunk.result => Change::Equal,
        (false, false) => {
            if source
                .split_inclusive('\n')
                .zip(chunk.result.split_inclusive('\n'))
                .all(|(old, new)| revision_equal(old, new))
            {
                Change::Equal
            } else {
                Change::Modified
            }
        }
    };
    match pane {
        0 => compare(&chunk.left),
        2 => compare(&chunk.right),
        _ => compare(&chunk.left).max(compare(&chunk.right)),
    }
    .tint()
}

pub(super) fn revision_line(texts: [Option<&str>; 3], pane: usize) -> LineChange<'_> {
    let mut change = LineChange::default();
    if let Some(line) = texts[pane] {
        if pane == 1 {
            change.record(line, texts[0], true, 0, true);
            change.record(line, texts[2], true, 1, true);
        } else {
            change.record(line, texts[1], false, 0, true);
        }
    }
    change
}

#[derive(Clone, Copy, Default)]
pub(super) struct LineChange<'a> {
    kind: Change,
    equivalent_indent: bool,
    // At most two parents/sources contribute to one result line.
    comparisons: [Option<(&'a str, bool)>; 2],
}

pub(super) struct Fragment {
    pub range: Range<usize>,
    pub background: Color,
}

impl<'a> LineChange<'a> {
    pub fn tint(&self) -> Option<Tint> {
        self.kind.tint()
    }

    pub fn indent_marker(&self) -> bool {
        self.equivalent_indent && self.kind == Change::Equal
    }

    fn record(
        &mut self,
        current: &str,
        other: Option<&'a str>,
        after: bool,
        slot: usize,
        revision: bool,
    ) {
        let kind = match other {
            Some(other) if other == current => Change::Equal,
            Some(other) if revision && revision_equal(other, current) => {
                self.equivalent_indent = true;
                Change::Equal
            }
            Some(other) => {
                self.comparisons[slot] = Some((other, after));
                Change::Modified
            }
            None if after => Change::Inserted,
            None => Change::Deleted,
        };
        self.kind = self.kind.max(kind);
    }

    pub fn fragments(&self, current: &str, revision: bool) -> Vec<Fragment> {
        let mut modified = Vec::new();
        let mut inserted = Vec::new();
        // Full added/deleted rows need no stronger character overlay.
        if !matches!(self.kind, Change::Modified | Change::Conflict) {
            return modified;
        }
        if current.len() > INNER_LIMIT
            || self
                .comparisons
                .into_iter()
                .flatten()
                .any(|(other, _)| other.trim_end_matches(['\n', '\r']).len() > INNER_LIMIT)
        {
            return modified;
        }
        let deadline = Instant::now() + Duration::from_millis(2);
        for (other, after) in self.comparisons.into_iter().flatten() {
            let other = other.trim_end_matches(['\n', '\r']);
            let (old, new) = if after {
                (other, current)
            } else {
                (current, other)
            };
            let (mut old, mut new, mut prefix) = (old, new, 0);
            if revision {
                let (old_width, old_body) = visual_indent(old);
                let (new_width, new_body) = visual_indent(new);
                if old_width == new_width {
                    prefix = current.len()
                        - if after {
                            new_body.len()
                        } else {
                            old_body.len()
                        };
                    old = old_body;
                    new = new_body;
                }
            }
            // Keep replaced identifiers/numbers together. Matching individual
            // letters can otherwise turn a rename into scattered green inserts.
            let (old, old_offsets) = tokens(old);
            let (new, new_offsets) = tokens(new);
            let timed = old.len().saturating_mul(new.len()) > EXACT_WORK_LIMIT;
            let mut config = TextDiff::configure();
            // Short diffs should not lose their fragments if a busy process is
            // briefly descheduled during rendering.
            if timed {
                if Instant::now() >= deadline {
                    return Vec::new();
                }
                config.deadline(deadline);
            }
            let diff = config.diff_slices(&old, &new);
            if timed && Instant::now() >= deadline {
                // Never show only one parent's green fragments when the
                // replacement comparison against the other could not finish.
                return Vec::new();
            }
            for op in diff.ops() {
                let range = if after {
                    op.new_range()
                } else {
                    op.old_range()
                };
                if op.tag() == DiffTag::Equal || range.is_empty() {
                    continue;
                }
                let offsets = if after { &new_offsets } else { &old_offsets };
                let range = (prefix + offsets[range.start])..(prefix + offsets[range.end]);
                let addition = after && op.tag() == DiffTag::Insert;
                let fragment = Fragment {
                    range,
                    background: if self.kind == Change::Conflict {
                        CONFLICTED.word
                    } else if addition {
                        INSERTED.word
                    } else {
                        MODIFIED.word
                    },
                };
                if addition {
                    inserted.push(fragment);
                } else {
                    modified.push(fragment);
                }
            }
        }
        // A replacement against either parent takes precedence over an insertion
        // against the other. The renderer uses the first intersecting fragment.
        modified.extend(inserted);
        modified
    }
}

impl HighlightCache {
    pub fn clear(&mut self) {
        self.pairs.get_mut().clear();
    }

    pub fn line_changes<'a>(
        &self,
        index: usize,
        chunk: &'a Chunk,
        texts: &[&'a str; 3],
        pane: usize,
    ) -> Vec<LineChange<'a>> {
        let mut changes = Vec::new();
        if let Some(side) = FileSide::ALL.into_iter().find(|side| side.pane() == pane) {
            if chunk.pending(side) {
                self.compare(
                    &mut changes,
                    (index, pane / 2),
                    &chunk.base,
                    texts[pane],
                    true,
                    0,
                );
            }
        } else if chunk.done() {
            self.compare(&mut changes, (index, 2), &chunk.base, texts[1], true, 0);
        } else {
            for (slot, side) in FileSide::ALL.into_iter().enumerate() {
                if chunk.pending(side) {
                    self.compare(
                        &mut changes,
                        (
                            index,
                            if texts[1] == chunk.base {
                                slot
                            } else {
                                3 + slot
                            },
                        ),
                        texts[1],
                        texts[side.pane()],
                        false,
                        slot,
                    );
                }
            }
        }
        if chunk.kind == Kind::Conflict
            && FileSide::ALL
                .into_iter()
                .any(|side| (pane == 1 || side.pane() == pane) && chunk.pending(side))
        {
            if changes.is_empty() {
                changes.resize_with(texts[pane].split_inclusive('\n').count(), Default::default);
            }
            for change in &mut changes {
                change.kind = Change::Conflict;
            }
        }
        changes
    }

    #[allow(clippy::too_many_arguments)]
    fn compare<'a>(
        &self,
        changes: &mut Vec<LineChange<'a>>,
        key: (usize, usize),
        old: &'a str,
        new: &'a str,
        after: bool,
        slot: usize,
    ) {
        if old == new {
            return;
        }
        let old_lines = split(old);
        let new_lines = split(new);
        let mut cache = self.pairs.borrow_mut();
        let entry = cache.entry(key).or_insert_with(|| CachedPair {
            old: old.into(),
            new: new.into(),
            pairs: pair_lines(
                &old_lines,
                &new_lines,
                Algorithm::Myers,
                Duration::from_millis(5),
            ),
        });
        if entry.old != old || entry.new != new {
            *entry = CachedPair {
                old: old.into(),
                new: new.into(),
                pairs: pair_lines(
                    &old_lines,
                    &new_lines,
                    Algorithm::Myers,
                    Duration::from_millis(5),
                ),
            };
        }
        let (current, others, pairs) = if after {
            (&new_lines, &old_lines, &entry.pairs[1])
        } else {
            (&old_lines, &new_lines, &entry.pairs[0])
        };
        changes.resize_with(current.len(), Default::default);
        for (row, change) in changes.iter_mut().enumerate() {
            change.record(
                current[row],
                pairs[row].map(|index| others[index]),
                after,
                slot,
                false,
            );
        }
    }
}

fn split(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn tokens(text: &str) -> (Vec<&str>, Vec<usize>) {
    let mut tokens = Vec::new();
    let mut offsets = vec![0];
    let (mut start, mut kind) = (0, 0);
    for (count, (byte, ch)) in text.char_indices().enumerate() {
        // Wide letters can form unspaced text. Keep them separate so an
        // inserted phrase can match the surrounding characters on both sides.
        let next = if (ch.is_alphanumeric() && ch.width() == Some(1)) || matches!(ch, '_' | '$') {
            1
        } else if ch.is_whitespace() {
            2
        } else {
            0
        };
        if byte > start && (kind == 0 || kind != next) {
            tokens.push(&text[start..byte]);
            offsets.push(count);
            start = byte;
        }
        kind = next;
    }
    if start < text.len() {
        tokens.push(&text[start..]);
        offsets.push(text.chars().count());
    }
    (tokens, offsets)
}

pub(super) fn pair_lines(
    old: &[&str],
    new: &[&str],
    algorithm: Algorithm,
    timeout: Duration,
) -> Pairing {
    if old == new {
        let positions: Vec<_> = (0..old.len()).map(Some).collect();
        return [positions.clone(), positions];
    }
    let mut pairs = [vec![None; old.len()], vec![None; new.len()]];
    if old.is_empty() || new.is_empty() {
        return pairs;
    }
    let deadline = Instant::now() + timeout;
    let mut config = TextDiff::configure();
    config.algorithm(algorithm);
    if old.len().saturating_mul(new.len()) > PAIR_LIMIT {
        config.deadline(deadline);
    }
    let diff = config.diff_slices(old, new);
    for op in diff.ops() {
        let before = op.old_range();
        let after = op.new_range();
        let matches = match op.tag() {
            DiffTag::Equal => before.zip(after).collect(),
            DiffTag::Replace => pair_replacements(old, new, before, after, deadline),
            _ => continue,
        };
        for (before, after) in matches {
            pairs[0][before] = Some(after);
            pairs[1][after] = Some(before);
        }
    }
    pairs
}

fn pair_replacements(
    old: &[&str],
    new: &[&str],
    before: Range<usize>,
    after: Range<usize>,
    deadline: Instant,
) -> Vec<(usize, usize)> {
    let m = before.len();
    let n = after.len();
    if m == 1 && n == 1 {
        return vec![(before.start, after.start)];
    }
    // Huge blocks use bounded lookahead instead of a quadratic allocation.
    let fallback = || pair_surplus(old, new, before.clone(), after.clone());
    if m.saturating_mul(n) > PAIR_LIMIT {
        return fallback();
    }
    let sample_bytes = |lines: &[&str]| {
        lines
            .iter()
            .map(|line| line.len().min(INNER_LIMIT))
            .sum::<usize>()
    };
    let timed = sample_bytes(&old[before.clone()])
        .saturating_mul(sample_bytes(&new[after.clone()]))
        > EXACT_WORK_LIMIT;
    let mut config = TextDiff::configure();
    if timed {
        config.deadline(deadline);
    }
    const GAP: u32 = 1000;
    let width = n + 1;
    let mut costs = vec![0; (m + 1) * width];
    let mut steps = vec![0; m * n];
    for i in 0..=m {
        costs[i * width + n] = (m - i) as u32 * GAP;
    }
    for j in 0..=n {
        costs[m * width + j] = (n - j) as u32 * GAP;
    }
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            if timed && Instant::now() >= deadline {
                return fallback();
            }
            let a = sample(old[before.start + i].trim());
            let b = sample(new[after.start + j].trim());
            let similarity = config.diff_chars(a.as_ref(), b.as_ref()).ratio();
            let replace =
                costs[(i + 1) * width + j + 1] + 2 * GAP - (similarity * (2 * GAP) as f32) as u32;
            let delete = costs[(i + 1) * width + j] + GAP;
            let insert = costs[i * width + j + 1] + GAP;
            let (cost, step) = [(replace, 0), (delete, 1), (insert, 2)]
                .into_iter()
                .min()
                .unwrap();
            costs[i * width + j] = cost;
            steps[i * n + j] = step;
        }
    }
    if timed && Instant::now() >= deadline {
        return fallback();
    }
    let (mut i, mut j) = (0, 0);
    let mut pairs = Vec::new();
    while i < m && j < n {
        match steps[i * n + j] {
            0 => {
                pairs.push((before.start + i, after.start + j));
                i += 1;
                j += 1;
            }
            1 => i += 1,
            _ => j += 1,
        }
    }
    pairs
}

fn sample(text: &str) -> Cow<'_, str> {
    if text.len() <= INNER_LIMIT {
        return Cow::Borrowed(text);
    }
    let mut first = INNER_LIMIT / 4;
    while !text.is_char_boundary(first) {
        first -= 1;
    }
    let mut last = text.len() - INNER_LIMIT / 4;
    while !text.is_char_boundary(last) {
        last += 1;
    }
    Cow::Owned(format!("{}{}", &text[..first], &text[last..]))
}

fn edge_similarity(a: &str, b: &str) -> usize {
    let a = sample(a.trim());
    let b = sample(b.trim());
    let prefix = a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count();
    let suffix = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let shared = (prefix + suffix).min(a.len().min(b.len()));
    2000 * shared / (a.len() + b.len()).max(1)
}

fn pair_surplus(
    old: &[&str],
    new: &[&str],
    before: Range<usize>,
    after: Range<usize>,
) -> Vec<(usize, usize)> {
    let (mut i, mut j) = (before.start, after.start);
    let mut pairs = Vec::new();
    while i < before.end && j < after.end {
        let old_left = before.end - i;
        let new_left = after.end - j;
        // Check a few nearby rows and the far edge of the surplus. This also
        // recognizes a large insertion before a long/generated modified line.
        if new_left > old_left {
            let extra = new_left - old_left;
            let shift = (0..=extra.min(8))
                .chain(std::iter::once(extra))
                .max_by_key(|&shift| {
                    (
                        edge_similarity(old[i], new[j + shift]),
                        std::cmp::Reverse(shift),
                    )
                })
                .unwrap();
            j += shift;
        } else if old_left > new_left {
            let extra = old_left - new_left;
            let shift = (0..=extra.min(8))
                .chain(std::iter::once(extra))
                .max_by_key(|&shift| {
                    (
                        edge_similarity(old[i + shift], new[j]),
                        std::cmp::Reverse(shift),
                    )
                })
                .unwrap();
            i += shift;
        }
        pairs.push((i, j));
        i += 1;
        j += 1;
    }
    pairs
}
