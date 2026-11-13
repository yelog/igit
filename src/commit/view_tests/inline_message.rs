use super::*;

#[test]
fn inline_message_tab_inserts_indentation_at_caret_and_replaces_selection() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.message = Some("Subject\n中文 body".into());
    p.edit_message(&c);
    key(&mut p, &c, K::Home);
    key(&mut p, &c, K::Tab);
    assert!(p.message_editing());
    assert_eq!(p.focused_pane(), PaneId::Message);
    assert_eq!(p.buf, "Subject\n    中文 body");
    assert_eq!(p.caret, "Subject\n    ".len());
    p.event(&Event::Key(KeyEvent::new(K::Right, Mod::SHIFT)), &c);
    p.event(&Event::Key(KeyEvent::new(K::Right, Mod::SHIFT)), &c);
    key(&mut p, &c, K::Tab);
    assert_eq!(p.buf, "Subject\n         body");
    assert_eq!(p.caret, "Subject\n        ".len());
    assert!(p.message_selection.range().is_none());
    key(&mut p, &c, K::Esc);
    assert_eq!(p.saved.message.as_deref(), Some("Subject\n         body"));
}

fn hit(p: &CommitPanel, predicate: impl Fn(&Hit) -> bool) -> Rect {
    p.hits.iter().find(|(_, h)| predicate(h)).unwrap().0
}

#[test]
fn inline_message_click_keeps_editor_in_composer_and_positions_first_caret() {
    let (_dir, c) = fixture();
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        p.saved.message = Some("Subject ＵＩ\nBody".into());
        let checks = p.checks.clone();
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        draw(&mut p, &mut t, &c);
        let composer = p.panes.rect(PaneId::Message).unwrap();
        let draft = hit(&p, |h| matches!(h, Hit::Msg));
        click(&mut p, &c, draft.x + 3, draft.y);
        key(&mut p, &c, K::Char('!'));
        assert_eq!(p.buf, "Sub!ject ＵＩ\nBody");
        draw(&mut p, &mut t, &c);
        assert_eq!(p.edit_view.0, draft, "{w}x{h}");
        assert_eq!(composer.intersection(p.edit_view.0), p.edit_view.0);
        assert_eq!(t.backend().buffer()[(composer.x, composer.y)].fg, BLUE);
        assert!(read(t.backend().buffer(), p.tree).contains("Changes"));
        assert!(p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::MessageAction(MessageAction::Commit))));
        assert!(!p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::EditSave | Hit::EditCancel)));
        assert_eq!(p.checks, checks);
    }
}

#[test]
fn inline_message_first_drag_copies_and_escape_saves_the_edited_draft() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.message = Some("Subject ＵＩ\nBody".into());
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    draw(&mut p, &mut t, &c);
    let draft = hit(&p, |h| matches!(h, Hit::Msg));
    click(&mut p, &c, draft.x, draft.y);
    mouse(&mut p, &c, M::Drag(MouseButton::Left), draft.x + 7, draft.y);
    let action = mouse(&mut p, &c, M::Up(MouseButton::Left), draft.x + 7, draft.y);
    assert!(matches!(action, Action::Copy { text, .. } if text == "Subject"));
    key(&mut p, &c, K::Char('!'));
    assert_eq!(p.buf, "Subject! ＵＩ\nBody");
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::Esc);
    assert!(!p.editing());
    assert_eq!(p.focused_pane(), PaneId::Message);
    assert_eq!(p.saved.message.as_deref().unwrap(), "Subject! ＵＩ\nBody");
    key(&mut p, &c, K::Esc);
    assert_eq!(p.focused_pane(), PaneId::Message);
    assert!(p.preview_open);
    assert_eq!(p.saved.message.as_deref().unwrap(), "Subject! ＵＩ\nBody");
}

