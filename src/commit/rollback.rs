//! A review of fixed Git snapshots with its own selection and presentation.
use super::{backend::discard, tree::Tree, *};
use crate::ui::{
    help::{HelpContent, HelpSection},
    idea::{button_style, checkbox, clipped, diff_line, BLUE, DIM, EDGE, INK, SELECT, TAG},
    pane,
};
use ratatui::{
    layout::{Constraint, Layout},
    style::Stylize,
    text::Span,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Tree,
    Added,
    Close,
    Apply,
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::core::git;
    use crossterm::event::KeyEvent;
    use ratatui::{backend::TestBackend, Terminal};

    fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
        let dir = tempfile::tempdir().unwrap();
        for name in ["visible", "hidden"] {
            let repo = dir.path().join(name);
            fs::create_dir(&repo).unwrap();
            git(&repo, &["init", "-q", "-b", "main"]).unwrap();
            for (key, value) in [
                ("user.name", "Recovery tester"),
                ("user.email", "recovery@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.hooksPath", "/dev/null"),
            ] {
                git(&repo, &["config", key, value]).unwrap();
            }
            fs::write(repo.join("file"), "base\n").unwrap();
            git(&repo, &["add", "."]).unwrap();
            git(&repo, &["commit", "-qm", "base"]).unwrap();
            fs::write(repo.join("file"), format!("{name} working\n")).unwrap();
        }
        let mut ctx = Context::discover(dir.path()).unwrap();
        ctx.state_dir = dir.path().join("state");
        for (root, repo) in ctx.repos.iter().enumerate() {
            let (plan, unavailable) = discard::review(
                root,
                &repo.path,
                discard::Target::Files(["file".into()].into()),
            )
            .unwrap();
            assert!(unavailable.is_empty());
            discard::execute_in(&plan, true, &ctx.state_dir).unwrap();
        }
        let mut panel = CommitPanel::default();
        panel.refresh(&ctx).unwrap();
        (dir, ctx, panel)
    }

    fn key(dialog: &mut Rollback, ctx: &Context, code: K) -> RollbackEvent {
        dialog.event(&Event::Key(KeyEvent::new(code, Mod::NONE)), ctx)
    }

    #[test]
    fn rollback_recovery_dialog_opens_with_clean_commit_and_reviews_only_visible_roots() {
        let (_dir, mut ctx, mut panel) = fixture();
        let visible = ctx
            .repos
            .iter()
            .position(|repo| repo.name == "visible")
            .unwrap();
        let hidden = ctx
            .repos
            .iter()
            .position(|repo| repo.name == "hidden")
            .unwrap();
        ctx.scope = [visible].into();
        panel.refresh(&ctx).unwrap();
        assert!(matches!(
            panel.discard_review(&ctx).unwrap(),
            Action::ReviewRollback
        ));
        let mut dialog = panel.take_rollback().unwrap();
        assert!(matches!(
            key(&mut dialog, &ctx, K::Char('u')),
            RollbackEvent::Keep
        ));
        let review = dialog.restore.as_ref().unwrap();
        assert_eq!(review.plans.len(), 1);
        assert_eq!(review.plans[0].root, visible);
        assert!(!review.apply);
        for (width, height) in [(120, 36), (80, 24), (44, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| dialog.draw(frame, frame.area(), &ctx))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                text.contains("Restore Last Rollback"),
                "{width}x{height}: {text}"
            );
            assert!(text.contains(" Restore "), "{width}x{height}: {text}");
        }
        assert_eq!(
            fs::read(ctx.repos[visible].path.join("file")).unwrap(),
            b"base\n"
        );
        key(&mut dialog, &ctx, K::Tab);
        assert!(matches!(
            key(&mut dialog, &ctx, K::Enter),
            RollbackEvent::Apply
        ));
        assert!(matches!(
            dialog.apply(&ctx),
            Action::Refresh(NoticeLevel::Success, _)
        ));
        assert_eq!(
            fs::read(ctx.repos[visible].path.join("file")).unwrap(),
            b"visible working\n"
        );
        assert_eq!(
            fs::read(ctx.repos[hidden].path.join("file")).unwrap(),
            b"base\n"
        );
    }

    #[test]
    fn rollback_recovery_dialog_rechecks_roots_before_restoring() {
        let (_dir, mut ctx, panel) = fixture();
        let mut dialog = Rollback::new(&panel, &ctx, &BTreeMap::new());
        key(&mut dialog, &ctx, K::Char('u'));
        assert!(!dialog.restore.as_ref().unwrap().plans.is_empty());
        ctx.scope.clear();
        assert!(matches!(
            dialog.apply(&ctx),
            Action::Refresh(NoticeLevel::Error, message) if message.contains("hidden or changed")
        ));
        for repo in &ctx.repos {
            assert_eq!(fs::read(repo.path.join("file")).unwrap(), b"base\n");
        }
    }
}

