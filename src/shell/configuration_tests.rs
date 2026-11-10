use super::{test_support::*, *};
use crate::config::keys::parse_sequence;
use crate::core::git;
use crate::ui::clipboard::CopyCapture;
use crossterm::event::MouseEvent;

#[test]
fn ai_settings_reach_commit_from_application_configuration() {
    let (_dir, original) = fixture();
    let config = AppConfig::from_toml("[ai]\nendpoint='http://localhost:11434/v1/chat/completions'\nmodel='configured-model'\nprompt='Custom prompt'\napi_key_env='CUSTOM_AI_KEY'").unwrap();
    let mut app = Shell::with_config(original.ctx.clone(), config);
    send(&mut app, "S");
    let settings = screen(&mut app, 120, 36);
    assert!(settings.contains("configured-model"));
    assert!(settings.contains("http://localhost:11434/v1/chat/completions"));
    assert!(settings.contains("CUSTOM_AI_KEY"));
    send(&mut app, "Esc");
}

#[test]
fn configuration_hunk_view_remaps_and_help_review_keeps_git_unchanged_on_cancel() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    crate::core::git(&repo, &["config", "core.autocrlf", "false"]).unwrap();
    configure(&mut app, "[keybindings.commit]\nstage-hunks=['Ctrl-y']");
    screen(&mut app, 120, 36);
    send(&mut app, "Home Down");
    let before = snapshot(&repo);
    send(&mut app, "Ctrl-g");
    assert_ne!(app.help_content().context(), "Commit / Stage Hunks");
    send(&mut app, "Ctrl-y");
    assert_eq!(app.help_content().context(), "Commit / Stage Hunks");
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Review selected hunk".into()));
    send(&mut app, "Enter");
    assert!(
        matches!(&app.pop, Some(Popup::Confirm { token, yes: false, .. }) if token == "commit-hunk")
    );
    send(&mut app, "Esc Esc");
    assert!(!app.commit.editing());
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn running_commit_blocks_push_fetch_roots_workspace_replacement_and_quit() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    let complete = app.commit.hold_execution();
    app.go(Push);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.push.reads_pending() {
        app.poll();
        assert!(Instant::now() < deadline, "Push reads did not finish");
        std::thread::yield_now();
    }
    screen(&mut app, 120, 36);
    send(&mut app, "F");
    assert!(!app.push.busy());
    assert!(
        app.notifications.latest_body().contains("Git write"),
        "{}",
        app.notifications.latest_body()
    );
    for command in ["r", "f", "p", "q", "Ctrl-c"] {
        send(&mut app, command);
        assert!(
            app.pop.is_none(),
            "{command} bypassed the running commit guard"
        );
        assert!(!app.quit);
    }
    let open = app
        .menu_actions()
        .position(|action| matches!(action, MenuAction::Open))
        .unwrap();
    app.menu_action(open);
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    complete();
    app.poll();
    assert!(!app.commit.execution_running());
}

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
    app.go(Log);
    configure(&mut app, SMART_RESIZE);
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(120, 36), (180, 48)] {
        send(&mut app, "Ctrl-w =");
        let initial = screen(&mut app, width, height);
        let refs = app.log.panes.rect(PaneId::Refs).unwrap();
        let graph = app.log.panes.rect(PaneId::Graph).unwrap();
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
        send(&mut app, "Tab");
        let info = app.log.panes.rect(PaneId::Info).unwrap();
        send(&mut app, "Cmd+Ctrl+Shift+j");
        screen(&mut app, width, height);
        assert_eq!(app.log.panes.rect(PaneId::Info).unwrap().y, info.y + 3);
        send(&mut app, "Cmd+Ctrl+Shift+k Shift-Tab");
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
    app.go(Log);
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
    // Log's dividers remain usable at 80 columns, but are fixed at 44.
    screen(&mut app, 80, 24);
    let graph = app.log.panes.rect(PaneId::Graph).unwrap();
    send(&mut app, "Cmd+Ctrl+Shift+h");
    screen(&mut app, 80, 24);
    assert_eq!(
        app.log.panes.rect(PaneId::Graph).unwrap().width,
        graph.width - 3
    );
    assert_eq!(app.kitty.resizes, [Direction::Right]);
    screen(&mut app, 44, 12);
    send(&mut app, "Cmd+Ctrl+Shift+h");
    assert_eq!(app.kitty.resizes, [Direction::Right, Direction::Left]);
    assert_eq!(app.help_content().context(), "Log / Commits");
}