#[test]
fn default_preview_and_three_composer_actions_fit_wide_and_narrow_windows() {
    let (_dir, c) = fixture();
    for (w, h) in [(44, 12), (80, 24), (92, 12), (96, 36), (120, 36), (180, 36)] {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            let mut p = CommitPanel {
                icons,
                ..Default::default()
            };
            p.refresh(&c).unwrap();
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            for editing in [false, true] {
                if editing {
                    p.edit_message(&c);
                }
                draw(&mut p, &mut t, &c);
                assert!(p.preview_open);
                assert_eq!(p.diff.is_empty(), w < 92);
                let composer = p.panes.rect(PaneId::Message).unwrap();
                let actions: Vec<_> = p
                    .hits
                    .iter()
                    .filter(|(r, hit)| !matches!(hit, Hit::Msg) && composer.intersection(*r) == *r)
                    .collect();
                assert_eq!(
                    actions.len(),
                    3,
                    "{w}x{h}: {}",
                    read(t.backend().buffer(), composer)
                );
                assert!(matches!(
                    actions[0].1,
                    Hit::MessageAction(MessageAction::Commit)
                ));
                assert!(matches!(
                    actions[1].1,
                    Hit::MessageAction(MessageAction::CommitAndPush)
                ));
                assert!(matches!(
                    actions[2].1,
                    Hit::MessageAction(MessageAction::Generate)
                ));
                let ai = actions[2].0;
                assert!(read(t.backend().buffer(), ai).contains(icons.glyph(Icon::AutoResolve)));
                assert_eq!(
                    t.backend().buffer()[(ai.x, ai.y)].bg,
                    crate::ui::idea::button_style(false).bg.unwrap()
                );
                assert!(!p.edit_view.0.is_empty());
                for (index, (area, _)) in actions.iter().enumerate() {
                    assert!(actions[index + 1..]
                        .iter()
                        .all(|(other, _)| area.intersection(*other).is_empty()));
                }
            }
        }
    }
}

#[test]
fn message_action_highlight_follows_arrows_at_wide_and_compact_widths() {
    let (_dir, c) = fixture();
    for (w, h) in [(120, 36), (44, 12), (34, 6)] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        p.focus_pane(PaneId::Message);
        for (arrow, expected) in [
            (K::Left, MessageAction::Commit),
            (K::Right, MessageAction::CommitAndPush),
            (K::Right, MessageAction::Generate),
            (K::Right, MessageAction::Generate),
            (K::Left, MessageAction::CommitAndPush),
            (K::Left, MessageAction::Commit),
        ] {
            key(&mut p, &c, arrow);
            draw(&mut p, &mut t, &c);
            let actions = p.hits.iter().filter_map(|(area, hit)| match hit {
                Hit::MessageAction(action) => Some((*area, *action)),
                _ => None,
            });
            assert_eq!(actions.clone().count(), 3, "{w}x{h}");
            for (area, action) in actions {
                let style = crate::ui::idea::button_style(action == expected);
                assert_eq!(
                    t.backend().buffer()[(area.x, area.y)].bg,
                    style.bg.unwrap(),
                    "{w}x{h}"
                );
            }
            assert_eq!(p.focused_pane(), PaneId::Message);
            assert!(!p.editing());
        }
    }
}

