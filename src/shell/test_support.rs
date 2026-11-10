use super::*;
use crate::core::git;
use ratatui::backend::TestBackend;
use std::{fs, path::Path};

pub(super) fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub(super) fn open_review(app: &mut Shell) {
    app.action(Action::Confirm {
        title: "Review action".into(),
        body: "Pending review body".into(),
        token: "test-review".into(),
    });
}

pub(super) fn fixture() -> (tempfile::TempDir, Shell) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    for (name, value) in [
        ("user.name", "Help tester"),
        ("user.email", "help@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&repo, &["config", name, value]).unwrap();
    }
    fs::write(repo.join("mixed.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "base\n").unwrap();
    git(&repo, &["add", "."]).unwrap();
    git(&repo, &["commit", "-qm", "base"]).unwrap();
    fs::write(repo.join("mixed.txt"), "staged\n").unwrap();
    fs::write(repo.join("other.txt"), "unselected staged\n").unwrap();
    git(&repo, &["add", "."]).unwrap();
    fs::write(repo.join("mixed.txt"), "working tree\n").unwrap();
    let mut ctx = Context::discover(&repo).unwrap();
    ctx.state_dir = dir.path().join("state");
    (dir, Shell::new(ctx, IconMode::Ascii))
}

pub(super) fn finish_commit(app: &mut Shell) {
    assert!(
        app.commit.execution_running(),
        "{}",
        app.notifications.latest_body()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.commit.execution_running() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "commit timed out: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Turn the disposable repository from `fixture` into a conflicted merge.
pub(super) fn merge_conflict(repo: &Path) {
    let run = |args: &[&str]| git(repo, args).unwrap();
    run(&["restore", "--source=HEAD", "--staged", "--worktree", "."]);
    for (name, value) in [
        ("user.name", "Conflict tester"),
        ("user.email", "conflict@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        run(&["config", name, value]);
    }
    run(&["switch", "-c", "incoming"]);
    fs::write(repo.join("mixed.txt"), "incoming\n").unwrap();
    run(&["commit", "-am", "incoming"]);
    run(&["switch", "main"]);
    fs::write(repo.join("mixed.txt"), "current\n").unwrap();
    run(&["commit", "-am", "current"]);
    assert!(git(repo, &["merge", "incoming"]).is_err());
    assert!(!run(&["ls-files", "--unmerged"]).is_empty());
}

pub(super) fn screen(app: &mut Shell, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x += symbol.width().max(1) as u16;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn click_push_cancel(app: &mut Shell, width: u16, height: u16) {
    let text = screen(app, width, height);
    let (row, line) = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(" Cancel "))
        .expect("Push footer has a Cancel button");
    let column = line[..line.find("Cancel").unwrap()].width() as u16;
    app.dispatch(&Event::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));
}

pub(super) fn snapshot(repo: &Path) -> Vec<String> {
    [
        vec!["rev-parse", "HEAD"],
        vec!["ls-files", "--stage"],
        vec!["diff", "--binary"],
        vec!["status", "--porcelain=v1"],
    ]
    .into_iter()
    .map(|args| git(repo, &args).unwrap())
    .collect()
}
