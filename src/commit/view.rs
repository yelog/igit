use super::{
    backend, rid, rows::CONFLICT_SELECTION, slice, word, CommitPanel, Focus, Hit, Kind, Row,
};
use crate::{
    core::Context,
    ui::icons::{Icon, IconMode},
    ui::idea::{
        button_style, clipped as fit, diff_line, repo_tint, BLUE, DIM, EDGE, GREEN, INK, PANEL,
        RED, SELECT, TAG,
    },
    ui::panes::{pane_frame, Axis, PaneId, SplitId},
    ui::selection::{TextLayout, TextSelection, Wrap},
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

const SPLIT_AT: u16 = 92;

fn fg(color: Color) -> Style {
    Style::default().fg(color)
}
fn bold(color: Color) -> Style {
    fg(color).add_modifier(Modifier::BOLD)
}
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
fn file_line(icons: IconMode, change: &backend::Change, width: usize) -> Line<'static> {
    // Keep both porcelain columns (e.g. MM), and use the version being committed for the tint.
    let color = if change.ignored() {
        DIM
    } else if change.conflict {
        RED
    } else {
        match if change.staged() {
            change.xy[0]
        } else {
            change.xy[1]
        } {
            b'A' => GREEN,
            b'D' => RED,
            b'?' => RED,
            _ => BLUE,
        }
    };
    let xy = String::from_utf8_lossy(&change.xy[..width.min(2)]);
    let mut line = Line::from(Span::styled(xy.into_owned(), bold(color).bg(TAG)));
    if width > 3 {
        line.push_span(" ");
        let name = change.label();
        let (glyph, tint) = if change.directory() {
            (icons.folder(false), DIM)
        } else {
            icons.file(&change.path)
        };
        if width - 3 > name.width() + glyph.width() {
            line.push_span(Span::styled(glyph, fg(tint)));
            line.push_span(" ");
        }
        line.push_span(Span::styled(fit(&name, width - line.width()), fg(color)));
    }
    line
}
impl CommitPanel {
    pub(super) fn hit_button(
        &mut self,
        f: &mut Frame,
        bar: Rect,
        x: &mut u16,
        txt: &str,
        hit: Hit,
    ) {
        let w = txt.width() + 2;
        if w > usize::from(bar.right().saturating_sub(*x)) || bar.is_empty() {
            return;
        }
        let area = Rect::new(*x, bar.y, w as u16, 1);
        let style = match hit {
            Hit::Submit(false) | Hit::EditSave => button_style(true),
            Hit::Submit(true) | Hit::EditCancel => button_style(false),
            Hit::Repo(r) if r == self.repo => fg(INK).bg(TAG),
            Hit::Repo(r) => fg(repo_tint(r)),
            Hit::Amend if self.amend.contains(&self.repo) => fg(BLUE),
            Hit::TogglePreview if self.preview_open => fg(BLUE).bg(TAG),
            Hit::ViewOptions if self.view_menu.is_some() => fg(BLUE).bg(TAG),
            Hit::Directories if self.options.directories => fg(BLUE).bg(TAG),
            Hit::Status if self.options.status => fg(BLUE).bg(TAG),
            Hit::UndoCheck if self.last_check.is_empty() => fg(Color::DarkGray),
            _ => fg(DIM),
        };
        word(f, area, &format!(" {txt} "), style);
        self.hits.push((area, hit));
        *x = area.right().saturating_add(1);
    }

    pub(super) fn draw_workbench(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.panes.begin();
        self.hits.clear();
        self.tree = Rect::default();
        self.diff = Rect::default();
        self.edit_view.0 = Rect::default();
        self.stacked = area.width < SPLIT_AT;
        let [sidebar, preview] = if self.panes.maximized {
            if self.focus == Focus::Diff {
                [Rect::default(), area]
            } else {
                [area, Rect::default()]
            }
        } else {
            if !self.preview_open || (self.stacked && self.focus != Focus::Diff) {
                [area, Rect::default()]
            } else if self.stacked {
                [Rect::default(), area]
            } else {
                self.panes.divide(
                    SplitId::CommitPreview,
                    area,
                    Axis::Width,
                    (u32::from(area.width) * 54 / 100) as u16,
                    [34, 24],
                )
            }
        };
        let [changes, message] = if self.panes.maximized {
            if self.focus == Focus::Message {
                [Rect::default(), sidebar]
            } else {
                [sidebar, Rect::default()]
            }
        } else if sidebar.is_empty() {
            [Rect::default(); 2]
        } else {
            let message_height = match sidebar.height {
                0..=6 => 2,
                7..=13 => 3,
                _ => 5,
            };
            self.panes.divide(
                SplitId::CommitMessage,
                sidebar,
                Axis::Height,
                sidebar.height.saturating_sub(message_height),
                [2, 2],
            )
        };
        self.panes.add(PaneId::Changes, changes);
        if self.preview_open {
            self.panes.add(PaneId::Preview, preview);
        }
        self.panes.add(PaneId::Message, message);
        if message.is_empty() {
            self.message_selection.reset();
        }
        if !sidebar.is_empty() {
            self.draw_sidebar(f, changes, message, c);
        }
        if !preview.is_empty() {
            let inner = pane_frame(f, preview, self.pane_focused(PaneId::Preview));
            self.draw_preview(f, inner, c, self.stacked);
        }
    }

