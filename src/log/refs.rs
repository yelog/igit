//! One branch tree for the selected repositories. Folder IDs end in '/' so that
//! a branch in one root and a folder of the same name in another can coexist.
use super::{
    cells, fg, fold_slot, model::History, toolbar_field, window, Field, Filter, Hit, Icon,
    IconMode, LogPanel, Pane, BLUE, DIM, EDGE, GOLD, INK, PANEL, SELECT,
};
use crate::core::Context;
use anyhow::Result;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use std::collections::{BTreeMap, BTreeSet};

const GROUPS: [(&str, &str); 3] = [
    ("refs/heads/", "Local"),
    ("refs/remotes/", "Remote"),
    ("refs/tags/", "Tags"),
];
const HEAD: &str = "HEAD (Current Branch)";
const AHEAD: Color = Color::Rgb(102, 158, 95);

pub(super) fn ref_icon(full_name: &str) -> Icon {
    if full_name == "HEAD" {
        Icon::Head
    } else if full_name.starts_with("refs/tags/") {
        Icon::Tag
    } else if full_name.starts_with("refs/remotes/") {
        Icon::Remote
    } else {
        Icon::Branch
    }
}

#[derive(Default)]
pub(super) struct RefTree {
    rows: Vec<Row>,
    cursor: usize,
    // A folder's normal state depends on its level and whether it contains HEAD.
    expanded: BTreeMap<String, bool>,
    query: String,
    // Searches open matching paths without disturbing the user's regular tree.
    search_closed: BTreeSet<String>,
}

struct Row {
    id: String,
    action: RowAction,
    line: Line<'static>,
}

#[derive(Clone, Copy)]
enum RowAction {
    Select,
    Fold { open: bool },
    Empty,
}

#[derive(Default)]
struct Owners {
    roots: Vec<usize>,
    current: bool,
    remote_head: bool,
    ahead: usize,
    behind: usize,
}

impl Owners {
    fn pinned(&self) -> bool {
        self.current || self.remote_head
    }
}

#[derive(Default)]
struct Folder<'a> {
    dirs: BTreeMap<&'a str, Folder<'a>>,
    leaves: Vec<Leaf<'a>>,
    has_pin: bool,
}

struct Leaf<'a> {
    name: &'a str,
    label: &'a str,
    owners: Owners,
}

impl<'a> Folder<'a> {
    fn insert(&mut self, name: &'a str, short: &'a str, owners: Owners) {
        let mut parts = short.split('/').peekable();
        let mut dir = self;
        while let Some(part) = parts.next() {
            dir.has_pin |= owners.pinned();
            if parts.peek().is_none() {
                dir.leaves.push(Leaf {
                    name,
                    label: part,
                    owners,
                });
                break;
            }
            dir = dir.dirs.entry(part).or_default();
        }
    }
}

impl RefTree {
    pub(super) fn set_cursor(&mut self, n: usize) {
        self.cursor = n.min(self.rows.len().saturating_sub(1));
    }

    pub(super) fn move_by(&mut self, d: isize) {
        self.set_cursor(self.cursor.saturating_add_signed(d));
    }

    pub(super) fn reset_cursor(&mut self) {
        let first = self
            .rows
            .iter()
            .position(|row| matches!(row.action, RowAction::Select))
            .unwrap_or(0);
        self.set_cursor(first);
    }

