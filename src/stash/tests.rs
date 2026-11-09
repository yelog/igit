use super::*;
use crate::core::git;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::{fs, path::Path, time::Instant};

fn run(path: &Path, args: &[&str]) -> String {
    git(path, args).unwrap()
}

pub(super) fn fixture() -> (tempfile::TempDir, Context, StashPanel) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["api", "web", "empty"] {
        let repo = temp.path().join(name);
        fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Stash tester"),
            ("user.email", "stash@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            run(&repo, &["config", key, value]);
        }
        if name == "empty" {
            continue;
        }
        fs::write(repo.join("file.txt"), "base\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "base"]);
        for n in 1..=2 {
            fs::write(repo.join("file.txt"), format!("saved {name} {n}\n")).unwrap();
            fs::write(repo.join("新增.txt"), format!("untracked {name} {n}\n")).unwrap();
            run(
                &repo,
                &["stash", "push", "-u", "-m", &format!("{name} work {n}")],
            );
        }
    }
    let mut c = Context::discover(temp.path()).unwrap();
    c.state_dir = temp.path().join("state");
    let mut panel = StashPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    panel.refresh(&c).unwrap();
    (temp, c, panel)
}

fn key(panel: &mut StashPanel, c: &Context, key: K) -> Action {
    panel.event(&Event::Key(KeyEvent::new(key, Mods::NONE)), c)
}

fn finish(panel: &mut StashPanel, c: &Context) {
    let timeout = Instant::now() + Duration::from_secs(5);
    while panel.preview_wakeup().is_some() {
        panel.poll_preview(c);
        assert!(Instant::now() < timeout, "Stash preview did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        panel.preview.message().is_none(),
        "{:?}",
        panel.preview.message()
    );
}

fn render(panel: &mut StashPanel, c: &Context, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), c)).unwrap();
    terminal.backend().buffer().clone()
}

fn text_in(buffer: &Buffer, area: Rect) -> String {
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn screen(panel: &mut StashPanel, c: &Context, w: u16, h: u16) -> String {
    let buffer = render(panel, c, w, h);
    text_in(&buffer, buffer.area)
}

#[test]
fn stash_lists_saved_entries_on_one_row_and_groups_empty_repositories() {
    let (_temp, c, mut panel) = fixture();
    assert_eq!(panel.repos.len(), 3);
    for repo in &panel.repos {
        if c.repos[repo.root].name == "empty" {
            assert!(repo.entries.is_empty() && repo.error.is_none());
        } else {
            assert_eq!(repo.entries.len(), 2);
            assert_eq!(repo.entries[0].selector, "stash@{0}");
            assert!(repo.entries[0].subject.ends_with("work 2"));
            assert_eq!(repo.entries[1].selector, "stash@{1}");
            assert_eq!(repo.entries[0].saved.len(), 25);
        }
    }
    finish(&mut panel, &c);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        let buffer = render(&mut panel, &c, w, h);
        let list = text_in(&buffer, panel.viewport);
        let diff = text_in(&buffer, panel.panes.rect(PaneId::Files).unwrap());
        let row = list.lines().find(|row| row.contains("api work 2")).unwrap();
        assert!(row.trim_start().starts_with("api work 2"), "{list}");
        assert_eq!(
            list.lines()
                .filter(|row| row.contains("api work 2"))
                .count(),
            1
        );
        assert!(
            diff.contains("api / stash@{0}") && diff.contains("file.txt"),
            "{diff}"
        );
        if w == 120 {
            let saved = &panel.selection().unwrap().1.saved;
            assert!(row.contains("#0") && row.contains(&saved[..16]), "{row}");
            assert!(diff.contains(saved), "{diff}");
        }
        if w == 44 {
            assert!(!row.contains("#0"), "{row}");
            assert!(list.contains("1 without stashes"), "{list}");
        }
    }
    let text = screen(&mut panel, &c, 120, 36);
    assert_eq!(panel.hits.len(), 7, "{text}");
    assert!(text.find("web work 1").unwrap() < text.find("1 repo without stashes").unwrap());
    assert!(!text.contains("empty"), "{text}");

    let summary = panel.hits.last().unwrap().0;
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: summary.x + 1,
            row: summary.y,
            modifiers: Mods::NONE,
        }),
        &c,
    );
    assert!(screen(&mut panel, &c, 120, 36).contains("empty"));
    panel.refresh(&c).unwrap();
    assert!(screen(&mut panel, &c, 120, 36).contains("empty"));
    key(&mut panel, &c, K::Char('l')); // Enter the expanded group.
    for shortcut in ['a', 'P', 'd'] {
        assert!(
            matches!(key(&mut panel, &c, K::Char(shortcut)), Action::Notice(_)),
            "An empty repository must not target a stash"
        );
    }
    key(&mut panel, &c, K::Char('h')); // Return to the summary.
    key(&mut panel, &c, K::Char('h')); // Collapse it.
    assert!(!screen(&mut panel, &c, 120, 36).contains("empty"));
    key(&mut panel, &c, K::Enter);
    assert!(screen(&mut panel, &c, 120, 36).contains("empty"));
}