#[test]
fn configuration_smart_resize_keeps_tabs_isolated_and_help_executes_the_action() {
    use crate::ui::panes::{Direction, PaneId};
    let (_dir, mut app) = fixture();
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    configure(&mut app, SMART_RESIZE);
    screen(&mut app, 120, 36);
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
    assert!(app.log.panes.nodes().is_empty());
    send(&mut app, "Esc 6");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+Shift+h");
    assert_eq!(app.tab, Conflicts);
    assert_eq!(app.kitty.resizes, [Direction::Down, Direction::Left]);
    send(&mut app, "Esc 2 F1");
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
fn configuration_smart_resize_preserves_drafts_and_selection() {
    let (_dir, mut app) = fixture();
    configure(&mut app, SMART_RESIZE);
    let before = snapshot(&app.ctx.repos[0].path);
    screen(&mut app, 120, 36);
    send(&mut app, "G Space i");
    app.dispatch(&Event::Paste("Resize draft 中文".into()));
    send(&mut app, "Ctrl-a");
    let body = app.regions.body;
    send(
        &mut app,
        "Cmd+Ctrl+Shift+h Cmd+Ctrl+Shift+j Cmd+Ctrl+Shift+k Cmd+Ctrl+Shift+l",
    );
    assert_eq!(app.panel().selected_text(), Some("Resize draft 中文"));
    assert_eq!(app.regions.body, body);
    send(&mut app, "Esc");
    open_review(&mut app);
    send(
        &mut app,
        "Cmd+Ctrl+Shift+h Cmd+Ctrl+Shift+j Cmd+Ctrl+Shift+k Cmd+Ctrl+Shift+l",
    );
    assert!(
        matches!(&app.pop, Some(Popup::Confirm { token, yes: false, .. }) if token == "test-review")
    );
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
    let text = screen(&mut app, 120, 36);
    assert!(
        text.contains("Resize draft 中文") && text.contains("1 file selected"),
        "{text}"
    );
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn configuration_smart_focus_uses_internal_neighbors_then_kitty_edges() {
    use crate::ui::panes::Direction;
    let (_dir, mut app) = fixture();
    app.go(Log);
    configure(&mut app, SMART_FOCUS);
    let before = snapshot(&app.ctx.repos[0].path);
    screen(&mut app, 120, 36);
    for (keys, context) in [
        ("Cmd+Ctrl+h", "Log / Branch Tree"),
        ("Cmd+Ctrl+h", "Log / Branch Tree"),
        ("Cmd+Ctrl+k", "Log / Branch Tree"),
        ("Cmd+Ctrl+j", "Log / Branch Tree"),
        ("Cmd+Ctrl+l", "Log / Commits"),
        ("Cmd+Ctrl+l", "Log / File Tree"),
        ("Cmd+Ctrl+j", "Log / Commit Details"),
        ("Cmd+Ctrl+k", "Log / File Tree"),
        ("Cmd+Ctrl+l", "Log / File Tree"),
        ("1", "Commit / Changes"),
        ("Cmd+Ctrl+j", "Commit / Message"),
        ("Cmd+Ctrl+k", "Commit / Changes"),
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
    send(&mut app, "i");
    app.dispatch(&Event::Paste("Keep draft 中文".into()));
    send(&mut app, "Ctrl-a");
    send(&mut app, "Cmd+Ctrl+h Cmd+Ctrl+j Cmd+Ctrl+k Cmd+Ctrl+l");
    assert_eq!(app.panel().selected_text(), Some("Keep draft 中文"));
    assert!(app.panel().editing());
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Kitty".into()));
    send(&mut app, "Cmd+Ctrl+j");
    assert!(screen(&mut app, 120, 36).contains("No matching shortcuts"));
    send(&mut app, "Esc Esc");
    open_review(&mut app);
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
    app.go(Log);
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
fn configuration_smart_focus_stays_in_the_tab_and_survives_workspace_rescan() {
    use crate::ui::panes::{Direction, PaneId};
    let (_dir, mut app) = fixture();
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    configure(&mut app, SMART_FOCUS);
    screen(&mut app, 120, 36);
    send(&mut app, "3");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+l");
    assert_eq!(app.push.focused_pane(), PaneId::PushFiles);
    send(&mut app, "Cmd+Ctrl+k");
    assert_eq!(app.tab, Push);
    assert_eq!(app.kitty.requests, [Direction::Up]);
    send(&mut app, "Esc 6");
    screen(&mut app, 120, 36);
    send(&mut app, "Cmd+Ctrl+h");
    assert_eq!(app.tab, Conflicts);
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
fn configuration_remapped_message_focus_keeps_help_and_commits_only_on_enter() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    configure(
        &mut app,
        "[keybindings.global]\nnotification-history=[]\nfocus-commit=['F8']\n[keybindings.commit]\nreview=['Ctrl-r']",
    );
    send(&mut app, "2");
    assert_eq!(app.tab, Log);
    send(&mut app, "1");
    assert_eq!(
        app.tab, Log,
        "the replaced Commit shortcut must stay inactive"
    );
    send(&mut app, "F8");
    assert_eq!(app.tab, Commit);
    screen(&mut app, 120, 36);
    // mixed.txt is selected, while other.txt has unrelated staged content.
    send(&mut app, "G Space c");
    assert!(!app.commit.message_editing());
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Focus and edit commit message".into()));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("Ctrl-r"), "{help}");
    send(&mut app, "Esc Ctrl-r");
    assert!(app.commit.message_editing());
    app.dispatch(&Event::Paste("configured commit".into()));
    send(&mut app, "Esc Ctrl-r");
    assert!(!app.commit.message_editing());
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "Enter");
    assert!(app.pop.is_none());
    assert_eq!(git(&repo, &["rev-parse", "HEAD^"]).unwrap(), before[0]);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B"]).unwrap(),
        "configured commit"
    );
    assert_eq!(git(&repo, &["show", "HEAD:mixed.txt"]).unwrap(), "staged");
    assert_eq!(
        git(&repo, &["diff", "--cached", "--name-only"]).unwrap(),
        "other.txt"
    );
    assert_eq!(git(&repo, &["diff", "--binary"]).unwrap(), before[2]);
}

#[test]
fn configuration_add_to_git_defaults_remapping_and_help_execute_without_confirmation() {
    use crate::core::git;
    use std::fs;

    fn finish_add(app: &mut Shell) {
        assert!(app.commit.add_running());
        let deadline = Instant::now() + Duration::from_secs(15);
        while app.commit.add_running() {
            app.poll();
            assert!(Instant::now() < deadline, "Addition did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.pop.is_none());
    }

    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let original = snapshot(&repo);
    for (shortcut, path) in [("a", "new 中文.txt"), ("Insert", "insert.txt")] {
        fs::write(repo.join(path), "new file\n").unwrap();
        app.refresh();
        send(&mut app, "G");
        send(&mut app, shortcut);
        assert!(app.pop.is_none());
        finish_add(&mut app);
        assert_eq!(
            git(&repo, &["show", &format!(":{path}")]).unwrap(),
            "new file"
        );
        for (width, height) in [(120, 36), (80, 24), (44, 12)] {
            screen(&mut app, width, height);
        }
    }
    fs::write(repo.join("remapped.txt"), "remapped file\n").unwrap();
    app.refresh();
    send(&mut app, "G");
    let before = snapshot(&repo);
    configure(&mut app, "[keybindings.commit]\nadd-to-git=['Alt-a']");
    send(&mut app, "a Insert I");
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Add untracked".into()));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("Alt-a"), "{help}");
    send(&mut app, "Enter");
    assert!(app.pop.is_none());
    finish_add(&mut app);
    assert_eq!(
        git(&repo, &["show", ":remapped.txt"]).unwrap(),
        "remapped file"
    );
    fs::write(repo.join("direct.txt"), "direct remapped file\n").unwrap();
    app.refresh();
    send(&mut app, "G Alt-a");
    assert!(app.pop.is_none());
    finish_add(&mut app);
    assert_eq!(
        git(&repo, &["show", ":direct.txt"]).unwrap(),
        "direct remapped file"
    );
    assert_eq!(git(&repo, &["show", ":mixed.txt"]).unwrap(), "staged");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), original[0]);
    assert_eq!(fs::read(repo.join("mixed.txt")).unwrap(), b"working tree\n");
    assert_eq!(fs::read(repo.join("new 中文.txt")).unwrap(), b"new file\n");
}

#[cfg(unix)]
#[test]
fn add_to_git_slow_shell_operation_keeps_feedback_help_tabs_and_write_guards_available() {
    use crate::core::git;
    use std::fs;
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let ready = repo.join(".git/add-shell-ready");
    let release = repo.join(".git/add-shell-release");
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "release");
        }
    }
    let _release = Release(release.clone());
    let script = repo.join(".git/add-shell-filter.sh");
    fs::write(
        &script,
        "printf ready > \"$1\"\ni=0\nwhile [ ! -f \"$2\" ] && [ -f \"$0\" ] && [ \"$i\" -lt 1000 ]; do sleep 0.01; i=$((i+1)); done\n[ -f \"$2\" ] || exit 1\ncat\n",
    )
    .unwrap();
    let quote =
        |path: &std::path::Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    git(
        &repo,
        &[
            "config",
            "filter.hold.clean",
            &format!(
                "sh {} {} {}",
                quote(&script),
                quote(&ready),
                quote(&release)
            ),
        ],
    )
    .unwrap();
    fs::write(repo.join(".gitattributes"), "zz-*.txt filter=hold\n").unwrap();
    fs::write(repo.join("zz-new-file.txt"), "new file\n").unwrap();
    app.refresh();
    send(&mut app, "G");
    let before = snapshot(&repo);
    let started = Instant::now();
    send(&mut app, "a");
    let first_frame = screen(&mut app, 120, 36);
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(250),
        "First feedback took {elapsed:?}"
    );
    eprintln!("ADD_UI_FEEDBACK first_frame_us={}", elapsed.as_micros());
    assert!(first_frame.contains("Adding 1 file"), "{first_frame}");
    assert!(app.pop.is_none() && app.git_busy());
    assert!(app.poll_timeout() <= Duration::from_millis(100));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        app.poll();
        assert!(Instant::now() < deadline, "Filter did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        let text = screen(&mut app, width, height);
        assert!(text.contains("Adding 1 file"), "{text}");
    }
    send(&mut app, "F1");
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    assert!(screen(&mut app, 120, 36).contains("Adding 1 file"));
    send(&mut app, "Esc r p f");
    app.commit.focus_pane(crate::ui::panes::PaneId::Message);
    send(&mut app, "Enter");
    assert!(app.pop.is_none());
    send(&mut app, "i");
    app.dispatch(&Event::Paste("Draft while adding".into()));
    send(&mut app, "F2 2");
    assert_eq!(app.tab, Log);
    send(&mut app, "p f 3 F");
    assert_eq!(app.tab, Push);
    assert!(!app.push.busy());
    send(&mut app, "1 q Esc");
    assert!(!app.quit && app.pop.is_none() && app.commit.add_running());
    app.commit.focus_pane(crate::ui::panes::PaneId::Changes);
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Add to Git progress".into()));
    fs::write(release, "release").unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.commit.add_running() {
        app.poll();
        assert!(Instant::now() < deadline, "Addition did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Busy help must not turn a former progress action into a new Add after completion.
    send(&mut app, "Enter");
    assert!(matches!(app.pop, Some(Popup::Help { .. })) && !app.commit.add_running());
    send(&mut app, "Esc");
    assert!(app.pop.is_none());
    assert!(
        app.notifications
            .latest_body()
            .contains("1 added · 0 failed"),
        "{}",
        app.notifications.latest_body()
    );
    assert_eq!(
        git(&repo, &["show", ":zz-new-file.txt"]).unwrap(),
        "new file"
    );
    assert_eq!(git(&repo, &["show", ":mixed.txt"]).unwrap(), "staged");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), before[0]);
    assert!(git(
        &repo,
        &["ls-files", "--error-unmatch", "--", ".gitattributes"]
    )
    .is_err());
    assert_eq!(git(&repo, &["diff", "--binary"]).unwrap(), before[2]);
    assert!(screen(&mut app, 120, 36).contains("Draft while adding"));
}