#[test]
fn message_action_shortcuts_highlight_only_in_focused_normal_mode() {
    use ratatui::style::Modifier;

    let (_dir, c) = fixture();
    for (w, h) in [(34, 6), (44, 12), (80, 24), (120, 36), (180, 36)] {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            let mut p = CommitPanel {
                icons,
                ..Default::default()
            };
            p.refresh(&c).unwrap();
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut original_areas = Vec::new();
            for (focus, editing, active) in [
                (Focus::Tree, false, true),
                (Focus::Message, false, true),
                (Focus::Message, true, true),
                (Focus::Message, false, true),
                (Focus::Message, false, false),
                (Focus::Tree, false, true),
            ] {
                p.edit = None;
                p.set_focus(focus);
                p.panes.active = active;
                if editing {
                    p.edit_message(&c);
                }
                draw(&mut p, &mut t, &c);
                let actions: Vec<_> = p
                    .hits
                    .iter()
                    .filter_map(|(area, hit)| match hit {
                        Hit::MessageAction(action) => Some((*area, *action)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(actions.len(), 3, "{w}x{h}");
                let areas: Vec<_> = actions.iter().map(|(area, _)| *area).collect();
                if original_areas.is_empty() {
                    original_areas = areas;
                } else {
                    assert_eq!(areas, original_areas);
                }
                for (area, action) in actions {
                    let shortcut = match action {
                        MessageAction::Commit => "c",
                        MessageAction::CommitAndPush => "a",
                        MessageAction::Generate => "g",
                    };
                    let highlighted: Vec<_> = (area.x..area.right())
                        .map(|x| &t.backend().buffer()[(x, area.y)])
                        .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
                        .collect();
                    let expected = focus == Focus::Message && !editing && active;
                    assert_eq!(highlighted.len(), usize::from(expected), "{w}x{h}");
                    if let Some(cell) = highlighted.first() {
                        assert_eq!(cell.symbol(), shortcut);
                        assert_eq!(cell.fg, crate::ui::idea::GOLD);
                        assert!(cell.modifier.contains(Modifier::BOLD));
                    }
                }
            }
        }
    }
}

#[test]
fn message_action_shortcuts_type_in_editor_and_ignore_repeated_presses() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let checks = p.checks.clone();
    let index = git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap();
    p.focus_pane(PaneId::Message);
    for shortcut in ['c', 'a', 'g'] {
        let mut repeated = KeyEvent::new(K::Char(shortcut), Mod::NONE);
        repeated.kind = KeyEventKind::Repeat;
        assert!(matches!(p.event(&Event::Key(repeated), &c), Action::None));
        assert!(p.message_action == MessageAction::Commit);
        assert!(!p.ai_running());
        assert!(!p.execution_running());
    }
    key(&mut p, &c, K::Char('i'));
    for shortcut in ['c', 'a', 'g'] {
        assert!(matches!(
            p.event(&Event::Key(KeyEvent::new(K::Char(shortcut), Mod::NONE)), &c),
            Action::None
        ));
    }
    assert_eq!(p.buf, "cag");
    assert!(p.message_editing());
    assert_eq!(p.checks, checks);
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        index
    );
    assert!(git(&c.repos[0].path, &["rev-parse", "--verify", "HEAD"]).is_err());
}

#[test]
fn message_action_shortcuts_hide_remapped_and_disabled_mnemonics() {
    use ratatui::style::Modifier;

    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.focus_pane(PaneId::Message);
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    for (config, expected) in [
        ("", 3),
        ("[keybindings.commit]\ncommit=['F7']", 2),
        (
            "[keybindings.commit]\ncommit=[]\ncommit-and-push=[]\ngenerate-message=[]",
            0,
        ),
        ("[keybindings.commit]\ncommit=['F7', 'c']", 3),
    ] {
        p.configure_message_shortcuts(&crate::config::AppConfig::from_toml(config).unwrap());
        draw(&mut p, &mut t, &c);
        let highlighted = p
            .hits
            .iter()
            .filter(|(_, hit)| matches!(hit, Hit::MessageAction(_)))
            .flat_map(|(area, _)| {
                (area.x..area.right()).map(|x| &t.backend().buffer()[(x, area.y)])
            })
            .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
            .count();
        assert_eq!(highlighted, expected);
    }
}

