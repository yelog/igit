//! Push presentation uses shared column geometry for text and mouse controls.
use super::*;
use crate::ui::idea::{
    button_style, clipped, clipped_path, repo_tint, tracking_spans, CONFLICT, GOLD,
};
use crate::ui::search::Highlight;
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
            + icons.check(false).width()
            + 1
            + icons.repo(false).width().max(icons.repo(true).width())
            + 1;
        let space = width.saturating_sub(prefix);
        let arrow = icons.glyph(Icon::Right).width() + 2;
        let tracking_width = panel
            .entries
            .iter()
            .map(|entry| {
                Line::from(tracking_spans(icons, (entry.n, entry.behind.unwrap_or(0)))).width()
            })
            .max()
            .unwrap_or(0);
        let count = tracking_width > 0 && space >= 3 + tracking_width;
        let branches = space >= 6 + 2 + 3 + arrow + 5 + if count { tracking_width } else { 0 };
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
            if count { tracking_width } else { 0 },
            if module { 9 } else { 0 },
        ];
        let minimum = [
            if branches { 6 } else { 3 },
            if branches { 3 } else { 0 },
            if branches { 5 } else { 0 },
            if count { tracking_width } else { 0 },
            if module { 9 } else { 0 },
        ];
        let gaps = usize::from(branches) * (2 + arrow) + usize::from(module) * 2;
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
        let count = column(widths[3], 0);
        let arrow = column(if branches { arrow } else { 0 }, 0);
        let target = column(widths[2], 0);
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
    put_search(line, column, text, style, Highlight::default());
}

fn put_search(
    line: &mut Line<'static>,
    column: Column,
    text: &str,
    style: Style,
    highlight: Highlight<'_>,
) {
    if column.width > 0 {
        pad_to(line, column.start);
        line.spans
            .extend(highlight.clipped(text, column.width, style));
    }
}

