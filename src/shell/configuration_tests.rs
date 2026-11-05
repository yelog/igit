use super::{test_support::*, *};
use crate::config::keys::parse_sequence;
use crate::ui::clipboard::CopyCapture;
use crossterm::event::MouseEvent;

fn configure(app: &mut Shell, text: &str) {
    app.config = AppConfig::from_toml(&format!("[ui]\nicons='ascii'\n{text}")).unwrap();
    app.merge.shortcut_labels = app.config.shortcut_labels("merge");
    app.push.shortcut_labels = app.config.shortcut_labels("push");
}
fn send(app: &mut Shell, text: &str) {
    for key in parse_sequence(text).unwrap() {
        app.dispatch(&Event::Key(key));
    }
}

const SMART_FOCUS: &str = "[keybindings.panes]\n\
smart-focus-pane-left=['Cmd+Ctrl+h']\n\
smart-focus-pane-down=['Cmd+Ctrl+j']\n\
smart-focus-pane-up=['Cmd+Ctrl+k']\n\
smart-focus-pane-right=['Cmd+Ctrl+l']";

const SMART_RESIZE: &str = "[keybindings.panes]\n\
smart-resize-pane-left=['Cmd+Ctrl+Shift+h']\n\
smart-resize-pane-down=['Cmd+Ctrl+Shift+j']\n\
smart-resize-pane-up=['Cmd+Ctrl+Shift+k']\n\
smart-resize-pane-right=['Cmd+Ctrl+Shift+l']";

#[test]
fn configuration_smart_resize_moves_width_height_and_repeats_without_changing_focus() {
    use crate::ui::panes::PaneId;
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_RESIZE);
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(120, 36), (180, 48)] {
        send(&mut app, "Ctrl-w =");
        let initial = screen(&mut app, width, height);
        let refs = app.log.panes.rect(PaneId::Refs).unwrap();
        let graph = app.log.panes.rect(PaneId::Graph).unwrap();
        let log_area = app.regions.log;
        send(&mut app, "Ctrl-w Cmd+Ctrl+Shift+l");
        assert!(app.window_prefix.is_none());
        screen(&mut app, width, height);
        assert_eq!(
            app.log.panes.rect(PaneId::Graph).unwrap().width,
            graph.width + 3
        );
        assert_eq!(app.log.panes.rect(PaneId::Refs).unwrap(), refs);
        let mut repeat = parse_sequence("Cmd+Ctrl+Shift+h").unwrap()[0];
        repeat.kind = KeyEventKind::Repeat;
        app.dispatch(&Event::Key(repeat));
        repeat.kind = KeyEventKind::Release;
        app.dispatch(&Event::Key(repeat));
        screen(&mut app, width, height);
        assert_eq!(app.log.panes.rect(PaneId::Graph).unwrap(), graph);
        send(&mut app, "Cmd+Ctrl+Shift+j");
        screen(&mut app, width, height);
        assert_eq!(app.regions.log.y, log_area.y + 3);
        send(&mut app, "Cmd+Ctrl+Shift+k");
        assert_eq!(screen(&mut app, width, height), initial);
        assert_eq!(app.log.focused_pane(), PaneId::Graph);
        assert!(app.kitty.resizes.is_empty());
        assert!(app.kitty.requests.is_empty());
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn configuration_smart_resize_hands_off_only_at_limits_or_missing_dividers() {
    use crate::ui::panes::{Direction, PaneId};
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_RESIZE);
    screen(&mut app, 120, 36);
    for _ in 0..100 {
        send(&mut app, "Cmd+Ctrl+Shift+l");
        screen(&mut app, 120, 36);
        if !app.kitty.resizes.is_empty() {
            break;
        }
    }
    assert_eq!(app.log.panes.rect(PaneId::Files).unwrap().width, 16);
    assert_eq!(app.kitty.resizes, [Direction::Right]);
    send(&mut app, "Cmd+Ctrl+Shift+h");
    screen(&mut app, 120, 36);
    assert_eq!(app.log.panes.rect(PaneId::Files).unwrap().width, 19);
    assert_eq!(app.kitty.resizes.len(), 1);
    // A narrow, tabbed Log has no internal width divider.
    for (width, height) in [(80, 24), (44, 12)] {
        screen(&mut app, width, height);
        send(&mut app, "Cmd+Ctrl+Shift+h");
    }
    assert_eq!(
        app.kitty.resizes,
        [Direction::Right, Direction::Left, Direction::Left]
    );
    assert_eq!(app.help_content().context(), "Log / Commits");
}