    fn rebuild(
        &mut self,
        hist: &History,
        filter: &Filter,
        c: &Context,
        query: &str,
        icons: IconMode,
    ) {
        let query = query.trim().to_lowercase();
        if query != self.query {
            self.search_closed.clear();
            self.query = query;
        }
        let previous = self.rows.get(self.cursor).map(|row| row.id.clone());
        self.rows.clear();
        if HEAD.to_lowercase().contains(&self.query) {
            self.rows.push(Row {
                id: "HEAD".into(),
                action: RowAction::Select,
                line: Line::from(vec![
                    Span::styled(icons.glyph(Icon::Head), fg(GOLD)),
                    Span::styled(format!(" {HEAD}"), fg(INK)),
                ]),
            });
        }
        // Scope refs by the applied paths, even when no commits match those paths.
        let roots: BTreeSet<_> = c
            .scope
            .iter()
            .copied()
            .filter(|&root| filter.includes_root(root) && filter.paths.for_root(root).is_some())
            .collect();
        let mut by_name = BTreeMap::<&str, Owners>::new();
        for branch in &hist.branches {
            if !roots.contains(&branch.root) || c.repos.get(branch.root).is_none() {
                continue;
            }
            let Some((prefix, _)) = GROUPS.iter().find(|(p, _)| branch.name.starts_with(p)) else {
                continue;
            };
            let searchable = if self.query.starts_with("refs/") {
                branch.name.as_str()
            } else {
                &branch.name[prefix.len()..]
            };
            if !self.query.is_empty() && !searchable.to_lowercase().contains(&self.query) {
                continue;
            }
            let owners = by_name.entry(&branch.name).or_default();
            owners.roots.push(branch.root);
            owners.current |= hist.current.get(&branch.root) == Some(&branch.name);
            let key = (branch.root, branch.name.clone());
            owners.remote_head |= hist.remote_heads.contains(&key);
            if let Some(&(ahead, behind)) = hist.tracking.get(&key) {
                // A shared row represents all of its roots, including their outgoing/incoming commits.
                owners.ahead = owners.ahead.saturating_add(ahead);
                owners.behind = owners.behind.saturating_add(behind);
            }
        }
        let mut groups = [Folder::default(), Folder::default(), Folder::default()];
        for (name, owners) in by_name {
            for (group, &(prefix, _)) in groups.iter_mut().zip(&GROUPS) {
                if let Some(short) = name.strip_prefix(prefix) {
                    group.insert(name, short, owners);
                    break;
                }
            }
        }
        for (dir, (prefix, label)) in groups.into_iter().zip(GROUPS) {
            if self.query.is_empty() || !dir.leaves.is_empty() || !dir.dirs.is_empty() {
                self.append_folder(&dir, prefix, label, 0, c, icons);
            }
        }
        let n = previous
            .and_then(|id| self.rows.iter().position(|r| r.id == id))
            .unwrap_or(self.cursor);
        self.set_cursor(n);
    }

    fn append_folder(
        &mut self,
        dir: &Folder,
        id: &str,
        label: &str,
        depth: usize,
        c: &Context,
        icons: IconMode,
    ) {
        let empty = dir.dirs.is_empty() && dir.leaves.is_empty();
        let open = if self.query.is_empty() {
            let default_open =
                depth == 0 || (depth == 1 && id.starts_with("refs/remotes/")) || dir.has_pin;
            self.expanded.get(id).copied().unwrap_or(default_open)
        } else {
            !self.search_closed.contains(id)
        };
        let mut spans = vec![
            Span::raw("  ".repeat(depth)),
            Span::styled(fold_slot(icons, (!empty).then_some(open)), fg(DIM)),
        ];
        if depth > 0 {
            spans.push(Span::styled(
                format!("{} ", icons.folder(open && !empty)),
                fg(INK),
            ));
        }
        spans.push(Span::styled(
            label.to_owned(),
            fg(if empty { DIM } else { INK }),
        ));
        self.rows.push(Row {
            id: id.into(),
            action: if empty {
                RowAction::Empty
            } else {
                RowAction::Fold { open }
            },
            line: Line::from(spans),
        });
        if !open || empty {
            return;
        }
        // Git names are sorted within each section. Pinned branch paths stay
        // visible even when the checked-out branch itself contains slashes.
        for leaf in dir.leaves.iter().filter(|leaf| leaf.owners.pinned()) {
            self.append_leaf(leaf, depth + 1, c, icons);
        }
        for pinned in [true, false] {
            for (&name, sub) in dir.dirs.iter().filter(|(_, sub)| sub.has_pin == pinned) {
                self.append_folder(sub, &format!("{id}{name}/"), name, depth + 1, c, icons);
            }
        }
        for leaf in dir.leaves.iter().filter(|leaf| !leaf.owners.pinned()) {
            self.append_leaf(leaf, depth + 1, c, icons);
        }
    }

