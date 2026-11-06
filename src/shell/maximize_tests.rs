use super::{test_support::*, *};
use crate::{config::keys::parse_sequence, ui::panes::PaneId};

fn send(app: &mut Shell, sequence: &str) {
    for chord in sequence.split_whitespace() {
        for event in parse_sequence(chord).unwrap() {
            app.dispatch(&Event::Key(event));
        }
    }
}

fn smart_config(app: &mut Shell) {
    app.config = AppConfig::from_toml(
        "[ui]\nicons='ascii'\n[keybindings.panes]\nsmart-toggle-pane-maximized=['Cmd+Ctrl+f']",
    )
    .unwrap();
}

fn workspace_nodes(app: &Shell) -> Vec<(PaneId, Rect)> {
    app.commit
        .panes
        .nodes()
        .iter()
        .chain(app.log.panes.nodes())
        .copied()
        .collect()
}

#[test]
fn maximize_preserves_commit_sizes_and_log_stays_tiled_and_resizable() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w 2 +");
    screen(&mut app, 120, 36);
    let commit_sizes = workspace_nodes(&app);
    send(&mut app, "2");
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w 4 >");
    screen(&mut app, 120, 36);
    let log_sizes = workspace_nodes(&app);
    send(&mut app, "1");
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w f");
    assert!(app.pane_maximized);
    for expected in [
        PaneId::Changes,
        PaneId::Preview,
        PaneId::Message,
        PaneId::Changes,
    ] {
        let text = screen(&mut app, 120, 36);
        let visible: Vec<_> = workspace_nodes(&app)
            .into_iter()
            .filter(|(_, r)| !r.is_empty())
            .collect();
        assert_eq!(visible.len(), 1, "{expected:?}: {text}");
        assert_eq!(visible[0].0, expected, "{text}");
        assert!(
            visible[0].1.width >= 118 && visible[0].1.height >= 31,
            "{visible:?}"
        );
        assert!(text.lines().last().unwrap().starts_with(" IGit "));
        send(&mut app, "Tab");
    }

    send(&mut app, "2");
    let text = screen(&mut app, 120, 36);
    assert!(text.lines().last().unwrap().starts_with(" IGit "));
    assert_eq!(workspace_nodes(&app), log_sizes);
    let graph = app.log.panes.rect(PaneId::Graph).unwrap();
    send(&mut app, "Ctrl-w >");
    screen(&mut app, 120, 36);
    assert_eq!(
        app.log.panes.rect(PaneId::Graph).unwrap().width,
        graph.width + 1
    );
    send(&mut app, "Ctrl-w < Ctrl-w f");
    assert!(
        app.pane_maximized,
        "Log must leave the other tabs' mode alone"
    );
    send(&mut app, "1");
    assert!(screen(&mut app, 120, 36)
        .lines()
        .last()
        .unwrap()
        .starts_with(" IGit "));

    // Neither local resize nor reset changes the saved split while maximized.
    send(&mut app, "Ctrl-w 9 > Ctrl-w + Ctrl-w =");
    screen(&mut app, 44, 12);
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w f");
    screen(&mut app, 120, 36);
    assert!(!app.pane_maximized);
    assert_eq!(workspace_nodes(&app), commit_sizes);
    send(&mut app, "2");
    screen(&mut app, 120, 36);
    assert_eq!(workspace_nodes(&app), log_sizes);
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn maximize_works_on_narrow_screens_while_log_stays_tiled() {
    let (_dir, mut app) = fixture();
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    for (width, height) in [(120, 36), (80, 25), (44, 12)] {
        send(&mut app, "1");
        screen(&mut app, width, height);
        send(&mut app, "Ctrl-w f");
        for command in ["Tab", "2", "3", "Tab", "4"] {
            send(&mut app, command);
            let text = screen(&mut app, width, height);
            assert!(app.pane_maximized, "{command}: {text}");
            let panes = if app.tab == Log { 4 } else { 1 };
            assert!(text.lines().last().unwrap().starts_with(" IGit "), "{text}");
            let layout = match app.tab {
                Commit => &app.commit.panes,
                Log => &app.log.panes,
                Push => &app.push.panes,
                Conflicts => &app.merge.panes,
                Branch => &app.branch.panes,
                Stash => &app.stash.panes,
            };
            assert_eq!(
                layout.nodes().iter().filter(|(_, r)| !r.is_empty()).count(),
                panes
            );
        }
        send(&mut app, "1");
        screen(&mut app, width, height);
        send(&mut app, "Ctrl-w f");
        assert!(!app.pane_maximized);
    }
}

