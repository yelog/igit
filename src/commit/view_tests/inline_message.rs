use super::*;

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
        assert!(p.hits.iter().any(|(_, h)| matches!(h, Hit::Submit(false))));
        assert!(!p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::EditSave | Hit::EditCancel)));
        assert_eq!(p.checks, checks);
    }
}

#[test]
fn inline_message_commit_buttons_save_current_input_before_review() {
    let (_dir, c) = fixture();
    let before = git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap();
    for push in [false, true] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        let checks = p.checks.clone();
        let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
        draw(&mut p, &mut t, &c);
        key(&mut p, &c, K::Char('i'));
        p.event(&Event::Key(KeyEvent::new(K::Char('a'), Mod::CONTROL)), &c);
        p.event(&Event::Paste("Inline subject\n\nBody ＵＩ".into()), &c);
        draw(&mut p, &mut t, &c);
        let button = hit(&p, |h| matches!(h, Hit::Submit(value) if *value == push));
        let action = mouse(
            &mut p,
            &c,
            M::Down(MouseButton::Left),
            button.x + 1,
            button.y,
        );
        assert!(
            matches!(action, Action::Confirm { body, .. } if body.contains("Inline subject") && body.contains("Body ＵＩ"))
        );
        assert!(p.edit.is_none());
        assert_eq!(
            p.saved.message.as_deref().unwrap(),
            "Inline subject\n\nBody ＵＩ"
        );
        assert_eq!(p.checks, checks);
    }
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        before
    );
}

#[test]
fn inline_message_first_drag_copies_and_inline_cancel_restores_saved_draft() {
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
    assert_eq!(p.saved.message.as_deref().unwrap(), "Subject ＵＩ\nBody");
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
                assert!(matches!(actions[0].1, Hit::Submit(false)));
                assert!(matches!(actions[1].1, Hit::Submit(true)));
                assert!(matches!(actions[2].1, Hit::Ai));
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
    p.event(&Event::Paste(" discarded".into()), &c);
    key(&mut p, &c, K::Esc);
    let mut reopened = CommitPanel::default();
    reopened.refresh(&c).unwrap();
    assert_eq!(
        reopened.saved.message.as_deref().unwrap(),
        "app draft updated"
    );
    assert_eq!(p.checks, checks);
}

#[test]
fn inline_message_save_failure_keeps_editing_and_blocks_review() {
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
    let button = hit(&p, |h| matches!(h, Hit::Submit(false)));
    let action = mouse(
        &mut p,
        &c,
        M::Down(MouseButton::Left),
        button.x + 1,
        button.y,
    );
    assert!(matches!(action, Action::Notice(_)));
    assert!(p.message_editing() && p.plans.is_empty());
    assert_eq!(p.buf, "Original unsaved");
    key(&mut p, &c, K::Esc);
    assert_eq!(p.saved.message.as_deref().unwrap(), "Original");
}