    fn append_leaf(&mut self, leaf: &Leaf, depth: usize, c: &Context, icons: IconMode) {
        let owners = &leaf.owners;
        let (mark, ink) = if owners.current {
            (Icon::Current, GOLD)
        } else if owners.remote_head {
            (Icon::Favorite, GOLD)
        } else {
            let kind = ref_icon(leaf.name);
            (kind, if kind == Icon::Tag { GOLD } else { DIM })
        };
        let mut spans = vec![
            Span::raw(format!("{}{}", "  ".repeat(depth), fold_slot(icons, None))),
            Span::styled(format!("{} ", icons.glyph(mark)), fg(ink)),
            Span::styled(
                leaf.label.to_owned(),
                if owners.pinned() {
                    fg(INK).add_modifier(Modifier::BOLD)
                } else {
                    fg(INK)
                },
            ),
        ];
        if owners.roots.len() < c.scope.len() {
            let names = owners
                .roots
                .iter()
                .map(|&i| c.repos[i].name.as_str())
                .collect::<Vec<_>>();
            spans.push(Span::styled(format!(" ({})", names.join(", ")), fg(DIM)));
        }
        if owners.behind > 0 {
            spans.push(Span::styled(
                format!(" {}{}", icons.glyph(Icon::Behind), owners.behind),
                fg(BLUE),
            ));
        }
        if owners.ahead > 0 {
            spans.push(Span::styled(
                format!(" {}{}", icons.glyph(Icon::Ahead), owners.ahead),
                fg(AHEAD),
            ));
        }
        self.rows.push(Row {
            id: leaf.name.into(),
            action: RowAction::Select,
            line: Line::from(spans),
        });
    }

    fn toggle(&mut self) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        let RowAction::Fold { open } = row.action else {
            return;
        };
        if self.query.is_empty() {
            self.expanded.insert(row.id.clone(), !open);
        } else if open {
            self.search_closed.insert(row.id.clone());
        } else {
            self.search_closed.remove(&row.id);
        }
    }
}

impl LogPanel {
    pub(super) fn rebuild_refs(&mut self, c: &Context) {
        let query = match &self.editor {
            Some((Field::Refs, text)) => text,
            _ => &self.ref_query,
        };
        self.refs
            .rebuild(&self.hist, &self.filter, c, query, self.icons);
    }

    pub(super) fn ref_horizontal(&mut self, left: bool, c: &Context) -> bool {
        if self
            .refs
            .rows
            .get(self.refs.cursor)
            .is_some_and(|row| matches!(row.action, RowAction::Fold { open } if open == left))
        {
            self.refs.toggle();
            self.rebuild_refs(c);
            true
        } else {
            false
        }
    }