#[test]
fn configuration_swaps_unbinds_and_preserves_text_input() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let _capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.commit]\nedit-message=['c']\nreview=['i']\nai-draft=[]",
    );
    send(&mut app, "A");
    assert!(app.pop.is_none());
    assert!(!app.panel().editing());
    send(&mut app, "c");
    assert!(app.panel().editing());
    send(&mut app, "c i A q");
    app.dispatch(&Event::Paste(" 中文".into()));
    send(&mut app, "Ctrl-a Ctrl-c");
    assert_eq!(app.panel().selected_text(), Some("ciAq 中文"));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "Esc i");
    assert!(!app.commit.message_editing());
    send(&mut app, "c");
    assert!(app.commit.message_editing());
    send(&mut app, "Esc Enter");
    assert!(app.pop.is_none());
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B"]).unwrap(),
        "ciAq 中文"
    );
}

#[test]
fn configuration_sequences_share_prefixes_and_use_custom_hints() {
    let (_dir, mut app) = fixture();
    app.go(Log);
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
fn configuration_push_commit_copy_remaps_without_affecting_file_copy() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]).unwrap();
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    send(&mut app, "j");
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.push]\ncopy-id=['g y']\ncopy-message=['g m']\ncopy-author=['g a']",
    );
    send(&mut app, "g");
    assert!(app.pending_keys.is_some());
    app.key_picker
        .poll(Instant::now() + Duration::from_millis(450));
    let hints = screen(&mut app, 120, 36);
    assert!(hints.contains("Copy author name and email"), "{hints}");
    send(&mut app, "a");
    assert_eq!(capture.texts(), ["Help tester <help@example.invalid>"]);
    send(&mut app, "g y");
    assert_eq!(
        capture.texts().last().unwrap(),
        &git(&repo, &["rev-parse", "HEAD"]).unwrap()
    );
    send(&mut app, "g m");
    assert_eq!(
        capture.texts().last().unwrap(),
        &git(&repo, &["show", "-s", "--format=%B", "HEAD"]).unwrap()
    );
    for removed in ["y y", "y m", "y a"] {
        send(&mut app, removed);
        assert_eq!(capture.texts().len(), 3);
    }
    app.push.finish_loading(&app.ctx);
    app.push.focus_pane(crate::ui::panes::PaneId::PushFiles);
    send(&mut app, "y r");
    assert_eq!(capture.texts().len(), 4);
    assert!(app.pop.is_none());
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
    send(&mut app, "q");
    assert!(!app.quit && app.pop.is_none());
    send(&mut app, "Q Q");
    assert!(app.quit && app.pop.is_none());
    for (w, h) in [(44, 12), (80, 24), (120, 36)] {
        let (_dir, mut app) = fixture();
        configure(&mut app, "[keybindings.global]\nquit=['Q Q']");
        send(&mut app, "Q");
        app.key_picker
            .poll(Instant::now() + Duration::from_millis(200));
        let text = screen(&mut app, w, h);
        let (x, y) = text
            .lines()
            .enumerate()
            .find_map(|(y, line)| line.find(" Q  Quit IGit").map(|x| (x as u16, y as u16)))
            .unwrap();
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.quit && app.pop.is_none());
    }
}