#[test]
fn configuration_smart_resize_keeps_tools_isolated_and_help_executes_the_action() {
    use crate::ui::panes::{Direction, PaneId};
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_RESIZE);
    screen(&mut app, 120, 36);
    let background = (app.regions.commit, app.regions.log);
    send(&mut app, "3");
    screen(&mut app, 120, 36);
    let outgoing = app.push.panes.rect(PaneId::Outgoing).unwrap();
    send(&mut app, "Cmd+Ctrl+Shift+l");
    screen(&mut app, 120, 36);
    assert_eq!(
        app.push.panes.rect(PaneId::Outgoing).unwrap().width,
        outgoing.width + 3
    );
    send(&mut app, "Tab Cmd+Ctrl+Shift+h");
    screen(&mut app, 120, 36);
    assert_eq!(app.push.panes.rect(PaneId::Outgoing).unwrap(), outgoing);
    assert_eq!(app.push.focused_pane(), PaneId::PushFiles);
    send(&mut app, "Cmd+Ctrl+Shift+j");
    assert_eq!(app.kitty.resizes, [Direction::Down]);
    assert_eq!((app.regions.commit, app.regions.log), background);
    send(&mut app, "Esc 4");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+Shift+h");
    assert_eq!(app.surface(), Dialog(Merge));
    assert_eq!(app.kitty.resizes, [Direction::Down, Direction::Left]);
    send(&mut app, "Esc 1 F1");
    app.dispatch(&Event::Paste("Move divider right".into()));
    assert!(screen(&mut app, 120, 36).contains("Ctrl-Super-L"));
    let graph = app.log.panes.rect(PaneId::Graph).unwrap();
    send(&mut app, "Enter");
    screen(&mut app, 120, 36);
    assert!(app.pop.is_none());
    assert_eq!(
        app.log.panes.rect(PaneId::Graph).unwrap().width,
        graph.width + 3
    );
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
}