#[test]
fn stash_narrow_diff_keeps_repository_name_and_selector_visible_above_files() {
    let (_temp, mut c, mut panel) = fixture();
    let (root, _) = panel.identity().unwrap();
    c.repos[root].name = "界面工具-web".into();
    finish(&mut panel, &c);
    key(&mut panel, &c, K::Enter);
    let buffer = render(&mut panel, &c, 44, 12);
    let diff = text_in(&buffer, panel.panes.rect(PaneId::Files).unwrap());
    let repo_row = diff
        .lines()
        .position(|row| row.contains("界 面 工 具 -web"));
    let selector_row = diff.lines().position(|row| row.contains("stash@{0}"));
    let toolbar_row = diff.lines().position(|row| row.contains(" Diff "));
    assert!(
        repo_row.is_some() && repo_row < selector_row && selector_row < toolbar_row,
        "{diff}"
    );
    assert!(diff.contains("file.txt"), "{diff}");
    key(&mut panel, &c, K::Enter);
    assert!(panel.editing());
}

#[test]
fn stash_selection_survives_renumbering_and_roots_never_keep_a_hidden_preview() {
    let (_temp, mut c, mut panel) = fixture();
    let (root, oid) = panel.identity().unwrap();
    let repo = &c.repos[root].path;
    fs::write(repo.join("file.txt"), "new top\n").unwrap();
    run(repo, &["stash", "push", "-m", "new top"]);
    panel.refresh(&c).unwrap();
    assert_eq!(panel.identity(), Some((root, oid)));
    assert_eq!(panel.selection().unwrap().1.selector, "stash@{1}");
    let text = screen(&mut panel, &c, 120, 36);
    assert!(
        text.contains("api / stash@{1}") && text.contains("Loading saved files"),
        "{text}"
    );
    panel.poll_preview(&c); // A result for the old scope must never be installed.
    c.scope.remove(&root);
    panel.refresh(&c).unwrap();
    finish(&mut panel, &c);
    assert_ne!(panel.identity().unwrap().0, root);
    let text = screen(&mut panel, &c, 120, 36);
    assert!(!text.contains("new top"));
    c.scope.clear();
    panel.refresh(&c).unwrap();
    finish(&mut panel, &c);
    assert!(panel.rows.is_empty() && panel.identity().is_none());
    assert!(screen(&mut panel, &c, 80, 25).contains("No repositories in Roots"));
}

