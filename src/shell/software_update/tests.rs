use super::*;
use crate::{
    shell::{test_support::*, *},
    ui::icons::IconMode,
};
use crossterm::event::MouseEvent;
use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

fn report(status: Status) -> Inspection {
    Inspection {
        schema: 1,
        product: "igit".into(),
        channel: "beta".into(),
        current_version: env!("CARGO_PKG_VERSION").into(),
        target_version: "0.2.0-beta.1".into(),
        status,
        action: Some("Update with the original package manager or rebuild IGit.".into()),
    }
}

fn click(x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

fn complete(
    update: &mut SoftwareUpdate,
    state: State,
    result: Result<Inspection, String>,
    automatic: bool,
) {
    let (sender, receiver) = mpsc::channel();
    sender.send(result).unwrap();
    update.state = state;
    update.job = Some(receiver);
    update.automatic = automatic;
    assert!(update.poll());
}

#[test]
fn footer_version_status_and_click_geometry_survive_narrow_layouts() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        app.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            for (state, expected) in [
                (State::Idle, ""),
                (State::Checking, "Checking"),
                (State::Report(report(Status::UpdateAvailable)), "Update"),
                (
                    State::Installing(report(Status::UpdateAvailable)),
                    "Updating",
                ),
                (State::Ready(report(Status::Updated)), "Restart"),
                (State::Failed("offline".into()), "Update failed"),
            ] {
                app.software_update.state = state;
                let text = screen(&mut app, width, height);
                let footer = text.lines().last().unwrap();
                assert!(
                    footer.starts_with(&format!(" IGit  v{}", env!("CARGO_PKG_VERSION"))),
                    "{footer}"
                );
                assert!(footer.contains(expected), "{footer}");
                let hit = app.regions.software_update;
                assert_eq!(hit.y, height - 1);
                assert_eq!(hit.x, 6);
                assert!(hit.right() <= width);
            }
            app.software_update.state = State::Report(report(Status::UpToDate));
            screen(&mut app, width, height);
            let hit = app.regions.software_update;
            app.dispatch(&click(1, hit.y));
            assert!(app.pop.is_none(), "brand is outside the version target");
            app.dispatch(&click(hit.right(), hit.y));
            assert!(app.pop.is_none(), "hint is outside the version target");
            app.dispatch(&click(hit.x, hit.y));
            assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
            app.dispatch(&key(KeyCode::Enter));
            assert!(app.pop.is_none(), "Close is focused initially");
            screen(&mut app, 30, 8);
            assert!(app.regions.software_update.is_empty());
            app.dispatch(&click(hit.x, hit.y));
            assert!(app.pop.is_none(), "old geometry is cleared after resize");
        }
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn update_center_defaults_to_close_and_preserves_editing_and_git_state() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.software_update.state = State::Report(report(Status::UpdateAvailable));
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        app.dispatch(&key(KeyCode::F(9)));
        assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
        let text = screen(&mut app, width, height);
        assert!(
            text.contains("Update Center") && text.contains("Install update"),
            "{text}"
        );
        assert!(text.contains("Running v"), "{text}");
        app.dispatch(&key(KeyCode::Enter));
        assert!(app.pop.is_none());
        assert!(app.software_update.job.is_none());
    }
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("draft kept".into()));
    assert!(app.commit.message_editing());
    screen(&mut app, 120, 36);
    let hit = app.regions.software_update;
    app.dispatch(&click(hit.x, hit.y));
    assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
    app.dispatch(&key(KeyCode::F(1)));
    assert!(
        matches!(&app.pop, Some(Popup::Help { state, .. }) if state.content.context() == "Update Center")
    );
    app.dispatch(&key(KeyCode::Esc));
    assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.commit.message_editing());
    assert!(screen(&mut app, 120, 36).contains("draft kept"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn update_shortcut_respects_remapping_and_other_reviews() {
    let (_dir, mut app) = fixture();
    app.software_update.state = State::Report(report(Status::UpToDate));
    app.config = AppConfig::from_toml("[keybindings.global]\nupdate=['U U']").unwrap();
    app.dispatch(&key(KeyCode::F(9)));
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('U')));
    app.dispatch(&key(KeyCode::Char('U')));
    assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
    app.dispatch(&key(KeyCode::Esc));
    app.global(3);
    app.dispatch(&key(KeyCode::F(9)));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    let action = app
        .menu_actions()
        .position(|action| matches!(action, MenuAction::SoftwareUpdate))
        .unwrap();
    app.menu_action(action);
    assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
}