#[test]
fn configuration_smart_resize_preserves_drafts_and_reviewed_selection() {
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_RESIZE);
    let before = snapshot(&app.ctx.repos[0].path);
    send(&mut app, "2");
    screen(&mut app, 120, 36);
    send(&mut app, "G Space i");
    app.dispatch(&Event::Paste("Resize draft 中文".into()));
    send(&mut app, "Ctrl-a");
    let regions = (app.regions.commit, app.regions.log);
    send(
        &mut app,
        "Cmd+Ctrl+Shift+h Cmd+Ctrl+Shift+j Cmd+Ctrl+Shift+k Cmd+Ctrl+Shift+l",
    );
    assert_eq!(app.panel().selected_text(), Some("Resize draft 中文"));
    assert_eq!((app.regions.commit, app.regions.log), regions);
    send(&mut app, "Esc F2 c");
    let review = match &app.pop {
        Some(Popup::Confirm { body, .. }) => body.clone(),
        _ => panic!("expected commit review"),
    };
    assert!(
        review.contains("mixed.txt") && !review.contains("other.txt"),
        "{review}"
    );
    send(
        &mut app,
        "Cmd+Ctrl+Shift+h Cmd+Ctrl+Shift+j Cmd+Ctrl+Shift+k Cmd+Ctrl+Shift+l",
    );
    assert!(matches!(&app.pop, Some(Popup::Confirm { body, yes: false, .. }) if body == &review));
    send(&mut app, "Esc");
    for chord in [
        "Cmd+Ctrl+Shift+h",
        "Cmd+Ctrl+Shift+j",
        "Cmd+Ctrl+Shift+k",
        "Cmd+Ctrl+Shift+l",
    ] {
        send(&mut app, chord);
        screen(&mut app, 120, 36);
    }
    send(&mut app, "c");
    assert!(matches!(&app.pop, Some(Popup::Confirm { body, .. }) if body == &review));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn configuration_smart_focus_uses_internal_neighbors_then_kitty_edges() {
    use crate::ui::panes::Direction;
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_FOCUS);
    let before = snapshot(&app.ctx.repos[0].path);
    screen(&mut app, 120, 36);
    for (keys, context) in [
        ("Cmd+Ctrl+h", "Log / Branch Tree"),
        ("Cmd+Ctrl+h", "Log / Branch Tree"),
        ("Cmd+Ctrl+k", "Commit / Message"),
        ("Cmd+Ctrl+k", "Commit / Changes"),
        ("Cmd+Ctrl+k", "Commit / Changes"),
        ("Cmd+Ctrl+j", "Commit / Message"),
        ("Cmd+Ctrl+j", "Log / Commits"),
        ("Cmd+Ctrl+j", "Log / Commits"),
        ("Cmd+Ctrl+h", "Log / Branch Tree"),
        ("Cmd+Ctrl+l", "Log / Commits"),
        ("Cmd+Ctrl+l", "Log / File Tree"),
        ("Cmd+Ctrl+l", "Log / File Tree"),
    ] {
        send(&mut app, keys);
        assert_eq!(app.help_content().context(), context, "{keys}");
        screen(&mut app, 120, 36);
    }
    assert_eq!(
        app.kitty.requests,
        [
            Direction::Left,
            Direction::Up,
            Direction::Down,
            Direction::Right
        ]
    );
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn configuration_smart_focus_preserves_editors_help_and_confirmations() {
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_FOCUS);
    let before = snapshot(&app.ctx.repos[0].path);
    send(&mut app, "2 i");
    app.dispatch(&Event::Paste("Keep draft 中文".into()));
    send(&mut app, "Ctrl-a");
    send(&mut app, "Cmd+Ctrl+h Cmd+Ctrl+j Cmd+Ctrl+k Cmd+Ctrl+l");
    assert_eq!(app.panel().selected_text(), Some("Keep draft 中文"));
    assert!(app.panel().editing());
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Kitty".into()));
    send(&mut app, "Cmd+Ctrl+j");
    assert!(screen(&mut app, 120, 36).contains("No matching shortcuts"));
    send(&mut app, "Esc Esc F2 q");
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: false, .. })));
    send(&mut app, "Cmd+Ctrl+h Cmd+Ctrl+j Cmd+Ctrl+k Cmd+Ctrl+l");
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: false, .. })));
    assert!(!app.quit);
    assert!(app.kitty.requests.is_empty());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn configuration_smart_focus_ignores_repeats_cancels_prefixes_and_runs_from_help() {
    use crate::ui::panes::Direction;
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_FOCUS);
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-w Cmd+Ctrl+h");
    assert!(app.window_prefix.is_none());
    assert_eq!(app.help_content().context(), "Log / Branch Tree");
    for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
        let mut key = parse_sequence("Cmd+Ctrl+h").unwrap()[0];
        key.kind = kind;
        app.dispatch(&Event::Key(key));
    }
    assert!(app.kitty.requests.is_empty());
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Focus left pane or Kitty".into()));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("Ctrl-Super-h"), "{help}");
    send(&mut app, "Enter");
    assert!(app.pop.is_none());
    assert_eq!(app.kitty.requests, [Direction::Left]);
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Focus right pane or Kitty".into()));
    send(&mut app, "Enter");
    assert_eq!(app.help_content().context(), "Log / Commits");
    assert_eq!(app.kitty.requests.len(), 1);
}

#[test]
fn configuration_smart_focus_stays_in_tool_dialog_and_survives_workspace_rescan() {
    use crate::ui::panes::{Direction, PaneId};
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_FOCUS);
    screen(&mut app, 120, 36);
    send(&mut app, "3");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+l");
    assert_eq!(app.push.focused_pane(), PaneId::PushFiles);
    send(&mut app, "Cmd+Ctrl+k");
    assert_eq!(app.surface(), Dialog(Push));
    assert_eq!(app.kitty.requests, [Direction::Up]);
    send(&mut app, "Esc 4");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+h");
    assert_eq!(app.surface(), Dialog(Merge));
    assert_eq!(app.kitty.requests, [Direction::Up, Direction::Left]);
    send(&mut app, "Esc");
    app.pop = Some(Popup::Open {
        path: app.ctx.workspace.display().to_string(),
    });
    send(&mut app, "Enter");
    assert!(app.pop.is_none());
    assert!(app.config.smart_panes_enabled());
    assert_eq!(app.kitty.requests, [Direction::Up, Direction::Left]);
}