#[test]
fn stash_files_enter_mouse_diff_and_file_navigation_use_saved_content() {
    let (_temp, c, mut panel) = fixture();
    let (root, _) = panel.identity().unwrap();
    let path = &c.repos[root].path;
    fs::write(path.join("file.txt"), "LIVE WORKTREE MUST NOT APPEAR\n").unwrap();
    run(path, &["add", "file.txt"]);
    let state = || {
        [
            vec!["rev-parse", "HEAD"],
            vec!["ls-files", "--stage"],
            vec!["diff", "--binary"],
            vec!["diff", "--cached", "--binary"],
            vec!["stash", "list"],
            vec!["status", "--porcelain=v1"],
        ]
        .map(|args| run(path, &args))
    };
    let before = state();
    finish(&mut panel, &c);
    key(&mut panel, &c, K::Enter); // Focus the files, whose first leaf is selected.
    key(&mut panel, &c, K::Enter);
    assert!(panel.editing());
    let text = screen(&mut panel, &c, 120, 36);
    assert!(
        text.contains("Before") && text.contains("After") && text.contains("saved api 2"),
        "{text}"
    );
    assert!(!text.contains("LIVE WORKTREE"));
    assert_eq!(panel.keymap().context(), "Stash / Diff (read-only)");
    key(&mut panel, &c, K::Char(']'));
    let text = screen(&mut panel, &c, 120, 36);
    assert!(text.contains("untracked api 2"), "{text}");
    key(&mut panel, &c, K::Esc);
    assert!(!panel.editing());
    let text = screen(&mut panel, &c, 120, 36);
    let pane = panel.panes.rect(PaneId::Files).unwrap();
    // Click the inspector's actual Diff toolbar, shared with Log.
    let toolbar = text
        .lines()
        .position(|row| row.contains("+- Diff"))
        .unwrap();
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: pane.x + 3,
            row: toolbar as u16,
            modifiers: Mods::NONE,
        }),
        &c,
    );
    assert!(panel.editing());
    assert_eq!(
        state(),
        before,
        "Browsing must preserve index, worktree, HEAD and stash reflog"
    );
}

#[test]
fn stash_failed_repository_is_local_and_other_repositories_still_load() {
    let (_temp, mut c, mut panel) = fixture();
    c.repos[0].path = c.workspace.join("missing-repository");
    panel.refresh(&c).unwrap();
    assert!(panel.repos[0].error.is_some());
    assert!(panel.repos.iter().any(|r| !r.entries.is_empty()));
    finish(&mut panel, &c);
    let text = screen(&mut panel, &c, 120, 36);
    assert!(text.find("api  Error").unwrap() < text.find("1 repo without stashes").unwrap());
    assert!(!text.contains("api work"));
    panel.select(panel.row_index(Row::Repository(0)).unwrap());
    let buffer = render(&mut panel, &c, 44, 12);
    let diff = text_in(&buffer, panel.panes.rect(PaneId::Files).unwrap());
    assert!(
        diff.contains("Error:") && !diff.contains("stash@{"),
        "{diff}"
    );
}

#[test]
fn stash_operation_keys_review_selected_entry_in_both_panes_but_not_diff() {
    let (_temp, c, mut panel) = fixture();
    let (root, oid) = panel.identity().unwrap();
    let repo = &c.repos[root].path;
    let before = run(repo, &["stash", "list"]);
    for pane in [PaneId::Stashes, PaneId::Files] {
        panel.focus_pane(pane);
        for (shortcut, title) in [
            ('a', "Apply stash"),
            ('P', "Pop stash"),
            ('d', "Drop stash"),
        ] {
            let Action::Confirm {
                title: actual,
                body,
                token,
            } = key(&mut panel, &c, K::Char(shortcut))
            else {
                panic!("review");
            };
            assert_eq!(actual, title);
            assert_eq!(token, "stash-operation");
            assert!(body.contains(&oid));
            assert_eq!(run(repo, &["stash", "list"]), before);
            assert!(run(repo, &["status", "--porcelain=v1"]).is_empty());
        }
    }
    panel.review = None;
    finish(&mut panel, &c);
    key(&mut panel, &c, K::Enter);
    assert!(panel.editing());
    for shortcut in ['a', 'P', 'd'] {
        assert!(!matches!(
            key(&mut panel, &c, K::Char(shortcut)),
            Action::Confirm { .. }
        ));
        assert!(panel.review.is_none());
    }
    assert_eq!(run(repo, &["stash", "list"]), before);
    assert!(run(repo, &["status", "--porcelain=v1"]).is_empty());
}