    fn draw_sidebar(&mut self, f: &mut Frame, changes: Rect, message: Rect, c: &Context) {
        let mut list = pane_frame(f, changes, self.pane_focused(PaneId::Changes));
        let composer = pane_frame(f, message, self.pane_focused(PaneId::Message));
        let last_row = slice(changes, changes.height.saturating_sub(1));
        let rule = if changes.is_empty() || message.is_empty() {
            Rect::default()
        } else if list.bottom() < changes.bottom() {
            // Only the real facing borders can resize; a compact composer's
            // first row contains the repository tabs and the editable draft.
            if composer.y == message.y {
                self.panes.handle(SplitId::CommitMessage, last_row);
            }
            last_row
        } else if composer.y > message.y {
            // Short panes only have side borders. Use the adjacent top border instead.
            let edge = slice(message, 0);
            self.panes.handle(SplitId::CommitMessage, edge);
            edge
        } else if changes.height > 2 {
            list.height -= 1;
            self.panes.handle(SplitId::CommitMessage, last_row);
            word(
                f,
                last_row,
                &"─".repeat(last_row.width.into()),
                fg(if self.pane_focused(PaneId::Changes) {
                    BLUE
                } else {
                    EDGE
                }),
            );
            last_row
        } else {
            // At the minimum terminal size the toolbar and the only Changes row
            // need both lines; keep the drag handle on the frame, outside the row.
            let grip = Rect {
                width: last_row.width.min(1),
                ..last_row
            };
            self.panes.handle(SplitId::CommitMessage, grip);
            if !grip.is_empty() {
                f.buffer_mut()[(grip.x, grip.y)].set_symbol("└");
            }
            Rect::default()
        }
        .inner(Margin::new(1, 0));
        let [bar, tree] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(list);
        self.tree = tree;
        let tools_end = self.draw_tools(f, bar);
        if !tree.is_empty() {
            self.offset = self
                .offset
                .min(self.cursor)
                .min(self.view.len().saturating_sub(tree.height as usize))
                .max((self.cursor + 1).saturating_sub(tree.height as usize));
        }
        for (i, row) in self
            .view
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(tree.height as usize)
        {
            let r = slice(tree, (i - self.offset) as u16);
            let model = self.changes_tree();
            let style = if i == self.cursor {
                fg(INK).bg(if model.has_merge_conflict(row) {
                    CONFLICT_SELECTION
                } else {
                    SELECT
                })
            } else {
                fg(INK)
            };
            f.render_widget(
                Paragraph::new(model.change_row(row, c, r.width)).style(style),
                r,
            );
            if row.kind == Kind::Conflicts {
                let resolve = model.resolve_control(row, r);
                if !resolve.is_empty() {
                    self.hits.push((resolve, Hit::ResolveConflicts));
                }
            }
            self.hits.push((r, Hit::Row(i)));
        }

        let separate_options = composer.width < 70 && composer.height >= 3;
        let [message, options, actions] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(u16::from(separate_options)),
            Constraint::Length(1),
        ])
        .areas(composer);
        let roots = self.message_repos(c);
        if let Some(&first) = roots.first() {
            if !roots.contains(&self.repo) {
                self.repo = first;
            }
        }
        let tabs_width = self.tabs_width(&roots, c);
        let [tabs, draft] = Layout::horizontal([
            Constraint::Length(tabs_width.min(usize::from(message.width / 2)) as u16),
            Constraint::Min(1),
        ])
        .areas(message);
        self.repo_tabs(f, slice(tabs, 0), &roots, c);
        let msg = c
            .repos
            .get(self.repo)
            .and_then(|_| self.saved.drafts.get(&rid(c, self.repo)))
            .filter(|s| !s.trim().is_empty());
        if self.message_editing() {
            self.draw_input(f, draft, false);
        } else {
            self.edit_view = (draft, 0, 0);
            self.message_selection.render(
                f,
                draft,
                TextLayout::plain(msg.map_or("", String::as_str), draft.width, Wrap::Clip),
                0,
                0,
            );
            if msg.is_none() {
                word(f, slice(draft, 0), "Commit message…", fg(DIM));
            }
        }
        self.hits.push((draft, Hit::Msg));

        let mut x = actions.x;
        self.hit_button(f, actions, &mut x, "Commit…", Hit::Submit(false));
        self.hit_button(f, actions, &mut x, "Commit & Push…", Hit::Submit(true));
        if self.message_editing() {
            self.hit_button(f, actions, &mut x, "Save", Hit::EditSave);
            self.hit_button(f, actions, &mut x, "Cancel", Hit::EditCancel);
        }
        let submit_end = x;
        let options = if separate_options {
            x = options.x;
            options
        } else {
            actions
        };
        let amend = format!(
            "{} Amend",
            self.icons.check(self.amend.contains(&self.repo))
        );
        self.hit_button(f, options, &mut x, &amend, Hit::Amend);
        let ai = if self.ai_pending.is_some() {
            "Generating…"
        } else {
            "Generate message"
        };
        self.hit_button(
            f,
            options,
            &mut x,
            &format!("{} {ai}", self.icons.glyph(Icon::Ai)),
            Hit::Ai,
        );
        self.hit_button(
            f,
            options,
            &mut x,
            self.icons.glyph(Icon::Settings),
            Hit::Settings,
        );
        let selected = self.selection(c);
        let root_count = selected
            .iter()
            .map(|v| v.root)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let excluded = self
            .files
            .iter()
            .filter(|v| {
                v.change.selectable()
                    && !c.scope.contains(&v.root)
                    && self.checks.get(&v.key) == Some(&true)
            })
            .count();
        let mut status = format!(
            "{} selected · {}",
            count(selected.len(), "file"),
            count(root_count, "root")
        );
        if excluded != 0 {
            status.push_str(&format!(" · {excluded} excluded"));
        }
        // Leave the scope visible even when the compact action row is occupied.
        let w = status.width() as u16;
        let r = if actions
            .right()
            .saturating_sub(if separate_options { submit_end } else { x })
            > w
        {
            Rect::new(actions.right() - w, actions.y, w, actions.height)
        } else if bar.right().saturating_sub(tools_end) > w {
            Rect::new(bar.right() - w, bar.y, w, bar.height)
        } else {
            let w = w.min(rule.width);
            Rect::new(rule.right() - w, rule.y, w, rule.height)
        };
        word(f, r, &fit(&status, usize::from(r.width)), fg(DIM));
        if let Some(hint) = self.hover {
            let free = bar.right().saturating_sub(tools_end);
            let r = if usize::from(free) >= hint.width() {
                Rect::new(tools_end, bar.y, free, 1)
            } else {
                rule
            };
            word(f, r, &fit(hint, usize::from(r.width)), fg(INK).bg(PANEL));
        }
    }

    fn draw_tools(&mut self, f: &mut Frame, bar: Rect) -> u16 {
        let mut x = bar.x;
        if bar.is_empty() {
            return x;
        }
        for (icon, action) in [
            (Icon::Refresh, Hit::Refresh),
            (Icon::Undo, Hit::UndoCheck),
            (Icon::Move, Hit::Move),
            (Icon::Add, Hit::New),
            (Icon::Preview, Hit::ViewOptions),
            (Icon::Diff, Hit::TogglePreview),
            (Icon::ExpandAll, Hit::Expand(true)),
            (Icon::CollapseAll, Hit::Expand(false)),
        ] {
            let label = self.icons.glyph(icon);
            if matches!(action, Hit::ViewOptions | Hit::Expand(true)) {
                let divider = self.icons.glyph(Icon::Divider);
                if divider.width() + label.width() + 2 > usize::from(bar.right().saturating_sub(x))
                {
                    break;
                }
                let w = divider.width() as u16;
                word(f, Rect::new(x, bar.y, w, 1), divider, fg(EDGE));
                x += w;
            }
            self.hit_button(f, bar, &mut x, label, action);
        }
        x
    }

    fn tab_marker(&self, root: usize, c: &Context) -> &'static str {
        if self.repo == root {
            self.icons.glyph(Icon::Current)
        } else {
            self.icons.repo(c.repos[root].submodule)
        }
    }

    fn tabs_width(&self, roots: &[usize], c: &Context) -> usize {
        roots
            .iter()
            .map(|&r| self.tab_marker(r, c).width() + c.repos[r].name.width() + 4)
            .sum()
    }

    fn repo_tab(&mut self, f: &mut Frame, bar: Rect, x: &mut u16, root: usize, c: &Context) {
        let room = usize::from(bar.right().saturating_sub(*x)).saturating_sub(2);
        if room == 0 {
            return;
        }
        let marker = self.tab_marker(root, c);
        let name = &c.repos[root].name;
        let label = if marker.width() + 1 + name.width() <= room {
            format!("{marker} {name}")
        } else {
            fit(name, room)
        };
        self.hit_button(f, bar, x, &label, Hit::Repo(root));
    }

    fn repo_tabs(&mut self, f: &mut Frame, bar: Rect, roots: &[usize], c: &Context) {
        let mut x = bar.x;
        if roots.len() > 1 && self.tabs_width(roots, c) > usize::from(bar.width) + 1 {
            let prev = self.icons.glyph(Icon::Previous);
            let next = self.icons.glyph(Icon::Next);
            // In a tiny strip there is room for the current name or the arrows, not both.
            if bar.width < (prev.width() + next.width() + 9) as u16 {
                self.repo_tab(f, bar, &mut x, self.repo, c);
                return;
            }
            let i = roots.iter().position(|&r| r == self.repo).unwrap_or(0);
            let n = roots.len();
            self.hit_button(f, bar, &mut x, prev, Hit::Repo(roots[(i + n - 1) % n]));
            let end = bar.right() - next.width() as u16 - 2;
            self.repo_tab(
                f,
                Rect::new(x, bar.y, end - x - 1, bar.height),
                &mut x,
                self.repo,
                c,
            );
            x = end;
            self.hit_button(f, bar, &mut x, next, Hit::Repo(roots[(i + 1) % n]));
        } else {
            for &r in roots {
                self.repo_tab(f, bar, &mut x, r, c);
            }
        }
    }

    fn draw_preview(&mut self, f: &mut Frame, area: Rect, c: &Context, compact: bool) {
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        self.diff = body;
        let file = self.active().and_then(Row::file);
        let key = self.active().map_or("", |r| r.id.as_str());
        if key != self.diff_key {
            self.diff_key = key.to_owned();
            self.diff_selection.reset();
            self.diff_scroll = 0;
            self.diff_text = file
                .map(|i| {
                    let v = &self.files[i];
                    backend::diff(&c.repos[v.root].path, &v.change)
                        .unwrap_or_else(|e| e.to_string())
                })
                .unwrap_or_default();
        }
        let title = if file.is_some() {
            "Diff preview"
        } else {
            "Overview"
        };
        word(
            f,
            header,
            title,
            bold(if self.pane_focused(PaneId::Preview) {
                BLUE
            } else {
                INK
            }),
        );
        let close = if compact {
            format!("{} Tree", self.icons.glyph(Icon::Back))
        } else {
            self.icons.glyph(Icon::Close).to_owned()
        };
        let mut x = header
            .right()
            .saturating_sub(close.width() as u16 + 2)
            .max(header.x);
        let right = x.saturating_sub(1);
        self.hit_button(f, header, &mut x, &close, Hit::Show(Focus::Tree));
        let Some(row) = self.view.get(self.cursor) else {
            return;
        };
        if let Some(i) = file {
            let v = &self.files[i];
            let source = if v.change.ignored() {
                "Ignored · read-only".to_owned()
            } else if v.change.conflict {
                "Conflict".to_owned()
            } else if v.change.xy == *b"??" {
                "Untracked".to_owned()
            } else if v.change.staged() {
                format!("HEAD {} INDEX", self.icons.glyph(Icon::Right))
            } else {
                "Working tree".to_owned()
            };
            let tag = format!(" {} · {source} ", String::from_utf8_lossy(&v.change.xy));
            let w = tag.width() as u16;
            if right > header.x + title.width() as u16 + w {
                word(
                    f,
                    Rect::new(right - w, header.y, w, 1),
                    &tag,
                    fg(if v.change.conflict { RED } else { DIM }).bg(TAG),
                );
            }
            let repo = &c.repos[v.root];
            let (file_icon, tint) = if v.change.directory() {
                (self.icons.folder(false), DIM)
            } else {
                self.icons.file(&v.change.path)
            };
            let details = vec![Line::from(vec![
                Span::styled(
                    format!("{} {}  /  ", self.icons.repo(repo.submodule), repo.name),
                    fg(repo_tint(v.root)),
                ),
                Span::styled(format!("{file_icon} "), fg(tint)),
                Span::styled(v.change.label(), fg(INK)),
            ])];
            preview_body(
                f,
                body,
                &mut self.diff_scroll,
                &mut self.diff_selection,
                details,
                self.diff_text.split('\n').map(diff_line),
            );
        } else {
            let (checked, _) = self.check_counts(row);
            let conflicts = row
                .files
                .iter()
                .filter(|&&i| self.files[i].change.conflict)
                .count();
            let ignored = row
                .files
                .iter()
                .filter(|&&i| self.files[i].change.ignored())
                .count();
            let mut meta = vec![
                Span::styled(format!("{}  ", row.label), bold(INK)),
                Span::styled(
                    format!("{} · {checked} selected", count(row.files.len(), "file")),
                    fg(DIM),
                ),
            ];
            if conflicts > 0 {
                meta.push(Span::styled(
                    format!(" · {}", count(conflicts, "conflict")),
                    fg(RED),
                ));
            }
            if ignored > 0 {
                meta.push(Span::styled(
                    format!(" · {ignored} ignored · read-only"),
                    fg(DIM),
                ));
            }
            if let Kind::Repo(r) = row.kind {
                meta.push(Span::styled(
                    format!(" · on {}", c.repos[r].branch),
                    fg(DIM),
                ));
            }
            let mut details = vec![Line::from(meta)];
            match &row.kind {
                Kind::Repo(r) => details.push(Line::styled(
                    format!("Worktree  {}", super::display_path(&c.repos[*r].path)),
                    fg(DIM),
                )),
                Kind::Directory(path) if !self.options.modules => {
                    details.push(Line::styled(
                        format!(
                            "Workspace  {}",
                            super::display_path(&super::tree::directory_root(c).join(path))
                        ),
                        fg(DIM),
                    ));
                }
                Kind::Directory(path) => {
                    let root = self.files[row.files[0]].root;
                    details.push(Line::from(vec![
                        Span::styled(
                            format!(
                                "{} {}  /  ",
                                self.icons.repo(c.repos[root].submodule),
                                c.repos[root].name
                            ),
                            fg(repo_tint(root)),
                        ),
                        Span::styled(super::display_path(path), fg(INK)),
                    ]));
                }
                _ => {}
            }
            if row.files.is_empty() {
                preview_body(
                    f,
                    body,
                    &mut self.diff_scroll,
                    &mut self.diff_selection,
                    details,
                    std::iter::once(Line::styled("No changes in this changelist.", fg(DIM))),
                );
                return;
            }
            let groups = row
                .files
                .chunk_by(|a, b| self.files[*a].root == self.files[*b].root);
            let lines = groups
                .flat_map(|files| {
                    let root = self.files[files[0]].root;
                    std::iter::once((root, None, files.len()))
                        .chain(files.iter().map(move |&i| (root, Some(i), 0)))
                })
                .map(|(root, file, n)| {
                    if let Some(i) = file {
                        let v = &self.files[i];
                        let mut line = file_line(
                            self.icons,
                            &v.change,
                            usize::from(body.width.saturating_sub(2)),
                        );
                        line.spans.insert(0, Span::raw("  "));
                        line
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!("{} ", self.icons.repo(c.repos[root].submodule)),
                                fg(repo_tint(root)),
                            ),
                            Span::styled(c.repos[root].name.clone(), bold(INK)),
                            Span::styled(format!("  {}", count(n, "file")), fg(DIM)),
                        ])
                    }
                });
            preview_body(
                f,
                body,
                &mut self.diff_scroll,
                &mut self.diff_selection,
                details,
                lines,
            );
        }
    }
}

// Keep long paths readable with the same scroll controls as the patch, even on a short terminal.
fn preview_body<'a>(
    f: &mut Frame,
    area: Rect,
    scroll: &mut u16,
    selection: &mut TextSelection,
    details: Vec<Line<'a>>,
    content: impl Iterator<Item = Line<'a>>,
) {
    let width = usize::from(area.width);
    if width == 0 {
        return;
    }
    let mut layout = TextLayout::default();
    for line in details {
        layout.push(line, area.width, Wrap::Character);
    }
    layout.decoration(Line::styled("─".repeat(width), fg(EDGE)));
    for line in content {
        layout.push(line, area.width, Wrap::Clip);
    }
    let max = layout
        .rows
        .len()
        .saturating_sub(usize::from(area.height))
        .min(u16::MAX as usize);
    *scroll = (*scroll).min(max as u16);
    selection.render(f, area, layout, usize::from(*scroll), 0);
}