#[test]
fn configuration_settings_survive_rescan_and_empty_workspace() {
    let (_dir, mut app) = fixture();
    configure(
        &mut app,
        "[terminal]\nmouse=false\n[keybindings.global]\nnotification-history=[]\nroots=['F8']",
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
    use crate::ui::panes::PaneId;
    let (_dir, mut app) = fixture();
    app.go(Log);
    configure(&mut app, "[keybindings.panes]\nfocus-up=['Alt-k']");
    screen(&mut app, 120, 36);
    send(&mut app, "Tab Tab");
    let height = |app: &Shell| app.log.panes.rect(PaneId::Info).unwrap().height;
    let before = height(&app);
    send(&mut app, "Ctrl-w 3 +");
    screen(&mut app, 120, 36);
    assert_eq!(height(&app), before + 3);
    send(&mut app, "Ctrl-w =");
    screen(&mut app, 120, 36);
    assert_eq!(height(&app), before);
    send(&mut app, "Ctrl-w k");
    assert_eq!(app.log.focused_pane(), PaneId::Info);
    send(&mut app, "Alt-k");
    assert_eq!(app.log.focused_pane(), PaneId::Files);
}

#[test]
fn configuration_keeps_unmodified_prefix_mouse_actions_and_copy_precedence() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.global]\nquit=[]\nupdate=[]\nroots=['Q Q']\n[keybindings.log]\npage-down=['F9']",
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
    app.go(Log);
    configure(&mut app, "[terminal]\nmouse=false\n[keybindings.global]\nupdate=[]\nnotification-history=[]\nroots=['F8 Enter']\nfocus-commit=['F9 x y']");
    screen(&mut app, 120, 36);
    let header = app.regions.tab(Commit);
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: header.x,
        row: header.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.tab, Log);
    send(&mut app, "F9 x y");
    assert_eq!(app.tab, Commit);
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
    app.go(Log);
    let capture = CopyCapture::start();
    configure(
        &mut app,
        "[keybindings.global]\nnotification-history=[]\nfocus-commit=['F8']\n[keybindings.log]\nsearch=['m']",
    );
    send(&mut app, "y m");
    assert_eq!(capture.texts(), ["base"]); // m still completes ym while y is pending.
    assert!(!app.panel().editing());
    send(&mut app, "g F8");
    assert_eq!(app.tab, Commit);
    assert!(app.key_sequence().is_none());
}