#[test]
fn worker_results_stay_available_after_closing_and_failures_are_quiet_only_when_automatic() {
    let (_dir, mut app) = fixture();
    app.software_update.state = State::Installing(report(Status::UpdateAvailable));
    app.open_software_update();
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none() && app.software_update.installing());
    app.quit_box();
    assert!(app.pop.is_none());
    complete(
        &mut app.software_update,
        State::Installing(report(Status::UpdateAvailable)),
        Ok(report(Status::Updated)),
        false,
    );
    app.open_software_update();
    assert!(screen(&mut app, 80, 24).contains("Restart IGit"));
    assert!(!app.software_update.primary);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none() && !app.quit);
    complete(
        &mut app.software_update,
        State::Checking,
        Err("offline".into()),
        true,
    );
    assert!(matches!(app.software_update.state, State::Idle));
    complete(
        &mut app.software_update,
        State::Checking,
        Err("offline".into()),
        false,
    );
    assert!(matches!(app.software_update.state, State::Failed(_)));
    app.pop = Some(Popup::SoftwareUpdate);
    assert!(screen(&mut app, 80, 24).contains("offline"));
    let (sender, receiver) = mpsc::channel();
    app.software_update.state = State::Installing(report(Status::UpdateAvailable));
    app.software_update.job = Some(receiver);
    drop(sender);
    assert!(app.software_update.poll());
    assert!(matches!(app.software_update.state, State::Failed(_)));
}

#[test]
fn cache_expires_and_rejects_invalid_or_different_installations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update-check.json");
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
    let interval = Duration::from_secs(24 * 3600);
    assert!(cache::read(&path, now, interval).is_none());
    cache::write(&path, &report(Status::UpdateAvailable), now).unwrap();
    let valid = fs::read(&path).unwrap();
    assert!(cache::read(&path, now, interval).is_some());
    assert!(cache::read(&path, now + interval + Duration::from_secs(1), interval).is_none());
    assert!(cache::read(&path, now - Duration::from_secs(1), interval).is_none());
    for (pointer, value) in [
        ("/schema", serde_json::json!(2)),
        ("/checked_at", serde_json::json!(u64::MAX)),
        ("/identity/executable", serde_json::json!("/another/igit")),
        ("/identity/ownership", serde_json::json!([1, 2, 3])),
        ("/report/current_version", serde_json::json!("9.9.9")),
        ("/report/channel", serde_json::json!("stable")),
        ("/report/status", serde_json::json!("updated")),
    ] {
        let mut changed: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        *changed.pointer_mut(pointer).unwrap() = value;
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(cache::read(&path, now, interval).is_none(), "{pointer}");
    }
    fs::write(&path, "broken JSON").unwrap();
    assert!(cache::read(&path, now, interval).is_none());
}