#[test]
fn stash_group_empty_and_stale_rows_cannot_start_operations() {
    let (_temp, mut c, mut panel) = fixture();
    panel.select(0);
    for shortcut in ['a', 'P', 'd'] {
        assert!(matches!(
            key(&mut panel, &c, K::Char(shortcut)),
            Action::Notice(_)
        ));
        assert!(panel.review.is_none());
    }
    panel.select(1);
    let (root, source) = panel.selection().unwrap();
    let repo = &c.repos[root].path;
    let oid = source.oid.clone();
    run(repo, &["stash", "drop", "stash@{0}"]);
    assert!(matches!(
        key(&mut panel, &c, K::Char('d')),
        Action::Notice(_)
    ));
    assert!(panel.review.is_none());
    assert_ne!(model::entries(repo).unwrap()[0].oid, oid);
    c.scope.clear();
    panel.refresh(&c).unwrap();
    assert!(matches!(
        key(&mut panel, &c, K::Char('a')),
        Action::Notice(_)
    ));
}

#[test]
fn stash_drop_refresh_keeps_a_nearby_entry_in_the_same_repository_and_consumes_review() {
    let (_temp, c, mut panel) = fixture();
    let root = c.repos.iter().position(|repo| repo.name == "web").unwrap();
    let entries = panel.repository(root).unwrap().entries.clone();
    panel.select(panel.row_index(Row::Stash { root, entry: 1 }).unwrap());
    assert!(matches!(
        key(&mut panel, &c, K::Char('d')),
        Action::Confirm { .. }
    ));
    assert!(matches!(
        panel.confirm("stash-operation", &c),
        Action::Refresh(_)
    ));
    assert!(matches!(panel.confirm("stash-operation", &c), Action::None));
    panel.refresh(&c).unwrap();
    assert_eq!(panel.identity(), Some((root, entries[0].oid.clone())));
    key(&mut panel, &c, K::Char('d'));
    panel.confirm("stash-operation", &c);
    panel.refresh(&c).unwrap();
    assert_eq!(panel.rows[panel.cursor], Row::EmptyGroup);
    assert!(panel.selection().is_none());
    assert!(c
        .repos
        .iter()
        .enumerate()
        .any(|(index, repo)| index != root && !run(&repo.path, &["stash", "list"]).is_empty()));
}

#[test]
fn pane_search_stash_reveals_entries_and_saved_files_without_changing_the_reflog() {
    let (_temp, c, mut p) = fixture();
    let (root, _) = p.identity().unwrap();
    let path = &c.repos[root].path;
    let stashes = run(path, &["stash", "list"]);
    let status = run(path, &["status", "--porcelain=v1"]);
    p.folded.insert(root);
    p.rebuild();
    p.cursor = p.repository_row(root).unwrap();
    p.request_preview(Duration::ZERO);

    let find = |p: &mut StashPanel, term: &str| {
        key(p, &c, K::Char('/'));
        assert!(p.editing());
        p.event(&Event::Paste(term.into()), &c);
        key(p, &c, K::Enter);
    };
    find(&mut p, "API WORK");
    assert_eq!(p.selection().unwrap().1.message(), "api work 2");
    assert!(!p.folded.contains(&root));
    key(&mut p, &c, K::Char('n'));
    assert_eq!(p.selection().unwrap().1.message(), "api work 1");
    key(&mut p, &c, K::Char('N'));
    assert_eq!(p.selection().unwrap().1.message(), "api work 2");
    finish(&mut p, &c);
    p.focus_pane(PaneId::Files);
    key(&mut p, &c, K::Char('z'));
    find(&mut p, "新增.txt");
    let buffer = render(&mut p, &c, 80, 24);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "新"
        && (Some(cell.bg) == crate::ui::search::CURRENT.bg
            || cell.modifier.contains(ratatui::style::Modifier::REVERSED))));
    key(&mut p, &c, K::Enter);
    assert!(p.editing());
    assert!(screen(&mut p, &c, 80, 24).contains("untracked api 2"));
    assert_eq!(run(path, &["stash", "list"]), stashes);
    assert_eq!(run(path, &["status", "--porcelain=v1"]), status);
}
