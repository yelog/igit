//! Push presentation uses shared column geometry for text and mouse controls.
use super::*;
use crate::ui::idea::{button_style, clipped, clipped_path, diff_line, repo_tint, CONFLICT};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Default)]
struct Column {
    start: usize,
    width: usize,
}

struct Columns {
    name: Column,
    source: Column,
    arrow: Column,
    target: Column,
    count: Column,
    module: Column,
}

impl Columns {
    fn new(panel: &PushPanel, c: &Context, width: usize) -> Self {
        let icons = panel.icons;
        let prefix = icons.disclosure(false).width()
            + 1
            + icons
                .check(false)
                .width()
                .max(icons.glyph(Icon::Locked).width())
            + 1
            + icons.repo(false).width().max(icons.repo(true).width())
            + 1;
        let space = width.saturating_sub(prefix);
        let arrow = icons.glyph(Icon::Right).width() + 2;
        let branches = space >= 6 + 2 + 3 + arrow + 5;
        let count = branches && space >= 6 + 2 + 4 + arrow + 9 + 2 + 2;
        let module = count
            && panel.entries.iter().any(|e| c.repos[e.idx].submodule)
            && space >= 8 + 2 + 4 + arrow + 10 + 2 + 2 + 2 + 9;
        let result = 1 + icons
            .glyph(Icon::Success)
            .width()
            .max(icons.glyph(Icon::Error).width());
        let max = |values: Vec<usize>, min| values.into_iter().max().unwrap_or(min).max(min);
        let mut widths = [
            max(
                panel
                    .entries
                    .iter()
                    .map(|e| label(Path::new(&c.repos[e.idx].name)).width() + result)
                    .collect(),
                6,
            ),
            if branches {
                max(panel.entries.iter().map(|e| e.source.width()).collect(), 3)
            } else {
                0
            },
            if branches {
                max(
                    panel
                        .entries
                        .iter()
                        .map(|e| e.t.remote.width() + 1 + e.t.dst.width())
                        .collect(),
                    5,
                )
            } else {
                0
            },
            if count {
                max(
                    panel
                        .entries
                        .iter()
                        .map(|e| icons.glyph(Icon::Ahead).width() + e.n.to_string().width())
                        .collect(),
                    2,
                )
            } else {
                0
            },
            if module { 9 } else { 0 },
        ];
        let minimum = [
            6,
            if branches { 3 } else { 0 },
            if branches { 5 } else { 0 },
            if count { 2 } else { 0 },
            if module { 9 } else { 0 },
        ];
        let gaps =
            usize::from(branches) * (2 + arrow) + usize::from(count) * 2 + usize::from(module) * 2;
        while widths.iter().sum::<usize>() + gaps > space {
            let Some(i) = (0..widths.len())
                .filter(|&i| widths[i] > minimum[i])
                .max_by_key(|&i| widths[i] - minimum[i])
            else {
                break;
            };
            widths[i] -= 1;
        }
        widths[0] = widths[0].min(space);
        let mut x = prefix.min(width);
        let mut column = |w, gap| {
            x += gap;
            let out = Column { start: x, width: w };
            x += w;
            out
        };
        let name = column(widths[0], 0);
        let source = column(widths[1], if branches { 2 } else { 0 });
        let arrow = column(if branches { arrow } else { 0 }, 0);
        let target = column(widths[2], 0);
        let count = column(widths[3], usize::from(count) * 2);
        let module = column(widths[4], usize::from(module) * 2);
        Self {
            name,
            source,
            arrow,
            target,
            count,
            module,
        }
    }
}

fn pad_to(line: &mut Line<'static>, column: usize) {
    let n = column.saturating_sub(line.width());
    if n > 0 {
        line.push_span(" ".repeat(n));
    }
}

fn cell(row: Rect, column: usize, width: usize) -> Rect {
    let start = column.min(usize::from(row.width)) as u16;
    Rect::new(
        row.x + start,
        row.y,
        (width.min(usize::from(row.width - start))) as u16,
        1,
    )
}

fn put(line: &mut Line<'static>, column: Column, text: &str, style: Style) {
    if column.width > 0 {
        pad_to(line, column.start);
        line.push_span(Span::styled(clipped(text, column.width), style));
    }
}