#[test]
fn startup_uses_cached_results_delays_requests_and_respects_settings() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("workspace");
    let path = dir.path().join("update-check.json");
    let config = AppConfig::default();
    let mut update = SoftwareUpdate::default();
    assert!(!update.schedule(&state, &config.updates));
    let delay = update.startup_at.unwrap().duration_since(Instant::now());
    assert!(delay <= Duration::from_millis(1500) && delay > Duration::from_millis(1000));
    assert!(update.job.is_none());
    cache::write(&path, &report(Status::UpdateAvailable), SystemTime::now()).unwrap();
    let mut cached = SoftwareUpdate::default();
    assert!(cached.schedule(&state, &config.updates));
    assert!(matches!(cached.state, State::Report(_)));
    assert!(cached.job.is_none() && cached.startup_at.is_none());
    let mut disabled = SoftwareUpdate::default();
    let config =
        AppConfig::from_toml("[updates]\ncheck_on_startup=false\ncheck_interval_hours=1").unwrap();
    assert!(!disabled.schedule(&state, &config.updates));
    assert!(matches!(disabled.state, State::Idle));
    assert!(disabled.job.is_none() && disabled.startup_at.is_none());
    assert!(AppConfig::from_toml("[updates]\ncheck_interval_hours=0").is_err());
    assert!(
        AppConfig::from_toml(&format!("[updates]\ncheck_interval_hours={}", i64::MAX)).is_err()
    );
}

#[test]
fn manual_open_reports_a_check_failure_and_long_details_remain_scrollable() {
    let (_dir, mut app) = fixture();
    let (sender, receiver) = mpsc::channel();
    sender
        .send(Err(format!(
            "{}\nLast diagnostic line",
            "Network unavailable\n".repeat(30)
        )))
        .unwrap();
    app.software_update.job = Some(receiver);
    app.software_update.state = State::Checking;
    app.software_update.automatic = true;
    app.open_software_update();
    assert!(app.software_update.poll());
    assert!(matches!(app.software_update.state, State::Failed(_)));
    let initial = screen(&mut app, 44, 12);
    assert!(initial.contains("Update failed") && initial.contains("Check again"));
    for _ in 0..60 {
        app.dispatch(&key(KeyCode::Down));
    }
    let end = screen(&mut app, 44, 12);
    assert!(end.contains("Last diagnostic line"), "{end}");
    assert!(end.contains("Check again") && end.contains("Close"));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
}

#[test]
fn install_buttons_share_keyboard_mouse_routing_and_cannot_update_an_unmanaged_binary() {
    for mouse in [false, true] {
        let (_dir, mut app) = fixture();
        let before = snapshot(&app.ctx.repos[0].path);
        app.software_update.state = State::Report(report(Status::UpdateAvailable));
        app.open_software_update();
        screen(&mut app, 44, 12);
        if mouse {
            let button = app.pop_hits.iter().find(|(_, id)| *id == 0).unwrap().0;
            app.dispatch(&click(button.x, button.y));
        } else {
            app.dispatch(&key(KeyCode::Tab));
            app.dispatch(&key(KeyCode::Enter));
        }
        assert!(app.software_update.installing());
        assert!(app.software_update.job.is_some());
        app.dispatch(&key(KeyCode::Char('r')));
        assert!(
            app.software_update.installing(),
            "recheck cannot replace an active installation"
        );
        app.dispatch(&key(KeyCode::Esc));
        assert!(app.pop.is_none());
        assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    }
}

#[test]
fn conflict_editor_keeps_its_actions_above_the_clickable_version_footer() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    app.refresh_conflicts();
    app.go(Conflicts);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.merge.has_document());
    let before = snapshot(&repo);
    app.software_update.state = State::Report(report(Status::UpToDate));
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        let text = screen(&mut app, width, height);
        assert!(text.lines().last().unwrap().starts_with(" IGit  v"));
        let controls = text.lines().rev().nth(1).unwrap();
        assert!(
            controls.contains("Apply") && controls.contains("Close"),
            "{text}"
        );
        let hit = app.regions.software_update;
        app.dispatch(&click(hit.x, hit.y));
        assert!(matches!(app.pop, Some(Popup::SoftwareUpdate)));
        app.dispatch(&key(KeyCode::Esc));
        assert!(app.merge.has_document());
    }
    assert_eq!(snapshot(&repo), before);
}
