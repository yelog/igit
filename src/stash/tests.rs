use super::*;
use crate::core::git;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
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

fn screen(panel: &mut StashPanel, c: &Context, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), c)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn stash_lists_all_scoped_repositories_with_dates_and_empty_states() {
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
        let text = screen(&mut panel, &c, w, h);
        assert!(
            text.contains("stash@{0}") && text.contains("file.txt"),
            "{text}"
        );
        assert!(
            text.contains(&panel.selection().unwrap().1.saved[..19]),
            "{text}"
        );
    }
    let text = screen(&mut panel, &c, 120, 36);
    assert!(
        text.contains("empty") && text.contains("No stashes"),
        "{text}"
    );
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
    screen(&mut panel, &c, 120, 36);
    let pane = panel.panes.rect(PaneId::Files).unwrap();
    // Click the inspector's actual Diff toolbar, shared with Log.
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: pane.x + 3,
            row: pane.y + 1,
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
    assert!(screen(&mut panel, &c, 120, 36).contains("Error:"));
}