#[test]
fn inline_message_backward_deletion_preserves_suffix_unicode_and_selection() {
    let (_dir, c) = fixture();
    let index = git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap();
    for (prefix, suffix, key, expected) in [
        ("fix: hello world", "!", 'w', "fix: hello !"),
        ("fix: 中文　👩‍💻  ", "suffix", 'w', "fix: 中文　suffix"),
        ("e\u{301}", "tail", 'w', "tail"),
        ("", "tail", 'w', "tail"),
        ("first\n", "tail", 'w', "tail"),
        (
            "first\n中文 e\u{301}",
            " suffix\nlast",
            'u',
            "first\n suffix\nlast",
        ),
        ("first\n", "tail", 'u', "first\ntail"),
        ("", "tail", 'u', "tail"),
    ] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        let checks = p.checks.clone();
        p.edit_message(&c);
        p.buf = format!("{prefix}{suffix}");
        p.caret = prefix.len();
        p.event(&Event::Key(KeyEvent::new(K::Char(key), Mod::CONTROL)), &c);
        assert_eq!(p.buf, expected);
        assert_eq!(p.caret, expected.len() - suffix.len());
        assert!(p.message_editing());
        assert_eq!(p.checks, checks);
    }
    for key in ['w', 'u'] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        p.edit_message(&c);
        p.buf = "keep 中文 end".into();
        p.message_selection.sync_source(&p.buf);
        p.caret = "keep 中文".len();
        p.message_selection.extend("keep ".len(), p.caret);
        p.event(&Event::Key(KeyEvent::new(K::Char(key), Mod::CONTROL)), &c);
        assert_eq!(p.buf, "keep  end");
        assert_eq!(p.caret, "keep ".len());
        assert!(p.message_selection.range().is_none());
    }
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        index
    );
}

#[test]
fn inline_message_is_shared_across_repository_scope_and_reopening() {
    let (_dir, mut c) = fixture();
    let repo = c.workspace.join("api");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(repo.join("api.txt"), "api body").unwrap();
    git(&repo, &["add", "."]).unwrap();
    c.repos.push(Repo {
        path: repo,
        name: "api".into(),
        branch: "main".into(),
        parent: None,
        submodule: false,
    });
    c.scope.insert(1);
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.message = Some("app draft".into());
    let checks = p.checks.clone();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::Char('i'));
    p.event(&Event::Paste(" updated".into()), &c);
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::F(2));
    c.scope = [1].into();
    p.refresh(&c).unwrap();
    assert!(!p.editing());
    assert_eq!(p.saved.message.as_deref(), Some("app draft updated"));
    key(&mut p, &c, K::Char('i'));
    p.event(&Event::Paste(" and kept".into()), &c);
    key(&mut p, &c, K::Esc);
    let mut reopened = CommitPanel::default();
    reopened.refresh(&c).unwrap();
    assert_eq!(
        reopened.saved.message.as_deref().unwrap(),
        "app draft updated and kept"
    );
    assert_eq!(p.checks, checks);
}

#[test]
fn inline_message_save_failure_keeps_editing_and_blocks_commit() {
    let (_dir, mut c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.message = Some("Original".into());
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::Char('i'));
    p.event(&Event::Paste(" unsaved".into()), &c);
    draw(&mut p, &mut t, &c);
    c.state_dir = c.workspace.join("blocked-state");
    fs::write(&c.state_dir, "a file cannot hold state").unwrap();
    let button = hit(&p, |h| {
        matches!(h, Hit::MessageAction(MessageAction::Commit))
    });
    let action = mouse(
        &mut p,
        &c,
        M::Down(MouseButton::Left),
        button.x + 1,
        button.y,
    );
    assert!(matches!(action, Action::Notice(NoticeLevel::Error, _)));
    assert!(p.message_editing());
    assert!(git(&c.repos[0].path, &["rev-parse", "--verify", "HEAD"]).is_err());
    assert_eq!(p.buf, "Original unsaved");
    key(&mut p, &c, K::Esc);
    assert!(p.message_editing());
    assert_eq!(p.saved.message.as_deref().unwrap(), "Original");
    c.state_dir = c.workspace.join("state");
    key(&mut p, &c, K::Esc);
    assert!(!p.message_editing());
    assert_eq!(p.focused_pane(), PaneId::Message);
    assert_eq!(p.saved.message.as_deref().unwrap(), "Original unsaved");
}