    pub(super) fn activate_ref(&mut self, c: &Context) -> Result<()> {
        let Some(row) = self.refs.rows.get(self.refs.cursor) else {
            return Ok(());
        };
        match row.action {
            RowAction::Fold { .. } => {
                self.refs.toggle();
                self.rebuild_refs(c);
            }
            RowAction::Select if self.filter.single_branch() != Some(row.id.as_str()) => {
                self.filter.branch = Some(row.id.as_str().into());
                self.reload(c, false)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn draw_refs(&mut self, f: &mut Frame, a: Rect, _: &Context) {
        if a.is_empty() {
            return;
        }
        let bar = Rect::new(a.x, a.y, a.width, 1);
        let (text, editing) = match &self.editor {
            Some((Field::Refs, text)) => (text.as_str(), true),
            _ if self.ref_query.is_empty() => ("Branch or tag", false),
            _ => (self.ref_query.as_str(), false),
        };
        let clear = toolbar_field(
            f,
            bar,
            text,
            editing,
            self.icons,
            !editing && !self.ref_query.is_empty(),
        );
        self.hit(bar, Hit::Filter(Field::Refs));
        if let Some(clear) = clear {
            self.hit(clear, Hit::Clear(Field::Refs));
        }
        let avail = a.height.saturating_sub(1) as usize;
        if avail == 0 {
            return;
        }
        if self.refs.rows.is_empty() {
            f.render_widget(
                Paragraph::new(" No matching branches or tags").style(fg(DIM).bg(PANEL)),
                Rect::new(a.x, a.y + 1, a.width, 1),
            );
            return;
        }
        let count = self.refs.rows.len();
        let start = window(self.refs.cursor, avail, count);
        let overflow = count > avail && a.width > 1;
        for n in start..(start + avail).min(count) {
            let row = &self.refs.rows[n];
            let area = Rect::new(
                a.x,
                a.y + 1 + (n - start) as u16,
                a.width - u16::from(overflow),
                1,
            );
            let bg = if self.focus == Pane::Refs && n == self.refs.cursor {
                SELECT
            } else {
                PANEL
            };
            f.buffer_mut().set_style(area, fg(INK).bg(bg));
            let marker = self.icons.glyph(Icon::Filter);
            let mut content = area;
            let gutter = super::view::take(&mut content, cells(marker));
            if row.id == "HEAD" && row.line.width() > usize::from(content.width) {
                let head = self.icons.glyph(Icon::Head);
                let mut label = Line::default();
                if content.width >= cells(head) + 5 {
                    label.push_span(Span::styled(format!("{head} "), fg(GOLD)));
                }
                label.push_span(
                    if usize::from(content.width) >= label.width() + "HEAD (current)".len() {
                        "HEAD (current)"
                    } else {
                        "HEAD"
                    },
                );
                f.render_widget(label, content);
            } else if row.line.width() > usize::from(content.width) && row.id != "HEAD" {
                // Deep reference folders can exhaust a narrow pane before its
                // branch name. Reduce leading whitespace before clipping the text.
                let mut line = row.line.clone();
                if let Some(indent) = line.spans.first_mut() {
                    let keep = indent
                        .content
                        .len()
                        .min(usize::from(content.width).saturating_sub(13));
                    indent.content.to_mut().truncate(keep);
                }
                f.render_widget(line, content);
            } else {
                f.render_widget(&row.line, content);
            }
            if self.filter.single_branch() == Some(row.id.as_str()) {
                f.render_widget(Span::styled(marker, fg(BLUE).bg(bg)), gutter);
            }
            self.hit(area, Hit::Ref(n));
        }
        if overflow {
            let mut state = ScrollbarState::new(count - avail + 1).position(start);
            let scroll = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(None)
                .thumb_symbol("┃")
                .thumb_style(fg(EDGE).bg(PANEL));
            f.render_stateful_widget(
                scroll,
                Rect::new(a.x, a.y + 1, a.width, avail as u16),
                &mut state,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{git, Repo},
        log::model::{Branch, PathFilter},
        ui::Panel,
    };
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;
    use tempfile::TempDir;

    fn context() -> Context {
        Context {
            repos: ["api", "web", "kit"]
                .map(|name| Repo {
                    name: name.into(),
                    path: name.into(),
                    branch: "main".into(),
                    submodule: false,
                    parent: None,
                })
                .into(),
            scope: (0..3).collect(),
            workspace: Default::default(),
            state_dir: Default::default(),
        }
    }

    fn row(tree: &RefTree, id: &str) -> Option<usize> {
        tree.rows.iter().position(|r| r.id == id)
    }

    #[test]
    fn shared_rows_show_scopes_real_pins_and_upstream_counts() {
        let mut c = context();
        c.scope.remove(&2);
        let mut history = History::default();
        for (root, name) in [
            (0, "refs/heads/main"),
            (0, "refs/heads/feat/search"),
            (0, "refs/heads/one"),
            (1, "refs/heads/main"),
            (1, "refs/remotes/origin/primary"),
            (1, "refs/remotes/origin/bug/work"),
            (2, "refs/heads/outside"),
        ] {
            history.branches.push(Branch {
                root,
                name: name.into(),
            });
        }
        let main = "refs/heads/main".to_owned();
        history.current.insert(0, main.clone());
        history.tracking.insert((0, main.clone()), (4, 1));
        history.tracking.insert((1, main.clone()), (0, 2));
        history
            .remote_heads
            .insert((1, "refs/remotes/origin/primary".into()));
        let mut tree = RefTree::default();
        tree.rebuild(&history, &Filter::default(), &c, "", IconMode::Unicode);
        let ids: Vec<_> = tree.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "HEAD",
                "refs/heads/",
                "refs/heads/main",
                "refs/heads/feat/",
                "refs/heads/one",
                "refs/remotes/",
                "refs/remotes/origin/",
                "refs/remotes/origin/primary",
                "refs/remotes/origin/bug/",
                "refs/tags/",
            ]
        );
        let label = |id| tree.rows[row(&tree, id).unwrap()].line.to_string();
        assert_eq!(label(&main).trim(), "● main ↙3 ↗4");
        assert_eq!(
            label("refs/heads/one").trim(),
            format!("{} one (api)", IconMode::Unicode.glyph(Icon::Branch))
        );
        assert_eq!(
            label("refs/remotes/origin/primary").trim(),
            "★ primary (web)"
        );
    }

    #[test]
    fn paths_ref_tree_intersects_scopes_before_aggregating_owners() {
        let mut c = context();
        let mut panel = LogPanel {
            icons: IconMode::Unicode,
            ..Default::default()
        };
        for (root, repo) in c.repos.iter().enumerate() {
            for name in [
                "refs/heads/shared".to_owned(),
                format!("refs/heads/only-{}", repo.name),
                "refs/remotes/origin/shared".to_owned(),
                format!("refs/remotes/origin/only-{}", repo.name),
                "refs/tags/shared".to_owned(),
                format!("refs/tags/only-{}", repo.name),
            ] {
                panel.hist.branches.push(Branch { root, name });
            }
            panel
                .hist
                .tracking
                .insert((root, "refs/heads/shared".into()), (root + 1, root + 2));
        }
        // Pins and counts from an excluded repository must not leak into shared rows.
        panel.hist.current.insert(0, "refs/heads/shared".into());
        panel.hist.current.insert(1, "refs/heads/only-web".into());
        panel
            .hist
            .remote_heads
            .insert((0, "refs/remotes/origin/shared".into()));
        panel.filter.roots = Some([0, 1].into());
        panel.filter.paths = PathFilter::parse("web/docs\nkit/docs", &c).unwrap();
        panel.rebuild_refs(&c);
        let ids: BTreeSet<_> = panel
            .refs
            .rows
            .iter()
            .filter(|r| matches!(r.action, RowAction::Select))
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "HEAD",
                "refs/heads/shared",
                "refs/heads/only-web",
                "refs/remotes/origin/shared",
                "refs/remotes/origin/only-web",
                "refs/tags/shared",
                "refs/tags/only-web",
            ]
            .into()
        );
        let label = |id| {
            panel.refs.rows[row(&panel.refs, id).unwrap()]
                .line
                .to_string()
        };
        assert_eq!(
            label("refs/heads/shared").trim(),
            format!("{} shared (web) ↙3 ↗2", panel.icons.glyph(Icon::Branch))
        );
        assert_eq!(label("refs/heads/only-web").trim(), "● only-web (web)");
        assert_eq!(
            label("refs/remotes/origin/shared").trim(),
            format!("{} shared (web)", panel.icons.glyph(Icon::Remote))
        );

        panel.ref_query = "only-".into();
        panel.rebuild_refs(&c);
        assert!(row(&panel.refs, "refs/heads/only-web").is_some());
        assert!(row(&panel.refs, "refs/heads/only-api").is_none());
        assert!(row(&panel.refs, "refs/tags/only-kit").is_none());
        c.scope.remove(&1);
        panel.rebuild_refs(&c);
        assert!(panel.refs.rows.is_empty());
    }

    #[test]
    fn a_search_opens_ancestors_and_restores_the_previous_folds() {
        let c = context();
        let mut panel = LogPanel::default();
        // A branch and an identically named folder really can exist across two repos.
        for (root, name) in [
            (0, "refs/heads/feat"),
            (1, "refs/heads/feat/ui/needle"),
            (1, "refs/heads/feat/ui/unrelated"),
            (1, "refs/heads/feat/mobile/ios"),
        ] {
            panel.hist.branches.push(Branch {
                root,
                name: name.into(),
            });
        }
        panel.rebuild_refs(&c);
        assert!(row(&panel.refs, "refs/heads/feat").is_some());
        let parent = row(&panel.refs, "refs/heads/feat/").unwrap();
        panel.refs.set_cursor(parent);
        assert!(panel.ref_horizontal(false, &c));
        assert!(!panel.ref_horizontal(false, &c));
        assert!(row(&panel.refs, "refs/heads/feat/ui/").is_some());
        assert!(row(&panel.refs, "refs/heads/feat/ui/needle").is_none());
        panel.editor = Some((Field::Refs, "NEEDLE".into()));
        panel.rebuild_refs(&c);
        let ids: Vec<_> = panel.refs.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "refs/heads/",
                "refs/heads/feat/",
                "refs/heads/feat/ui/",
                "refs/heads/feat/ui/needle",
            ]
        );
        panel
            .refs
            .set_cursor(row(&panel.refs, "refs/heads/feat/ui/").unwrap());
        panel.activate_ref(&c).unwrap();
        assert!(row(&panel.refs, "refs/heads/feat/ui/needle").is_none());
        assert!(panel.ref_horizontal(false, &c));
        assert!(row(&panel.refs, "refs/heads/feat/ui/needle").is_some());
        panel.apply(&c).unwrap();
        assert_eq!(
            panel.refs.rows[panel.refs.cursor].id,
            "refs/heads/feat/ui/needle"
        );
        panel.clear_field(Field::Refs, &c).unwrap();
        assert!(row(&panel.refs, "refs/heads/feat/mobile/").is_some());
        assert!(row(&panel.refs, "refs/heads/feat/ui/needle").is_none());
        assert!(panel.filter.branch.is_none());
    }

    #[test]
    fn mouse_and_enter_select_the_actual_branch_or_tag_in_all_matching_repositories() {
        let d = TempDir::new().unwrap();
        let mut c = context();
        for (i, repo) in c.repos.iter_mut().enumerate() {
            repo.path = d.path().join(&repo.name);
            fs::create_dir(&repo.path).unwrap();
            git(&repo.path, &["init", "-q", "-b", "main"]).unwrap();
            git(
                &repo.path,
                &[
                    "-c",
                    "user.name=Tester",
                    "-c",
                    "user.email=refs@example.invalid",
                    "-c",
                    "commit.gpgSign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    &repo.name,
                ],
            )
            .unwrap();
            let op = if i < 2 { "branch" } else { "tag" };
            git(&repo.path, &["-c", "tag.gpgSign=false", op, "release/red"]).unwrap();
        }
        let mut panel = LogPanel::default();
        panel.refresh(&c).unwrap();
        panel.ref_query = "release/red".into();
        panel.rebuild_refs(&c);
        let mut term = Terminal::new(TestBackend::new(40, 14)).unwrap();
        term.draw(|f| panel.draw_refs(f, f.area(), &c)).unwrap();
        let id = row(&panel.refs, "refs/heads/release/red").unwrap();
        let area = panel
            .hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, Hit::Ref(n) if *n == id).then_some(*rect))
            .unwrap();
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: area.x + 1,
                row: area.y,
                modifiers: KeyModifiers::NONE,
            }),
            &c,
        );
        let roots = |p: &LogPanel| {
            p.visible
                .iter()
                .map(|&i| p.hist.revs[i].root)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(panel.filter.single_branch(), Some("refs/heads/release/red"));
        assert_eq!(roots(&panel), [0, 1].into());
        panel
            .refs
            .set_cursor(row(&panel.refs, "refs/tags/release/red").unwrap());
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        panel.event(&key(KeyCode::Enter), &c);
        assert_eq!(panel.filter.single_branch(), Some("refs/tags/release/red"));
        assert_eq!(roots(&panel), [2].into());
        panel.hits.clear();
        term.draw(|f| panel.draw_refs(f, f.area(), &c)).unwrap();
        let clear = panel
            .hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, Hit::Clear(Field::Refs)).then_some(*rect))
            .unwrap();
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: clear.x,
                row: clear.y,
                modifiers: KeyModifiers::NONE,
            }),
            &c,
        );
        assert!(panel.ref_query.is_empty());
        assert_eq!(roots(&panel), [2].into());
        panel.event(&key(KeyCode::Char('x')), &c);
        assert!(panel.filter.branch.is_none());
        assert!(panel.ref_query.is_empty());
        assert_eq!(roots(&panel), c.scope);
    }
}