#[test]
fn maximize_configuration_help_and_prefix_are_executable() {
    let (_dir, mut app) = fixture();
    app.config =
        AppConfig::from_toml("[keybindings.panes]\ntoggle-pane-maximized=['Alt-z']").unwrap();
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w f");
    assert!(!app.pane_maximized, "removed default still executes");
    assert!(
        app.pop.is_none(),
        "removed default leaked into another action"
    );
    send(&mut app, "Alt-z");
    assert!(app.pane_maximized);
    let mut repeat = parse_sequence("Alt-z").unwrap()[0];
    repeat.kind = KeyEventKind::Repeat;
    app.dispatch(&Event::Key(repeat));
    assert!(app.pane_maximized);
    screen(&mut app, 120, 36);
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("restore focused pane".into()));
    assert!(screen(&mut app, 120, 36).contains("Alt-z"));
    send(&mut app, "Enter");
    assert!(!app.pane_maximized && app.pop.is_none());

    app.config = AppConfig::default();
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w");
    app.key_picker.poll(Instant::now() + Duration::from_secs(1));
    assert!(screen(&mut app, 120, 36).contains("Maximize / restore"));
    send(&mut app, "f");
    assert!(app.pane_maximized);
}

#[test]
fn smart_maximize_falls_back_locally_and_preserves_input_and_reviews() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    smart_config(&mut app);
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+f");
    assert!(app.pane_maximized);
    for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
        let mut event = parse_sequence("Cmd+Ctrl+f").unwrap()[0];
        event.kind = kind;
        app.dispatch(&Event::Key(event));
    }
    assert!(app.pane_maximized);
    send(&mut app, "1 i");
    app.dispatch(&Event::Paste("draft 中文".into()));
    let input = screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+f");
    assert_eq!(screen(&mut app, 120, 36), input);
    send(&mut app, "Esc q");
    assert!(app.pop.is_some());
    send(&mut app, "Cmd+Ctrl+f");
    assert!(app.pop.is_some() && app.pane_maximized);
    send(&mut app, "Esc F1");
    app.dispatch(&Event::Paste("pane and Kitty layout".into()));
    send(&mut app, "Enter");
    assert!(app.pop.is_none() && !app.pane_maximized);
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn smart_maximize_applies_verified_target_and_keeps_state_on_error() {
    let (_dir, mut app) = fixture();
    smart_config(&mut app);
    screen(&mut app, 120, 36);
    for (result, expected) in [
        (Ok(kitty::Outcome::Maximized(Some(true))), true),
        (Ok(kitty::Outcome::Maximized(Some(true))), true),
        (Err("Kitty layout failed".into()), true),
        (Ok(kitty::Outcome::Maximized(Some(false))), false),
        (Ok(kitty::Outcome::Maximized(None)), true),
    ] {
        app.maximize_pending = true;
        app.kitty.set_pending_result(result);
        let focus = app.tab;
        send(&mut app, "2 i Cmd+Ctrl+f");
        app.dispatch(&Event::Paste("must not become a draft".into()));
        assert_eq!(app.tab, focus);
        assert!(!app.panel().editing());
        assert!(screen(&mut app, 120, 36).contains("Synchronizing Kitty layout"));
        assert!(app.poll());
        assert!(!app.maximize_pending);
        assert_eq!(app.pane_maximized, expected);
        screen(&mut app, 120, 36);
    }
}
