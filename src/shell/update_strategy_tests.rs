use super::{test_support::*, *};
use crate::core::strategy::{self, Strategy};
use std::fs;

fn send(app: &mut Shell, sequence: &str) {
    for event in crate::config::keys::parse_sequence(sequence).unwrap() {
        app.dispatch(&Event::Key(event));
    }
}

#[test]
fn push_and_pull_share_the_saved_default_but_keep_temporary_pull_choices_local() {
    let (_directory, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Push);
    send(&mut app, "s");
    assert!(app.pop.is_none());
    assert_eq!(app.push.strategy, Strategy::Rebase);
    assert_eq!(
        strategy::load(&app.ctx.state_dir).unwrap(),
        Strategy::Rebase
    );
    assert!(screen(&mut app, 80, 24).contains("Update strategy: Rebase"));

    let mut reopened = Shell::new(app.ctx.clone(), IconMode::Ascii);
    assert_eq!(reopened.push.strategy, Strategy::Rebase);
    send(&mut reopened, "p");
    let review = screen(&mut reopened, 80, 24);
    assert!(review.contains("(o) r Rebase"), "{review}");
    send(&mut reopened, "m");
    let review = screen(&mut reopened, 80, 24);
    assert!(review.contains("(o) m Merge"), "{review}");
    assert!(review.contains("Default: Rebase"), "{review}");
    send(&mut reopened, "Esc");
    assert_eq!(reopened.push.strategy, Strategy::Rebase);
    send(&mut reopened, "p s m Enter");
    assert!(matches!(reopened.pop, Some(Popup::Update(_))));
    assert_eq!(reopened.push.strategy, Strategy::Merge);
    assert_eq!(
        strategy::load(&reopened.ctx.state_dir).unwrap(),
        Strategy::Merge
    );
    send(&mut reopened, "Esc");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn push_strategy_can_be_remapped_and_toggles_without_a_selector() {
    let (_directory, original) = fixture();
    let config = AppConfig::from_toml("[keybindings.push]\nupdate-strategy=['u']").unwrap();
    let mut app = Shell::with_config(original.ctx.clone(), config);
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Push);
    send(&mut app, "s");
    assert!(app.pop.is_none());
    assert_eq!(app.push.strategy, Strategy::Merge);
    send(&mut app, "u");
    assert!(app.pop.is_none());
    assert_eq!(app.push.strategy, Strategy::Rebase);
    assert_eq!(
        strategy::load(&app.ctx.state_dir).unwrap(),
        Strategy::Rebase
    );
    assert!(screen(&mut app, 80, 24).contains("Update strategy: Rebase"));
    send(&mut app, "u");
    assert_eq!(app.push.strategy, Strategy::Merge);
    assert_eq!(strategy::load(&app.ctx.state_dir).unwrap(), Strategy::Merge);
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 80, 24).contains("Update strategy: Merge"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn failed_strategy_save_preserves_the_previous_default_without_opening_a_selector() {
    let (directory, mut app) = fixture();
    let file = directory.path().join("update-strategy.json");
    fs::create_dir(&file).unwrap();
    app.go(Push);
    send(&mut app, "s");
    assert!(app.pop.is_none());
    assert_eq!(app.push.strategy, Strategy::Merge);
    assert!(app.notifications.latest_body().contains("Could not save"));
    assert!(file.is_dir());
    app.notifications
        .expire(Instant::now() + Duration::from_secs(60));
    assert!(screen(&mut app, 80, 24).contains("Update strategy: Merge"));
}

#[test]
fn push_strategy_mouse_and_keyboard_toggle_the_same_saved_preference() {
    let (_directory, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Push);
    for expected in [Strategy::Rebase, Strategy::Merge] {
        let rendered = screen(&mut app, 120, 36);
        let (row, line) = rendered
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains("Update strategy:"))
            .unwrap();
        let column = line[..line.find("Update strategy:").unwrap()].width() as u16;
        app.dispatch(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.pop.is_none());
        assert_eq!(app.push.strategy, expected);
        assert_eq!(strategy::load(&app.ctx.state_dir).unwrap(), expected);
    }
    send(&mut app, "s");
    assert_eq!(app.push.strategy, Strategy::Rebase);
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn push_strategy_ignores_repeated_modified_and_busy_shortcuts() {
    let (_directory, mut app) = fixture();
    app.go(Push);
    send(&mut app, "s");
    assert_eq!(app.push.strategy, Strategy::Rebase);
    for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
        let mut event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
        event.kind = kind;
        app.dispatch(&Event::Key(event));
        assert_eq!(app.push.strategy, Strategy::Rebase);
        assert!(app.pop.is_none());
    }
    send(&mut app, "Ctrl-s");
    assert_eq!(app.push.strategy, Strategy::Rebase);
    let complete = app.commit.hold_execution();
    send(&mut app, "s");
    assert_eq!(app.push.strategy, Strategy::Rebase);
    assert_eq!(
        strategy::load(&app.ctx.state_dir).unwrap(),
        Strategy::Rebase
    );
    assert!(app.pop.is_none());
    assert!(app.notifications.latest_body().contains("Wait for"));
    complete();
    app.poll();
}

#[test]
fn conflicted_push_exposes_the_resolver_and_never_resumes_publication_after_resolution() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let g = |path: &std::path::Path, args: &[&str]| crate::core::git(path, args).unwrap();
    g(&repo, &["add", "."]);
    g(&repo, &["commit", "-qm", "clean fixture"]);
    let remote = directory.path().join("remote.git");
    let writer = directory.path().join("writer");
    g(
        &repo,
        &["clone", "--bare", "-q", ".", remote.to_str().unwrap()],
    );
    g(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    g(&repo, &["fetch", "-q", "origin"]);
    g(&repo, &["branch", "--set-upstream-to=origin/main"]);
    g(
        directory.path(),
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            writer.to_str().unwrap(),
        ],
    );
    for (name, value) in [
        ("user.name", "Update tester"),
        ("user.email", "update@invalid"),
        ("commit.gpgSign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        g(&writer, &["config", name, value]);
    }
    fs::write(repo.join("mixed.txt"), "local change\n").unwrap();
    g(&repo, &["commit", "-qam", "local change"]);
    fs::write(writer.join("mixed.txt"), "remote change\n").unwrap();
    g(&writer, &["commit", "-qam", "remote change"]);
    g(&writer, &["push", "-q", "origin", "main"]);
    let incoming = g(&remote, &["rev-parse", "main"]);
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    send(&mut app, "P Enter");
    assert!(app.push.busy());
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.push.busy() {
        app.poll();
        assert!(Instant::now() < deadline, "update-and-push timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.tab, Push);
    assert!(app.merge.has_pending_work());
    assert_eq!(g(&remote, &["rev-parse", "main"]), incoming);
    assert!(app
        .notifications
        .latest_body()
        .contains("Resolve conflicts"));
    send(&mut app, "C");
    assert_eq!(app.tab, Conflicts);
    fs::write(repo.join("mixed.txt"), "reviewed resolution\n").unwrap();
    g(&repo, &["add", "mixed.txt"]);
    g(&repo, &["commit", "-qm", "resolve merge"]);
    app.refresh();
    app.poll();
    assert_eq!(g(&remote, &["rev-parse", "main"]), incoming);
    assert!(!app.push.busy());
}