#[test]
fn configuration_remapped_review_and_help_keep_git_and_index_unchanged() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    configure(
        &mut app,
        "[keybindings.global]\nfocus-commit=['F8']\n[keybindings.commit]\nreview=['Ctrl-r']",
    );
    send(&mut app, "2");
    assert_eq!(app.surface(), Split(Log));
    send(&mut app, "F8");
    assert_eq!(app.surface(), Split(Commit));
    screen(&mut app, 120, 36);
    // mixed.txt is selected, while other.txt has unrelated staged content.
    send(&mut app, "G Space");
    send(&mut app, "i");
    app.dispatch(&Event::Paste("configured review".into()));
    send(&mut app, "F2");
    send(&mut app, "c");
    assert!(app.pop.is_none());
    send(&mut app, "Ctrl-r");
    match &app.pop {
        Some(Popup::Confirm { body, .. }) => {
            assert!(body.contains("mixed.txt"), "{body}");
            assert!(!body.contains("other.txt"), "{body}");
        }
        _ => panic!(
            "remapping must open review, not execute a commit: {}",
            app.msg
        ),
    }
    send(&mut app, "Esc F1");
    app.dispatch(&Event::Paste("Review commit of visible".into()));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("Ctrl-r"), "{help}");
    send(&mut app, "Enter");
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    send(&mut app, "Esc");
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn configuration_swaps_unbinds_and_preserves_text_input() {
    let (_dir, mut app) = fixture();
    let _capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.commit]\nedit-message=['c']\nreview=['i']\nai-draft=[]",
    );
    send(&mut app, "2 A");
    assert!(app.pop.is_none());
    assert!(!app.panel().editing());
    send(&mut app, "c");
    assert!(app.panel().editing());
    send(&mut app, "c i A q");
    app.dispatch(&Event::Paste(" 中文".into()));
    send(&mut app, "Ctrl-a Ctrl-c");
    assert_eq!(app.panel().selected_text(), Some("ciAq 中文"));
    assert!(app.pop.is_none());
    send(&mut app, "Esc F2 i");
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
}

#[test]
fn configuration_sequences_share_prefixes_and_use_custom_hints() {
    let (_dir, mut app) = fixture();
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings]\nhint_delay_ms=450\n[keybindings.log]\ncopy-author=['g a']",
    );
    send(&mut app, "g");
    assert!(app.pending_keys.is_some());
    assert!(app.key_picker.wakeup(Instant::now()).unwrap() > Duration::from_millis(200));
    app.key_picker
        .poll(Instant::now() + Duration::from_millis(450));
    let hints = screen(&mut app, 120, 36);
    assert!(hints.contains("Copy author name and email"), "{hints}");
    assert!(hints.contains("Select first commit"), "{hints}");
    send(&mut app, "a");
    assert_eq!(capture.texts(), ["Help tester <help@example.invalid>"]);
    send(&mut app, "y a");
    assert_eq!(capture.texts().len(), 1);
    // y a is no longer a copy command; a is reprocessed as the User filter.
    send(&mut app, "Esc y y");
    assert_eq!(capture.texts().len(), 2);
    send(&mut app, "g g");
    assert!(app.pending_keys.is_none());
    assert!(app.key_sequence().is_none());
}

#[test]
fn configuration_prefix_cancel_context_changes_and_mouse_picker() {
    let (_dir, mut app) = fixture();
    configure(&mut app, "[keybindings.global]\nquit=['Q Q']");
    for interrupt in [
        key(KeyCode::Esc),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Event::Resize(80, 24),
        Event::FocusLost,
        Event::Paste("Q".into()),
        key(KeyCode::Tab),
    ] {
        screen(&mut app, 120, 36);
        send(&mut app, "Q");
        app.dispatch(&interrupt);
        assert!(app.pending_keys.is_none());
        assert!(!app.quit);
        assert!(app.pop.is_none());
    }
    for (w, h) in [(44, 12), (80, 24), (120, 36)] {
        send(&mut app, "Q");
        app.key_picker
            .poll(Instant::now() + Duration::from_millis(200));
        let text = screen(&mut app, w, h);
        let (x, y) = text
            .lines()
            .enumerate()
            .find_map(|(y, line)| {
                line.find(" Q  Review and quit")
                    .map(|x| (x as u16, y as u16))
            })
            .unwrap();
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
        send(&mut app, "Esc");
    }
}

