//! The controls and text in a single Changes row share the same cell geometry.
use super::{display_path, Change, CommitPanel, Context, Kind, Row, IGNORED, NEW};
use crate::ui::{
    icons::Icon,
    idea::{clipped, clipped_path, repo_tint, BLUE, CHECK, CONFLICT, DIM, INK},
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

const TAG: Color = Color::Rgb(44, 45, 51);
const ADDED: Color = Color::Rgb(110, 191, 124);
const UNTRACKED: Color = Color::Rgb(240, 140, 134);
const DELETED: Color = Color::Rgb(205, 154, 135);
const FOLDER: Color = Color::Rgb(197, 173, 121);

struct Control {
    glyph: &'static str,
    style: Style,
    area: Rect,
}

impl Control {
    fn draw(&self, line: &mut Line<'static>) {
        if !self.area.is_empty() {
            pad_to(line, self.area.x);
            line.push_span(Span::styled(self.glyph, self.style));
        }
    }
}

struct Cells {
    disclosure: Control,
    checkbox: Control,
    body: Rect,
}

fn pad_to(line: &mut Line<'static>, column: u16) {
    let count = usize::from(column).saturating_sub(line.width());
    if count > 0 {
        line.push_span(" ".repeat(count));
    }
}

fn file_style(change: &Change) -> Style {
    if change.ignored() {
        return Style::new().fg(DIM);
    }
    if change.conflict {
        return Style::new().fg(CONFLICT).bold();
    }
    // A mixed path commits its index version. Tint AM as added and MD as modified.
    let status = change.xy[usize::from(!change.staged())];
    match status {
        b'A' | b'C' => Style::new().fg(ADDED),
        b'?' => Style::new().fg(UNTRACKED),
        b'D' => Style::new().fg(DELETED).add_modifier(Modifier::CROSSED_OUT),
        _ => Style::new().fg(BLUE),
    }
}

fn filename(label: &str, width: usize) -> String {
    if label.width() > width {
        if let Some((stem, suffix)) = label.rsplit_once('.') {
            let ext_width = suffix.width() + 1;
            if !stem.is_empty() && width >= ext_width + 3 {
                return format!("{}.{}", clipped(stem, width - ext_width), suffix);
            }
        }
    }
    clipped(label, width)
}

fn parent_hint(repo: &str, path: &Path, width: usize) -> String {
    let repo = display_path(Path::new(repo));
    let hint = match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => format!("{repo}/{}", display_path(parent)),
        None => repo,
    };
    clipped_path(hint, width)
}