fn files_count(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

fn filename(name: &str, width: usize) -> String {
    if name.width() > width {
        if let Some((stem, ext)) = name.rsplit_once('.') {
            if width >= ext.width() + 4 {
                return format!("{}.{}", clipped(stem, width - ext.width() - 1), ext);
            }
        }
    }
    clipped(name, width)
}

impl PushPanel {
    pub(crate) fn window_title(&self, c: &Context) -> String {
        if let Some(&root) = c.scope.first().filter(|_| c.scope.len() == 1) {
            format!("Push Commits to {}", label(Path::new(&c.repos[root].name)))
        } else {
            format!("Push Commits · {} repositories", c.scope.len())
        }
    }

    fn has_push_candidate(&self, c: &Context) -> bool {
        self.entries.iter().any(|e| {
            c.scope.contains(&e.idx)
                && self.checked.contains(&e.idx)
                && e.n > 0
                && e.error.is_empty()
        })
    }

    fn footer_options(&self) -> [(&'static str, bool, Hit); 2] {
        [
            ("Follow tags", self.follow_tags, Hit::Tags),
            ("Hooks", !self.skip_hooks, Hit::Hooks),
        ]
    }

    fn footer_actions(&self) -> [(&'static str, Hit); 2] {
        [
            (" Push ", Hit::Send),
            (
                if self.busy() { " Close " } else { " Cancel " },
                Hit::Cancel,
            ),
        ]
    }

    fn footer_wraps(&self, width: u16) -> bool {
        let options = self
            .footer_options()
            .iter()
            .map(|(label, checked, hit)| {
                let mut line = self.control_line(label, *hit, true);
                line.spans.insert(0, checkbox(self.icons, *checked));
                line.spans.insert(1, Span::raw(" "));
                line.width() + 2
            })
            .sum::<usize>();
        let actions = self
            .footer_actions()
            .iter()
            .map(|(label, _)| label.width())
            .sum::<usize>();
        options + actions + 3 > usize::from(width)
    }

    fn control_line(&self, text: &str, hit: Hit, enabled: bool) -> Line<'static> {
        let shortcut = match hit {
            Hit::Fetch => Some("f"),
            Hit::Strategy => Some("s"),
            Hit::Send => Some("P"),
            Hit::Tags => Some("F"),
            Hit::Hooks => Some("H"),
            _ => None,
        }
        .filter(|key| {
            enabled
                && self.panes.active
                && !self.editing()
                && self
                    .shortcut_labels
                    .get(key)
                    .is_none_or(|label| label == key)
        });
        if let Some(index) = shortcut.and_then(|key| {
            text.find(|character: char| {
                character.eq_ignore_ascii_case(&key.chars().next().unwrap())
            })
        }) {
            Line::from(vec![
                Span::raw(text[..index].to_string()),
                Span::styled(
                    text[index..index + 1].to_string(),
                    Style::new()
                        .fg(GOLD)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::raw(text[index + 1..].to_string()),
            ])
        } else {
            Line::raw(text.to_string())
        }
    }

    pub(super) fn draw_panel(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.hits.clear();
        self.panes.begin();
        if let Some(diff) = &mut self.diff {
            let name = self
                .preview
                .tree
                .file_index()
                .and_then(|i| self.preview.files.get(i))
                .map(|file| label(&file.path))
                .unwrap_or_default();
            diff.draw(f, area, self.icons, &name);
            return;
        }
        let hidden = self.checked.iter().any(|i| !c.scope.contains(i));
        let footer_height = 1 + u16::from(self.footer_wraps(area.width)) + u16::from(hidden);
        let [body, controls] = Layout::vertical([C::Min(2), C::Length(footer_height)]).areas(area);
        let [outgoing, files, details] = if self.panes.maximized {
            Self::PANES.map(|id| {
                if id == self.focused_pane() {
                    body
                } else {
                    Rect::default()
                }
            })
        } else {
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
            [outgoing, files, details]
        };
        self.panes.add(PaneId::Outgoing, outgoing);
        self.panes.add(PaneId::PushFiles, files);
        if self.show_details {
            self.panes.add(PaneId::PushDetails, details);
        }
        if !outgoing.is_empty() {
            self.draw_repositories(f, outgoing, c);
        }
        if !files.is_empty() {
            self.draw_files(f, files, c);
        }
        if self.show_details && !details.is_empty() {
            let block = pane(
                format!(
                    "Details · {} / Esc close",
                    self.shortcut_labels.get("d").map_or("d", String::as_str)
                ),
                self.active_pane(PaneId::PushDetails),
            );
            let inner = block.inner(details);
            f.render_widget(block, details);
            let layout = TextLayout::plain(&self.preview.body, inner.width, TextWrap::Word);
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
        self.draw_target_editor(f, area, c);
    }

    fn draw_repositories(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let title = if area.width >= 20 {
            "Outgoing commits"
        } else {
            "Commits"
        };
        let block = pane(title, self.active_pane(PaneId::Outgoing));
        let content = block.inner(area);
        f.render_widget(block, area);
        let [toolbar, inner] = Layout::vertical([C::Length(1), C::Min(0)]).areas(content);
        self.draw_outgoing_toolbar(f, toolbar, c);
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
        let spinners = self
            .activities
            .iter()
            .map(|(root, activity)| (*root, icons.spinner(activity.frame)))
            .collect::<BTreeMap<_, _>>();
        for (dy, &(id, commit)) in rows.iter().skip(self.offset).take(height).enumerate() {
            let e = self.entries.iter().find(|e| e.idx == id).unwrap();
            let row = Rect::new(inner.x, inner.y + dy as u16, inner.width, 1);
            let selected = self.offset + dy == self.pos;
            let muted = Style::new().fg(MUTED);
            self.hits.push((row, Hit::Row(self.offset + dy)));
            let mut line = Line::default();
            let key = (id, commit);
            if let Some(index) = commit {
                let h = &e.logs[index];
                // IDEA keeps the subject directly below its repository name.
                pad_to(&mut line, columns.name.start);
                let room = usize::from(row.width).saturating_sub(line.width());
                line.spans
                    .extend(self.outgoing_search.field(&key, 0).clipped_with(
                        &h.msg,
                        room,
                        Style::default(),
                        |text, _| text.into(),
                    ));
            } else {
                let repo = &c.repos[id];
                let fold = icons.disclosure(self.expanded.contains(&id));
                if !e.logs.is_empty() {
                    line.push_span(Span::styled(fold, muted));
                    self.hits.push((cell(row, 0, fold.width()), Hit::Fold(id)));
                }
                let check_start = fold.width() + 1;
                pad_to(&mut line, check_start);
                let mark = checkbox(icons, self.checked.contains(&id));
                if e.n > 0 {
                    self.hits
                        .push((cell(row, check_start, mark.width()), Hit::Check(id)));
                }
                line.push_span(mark);
                let repo_start = check_start + icons.check(false).width() + 1;
                pad_to(&mut line, repo_start);
                line.push_span(Span::styled(
                    icons.repo(repo.submodule),
                    Style::new().fg(repo_tint(id)),
                ));
                let result = self.results.get(&id).map(|r| {
                    let partial = matches!(r, Ok(PushOutcome::UpstreamNotSet(_)));
                    (
                        if r.is_ok() && !partial {
                            Icon::Success
                        } else {
                            Icon::Error
                        },
                        if partial {
                            CONFLICT
                        } else if r.is_ok() {
                            GREEN
                        } else {
                            RED
                        },
                    )
                });
                let marker = if let Some(glyph) = spinners.get(&id) {
                    Some((glyph.to_string(), ACCENT))
                } else {
                    result.map(|(icon, color)| (icons.glyph(icon).to_string(), color))
                };
                let marker_width = marker
                    .as_ref()
                    .map(|(glyph, _)| glyph.width() + 1)
                    .unwrap_or(0);
                put_search(
                    &mut line,
                    Column {
                        width: columns.name.width.saturating_sub(marker_width),
                        ..columns.name
                    },
                    &label(Path::new(&repo.name)),
                    Style::new()
                        .fg(if e.n > 0 || selected { FG } else { MUTED })
                        .add_modifier(Modifier::BOLD),
                    self.outgoing_search.field(&key, 0),
                );
                if let Some((glyph, color)) = marker {
                    line.push_span(Span::styled(format!(" {glyph}"), Style::new().fg(color)));
                }
                if e.error.is_empty() {
                    put_search(
                        &mut line,
                        columns.source,
                        &e.source,
                        Style::new().fg(if e.n > 0 || selected { FG } else { MUTED }),
                        self.outgoing_search.field(&key, 1),
                    );
                    if columns.count.width > 0 {
                        pad_to(&mut line, columns.count.start);
                        line.spans
                            .extend(tracking_spans(icons, (e.n, e.behind.unwrap_or(0))));
                    }
                    put(
                        &mut line,
                        columns.arrow,
                        &format!(" {} ", icons.glyph(Icon::Right)),
                        muted,
                    );
                    put_search(
                        &mut line,
                        columns.target,
                        &e.target_label(),
                        Style::new().fg(if e.n > 0 || selected { ACCENT } else { MUTED }),
                        self.outgoing_search.field(&key, 2),
                    );
                    if columns.target.width > 0 {
                        self.hits.push((
                            cell(row, columns.target.start, columns.target.width),
                            Hit::Target(id),
                        ));
                    }
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
        let repo = &c.repos[root];
        let name = node.kind.label(&self.preview.files, repo);
        let highlight = self.file_search.field(&id, 0);
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
            let room = width.saturating_sub(line.width());
            let parent = file
                .path
                .parent()
                .filter(|p| !self.grouping.enabled(Group::Directory) && !p.as_os_str().is_empty())
                .map(label);
            let reserve = parent
                .as_ref()
                .filter(|_| room >= 14)
                .map_or(0, |p| (p.width() + 2).min(room / 3).min(12));
            line.spans.extend(highlight.clipped_with(
                &name,
                room - reserve,
                Style::new().fg(color),
                filename,
            ));
            if let Some(parent) = parent {
                let room = width.saturating_sub(line.width() + 2);
                if room > 0 {
                    let hint = clipped_path(parent, room);
                    line.push_span(Span::styled(format!("  {hint}"), Style::new().fg(MUTED)));
                }
            }
        } else {
            let (icon, tint) = match &node.kind {
                Kind::Repository => (icons.repo(repo.submodule), repo_tint(root)),
                Kind::Module => (icons.folder(open), ACCENT),
                Kind::Directory { .. } => (icons.folder(open), MUTED),
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
            line.spans.extend(
                highlight.clipped(
                    &name,
                    room.saturating_sub(count.width()),
                    Style::new()
                        .fg(FG)
                        .add_modifier(if matches!(node.kind, Kind::Repository) {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            );
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
        let count = self.preview.files.len();
        let title = format!("Changed files · {count}");
        let title = if title.width() + 4 <= usize::from(area.width) {
            title
        } else {
            format!("Files · {count}")
        };
        let block = pane(title, self.active_pane(PaneId::PushFiles));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let folds_width =
            (icons.glyph(Icon::ExpandAll).width() + icons.glyph(Icon::CollapseAll).width() + 4)
                .min(usize::from(inner.width)) as u16;
        let tools_width = inner.width - folds_width;
        self.draw_toolbar(
            f,
            Rect::new(inner.x, inner.y, tools_width, 1),
            &[
                (Icon::Preview, "Group By", Hit::GroupBy),
                (Icon::Diff, "Details", Hit::Details),
            ],
        );
        self.draw_toolbar(
            f,
            Rect::new(inner.x + tools_width, inner.y, folds_width, 1),
            &[
                (Icon::ExpandAll, "", Hit::FoldAll(false)),
                (Icon::CollapseAll, "", Hit::FoldAll(true)),
            ],
        );
        let height = usize::from(inner.height.saturating_sub(1));
        if self.preview.tree.nodes.is_empty() {
            if height > 0 {
                let message = if self.load.loading() && self.entries.is_empty() {
                    "Loading outgoing commits…"
                } else if self.preview.body.is_empty() {
                    "No commits selected"
                } else {
                    self.preview.body.lines().next().unwrap_or_default()
                };
                f.render_widget(
                    Paragraph::new(message)
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

    fn draw_outgoing_toolbar(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            return;
        }
        let mut controls = vec![(Icon::Refresh, "fetch", Hit::Fetch)];
        if !self.last_fetch
            && self.entries.iter().any(|entry| {
                c.scope.contains(&entry.idx)
                    && entry.n > 0
                    && matches!(self.results.get(&entry.idx), Some(Err(_)))
            })
        {
            controls.push((Icon::Undo, "Retry", Hit::Retry));
        }
        if self.conflicted.iter().any(|root| c.scope.contains(root)) {
            controls.push((Icon::Error, "Conflicts", Hit::Conflicts));
        }
        let reserved = controls
            .iter()
            .map(|(icon, label, hit)| {
                if matches!(hit, Hit::Fetch) {
                    label.width() + 2
                } else {
                    self.icons.glyph(*icon).width() + 2
                }
            })
            .sum::<usize>();
        let shortcut = self.shortcut_labels.get("s").map_or("s", String::as_str);
        let labels = [
            format!("Update strategy: {}", self.strategy.label()),
            format!("Strategy: {}", self.strategy.label()),
            if shortcut.len() == 1 {
                format!("{shortcut} {}", self.strategy.label())
            } else {
                self.strategy.label().to_string()
            },
        ];
        let text = labels
            .iter()
            .find(|label| label.width() + reserved <= usize::from(area.width))
            .unwrap_or(&labels[2]);
        let width = text.width().min(usize::from(area.width)) as u16;
        let strategy = Rect::new(area.right() - width, area.y, width, 1);
        let enabled = !self.busy() && !self.writes_blocked;
        f.render_widget(
            Paragraph::new(self.control_line(text, Hit::Strategy, enabled))
                .style(Style::new().fg(if enabled { ACCENT } else { MUTED })),
            strategy,
        );
        if enabled {
            self.hits.push((strategy, Hit::Strategy));
        }
        let tools = Rect::new(area.x, area.y, strategy.x - area.x, 1);
        if self.busy() || self.read_status().is_some() {
            f.render_widget(
                Paragraph::new(format!(
                    " {}",
                    if self.busy() {
                        &self.busy
                    } else {
                        self.read_status().unwrap()
                    }
                ))
                .style(Style::new().fg(ACCENT)),
                tools,
            );
        } else {
            self.draw_toolbar(f, tools, &controls);
        }
    }

    fn draw_toolbar(&mut self, f: &mut Frame, area: Rect, controls: &[(Icon, &str, Hit)]) {
        if area.is_empty() {
            return;
        }
        let compact = controls
            .iter()
            .map(|(icon, label, _)| self.icons.glyph(*icon).width() + label.width() + 3)
            .sum::<usize>()
            > usize::from(area.width);
        let labels_fit = controls
            .iter()
            .map(|(icon, label, _)| {
                if label.is_empty() {
                    self.icons.glyph(*icon).width() + 2
                } else {
                    label.width() + 2
                }
            })
            .sum::<usize>()
            <= usize::from(area.width);
        let mut x = area.x;
        for &(icon, label, hit) in controls {
            let glyph = if matches!(hit, Hit::Fetch)
                && compact
                && !labels_fit
                && self
                    .shortcut_labels
                    .get("f")
                    .is_none_or(|label| label == "f")
            {
                "f"
            } else {
                self.icons.glyph(icon)
            };
            let text = if compact && labels_fit && !label.is_empty() {
                format!(" {label} ")
            } else if compact || label.is_empty() {
                format!(" {glyph} ")
            } else {
                format!(" {glyph} {label} ")
            };
            let width = text.width() as u16;
            if width > area.right().saturating_sub(x) {
                break;
            }
            let row = Rect::new(x, area.y, width, 1);
            let active = matches!(hit, Hit::GroupBy) && self.group_menu.is_some()
                || matches!(hit, Hit::Details) && self.show_details;
            let style = if active {
                button_style(false).fg(ACCENT)
            } else {
                Style::new().fg(MUTED)
            };
            f.render_widget(
                Paragraph::new(self.control_line(&text, hit, true)).style(style),
                row,
            );
            self.hits.push((row, hit));
            x += width;
        }
    }

    fn draw_controls(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        if area.is_empty() {
            return;
        }
        let ready = self.has_push_candidate(c) && !self.busy() && self.read_status().is_none();
        let hidden = self.checked.iter().filter(|i| !c.scope.contains(i)).count();
        let wrap = self.footer_wraps(area.width);
        let y = area.y;
        let mut x = area.x;
        for (label, checked, hit) in self.footer_options() {
            let mut line = self.control_line(label, hit, true);
            line.spans.insert(0, checkbox(self.icons, checked));
            line.spans.insert(1, Span::raw(" "));
            let width = line.width();
            let row = cell(
                Rect::new(area.x, y, area.width, 1),
                usize::from(x - area.x),
                width,
            );
            f.render_widget(
                Paragraph::new(line).style(Style::new().fg(MUTED).bg(BG)),
                row,
            );
            if !row.is_empty() {
                self.hits.push((row, hit));
            }
            x = row.right().saturating_add(2);
        }
        let action_y = (y + u16::from(wrap)).min(area.bottom() - 1);
        let mut right = area.right();
        for (label, hit) in self.footer_actions() {
            let width = (label.width() as u16).min(right - area.x);
            let row = Rect::new(right - width, action_y, width, 1);
            let primary = matches!(hit, Hit::Send);
            let enabled = !primary || ready;
            let style = if enabled {
                button_style(primary)
            } else {
                button_style(false).fg(MUTED)
            };
            f.render_widget(
                Paragraph::new(self.control_line(label, hit, enabled)).style(style),
                row,
            );
            if enabled && !row.is_empty() {
                self.hits.push((row, hit));
            }
            right = row.x.saturating_sub(1).max(area.x);
        }
        if hidden > 0 && action_y + 1 < area.bottom() {
            f.render_widget(
                Paragraph::new(format!(" {hidden} selected roots hidden"))
                    .style(Style::new().fg(MUTED)),
                Rect::new(area.x, action_y + 1, area.width, 1),
            );
        }
    }
}
