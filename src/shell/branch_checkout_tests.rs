use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::MouseEvent;

fn checkout_fixture() -> (tempfile::TempDir, Shell) {
    let (temp, mut app) = fixture();
    let repo = &app.ctx.repos[0].path;
    git(repo, &["stash", "push", "-m", "fixture changes"]).unwrap();
    git(repo, &["switch", "-c", "topic"]).unwrap();
    std::fs::write(repo.join("mixed.txt"), "topic changes\n").unwrap();
    git(repo, &["commit", "-am", "topic"]).unwrap();
    git(repo, &["switch", "main"]).unwrap();
    git(repo, &["stash", "pop", "--index"]).unwrap();
    app.go(Branch);
    app.branch.finish_loading(&app.ctx);
    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Enter));
    wait_for_checkout_popup(&mut app);
    assert!(
        matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == crate::branch::CHECKOUT_REVIEW_TOKEN),
        "{}",
        app.notifications.latest_body()
    );
    (temp, app)
}

fn wait_for_checkout_popup(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !matches!(
        &app.pop,
        Some(Popup::Confirm { token, .. }) if token == crate::branch::CHECKOUT_REVIEW_TOKEN
    ) {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "{}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn finish_checkout(app: &mut Shell) {
    assert!(app.branch.syncing());
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.branch.syncing() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "{}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn finish_log_load(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !app.log_load.ready() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "{}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn checkout_gate(
    temp: &std::path::Path,
    repo: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let hooks = temp.join("hooks");
    let started = temp.join("checkout-started");
    let gate = temp.join("checkout-gate");
    std::fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf started > '{}'\ni=0\nwhile [ ! -e '{}' ]; do i=$((i + 1)); [ \"$i\" -ge 2000 ] && exit 1; sleep 0.01; done\n",
            started.display(),
            gate.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]).unwrap();
    (started, gate)
}

fn wait_for_checkout_start(app: &mut Shell, started: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !started.exists() {
        assert!(app.branch.syncing());
        assert!(Instant::now() < deadline, "checkout hook did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn click(app: &mut Shell, text: &str, width: u16, height: u16) {
    let rendered = screen(app, width, height);
    let (row, line) = rendered
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(text))
        .unwrap_or_else(|| panic!("{text}\n{rendered}"));
    let column = line[..line.find(text).unwrap()].width() as u16;
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn smart_checkout_dialog_renders_buttons_and_cancels_without_writes() {
    let (_temp, mut app) = checkout_fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        app.branch.icons = icons;
        for (width, height) in [(44, 12), (80, 25), (120, 36)] {
            let rendered = screen(&mut app, width, height);
            for text in [
                "Git Checkout Problem",
                "Force Checkout",
                "Don't Checkout",
                "Smart Checkout",
            ] {
                assert!(rendered.contains(text), "{width}x{height}: {rendered}");
            }
            app.dispatch(&key(KeyCode::End));
            let preview = screen(&mut app, width, height);
            assert!(preview.contains("Smart Checkout"), "{preview}");
            app.dispatch(&key(KeyCode::Home));
        }
    }
    app.dispatch(&key(KeyCode::Char('y')));
    app.dispatch(&key(KeyCode::Char('n')));
    app.dispatch(&key(KeyCode::Char('1')));
    assert_eq!(app.tab, Branch);
    assert!(!app.branch.syncing());
    app.dispatch(&key(KeyCode::F(1)));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    assert!(!screen(&mut app, 120, 36).contains("Review shortcuts"));
    app.dispatch(&key(KeyCode::Esc));
    click(&mut app, "Don't Checkout", 44, 12);
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
}

#[test]
fn smart_checkout_dialog_requires_separate_force_confirmation() {
    let (_temp, mut app) = checkout_fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Enter));
    let rendered = screen(&mut app, 80, 25);
    assert!(rendered.contains("Discard local changes"), "{rendered}");
    assert!(!app.branch.syncing());
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);

    app.dispatch(&key(KeyCode::Enter));
    wait_for_checkout_popup(&mut app);
    click(&mut app, "Force Checkout", 44, 12);
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Enter));
    finish_checkout(&mut app);
    assert_eq!(git(&repo, &["branch", "--show-current"]).unwrap(), "topic");
    assert!(git(&repo, &["diff", "HEAD"]).unwrap().is_empty());
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
}

#[test]
fn smart_checkout_dialog_defaults_to_smart_and_opens_restoration_conflicts() {
    let (_temp, mut app) = checkout_fixture();
    let repo = app.ctx.repos[0].path.clone();
    app.dispatch(&key(KeyCode::Enter));
    finish_checkout(&mut app);
    assert_eq!(git(&repo, &["branch", "--show-current"]).unwrap(), "topic");
    assert_eq!(app.tab, Conflicts);
    assert!(app.pop.is_none());
    assert!(!git(&repo, &["ls-files", "--unmerged"]).unwrap().is_empty());
    assert!(git(&repo, &["stash", "list"])
        .unwrap()
        .contains("IGit Smart Checkout"));
    let rendered = screen(&mut app, 120, 36);
    assert!(rendered.contains("mixed.txt"), "{rendered}");
}

#[test]
fn smart_checkout_dialog_shift_tab_selects_cancel() {
    let (_temp, mut app) = checkout_fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::SHIFT,
    )));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert!(!app.branch.syncing());
    assert_eq!(snapshot(&repo), before);
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn checkout_enter_returns_immediately_and_does_not_reload_the_branch_list() {
    let (temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(&repo, &["branch", "topic"]).unwrap();
    app.go(Branch);
    app.branch.finish_loading(&app.ctx);

    let (started, gate) = checkout_gate(temp.path(), &repo);

    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.branch.syncing());
    assert_eq!(app.notifications.latest_body(), "Switching branch…");
    wait_for_checkout_start(&mut app, &started);

    app.dispatch(&key(KeyCode::Char('k')));
    assert_eq!(app.tab, Branch);
    assert!(app.branch.syncing());
    app.global(0);
    assert!(app.pop.is_none());
    assert!(app.notifications.latest_body().contains("changing Roots"));
    std::fs::write(&gate, "continue").unwrap();
    finish_checkout(&mut app);

    assert_eq!(app.ctx.repos[0].branch, "topic");
    assert_eq!(git(&repo, &["branch", "--show-current"]).unwrap(), "topic");
    assert!(app.pop.is_none());
    assert!(!app.branch.reads_pending());
    assert!(app.notifications.latest_body().contains("completed"));
}

#[cfg(unix)]
#[test]
fn checkout_completion_keeps_loaded_log_editor_active() {
    let (temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(&repo, &["branch", "topic"]).unwrap();

    app.go(Log);
    finish_log_load(&mut app);
    app.go(Branch);
    app.branch.finish_loading(&app.ctx);
    let (started, gate) = checkout_gate(temp.path(), &repo);

    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Enter));
    wait_for_checkout_start(&mut app, &started);

    app.go(Log);
    assert!(app.log_load.ready());
    app.dispatch(&key(KeyCode::Char('/')));
    assert!(app.log.editing());
    app.dispatch(&key(KeyCode::Char('t')));
    let editing = screen(&mut app, 120, 36);
    assert!(editing.contains('t'), "{editing}");

    std::fs::write(&gate, "continue").unwrap();
    finish_checkout(&mut app);

    assert!(app.log_load.ready());
    assert!(app.log.editing());
    app.dispatch(&key(KeyCode::Char('o')));
    assert!(app.log.editing());

    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.log.editing());
    app.poll();
    assert!(app.log_load.ready());
}