fn files_count(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

fn filename(path: &Path, width: usize) -> String {
    let name = label(path.file_name().unwrap_or(path.as_os_str()).as_ref());
    if name.width() > width {
        if let Some((stem, ext)) = name.rsplit_once('.') {
            if width >= ext.width() + 4 {
                return format!("{}.{}", clipped(stem, width - ext.width() - 1), ext);
            }
        }
    }
    clipped(&name, width)
}

impl PushPanel {
    pub(super) fn draw_panel(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.hits.clear();
        self.panes.begin();
        let [header, body, controls] =
            Layout::vertical([C::Length(1), C::Min(2), C::Length(2)]).areas(area);
        let [outgoing, right] = self.panes.divide(
            SplitId::PushColumns,
            body,
            Axis::Width,
            body.width / 2,
            [18, 18],
        );
        let [files, details] = if self.show_details {
            self.panes.divide(
                SplitId::PushDetails,
                right,
                Axis::Height,
                (u32::from(right.height) * 3 / 5) as u16,
                [4, 3],
            )
        } else {
            [right, Rect::default()]
        };
        self.panes.add(PaneId::Outgoing, outgoing);
        self.panes.add(PaneId::PushFiles, files);
        if self.show_details {
            self.panes.add(PaneId::PushDetails, details);
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} Push Commits", self.icons.glyph(Icon::Push)),
                    Style::new().fg(FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  /  {} roots", self.entries.len()),
                    Style::new().fg(MUTED),
                ),
                Span::styled(
                    if self.busy() {
                        format!("   {}", self.busy)
                    } else {
                        String::new()
                    },
                    Style::new().fg(ACCENT),
                ),
            ])),
            header,
        );
        self.draw_repositories(f, outgoing, c);
        self.draw_files(f, files, c);
        if self.show_details {
            let block = pane(
                "Details / Diff · d / Esc close",
                self.active_pane(PaneId::PushDetails),
            );
            let inner = block.inner(details);
            f.render_widget(block, details);
            let mut layout = TextLayout::default();
            for line in self.preview.body.split('\n') {
                layout.push(
                    if self.preview.is_patch {
                        diff_line(line)
                    } else {
                        Line::raw(line)
                    },
                    inner.width,
                    TextWrap::Word,
                );
            }
            self.detail_off = usize::from(self.detail_off)
                .min(layout.rows.len().saturating_sub(usize::from(inner.height)))
                .min(u16::MAX as usize) as u16;
            self.detail_selection
                .render(f, inner, layout, usize::from(self.detail_off), 0);
        } else {
            self.detail_selection.reset();
        }
        self.draw_controls(f, controls, c);
        self.draw_group_menu(f, area);
        if let Some((i, target)) = &self.target_edit {
            let pop = Rect::new(
                area.x + 2,
                area.y + area.height / 2,
                area.width.saturating_sub(4),
                3.min(area.height),
            );
            f.render_widget(Clear, pop);
            f.render_widget(
                Paragraph::new(format!("{target}{}", self.icons.glyph(Icon::Cursor))).block(pane(
                    format!(
                        "{} · remote:branch · Enter save / Esc cancel",
                        c.repos[*i].name
                    ),
                    self.panes.active,
                )),
                pop,
            );
        }
    }

    fn draw_repositories(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let block = pane("Outgoing commits", self.active_pane(PaneId::Outgoing));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = self.rows();
        let height = usize::from(inner.height);
        self.offset = self.offset.min(rows.len().saturating_sub(height));
        if height > 0 {
            self.offset = self
                .offset
                .min(self.pos)
                .max((self.pos + 1).saturating_sub(height));
        }
        let icons = self.icons;
        let columns = Columns::new(self, c, usize::from(inner.width));
        for (dy, &(id, commit)) in rows.iter().skip(self.offset).take(height).enumerate() {
            let e = self.entries.iter().find(|e| e.idx == id).unwrap();
            let row = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
            let selected = self.offset + dy == self.pos;
            let muted = Style::new().fg(MUTED);
            self.hits.push((row, Hit::Row(self.offset + dy)));
            let mut line = Line::default();
            if let Some(index) = commit {
                let h = &e.logs[index];
                // IDEA keeps the subject directly below its repository name.
                let end = icons.glyph(Icon::TreeEnd);
                pad_to(
                    &mut line,
                    columns.name.start.saturating_sub(end.width() + 1),
                );
                line.push_span(Span::styled(format!("{end} "), muted));
                line.push_span(Span::raw(h.msg.clone()));
            } else {
                let repo = &c.repos[id];
                let fold = icons.disclosure(self.expanded.contains(&id));
                if !e.logs.is_empty() {
                    line.push_span(Span::styled(fold, muted));
                    self.hits.push((cell(row, 0, fold.width()), Hit::Fold(id)));
                }
                let check_start = fold.width() + 1;
                pad_to(&mut line, check_start);
                if e.n == 0 {
                    line.push_span(Span::styled(icons.glyph(Icon::Locked), muted));
                } else {
                    let mark = checkbox(icons, self.checked.contains(&id));
                    self.hits
                        .push((cell(row, check_start, mark.width()), Hit::Check(id)));
                    line.push_span(mark);
                }
                let repo_start = check_start
                    + icons
                        .check(false)
                        .width()
                        .max(icons.glyph(Icon::Locked).width())
                    + 1;
                pad_to(&mut line, repo_start);
                line.push_span(Span::styled(
                    icons.repo(repo.submodule),
                    Style::new().fg(repo_tint(id)),
                ));
                let result = self.results.get(&id).map(|r| {
                    (
                        if r.is_ok() {
                            Icon::Success
                        } else {
                            Icon::Error
                        },
                        if r.is_ok() { GREEN } else { RED },
                    )
                });
                let result_width = result
                    .map(|(icon, _)| icons.glyph(icon).width() + 1)
                    .unwrap_or(0);
                put(
                    &mut line,
                    Column {
                        width: columns.name.width.saturating_sub(result_width),
                        ..columns.name
                    },
                    &label(Path::new(&repo.name)),
                    Style::new()
                        .fg(if e.n > 0 || selected { FG } else { MUTED })
                        .add_modifier(Modifier::BOLD),
                );
                if let Some((icon, color)) = result {
                    line.push_span(Span::styled(
                        format!(" {}", icons.glyph(icon)),
                        Style::new().fg(color),
                    ));
                }
                if e.error.is_empty() {
                    put(
                        &mut line,
                        columns.source,
                        &e.source,
                        Style::new().fg(if e.n > 0 || selected { FG } else { MUTED }),
                    );
                    put(
                        &mut line,
                        columns.arrow,
                        &format!(" {} ", icons.glyph(Icon::Right)),
                        muted,
                    );
                    put(
                        &mut line,
                        columns.target,
                        &format!("{}:{}", e.t.remote, e.t.dst),
                        Style::new().fg(if e.n > 0 || selected { ACCENT } else { MUTED }),
                    );
                    if columns.target.width > 0 {
                        self.hits.push((
                            cell(row, columns.target.start, columns.target.width),
                            Hit::Target(id),
                        ));
                    }
                    put(
                        &mut line,
                        columns.count,
                        &format!("{}{}", icons.glyph(Icon::Ahead), e.n),
                        muted,
                    );
                    if repo.submodule {
                        put(&mut line, columns.module, "submodule", muted);
                    }
                } else {
                    let start = columns.name.start + columns.name.width + 2;
                    put(
                        &mut line,
                        Column {
                            start,
                            width: usize::from(row.width).saturating_sub(start),
                        },
                        &format!("{} {}", icons.glyph(Icon::Error), e.error),
                        Style::new().fg(RED),
                    );
                }
            }
            f.render_widget(
                Paragraph::new(line).style(Style::new().fg(FG).bg(if selected { SEL } else { BG })),
                row,
            );
        }
        if rows.is_empty() {
            f.render_widget(
                Paragraph::new(" No repositories in scope").style(Style::new().fg(MUTED)),
                inner,
            );
        }
    }

    fn file_line(
        &self,
        id: usize,
        width: usize,
        c: &Context,
        root: usize,
    ) -> (Line<'static>, Option<Column>) {
        let node = &self.preview.tree.nodes[id];
        let icons = self.icons;
        let indent = (node.depth(self.grouping) * 2 + 1).min(width.saturating_sub(14));
        let mut line = Line::from(" ".repeat(indent));
        let folder = !matches!(node.kind, Kind::File(_));
        let open = node.end > id + 1 && !self.preview.tree.folded.contains(&id);
        let fold = icons.disclosure(open);
        let slot = fold.width() + 1;
        let disclosure = if width >= line.width() + slot + 4 {
            let column = Column {
                start: line.width(),
                width: fold.width(),
            };
            line.push_span(Span::styled(
                if folder && node.end > id + 1 {
                    format!("{fold} ")
                } else {
                    " ".repeat(slot)
                },
                Style::new().fg(MUTED),
            ));
            (folder && node.end > id + 1).then_some(column)
        } else {
            None
        };
        if let Kind::File(i) = node.kind {
            let file = &self.preview.files[i];
            let (icon, tint) = icons.file(&file.path);
            if width >= line.width() + icon.width() + 6 {
                line.push_span(Span::styled(format!("{icon} "), Style::new().fg(tint)));
            }
            let color = match file.status {
                b'A' | b'C' => GREEN,
                b'D' => RED,
                b'R' | b'T' => CONFLICT,
                _ => ACCENT,
            };
            let status = format!(" {}", char::from(file.status));
            let room = width.saturating_sub(line.width() + status.width());
            let parent = file
                .path
                .parent()
                .filter(|p| !self.grouping.enabled(Group::Directory) && !p.as_os_str().is_empty())
                .map(label);
            let reserve = parent
                .as_ref()
                .filter(|_| room >= 14)
                .map_or(0, |p| (p.width() + 2).min(room / 3).min(12));
            line.push_span(Span::styled(
                filename(&file.path, room - reserve),
                Style::new().fg(color),
            ));
            line.push_span(Span::styled(status, Style::new().fg(MUTED)));
            if let Some(parent) = parent {
                let room = width.saturating_sub(line.width() + 2);
                if room > 0 {
                    let hint = clipped_path(parent, room);
                    line.push_span(Span::styled(format!("  {hint}"), Style::new().fg(MUTED)));
                }
            }
        } else {
            let repo = &c.repos[root];
            let (icon, tint, name) = match &node.kind {
                Kind::Repository => (
                    icons.repo(repo.submodule),
                    repo_tint(root),
                    label(Path::new(&repo.name)),
                ),
                Kind::Module => (
                    icons.folder(open),
                    ACCENT,
                    label(
                        repo.path
                            .file_name()
                            .unwrap_or(repo.path.as_os_str())
                            .as_ref(),
                    ),
                ),
                Kind::Directory { name, .. } => (icons.folder(open), MUTED, name.clone()),
                Kind::File(_) => unreachable!(),
            };
            if width >= line.width() + icon.width() + 5 {
                line.push_span(Span::styled(format!("{icon} "), Style::new().fg(tint)));
            }
            let count = format!(" {}", files_count(node.count));
            let room = width.saturating_sub(line.width());
            let count = if room >= count.width() + 4 {
                count
            } else {
                String::new()
            };
            line.push_span(Span::styled(
                clipped(&name, room.saturating_sub(count.width())),
                Style::new()
                    .fg(FG)
                    .add_modifier(if matches!(node.kind, Kind::Repository) {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ));
            line.push_span(Span::styled(count, Style::new().fg(MUTED)));
            if matches!(node.kind, Kind::Module) {
                let relative = repo
                    .path
                    .strip_prefix(&c.workspace)
                    .ok()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(label);
                if let Some(path) = relative.filter(|s| line.width() + 2 + s.width() <= width) {
                    line.push_span(Span::styled(format!("  {path}"), Style::new().fg(MUTED)));
                }
            }
        }
        (line, disclosure)
    }

    fn draw_files(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let icons = self.icons;
        let block = pane(
            format!("Changed files · {}", self.preview.files.len()),
            self.active_pane(PaneId::PushFiles),
        );
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let controls = [
            (icons.glyph(Icon::Preview), "Group By", Hit::GroupBy),
            (icons.glyph(Icon::Diff), "Details", Hit::Details),
            (icons.glyph(Icon::ExpandAll), "Expand", Hit::FoldAll(false)),
            (
                icons.glyph(Icon::CollapseAll),
                "Collapse",
                Hit::FoldAll(true),
            ),
        ];
        let compact = controls
            .iter()
            .map(|(glyph, label, _)| glyph.width() + label.width() + 3)
            .sum::<usize>()
            > usize::from(inner.width);
        let mut x = inner.x;
        for (glyph, label, hit) in controls {
            let text = if compact {
                format!(" {glyph} ")
            } else {
                format!(" {glyph} {label} ")
            };
            let width = (text.width() as u16).min(inner.right() - x);
            if width == 0 {
                break;
            }
            let row = Rect::new(x, inner.y, width, 1);
            let active = matches!(hit, Hit::GroupBy) && self.group_menu.is_some()
                || matches!(hit, Hit::Details) && self.show_details;
            f.render_widget(
                Paragraph::new(text).style(Style::new().fg(if active { ACCENT } else { MUTED })),
                row,
            );
            self.hits.push((row, hit));
            x += width;
        }
        let height = usize::from(inner.height.saturating_sub(1));
        if self.preview.tree.nodes.is_empty() && self.preview.body.is_empty() {
            if height > 0 {
                f.render_widget(
                    Paragraph::new("No commits selected")
                        .style(Style::new().fg(MUTED))
                        .centered(),
                    Rect::new(
                        inner.x,
                        inner.y + 1 + (height as u16 - 1) / 2,
                        inner.width,
                        1,
                    ),
                );
            }
            return;
        }
        let Some((root, _)) = self.selected() else {
            return;
        };
        let tree = &mut self.preview.tree;
        tree.offset = tree.offset.min(tree.visible.len().saturating_sub(height));
        if (self.focus == 1 || self.show_details || self.group_menu.is_some()) && height > 0 {
            tree.offset = tree
                .offset
                .min(tree.cursor)
                .max((tree.cursor + 1).saturating_sub(height));
        }
        let start = tree.offset;
        for i in start..(start + height).min(self.preview.tree.visible.len()) {
            let id = self.preview.tree.visible[i];
            let row = Rect::new(inner.x, inner.y + 1 + (i - start) as u16, inner.width, 1);
            let (line, fold) = self.file_line(id, usize::from(inner.width), c, root);
            f.render_widget(
                Paragraph::new(line).style(Style::new().fg(FG).bg(
                    if self.focus == 1 && i == self.preview.tree.cursor {
                        SEL
                    } else {
                        BG
                    },
                )),
                row,
            );
            self.hits.push((row, Hit::File(i)));
            if let Some(fold) = fold {
                self.hits
                    .push((cell(row, fold.start, fold.width), Hit::FileFold(i)));
            }
        }
        let empty_row = self.preview.tree.visible.len();
        if self.preview.files.is_empty() && empty_row < height {
            let text = if self.preview.tree.nodes.is_empty() {
                self.preview
                    .body
                    .lines()
                    .next()
                    .unwrap_or("No changed files")
            } else {
                "No changed files"
            };
            f.render_widget(
                Paragraph::new(format!("   {text}")).style(Style::new().fg(MUTED)),
                Rect::new(inner.x, inner.y + 1 + empty_row as u16, inner.width, 1),
            );
        }
    }

    fn draw_controls(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let n = self
            .entries
            .iter()
            .filter(|e| self.checked.contains(&e.idx) && e.n > 0)
            .count();
        let hidden = self.checked.iter().filter(|i| !c.scope.contains(i)).count();
        let controls = [
            (
                format!("{} Follow tags", self.icons.check(self.follow_tags)),
                Hit::Tags,
            ),
            (
                format!("{} Hooks", self.icons.check(!self.skip_hooks)),
                Hit::Hooks,
            ),
            (
                " Target ".into(),
                Hit::Target(self.selected().map(|r| r.0).unwrap_or(0)),
            ),
            (" Fetch ".into(), Hit::Fetch),
            (" Retry ".into(), Hit::Retry),
            (format!(" Push {n} "), Hit::Send),
        ];
        let mut x = area.x;
        let mut y = area.y;
        for (label, hit) in controls {
            let width = label.width() as u16;
            if x != area.x && x.saturating_add(width) > area.right() {
                y = y.saturating_add(1);
                x = area.x;
            }
            if y >= area.bottom() {
                break;
            }
            let primary = matches!(hit, Hit::Send);
            if primary {
                x = area.right().saturating_sub(width.min(area.width)).max(x);
            }
            let row = Rect::new(x, y, width.min(area.right() - x), 1);
            f.render_widget(Paragraph::new(label).style(button_style(primary)), row);
            self.hits.push((row, hit));
            x = row.right().saturating_add(1);
        }
        if hidden > 0 && area.height > 1 && y == area.y {
            f.render_widget(
                Paragraph::new(format!(" {hidden} selected roots hidden"))
                    .style(Style::new().fg(MUTED)),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
    }
}
