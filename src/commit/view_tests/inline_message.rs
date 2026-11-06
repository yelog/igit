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
        p.saved
            .drafts
            .insert(rid(&c, 0), "Subject ＵＩ\nBody".into());
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
        assert!(p.hits.iter().any(|(_, h)| matches!(h, Hit::EditCancel)));
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
        assert_eq!(p.saved.drafts[&rid(&c, 0)], "Inline subject\n\nBody ＵＩ");
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
    p.saved
        .drafts
        .insert(rid(&c, 0), "Subject ＵＩ\nBody".into());
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
    let cancel = hit(&p, |h| matches!(h, Hit::EditCancel));
    click(&mut p, &c, cancel.x + 1, cancel.y);
    assert!(!p.editing());
    assert_eq!(p.saved.drafts[&rid(&c, 0)], "Subject ＵＩ\nBody");
}

#[test]
fn inline_message_repo_switch_saves_only_the_previous_repository() {
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
    p.saved.drafts.insert(rid(&c, 0), "app draft".into());
    p.saved.drafts.insert(rid(&c, 1), "api draft".into());
    let checks = p.checks.clone();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::Char('i'));
    p.event(&Event::Paste(" updated".into()), &c);
    draw(&mut p, &mut t, &c);
    let next = hit(&p, |h| matches!(h, Hit::Repo(1)));
    click(&mut p, &c, next.x + 1, next.y);
    assert_eq!(p.repo, 1);
    assert!(!p.editing());
    assert_eq!(p.saved.drafts[&rid(&c, 0)], "app draft updated");
    assert_eq!(p.saved.drafts[&rid(&c, 1)], "api draft");
    key(&mut p, &c, K::Char('i'));
    p.event(&Event::Paste(" discarded".into()), &c);
    key(&mut p, &c, K::Esc);
    let mut reopened = CommitPanel::default();
    reopened.refresh(&c).unwrap();
    assert_eq!(reopened.saved.drafts[&rid(&c, 0)], "app draft updated");
    assert_eq!(reopened.saved.drafts[&rid(&c, 1)], "api draft");
    assert_eq!(p.checks, checks);
}

#[test]
fn inline_message_save_failure_keeps_editing_and_blocks_review() {
    let (_dir, mut c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.drafts.insert(rid(&c, 0), "Original".into());
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
    assert_eq!(p.saved.drafts[&rid(&c, 0)], "Original");
}