#[test]
fn configuration_settings_survive_rescan_and_empty_workspace() {
    let (_dir, mut app) = fixture();
    configure(
        &mut app,
        "[terminal]\nmouse=false\n[keybindings.global]\nroots=['F8']",
    );
    app.pop = Some(Popup::Open {
        path: app.ctx.workspace.display().to_string(),
    });
    send(&mut app, "Enter");
    assert!(!app.config.mouse);
    assert_eq!(app.config.icons, IconMode::Ascii);
    send(&mut app, "r");
    assert!(app.pop.is_none());
    send(&mut app, "F8");
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));
    let empty = tempfile::tempdir().unwrap();
    app.pop = Some(Popup::Open {
        path: empty.path().display().to_string(),
    });
    send(&mut app, "Enter F8");
    assert!(app.ctx.repos.is_empty());
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));
}

#[test]
fn configuration_window_commands_keep_counts_and_original_context() {
    let (_dir, mut app) = fixture();
    configure(&mut app, "[keybindings.panes]\nfocus-up=['Alt-k']");
    screen(&mut app, 120, 36);
    let before = app.regions.commit.height;
    send(&mut app, "Ctrl-w 3 +");
    screen(&mut app, 120, 36);
    assert_eq!(app.regions.commit.height, before - 3);
    send(&mut app, "Ctrl-w =");
    screen(&mut app, 120, 36);
    assert_eq!(app.regions.commit.height, before);
    send(&mut app, "Ctrl-w k");
    assert_eq!(app.surface(), Split(Log));
    send(&mut app, "Alt-k");
    assert_eq!(app.surface(), Split(Commit));
}

#[test]
fn configuration_keeps_unmodified_prefix_mouse_actions_and_copy_precedence() {
    let (_dir, mut app) = fixture();
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.global]\nquit=[]\nroots=['Q Q']\n[keybindings.log]\npage-down=['F9']",
    );
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Page down".into()));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("F9") && !help.contains("Ctrl-f"), "{help}");
    send(&mut app, "Esc y");
    assert!(app.key_sequence().is_some());
    app.key_picker
        .poll(Instant::now() + Duration::from_millis(200));
    let text = screen(&mut app, 120, 36);
    let (x, y) = text
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find(" y  Copy full commit ID")
                .map(|x| (x as u16, y as u16))
        })
        .unwrap();
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(capture.texts().len(), 1);
    send(&mut app, "Tab Tab");
    screen(&mut app, 120, 36);
    send(&mut app, "Ctrl-a");
    let selected = app.panel().selected_text().unwrap().to_owned();
    send(&mut app, "Q Ctrl-c");
    assert_eq!(capture.texts().len(), 1); // Pending-prefix cancellation wins over copying.
    send(&mut app, "Ctrl-c");
    assert_eq!(capture.texts().last(), Some(&selected));
    assert!(app.pop.is_none());
    send(&mut app, "Esc Ctrl-c");
    assert!(app.pop.is_none()); // Ctrl-c was unbound outside selection/prefix cancellation.
}

#[test]
fn configuration_multistep_named_continuations_and_mouse_off() {
    let (_dir, mut app) = fixture();
    configure(&mut app, "[terminal]\nmouse=false\n[keybindings.global]\nroots=['F8 Enter']\nfocus-commit=['F9 x y']");
    screen(&mut app, 120, 36);
    let header = app.regions.commit;
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: header.x,
        row: header.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.surface(), Split(Log));
    send(&mut app, "F9 x y");
    assert_eq!(app.surface(), Split(Commit));
    send(&mut app, "F8");
    app.key_picker
        .poll(Instant::now() + Duration::from_millis(200));
    screen(&mut app, 120, 36);
    send(&mut app, "Enter");
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));
    send(&mut app, "Esc");
    app.dispatch(&Event::Key(KeyEvent::new_with_kind(
        KeyCode::F(8),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    )));
    assert!(app.pending_keys.is_none());
}

#[test]
fn configuration_new_shortcuts_interrupt_default_prefixes_without_stealing_suffixes() {
    let (_dir, mut app) = fixture();
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.global]\nfocus-commit=['F8']\n[keybindings.log]\nsearch=['m']",
    );
    send(&mut app, "y m");
    assert_eq!(capture.texts(), ["base"]); // m still completes ym while y is pending.
    assert!(!app.panel().editing());
    send(&mut app, "g F8");
    assert_eq!(app.surface(), Split(Commit));
    assert!(app.key_sequence().is_none());
}