impl CommitPanel {
    fn row_cells(&self, row: &Row, area: Rect) -> Cells {
        let mut x = area.x.saturating_add(row.indent(area.width));
        let mut slot = |glyph: &'static str, style, visible| {
            let width = glyph.width() as u16;
            let available = area.right().saturating_sub(x);
            // Reserve the column for alignment, but never paint or click half a control.
            let rect = Rect::new(
                x,
                area.y,
                if visible && width <= available {
                    width
                } else {
                    0
                },
                area.height.min(1),
            );
            x = x.saturating_add((width + 1).min(available));
            Control {
                glyph,
                style,
                area: rect,
            }
        };
        let disclosure = slot(
            self.icons.disclosure(!self.fold.contains(&row.id)),
            Style::new().fg(DIM),
            row.foldable(),
        );
        let (checked, eligible) = self.check_counts(row);
        let state = match (checked, eligible) {
            (_, 0) => Icon::Locked,
            (0, _) => Icon::Unchecked,
            (n, m) if n == m => Icon::Checked,
            _ => Icon::Partial,
        };
        let checkbox = slot(
            self.icons.glyph(state),
            if checked > 0 {
                Style::new().fg(CHECK).bold()
            } else {
                Style::new().fg(DIM)
            },
            !row.files.is_empty(),
        );
        let body = Rect::new(
            x,
            area.y,
            area.right().saturating_sub(x),
            area.height.min(1),
        );
        Cells {
            disclosure,
            checkbox,
            body,
        }
    }

    pub(super) fn row_controls(&self, row: &Row, area: Rect) -> (Rect, Rect) {
        let c = self.row_cells(row, area);
        (c.disclosure.area, c.checkbox.area)
    }

    pub(super) fn change_row(&self, row: &Row, c: &Context, width: u16) -> Line<'static> {
        let mono = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        self.row_line(row, c, width, mono)
    }

    fn row_line(&self, row: &Row, c: &Context, width: u16, mono: bool) -> Line<'static> {
        let cells = self.row_cells(row, Rect::new(0, 0, width, 1));
        let mut line = Line::default();
        cells.disclosure.draw(&mut line);
        cells.checkbox.draw(&mut line);
        pad_to(&mut line, cells.body.x);
        if !cells.body.is_empty() {
            match row.kind {
                Kind::File(i) => {
                    let file = &self.files[i];
                    let change = &file.change;
                    let mut room = usize::from(cells.body.width);
                    if self.options.status || mono || change.conflict {
                        let xy = String::from_utf8_lossy(&change.xy);
                        let status = if room == 1 {
                            xy.trim().chars().take(1).collect()
                        } else if change.conflict && room >= row.label.width().min(24) + 12 {
                            format!("{xy} conflict")
                        } else {
                            xy.into_owned()
                        };
                        let style = Style::new()
                            .fg(if change.conflict { CONFLICT } else { INK })
                            .bg(TAG);
                        line.push_span(Span::styled(clipped(&status, room), style));
                        if line.width() < usize::from(width) {
                            line.push_span(" ");
                        }
                        room = usize::from(width) - line.width();
                    }
                    let (icon, color) = if change.directory() {
                        (self.icons.folder(false), FOLDER)
                    } else {
                        self.icons.file(&change.path)
                    };
                    let icon_width = icon.width() + 1;
                    if room >= row.label.width() + icon_width
                        || (change.directory() && room >= icon_width + row.label.width().min(3))
                    {
                        line.push_span(Span::styled(icon, Style::new().fg(color)));
                        line.push_span(" ");
                        room -= icon_width;
                    }
                    line.push_span(Span::styled(filename(&row.label, room), file_style(change)));
                    let room = usize::from(width) - line.width();
                    if !self.options.directories && room >= 3 {
                        let hint = parent_hint(&c.repos[file.root].name, &change.path, room - 1);
                        line.push_span(Span::styled(format!(" {hint}"), Style::new().fg(DIM)));
                    }
                }
                _ => {
                    let room = usize::from(cells.body.width);
                    let label = match row.kind {
                        Kind::Directory(_) => format!("{}/", row.label),
                        Kind::Repo(_) => display_path(Path::new(&row.label)),
                        _ => row.label.clone(),
                    };
                    let (glyph, color) = match row.kind {
                        Kind::Repo(root) => (
                            if c.repos[root].submodule {
                                self.icons.repo(true)
                            } else {
                                self.icons.folder(!self.fold.contains(&row.id))
                            },
                            repo_tint(root),
                        ),
                        Kind::Directory(_) => {
                            (self.icons.folder(!self.fold.contains(&row.id)), FOLDER)
                        }
                        Kind::Group if row.label == NEW => {
                            (self.icons.glyph(Icon::Untracked), UNTRACKED)
                        }
                        Kind::Group if row.label == IGNORED => {
                            (self.icons.glyph(Icon::Locked), DIM)
                        }
                        _ => (self.icons.glyph(Icon::Changes), DIM),
                    };
                    if room >= glyph.width() + 1 + label.width().min(3) {
                        line.push_span(Span::styled(glyph, Style::new().fg(color)));
                        line.push_span(" ");
                    }
                    let count = format!(
                        " {} file{}",
                        row.files.len(),
                        if row.files.len() == 1 { "" } else { "s" }
                    );
                    let room = usize::from(width) - line.width();
                    let show_count = room >= count.width() + label.width().min(8);
                    let name_room = room - if show_count { count.width() } else { 0 };
                    let style = if matches!(row.kind, Kind::Directory(_)) {
                        Style::new().fg(INK)
                    } else {
                        Style::new().fg(INK).bold()
                    };
                    line.push_span(Span::styled(clipped(&label, name_room), style));
                    if show_count {
                        line.push_span(Span::styled(count, Style::new().fg(DIM)));
                    }
                    if let Kind::Repo(r) = row.kind {
                        let branch = &c.repos[r].branch;
                        let room = usize::from(width) - line.width();
                        if room >= 4 && !branch.is_empty() {
                            let glyph = self.icons.glyph(Icon::Branch);
                            let badge = if room >= 4 + glyph.width() + branch.width() {
                                format!(" {glyph} {branch} ")
                            } else {
                                format!(" {} ", clipped(branch, room - 3))
                            };
                            line.push_span(" ");
                            line.push_span(Span::styled(badge, Style::new().fg(INK).bg(TAG)));
                        }
                    }
                }
            }
        }
        if mono {
            for span in &mut line.spans {
                span.style.fg = None;
                span.style.bg = None;
            }
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{commit::File, core::Repo, ui::icons::IconMode};
    use ratatui::{
        buffer::Buffer,
        widgets::{Paragraph, Widget},
    };

    fn fixture() -> (CommitPanel, Context) {
        let c = Context {
            workspace: "/workspace".into(),
            state_dir: "/state".into(),
            scope: [0].into(),
            repos: vec![Repo {
                path: "/workspace/moss-web".into(),
                name: "moss-web".into(),
                branch: "master".into(),
                parent: None,
                submodule: false,
            }],
        };
        let mut p = CommitPanel::default();
        for (path, xy, conflict) in [
            ("src/locales/lang/en_US/system.ts", *b"MM", false),
            ("src/locales/lang/zh_CN/system.ts", *b" M", false),
            ("package.json", *b"AM", false),
            ("docs/new-document.md", *b"??", false),
            ("conflicts/App.vue", *b"UU", true),
            ("once.js", *b"D ", false),
        ] {
            p.files.push(File {
                key: path.into(),
                root: 0,
                group: "Changes".into(),
                change: Change {
                    path: path.into(),
                    xy,
                    conflict,
                },
            });
            p.checks.insert(path.into(), xy != *b"??");
        }
        (p, c)
    }

    fn leaf(p: &CommitPanel, index: usize) -> Row {
        let file = &p.files[index];
        Row {
            id: file.key.clone(),
            label: display_path(Path::new(file.change.path.file_name().unwrap())),
            depth: 2,
            files: vec![index],
            kind: Kind::File(index),
        }
    }

    #[test]
    fn same_named_files_keep_their_parent_and_root_files_have_no_trailing_slash() {
        let (mut p, c) = fixture();
        for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
            p.icons = mode;
            for (i, lang) in [(0, "en_US"), (1, "zh_CN")] {
                let row = leaf(&p, i);
                let (glyph, _) = mode.file(&p.files[i].change.path);
                let full = p.row_line(&row, &c, 80, false);
                assert!(full.to_string().contains(&format!(
                    "{glyph} system.ts moss-web/src/locales/lang/{lang}"
                )));
                let narrow = p.row_line(&row, &c, 32, false).to_string();
                assert!(narrow.contains("system.ts …/") && narrow.ends_with(lang));
                // At this width only the controls and the full name fit.
                let width = mode.disclosure(true).width()
                    + mode.check(true).width()
                    + 2
                    + row.label.width();
                let tight = p.row_line(&row, &c, width as u16, false);
                assert!(tight.to_string().ends_with("system.ts"));
                assert!(!tight.spans.iter().any(|s| s.content == glyph));
            }
        }
        let row = leaf(&p, 2);
        let (glyph, _) = p.icons.file(&p.files[2].change.path);
        assert!(p
            .row_line(&row, &c, 80, false)
            .to_string()
            .ends_with(&format!("{glyph} package.json moss-web")));
        p.options.directories = true;
        assert!(p
            .row_line(&row, &c, 80, false)
            .to_string()
            .ends_with(&format!("{glyph} package.json")));
    }

    #[test]
    fn styles_reflect_the_committed_version_and_conflicts_are_always_visible() {
        let (mut p, c) = fixture();
        for (i, color) in [
            (0, BLUE),
            (1, BLUE),
            (2, ADDED),
            (3, UNTRACKED),
            (4, CONFLICT),
            (5, DELETED),
        ] {
            let row = leaf(&p, i);
            let line = p.row_line(&row, &c, 72, false);
            let name = line.spans.iter().find(|s| s.content == row.label).unwrap();
            assert_eq!(name.style.fg, Some(color));
            if i == 4 {
                assert!(line.to_string().contains("UU conflict"));
                assert!(line
                    .spans
                    .iter()
                    .any(|s| s.content == p.icons.glyph(Icon::Locked)));
            }
            let mono = p.row_line(&row, &c, 72, true);
            let xy = String::from_utf8_lossy(&p.files[i].change.xy);
            assert!(mono
                .spans
                .iter()
                .any(|s| s.content.starts_with(xy.as_ref())));
            assert!(mono
                .spans
                .iter()
                .all(|s| s.style.bg.is_none() && s.style.fg.is_none()));
        }
        p.options.status = true;
        let row = leaf(&p, 1);
        assert!(p
            .row_line(&row, &c, 72, false)
            .spans
            .iter()
            .any(|s| s.content == " M"));
    }

    #[test]
    fn controls_land_on_the_rendered_cells_even_when_the_row_is_clipped() {
        let (mut p, c) = fixture();
        let group = Row {
            id: "g:Changes".into(),
            label: "Changes".into(),
            depth: 0,
            files: vec![0, 3, 4],
            kind: Kind::Group,
        };
        let empty = Row {
            id: "g:empty".into(),
            label: "Empty".into(),
            depth: 0,
            files: vec![],
            kind: Kind::Group,
        };
        let (file, untracked, locked) = (leaf(&p, 0), leaf(&p, 3), leaf(&p, 4));
        for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
            p.icons = mode;
            for open in [true, false] {
                if !open {
                    p.fold.insert(group.id.clone());
                }
                for width in 0..=48 {
                    let area = Rect::new(9, 7, width, 1);
                    for (row, check) in [
                        (&group, Some(Icon::Partial)),
                        (&empty, None),
                        (&file, Some(Icon::Checked)),
                        (&untracked, Some(Icon::Unchecked)),
                        (&locked, Some(Icon::Locked)),
                    ] {
                        let line = p.row_line(row, &c, width, false);
                        assert!(line.width() <= usize::from(width));
                        let mut buf = Buffer::empty(area);
                        Paragraph::new(line).render(area, &mut buf);
                        let (arrow, checkbox) = p.row_controls(row, area);
                        let x = area.x + row.indent(width);
                        let disclosure = mode.disclosure(open || !row.foldable());
                        for (rect, start, expected) in [
                            (arrow, x, row.foldable().then_some(disclosure)),
                            (
                                checkbox,
                                x + disclosure.width() as u16 + 1,
                                check.map(|s| mode.glyph(s)),
                            ),
                        ] {
                            let Some(mark) = expected else {
                                assert!(rect.is_empty());
                                continue;
                            };
                            if mark.width() > usize::from(area.right().saturating_sub(start)) {
                                assert!(rect.is_empty());
                                for x in start..area.right() {
                                    assert_eq!(buf[(x, area.y)].symbol(), " ");
                                }
                            } else {
                                assert_eq!(rect, Rect::new(start, area.y, mark.width() as u16, 1));
                                let mut x = start;
                                for g in Span::raw(mark).styled_graphemes(Style::new()) {
                                    assert_eq!(buf[(x, area.y)].symbol(), g.symbol);
                                    x += g.symbol.width() as u16;
                                }
                            }
                        }
                    }
                }
                p.fold.remove(&group.id);
            }
        }
    }

    #[test]
    fn folders_keep_their_open_or_closed_icon_when_their_name_is_clipped() {
        let (mut p, c) = fixture();
        let directory = Row {
            id: "directory".into(),
            label: "a-very-long-directory-name".into(),
            depth: 2,
            files: vec![0],
            kind: Kind::Directory("src".into()),
        };
        let repo = Row {
            id: "repo".into(),
            label: "a-very-long-repository-name".into(),
            depth: 1,
            files: vec![0],
            kind: Kind::Repo(0),
        };
        for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
            p.icons = mode;
            for row in [&directory, &repo] {
                for open in [false, true] {
                    if open {
                        p.fold.remove(&row.id);
                    } else {
                        p.fold.insert(row.id.clone());
                    }
                    let line = p.row_line(row, &c, 28, false);
                    assert!(line.spans.iter().any(|s| s.content == mode.folder(open)));
                    assert!(line.width() <= 28);
                }
            }
        }
    }

    #[test]
    fn unicode_truncation_never_splits_a_grapheme_or_exceeds_the_row() {
        let (mut p, c) = fixture();
        p.files[0].change.path =
            "a-very-long-workspace/combined-e\u{301}👩‍💻/filename-under-review👩‍💻.ts".into();
        let row = leaf(&p, 0);
        let path = &p.files[0].change.path;
        for width in 0..=80 {
            let hint = parent_hint(&c.repos[0].name, path, width);
            assert!(hint.width() <= width);
            assert!(!hint.starts_with("…\u{301}") && !hint.starts_with("…\u{200d}"));
            let line = p.row_line(&row, &c, width as u16, false);
            assert!(line.width() <= width);
        }
        assert_eq!(parent_hint("root", path, 3), "…👩‍💻");
    }
}