#[test]
fn configuration_remaps_workspace_update_and_log_filter_prefixes() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    configure(&mut app, "[keybindings.log]\npaths-filter=['Space x']");
    send(&mut app, "Space p");
    assert!(app.pop.is_none() && !app.log.editing());
    send(&mut app, "p");
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    send(&mut app, "Esc");
    configure(
        &mut app,
        "[keybindings.global]\nnotification-history=[]\npull=['F8']\nfetch=[]\n[keybindings.log]\npaths-filter=['Space x']",
    );
    send(&mut app, "p f Space p");
    assert!(app.pop.is_none() && !app.log.editing());
    send(&mut app, "Space x");
    assert_eq!(app.help_content().context(), "Log / Paths Filter");
    send(&mut app, "Esc Space u");
    assert_eq!(app.help_content().context(), "Log / User Filter");
    send(&mut app, "Esc F8");
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    send(&mut app, "Esc F1");
    app.dispatch(&Event::Paste("Review Pull".into()));
    assert!(screen(&mut app, 120, 36).contains("F8"));
    send(&mut app, "Enter");
    assert!(matches!(app.pop, Some(Popup::Update(_))));
}

#[test]
fn pane_search_remaps_owns_text_input_and_keeps_other_pane_commands() {
    let (_dir, mut app) = fixture();
    configure(
        &mut app,
        "[keybindings.global]\nnotification-history=[]\n[keybindings.commit]\nfind=['F8']\nnext-match=['F7']",
    );
    screen(&mut app, 80, 24);
    send(&mut app, "/");
    assert!(!app.commit.editing());
    send(&mut app, "F8");
    assert_eq!(app.help_content().context(), "Pane Search");
    send(&mut app, "p r 5 q");
    assert_eq!(app.tab, Commit);
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 44, 12).contains("/pr5q"));
    send(&mut app, "F1 Esc");
    assert!(app.commit.editing());
    send(&mut app, "Ctrl-u");
    app.dispatch(&Event::Paste(".TXT".into()));
    send(&mut app, "Enter");
    assert!(screen(&mut app, 80, 24).contains("/.TXT  1/2"));
    send(&mut app, "n");
    assert!(screen(&mut app, 80, 24).contains("/.TXT  1/2"));
    send(&mut app, "F7");
    assert!(screen(&mut app, 80, 24).contains("/.TXT  2/2"));
    send(&mut app, "Esc Ctrl-n");
    assert_eq!(app.help_content().context(), "Commit / New Changelist");
}

#[test]
fn pane_search_escape_clears_push_search_and_cancel_returns_to_commit() {
    let (_dir, mut app) = fixture();
    app.go(Push);
    screen(&mut app, 80, 24);
    send(&mut app, "/");
    app.dispatch(&Event::Paste("no match".into()));
    send(&mut app, "Enter");
    assert!(screen(&mut app, 80, 24).contains("No matches"));
    send(&mut app, "Esc");
    assert_eq!(app.tab, Push);
    assert!(!app.push.focused_search().unwrap().visible());
    send(&mut app, "Esc");
    assert_eq!(app.tab, Push);
    click_push_cancel(&mut app, 80, 24);
    assert_eq!(app.tab, Commit);
}