#[derive(Clone, Copy)]
enum Hit {
    Row(usize),
    Check(usize),
    Fold(usize),
    FoldAll(bool),
    Diff,
    Groups,
    Group(usize),
    Added,
    Close,
    Apply,
    Help,
    Restore,
}

pub(crate) enum RollbackEvent {
    Keep,
    Close,
    Apply,
    Help,
}

struct Preview {
    path: String,
    text: String,
    scroll: usize,
}

struct RestoreReview {
    plans: Vec<discard::RestorePlan>,
    text: String,
    scroll: usize,
    apply: bool,
}

pub(crate) struct Rollback {
    plans: Vec<discard::Plan>,
    files: Vec<File>,
    unavailable: BTreeMap<String, String>,
    added_copies: BTreeSet<String>,
    groups: Vec<String>,
    checks: BTreeMap<String, bool>,
    fold: BTreeSet<String>,
    options: display::Options,
    icons: IconMode,
    rows: Vec<Row>,
    cursor: usize,
    offset: usize,
    page: usize,
    focus: Target,
    delete_added: bool,
    group_menu: Option<usize>,
    preview: Option<Preview>,
    tree_area: Rect,
    hits: Vec<(Rect, Hit)>,
    restore: Option<RestoreReview>,
}

impl Rollback {
    pub(super) fn new(
        panel: &CommitPanel,
        c: &Context,
        prechecked: &BTreeMap<usize, discard::Target>,
    ) -> Self {
        let files: Vec<_> = panel
            .files
            .iter()
            .filter(|f| c.scope.contains(&f.root))
            .cloned()
            .collect();
        let mut shown = BTreeMap::<usize, Vec<&File>>::new();
        let mut unavailable = BTreeMap::new();
        for f in &files {
            if f.change.ignored() {
                unavailable.insert(f.key.clone(), "Ignored files are read-only".into());
            } else if f.change.conflict {
                unavailable.insert(
                    f.key.clone(),
                    "Resolve the file in the Conflicts tab first".into(),
                );
            } else {
                shown.entry(f.root).or_default().push(f);
            }
        }
        let mut plans = vec![];
        let mut checks = BTreeMap::new();
        let mut added_copies = BTreeSet::new();
        for (root, shown) in shown {
            let target =
                discard::Target::Files(shown.iter().map(|f| f.change.path.clone()).collect());
            let (plan, mut rejected) = match discard::review(root, &c.repos[root].path, target) {
                Ok(review) => review,
                Err(error) => {
                    let reason = format!("{error:#}");
                    unavailable.extend(shown.iter().map(|f| (f.key.clone(), reason.clone())));
                    continue;
                }
            };
            let snapshots: BTreeMap<_, _> = plan
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| (f.path.as_path(), i))
                .collect();
            for file in shown {
                let path = file.change.path.as_path();
                let Some(i) = snapshots
                    .get(path)
                    .copied()
                    .filter(|&i| plan.files[i] == file.change)
                else {
                    unavailable.insert(
                        file.key.clone(),
                        rejected.remove(path).unwrap_or_else(|| {
                            "Change status no longer matches Commit. Close and press F5".into()
                        }),
                    );
                    continue;
                };
                if plan.added_copy(i) {
                    added_copies.insert(file.key.clone());
                }
                checks.insert(
                    file.key.clone(),
                    prechecked
                        .get(&root)
                        .is_some_and(|target| target.contains(path)),
                );
            }
            plans.push(plan);
        }
        let mut dialog = Self {
            plans,
            files,
            unavailable,
            added_copies,
            checks,
            groups: panel.saved.groups.clone(),
            fold: BTreeSet::new(),
            options: display::Options {
                directories: true,
                repositories: true,
                modules: false,
                ignored: panel.options.ignored,
                status: panel.options.status,
            },
            icons: panel.icons,
            rows: vec![],
            cursor: 0,
            offset: 0,
            page: 1,
            focus: Target::Apply,
            delete_added: false,
            group_menu: None,
            preview: None,
            tree_area: Rect::default(),
            hits: vec![],
            restore: None,
        };
        dialog.rebuild(c);
        if let Some(i) = panel
            .active()
            .and_then(|active| dialog.rows.iter().position(|r| r.id == active.id))
        {
            dialog.cursor = i;
        }
        dialog
    }

    fn tree(&self) -> Tree<'_> {
        Tree {
            files: &self.files,
            checks: &self.checks,
            fold: &self.fold,
            groups: &self.groups,
            options: &self.options,
            icons: self.icons,
            show_empty_groups: false,
            merge_conflicts: false,
        }
    }

    fn rebuild(&mut self, c: &Context) {
        let rows = self.tree().rows_with_folds(c, &self.fold);
        self.cursor = self
            .rows
            .get(self.cursor)
            .and_then(|old| rows.iter().position(|r| r.id == old.id))
            .unwrap_or(self.cursor.min(rows.len().saturating_sub(1)));
        self.rows = rows;
        self.hits.clear();
    }

    pub(crate) fn path_copy_target(&self, c: &Context) -> Option<crate::ui::path_copy::Target> {
        if self.restore.is_some()
            || self.focus != Target::Tree
            || self.preview.is_some()
            || self.group_menu.is_some()
        {
            return None;
        }
        self.rows
            .get(self.cursor)?
            .copy_target(c, &self.files, self.options.repository_grouping())
    }

    fn current_file(&self) -> Option<&File> {
        self.rows.get(self.cursor)?.file().map(|i| &self.files[i])
    }

    fn selected(&self) -> bool {
        self.checks.values().any(|&checked| checked)
    }

    fn can_delete_added(&self) -> bool {
        self.added_copies.iter().any(|key| self.checks[key])
    }

    fn check(&mut self, i: usize) {
        let Some(row) = self.rows.get(i) else { return };
        let (checked, eligible) = self.tree().check_counts(row);
        for &id in &row.files {
            if let Some(check) = self.checks.get_mut(&self.files[id].key) {
                *check = checked < eligible;
            }
        }
    }

    fn fold(&mut self, i: usize, c: &Context) {
        let Some(row) = self.rows.get(i).filter(|r| r.foldable()) else {
            return;
        };
        if !self.fold.remove(&row.id) {
            self.fold.insert(row.id.clone());
        }
        self.rebuild(c);
    }

    fn fold_all(&mut self, expand: bool, c: &Context) {
        if expand {
            self.fold.clear();
        } else {
            if let Some(i) = self
                .rows
                .iter()
                .take(self.cursor + 1)
                .rposition(|r| r.kind == Kind::Group)
            {
                self.cursor = i;
            }
            self.fold = self
                .tree()
                .rows_with_folds(c, &BTreeSet::new())
                .into_iter()
                .filter(Row::foldable)
                .map(|row| row.id)
                .collect();
        }
        self.rebuild(c);
    }

    fn regroup(&mut self, option: usize, c: &Context) {
        if option == 0 {
            self.options.directories = !self.options.directories;
        } else {
            self.options.repositories = !self.options.repositories;
        }
        self.fold.clear();
        self.rebuild(c);
    }

    fn toggle_diff(&mut self) {
        if self.preview.take().is_some() {
            return;
        }
        let Some(file) = self.current_file() else {
            return;
        };
        let text = if let Some(reason) = self.unavailable.get(&file.key) {
            format!("Cannot roll back: {reason}")
        } else {
            let plan = self.plans.iter().find(|p| p.root == file.root).unwrap();
            plan.patch(&file.change.path)
                .unwrap_or_else(|e| format!("Cannot show diff: {e:#}"))
        };
        self.preview = Some(Preview {
            path: file.change.label(),
            text,
            scroll: 0,
        });
        self.focus = Target::Tree;
    }

    fn move_row(&mut self, delta: isize) {
        self.focus = Target::Tree;
        if let Some(preview) = &mut self.preview {
            preview.scroll = preview.scroll.saturating_add_signed(delta);
        } else {
            self.cursor = self
                .cursor
                .saturating_add_signed(delta)
                .min(self.rows.len().saturating_sub(1));
        }
    }

    fn move_horizontal(&mut self, open: bool, c: &Context) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        self.focus = Target::Tree;
        if row.foldable() && self.fold.contains(&row.id) == open {
            self.fold(self.cursor, c);
        } else if open {
            if row.foldable() {
                self.move_row(1);
            }
        } else if let Some(i) = self.rows[..self.cursor]
            .iter()
            .rposition(|r| r.depth < row.depth)
        {
            self.cursor = i;
        }
    }

    fn cycle(&mut self, backwards: bool) {
        let enabled = |target| match target {
            Target::Added => self.can_delete_added(),
            Target::Apply => self.selected(),
            _ => true,
        };
        let targets = [Target::Tree, Target::Added, Target::Close, Target::Apply];
        let from = targets.iter().position(|&t| t == self.focus).unwrap();
        self.focus = (1..=4)
            .map(|n| targets[(from + if backwards { 4 - n } else { n }) % 4])
            .find(|&t| enabled(t))
            .unwrap();
    }

    pub(crate) fn event(&mut self, e: &Event, c: &Context) -> RollbackEvent {
        if self.restore.is_some() {
            return self.restore_event(e);
        }
        match e {
            Event::Key(k) if k.modifiers.is_empty() || k.modifiers == Mod::SHIFT => {
                if let Some(row) = self.group_menu {
                    match k.code {
                        K::Esc | K::Char('o') => self.group_menu = None,
                        K::Up | K::Down | K::Char('j' | 'k') | K::Tab | K::BackTab => {
                            self.group_menu = Some(1 - row)
                        }
                        K::Enter | K::Char(' ') => self.regroup(row, c),
                        K::Char('?') => return RollbackEvent::Help,
                        _ => (),
                    }
                    return RollbackEvent::Keep;
                }
                match k.code {
                    K::Char('u') => self.review_restore(c),
                    K::Esc if self.preview.is_some() => self.preview = None,
                    K::Esc | K::Char('n') => return RollbackEvent::Close,
                    K::Char('y') if self.focus != Target::Tree && self.selected() => {
                        return RollbackEvent::Apply
                    }
                    K::Char('?') => return RollbackEvent::Help,
                    K::Tab | K::BackTab => {
                        self.cycle(k.code == K::BackTab || k.modifiers == Mod::SHIFT)
                    }
                    K::Char('a') if self.can_delete_added() => {
                        self.delete_added = !self.delete_added
                    }
                    K::Char('v') => self.toggle_diff(),
                    K::Char('o') => self.group_menu = Some(0),
                    K::Char('t') if self.preview.is_none() => self.regroup(0, c),
                    K::Down | K::Char('j') => self.move_row(1),
                    K::Up | K::Char('k') => self.move_row(-1),
                    K::PageDown => self.move_row(self.page as isize),
                    K::PageUp => self.move_row(-(self.page as isize)),
                    K::Home | K::Char('g') => self.move_row(-isize::MAX),
                    K::End | K::Char('G') => self.move_row(isize::MAX),
                    K::Left | K::Char('h') if self.preview.is_none() => {
                        self.move_horizontal(false, c)
                    }
                    K::Right | K::Char('l') if self.preview.is_none() => {
                        self.move_horizontal(true, c)
                    }
                    K::Char('z' | 'Z') if self.preview.is_none() => {
                        self.fold_all(k.code == K::Char('Z'), c)
                    }
                    K::Char(' ') if self.focus == Target::Added && self.can_delete_added() => {
                        self.delete_added = !self.delete_added
                    }
                    K::Char(' ') if self.preview.is_none() => {
                        self.focus = Target::Tree;
                        self.check(self.cursor);
                    }
                    K::Enter => match self.focus {
                        Target::Close => return RollbackEvent::Close,
                        Target::Apply if self.selected() => return RollbackEvent::Apply,
                        Target::Added if self.can_delete_added() => {
                            self.delete_added = !self.delete_added
                        }
                        Target::Tree if self.preview.is_some() || self.current_file().is_some() => {
                            self.toggle_diff()
                        }
                        Target::Tree => self.fold(self.cursor, c),
                        _ => (),
                    },
                    _ => (),
                }
            }
            Event::Mouse(m) => match m.kind {
                M::Down(MouseButton::Left) => {
                    let hit = self
                        .hits
                        .iter()
                        .rev()
                        .find_map(|(r, hit)| inside(*r, m.column, m.row).then_some(*hit));
                    if self.group_menu.is_some() {
                        if let Some(Hit::Group(i)) = hit {
                            self.group_menu = Some(i);
                            self.regroup(i, c);
                        } else {
                            self.group_menu = None;
                        }
                        return RollbackEvent::Keep;
                    }
                    match hit {
                        Some(hit @ (Hit::Row(i) | Hit::Check(i) | Hit::Fold(i))) => {
                            self.focus = Target::Tree;
                            self.cursor = i;
                            match hit {
                                Hit::Check(_) => self.check(i),
                                Hit::Fold(_) => self.fold(i, c),
                                _ => (),
                            }
                        }
                        Some(Hit::Close) => return RollbackEvent::Close,
                        Some(Hit::Help) => return RollbackEvent::Help,
                        Some(Hit::Restore) => self.review_restore(c),
                        Some(Hit::Apply) if self.selected() => return RollbackEvent::Apply,
                        Some(Hit::Added) if self.can_delete_added() => {
                            self.focus = Target::Added;
                            self.delete_added = !self.delete_added;
                        }
                        Some(Hit::Groups) => self.group_menu = Some(0),
                        Some(Hit::Diff) => self.toggle_diff(),
                        Some(Hit::FoldAll(expand)) if self.preview.is_none() => {
                            self.fold_all(expand, c)
                        }
                        _ => (),
                    }
                }
                M::ScrollUp | M::ScrollDown
                    if self.group_menu.is_none() && inside(self.tree_area, m.column, m.row) =>
                {
                    self.move_row(if m.kind == M::ScrollUp { -3 } else { 3 });
                }
                _ => (),
            },
            _ => (),
        }
        RollbackEvent::Keep
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        let current = if self.restore.is_some() {
            HelpSection::new(
                "Rollback / Restore review",
                &[
                    ("j/k", "Scroll the saved paths and working copies"),
                    ("Tab", "Select Back or Restore"),
                    ("Enter", "Activate the selected control"),
                    ("Esc", "Return without restoring"),
                ],
            )
        } else if self.group_menu.is_some() {
            HelpSection::new(
                "Rollback / Group By",
                &[
                    ("j", "Select next grouping"),
                    ("k", "Select previous grouping"),
                    ("Space", "Toggle grouping"),
                    ("Esc", "Return to rollback"),
                ],
            )
        } else if self.preview.is_some() {
            HelpSection::new(
                "Rollback / Diff",
                &[
                    ("j", "Scroll down"),
                    ("k", "Scroll up"),
                    ("v", "Return to changes"),
                    ("Esc", "Return to changes"),
                ],
            )
        } else {
            HelpSection::new(
                "Rollback / Changes",
                &[
                    ("j", "Highlight next row"),
                    ("k", "Highlight previous row"),
                    ("Space", "Toggle checks at the highlighted row"),
                    ("h", "Collapse row or move to parent"),
                    ("l", "Expand row or move to child"),
                    ("yy", "Copy absolute path"),
                    ("yf", "Copy name"),
                    ("yn", "Copy name without extension"),
                    ("yr", "Copy relative path"),
                    ("PgUp", "Page up"),
                    ("PgDn", "Page down"),
                    ("v", "Preview staged and working-tree changes"),
                    ("o", "Open Group By options"),
                    ("t", "Toggle directory grouping"),
                    ("z", "Collapse all rows"),
                    ("Z", "Expand all rows"),
                    ("a", "Toggle deletion of added local copies"),
                    ("u", "Review the last saved rollback in visible Roots"),
                    ("Tab", "Focus next control"),
                    ("Shift-Tab", "Focus previous control"),
                    ("Enter", "Activate focused control"),
                    (
                        "y",
                        "Roll back checked files when focus is outside the tree",
                    ),
                    ("Esc", "Close without rolling back"),
                ],
            )
        };
        HelpContent::new(current, vec![])
    }

    pub(crate) fn apply(self, c: &Context) -> Action {
        if let Some(review) = self.restore {
            let mut results = vec![];
            let mut succeeded = 0;
            let mut issues = false;
            for plan in review.plans {
                let Some(repo) = c.repos.get(plan.root).filter(|repo| {
                    c.scope.contains(&plan.root)
                        && fs::canonicalize(&repo.path).ok().as_ref() == Some(&plan.repo)
                }) else {
                    issues = true;
                    results.push("Repository hidden or changed; restore skipped".into());
                    continue;
                };
                match plan.execute() {
                    Ok(count) => {
                        succeeded += 1;
                        results.push(format!("{}: restored {count} rollback path(s)", repo.name));
                    }
                    Err(error) => {
                        issues = true;
                        results.push(format!("{}: restore failed: {error:#}", repo.name));
                    }
                }
            }
            return if results.is_empty() {
                Action::None
            } else {
                Action::Refresh(result_level(succeeded, issues), results.join(" | "))
            };
        }
        let mut checked = BTreeMap::<usize, BTreeSet<PathBuf>>::new();
        for f in self
            .files
            .iter()
            .filter(|f| self.checks.get(&f.key) == Some(&true))
        {
            checked
                .entry(f.root)
                .or_default()
                .insert(f.change.path.clone());
        }
        let mut results = vec![];
        let mut succeeded = 0;
        let mut issues = false;
        for plan in self.plans {
            let Some(plan) = checked.get(&plan.root).and_then(|paths| plan.retain(paths)) else {
                continue;
            };
            let Some(repo) = c.repos.get(plan.root).filter(|r| r.path == plan.repo) else {
                issues = true;
                results
                    .push("Workspace changed; rollback skipped. Press F5 and review again".into());
                continue;
            };
            if !c.scope.contains(&plan.root) {
                issues = true;
                results.push(format!(
                    "{} is outside the current scope; rollback skipped",
                    repo.name
                ));
                continue;
            }
            match discard::execute_in(&plan, self.delete_added, &c.state_dir) {
                Ok(()) => {
                    succeeded += 1;
                    results.push(format!(
                        "{}: rolled back {} file{}",
                        repo.name,
                        plan.files.len(),
                        if plan.files.len() == 1 { "" } else { "s" }
                    ));
                }
                Err(e) => {
                    issues = true;
                    results.push(format!("{}: rollback failed: {e:#}", repo.name));
                }
            }
        }
        if results.is_empty() {
            Action::None
        } else {
            Action::Refresh(result_level(succeeded, issues), results.join(" | "))
        }
    }

    fn review_restore(&mut self, c: &Context) {
        let mut plans = vec![];
        let mut text = String::from(
            "Review saved working copies and index entries before restoring.\nUnrelated index entries stay unchanged. Hidden Roots are excluded.\n\n",
        );
        let mut failed = false;
        for &root in &c.scope {
            let Some(repo) = c.repos.get(root) else {
                continue;
            };
            match discard::RestorePlan::latest(root, &repo.path, &c.state_dir) {
                Ok(Some(plan)) => match plan.description() {
                    Ok(description) => {
                        text.push_str(&description);
                        plans.push(plan);
                    }
                    Err(error) => {
                        failed = true;
                        text.push_str(&format!("{}: {error:#}\n\n", repo.name));
                    }
                },
                Ok(None) => (),
                Err(error) => {
                    failed = true;
                    text.push_str(&format!("{}: {error:#}\n\n", repo.name));
                }
            }
        }
        if failed {
            plans.clear();
            text.push_str("Recovery refused. Backups are retained; inspect the errors above.\n");
        } else if plans.is_empty() {
            text.push_str("No saved rollback is available in visible Roots.\n");
        }
        self.restore = Some(RestoreReview {
            plans,
            text,
            scroll: 0,
            apply: false,
        });
        self.hits.clear();
    }

    fn restore_event(&mut self, event: &Event) -> RollbackEvent {
        let review = self.restore.as_mut().unwrap();
        match event {
            Event::Key(key) if key.modifiers.is_empty() || key.modifiers == Mod::SHIFT => {
                match key.code {
                    K::Esc | K::Char('n' | 'u') => self.restore = None,
                    K::Tab | K::BackTab if !review.plans.is_empty() => review.apply = !review.apply,
                    K::Enter if review.apply && !review.plans.is_empty() => {
                        return RollbackEvent::Apply
                    }
                    K::Enter => self.restore = None,
                    K::Char('?') => return RollbackEvent::Help,
                    K::Down | K::Char('j') => review.scroll = review.scroll.saturating_add(1),
                    K::Up | K::Char('k') => review.scroll = review.scroll.saturating_sub(1),
                    K::PageDown => review.scroll = review.scroll.saturating_add(self.page),
                    K::PageUp => review.scroll = review.scroll.saturating_sub(self.page),
                    K::Home | K::Char('g') => review.scroll = 0,
                    K::End | K::Char('G') => review.scroll = usize::MAX,
                    _ => (),
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                M::Down(MouseButton::Left) => {
                    let hit = self.hits.iter().rev().find_map(|(area, hit)| {
                        inside(*area, mouse.column, mouse.row).then_some(*hit)
                    });
                    match hit {
                        Some(Hit::Close) => self.restore = None,
                        Some(Hit::Help) => return RollbackEvent::Help,
                        Some(Hit::Apply) if !review.plans.is_empty() => {
                            return RollbackEvent::Apply
                        }
                        _ => (),
                    }
                }
                M::ScrollUp => review.scroll = review.scroll.saturating_sub(3),
                M::ScrollDown => review.scroll = review.scroll.saturating_add(3),
                _ => (),
            },
            _ => (),
        }
        RollbackEvent::Keep
    }

    fn draw_restore(&mut self, frame: &mut Frame, area: Rect) {
        self.hits.clear();
        let width = area.width.saturating_sub(4).min(74);
        let height = area.height.saturating_sub(2).min(32);
        if width < 34 || height < 9 {
            return;
        }
        let dialog = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.buffer_mut().set_style(area, Style::new().dim());
        frame.render_widget(Clear, dialog);
        let border = pane(" Restore Last Rollback ", true);
        let inner = border.inner(dialog);
        frame.render_widget(border, dialog);
        let [hint, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new("j/k Scroll · Tab Select · Enter Activate").style(Style::new().fg(DIM)),
            hint,
        );
        let review = self.restore.as_mut().unwrap();
        let mut layout = TextLayout::default();
        for line in review.text.lines() {
            layout.push(diff_line(line), body.width, TextWrap::Character);
        }
        self.page = usize::from(body.height).max(1);
        review.scroll = review
            .scroll
            .min(layout.rows.len().saturating_sub(self.page));
        frame.render_widget(
            Paragraph::new(
                layout
                    .rows
                    .into_iter()
                    .skip(review.scroll)
                    .take(self.page)
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
            ),
            body,
        );
        let enabled = !review.plans.is_empty();
        let active = review.apply;
        let back = " Back ";
        let restore = " Restore ";
        let restore_x = footer.right().saturating_sub(restore.len() as u16);
        let back_x = restore_x.saturating_sub(back.len() as u16 + 1);
        self.button(
            frame,
            Rect::new(back_x, footer.y, back.len() as u16, 1),
            back,
            button_style(!active),
            Some(Hit::Close),
        );
        self.button(
            frame,
            Rect::new(restore_x, footer.y, restore.len() as u16, 1),
            restore,
            if enabled {
                button_style(active)
            } else {
                Style::new().fg(DIM).bg(TAG)
            },
            enabled.then_some(Hit::Apply),
        );
    }

    fn button(
        &mut self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        style: Style,
        hit: Option<Hit>,
    ) -> u16 {
        let width = label.width() as u16;
        if width > area.width || area.height == 0 {
            return 0;
        }
        let r = Rect::new(area.x, area.y, width, 1);
        f.render_widget(Paragraph::new(label).style(style), r);
        if let Some(hit) = hit {
            self.hits.push((r, hit));
        }
        width
    }

    pub(crate) fn draw(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        if self.restore.is_some() {
            self.draw_restore(f, a);
            return;
        }
        self.hits.clear();
        let w = a.width.saturating_sub(4).min(74);
        let h = a.height.saturating_sub(2).min(32);
        if w < 34 || h < 9 {
            return;
        }
        let dialog = Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h);
        f.buffer_mut().set_style(a, Style::new().dim());
        f.render_widget(Clear, dialog);
        let border = pane(" Rollback Changes ", false);
        let inner = border.inner(dialog);
        f.render_widget(border, dialog);
        let close = format!(" {} ", self.icons.glyph(Icon::Close));
        let help = format!(" {} ", self.icons.glyph(Icon::Help));
        let cw = close.width() as u16;
        let hw = help.width() as u16;
        self.button(
            f,
            Rect::new(inner.right() - cw, dialog.y, cw, 1),
            &close,
            button_style(false),
            Some(Hit::Close),
        );
        self.button(
            f,
            Rect::new(inner.right() - cw - hw, dialog.y, hw, 1),
            &help,
            button_style(false),
            Some(Hit::Help),
        );

        let pad = u16::from(inner.width >= 44);
        let inner = Rect::new(inner.x + pad, inner.y, inner.width - 2 * pad, inner.height);
        let [bar, tree, summary, option, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(if h >= 20 { 2 } else { 1 }),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let can_diff = self.preview.is_some() || self.current_file().is_some();
        let diff = format!(
            " {} {} ",
            self.icons.glyph(Icon::Diff),
            if self.preview.is_some() {
                "Tree"
            } else {
                "Diff"
            }
        );
        let group = format!(" {} Group By ", self.icons.glyph(Icon::Preview));
        let x = bar.x
            + self.button(
                f,
                bar,
                &diff,
                if can_diff {
                    button_style(self.preview.is_some())
                } else {
                    Style::new().fg(DIM)
                },
                can_diff.then_some(Hit::Diff),
            )
            + 1;
        let group_width = self.button(
            f,
            Rect::new(x, bar.y, bar.right().saturating_sub(x), 1),
            &group,
            button_style(self.group_menu.is_some()),
            Some(Hit::Groups),
        );
        let restore_x = x + group_width;
        let restore_width = self.button(
            f,
            Rect::new(restore_x, bar.y, bar.right().saturating_sub(restore_x), 1),
            " u Restore ",
            button_style(false),
            Some(Hit::Restore),
        );
        let mut right = bar.right();
        for (icon, expand) in [(Icon::CollapseAll, false), (Icon::ExpandAll, true)] {
            let label = format!(" {} ", self.icons.glyph(icon));
            let width = label.width() as u16;
            right = right.saturating_sub(width);
            if right > restore_x + restore_width {
                self.button(
                    f,
                    Rect::new(right, bar.y, width, 1),
                    &label,
                    button_style(false),
                    self.preview.is_none().then_some(Hit::FoldAll(expand)),
                );
            }
        }
        self.draw_tree(f, tree, c);
        self.draw_footer(f, summary, option, footer);
        if let Some(row) = self.group_menu {
            self.draw_groups(f, tree, x, row);
        }
    }

    fn draw_tree(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        self.tree_area = area;
        let border = pane(
            self.preview.as_ref().map_or_else(String::new, |p| {
                format!(
                    " Diff · {} ",
                    clipped(&p.path, usize::from(area.width.saturating_sub(11)))
                )
            }),
            self.focus == Target::Tree,
        );
        let inner = border.inner(area);
        f.render_widget(border, area);
        self.page = usize::from(inner.height).max(1);
        if let Some(p) = &mut self.preview {
            let mut layout = TextLayout::default();
            for line in p.text.lines() {
                layout.push(diff_line(line), inner.width, TextWrap::Character);
            }
            p.scroll = p.scroll.min(layout.rows.len().saturating_sub(self.page));
            f.render_widget(
                Paragraph::new(
                    layout
                        .rows
                        .into_iter()
                        .skip(p.scroll)
                        .take(self.page)
                        .map(|row| row.line)
                        .collect::<Vec<_>>(),
                ),
                inner,
            );
            return;
        }
        self.offset = self
            .offset
            .min(self.cursor)
            .max((self.cursor + 1).saturating_sub(self.page))
            .min(self.rows.len().saturating_sub(self.page));
        for (y, i) in (self.offset..self.rows.len())
            .take(usize::from(inner.height))
            .enumerate()
        {
            let r = Rect::new(inner.x, inner.y + y as u16, inner.width, 1);
            let row = &self.rows[i];
            let model = self.tree();
            let (fold, check) = model.row_controls(row, r);
            let style = if i == self.cursor {
                Style::new().fg(INK).bg(SELECT)
            } else {
                Style::new().fg(INK)
            };
            f.render_widget(
                Paragraph::new(model.change_row(row, c, r.width)).style(style),
                r,
            );
            self.hits.extend([
                (r, Hit::Row(i)),
                (fold, Hit::Fold(i)),
                (check, Hit::Check(i)),
            ]);
        }
        if self.rows.len() > self.page {
            let mut scroll =
                ScrollbarState::new(self.rows.len() - self.page + 1).position(self.offset);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .thumb_style(Style::new().fg(DIM)),
                area,
                &mut scroll,
            );
        }
    }

    fn draw_footer(&mut self, f: &mut Frame, summary: Rect, option: Rect, footer: Rect) {
        let mut counts = [0; 4];
        for file in self
            .files
            .iter()
            .filter(|f| self.checks.get(&f.key) == Some(&true))
        {
            let xy = file.change.xy;
            counts[if xy == *b"??" {
                0
            } else if xy.contains(&b'A') {
                2
            } else if xy.contains(&b'D') {
                3
            } else {
                1
            }] += 1;
        }
        let labels = ["untracked to delete", "modified", "added", "deleted"];
        let stats = counts
            .into_iter()
            .zip(labels)
            .filter(|(n, _)| *n > 0)
            .map(|(n, label)| format!("{n} {label}"))
            .collect::<Vec<_>>()
            .join(" · ");
        let message = if stats.is_empty() {
            "No files selected".into()
        } else if summary.height > 1 {
            format!("{stats}\nBackup before rollback · u Restore last rollback")
        } else {
            format!("{stats} · u Restore last rollback")
        };
        f.render_widget(
            Paragraph::new(message).style(Style::new().fg(BLUE)),
            summary,
        );
        let enabled = self.can_delete_added();
        let added = self.icons.check(enabled && self.delete_added);
        let label = format!("{added} Delete local copies of added files");
        let style = if enabled {
            Style::new().fg(INK)
        } else {
            Style::new().fg(DIM).dim()
        };
        self.button(
            f,
            option,
            &label,
            if self.focus == Target::Added && enabled {
                style.bg(SELECT)
            } else {
                style
            },
            enabled.then_some(Hit::Added),
        );
        let apply = self.selected();
        let roll = " Rollback ";
        let close = " Close ";
        let right = footer.right().saturating_sub(roll.len() as u16);
        let left = right.saturating_sub(close.len() as u16 + 1);
        self.button(
            f,
            Rect::new(left, footer.y, close.len() as u16, 1),
            close,
            if self.focus == Target::Close {
                button_style(false).fg(Color::White).bold()
            } else {
                button_style(false)
            },
            Some(Hit::Close),
        );
        let style = if !apply {
            Style::new().fg(DIM).bg(TAG)
        } else if self.focus == Target::Apply {
            button_style(true).bg(BLUE).fg(PANEL)
        } else {
            button_style(true)
        };
        self.button(
            f,
            Rect::new(right, footer.y, roll.len() as u16, 1),
            roll,
            style,
            apply.then_some(Hit::Apply),
        );
        let hint = clipped(
            "u Restore last rollback · F1 Help",
            usize::from(left.saturating_sub(footer.x + 1)),
        );
        f.render_widget(
            Paragraph::new(hint).style(Style::new().fg(DIM)),
            Rect::new(footer.x, footer.y, left.saturating_sub(footer.x), 1),
        );
    }

    fn draw_groups(&mut self, f: &mut Frame, tree: Rect, x: u16, active: usize) {
        let width = tree.width.min(25);
        let area = Rect::new(
            x.min(tree.right() - width),
            tree.y,
            width,
            tree.height.min(4),
        );
        f.render_widget(Clear, area);
        let border = pane(" Group By ", true).border_style(Style::new().fg(EDGE));
        let inner = border.inner(area);
        f.render_widget(border, area);
        for (i, (label, on)) in [
            ("Directory", self.options.directories),
            ("Repository", self.options.repositories),
        ]
        .into_iter()
        .enumerate()
        .take(usize::from(inner.height))
        {
            let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
            let line = Line::from(vec![
                checkbox(self.icons, on),
                Span::raw(format!(" {label}")),
            ]);
            f.render_widget(
                Paragraph::new(line).style(if i == active {
                    Style::new().bg(SELECT)
                } else {
                    Style::default()
                }),
                r,
            );
            self.hits.push((r, Hit::Group(i)));
        }
    }
}
