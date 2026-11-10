use super::*;
use crossterm::event::KeyEvent;

fn press(panel: &mut PushPanel, context: &Context, ch: char) -> Action {
    panel.event(
        &Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
        context,
    )
}

fn fixture() -> (PushPanel, Context) {
    let context = Context {
        workspace: "/workspace".into(),
        state_dir: "/state".into(),
        scope: [0].into(),
        repos: vec![Repo {
            path: "/missing-copy-test-repository".into(),
            name: "repo".into(),
            branch: "main".into(),
            submodule: false,
            parent: None,
        }],
    };
    let panel = PushPanel {
        entries: vec![Entry {
            idx: 0,
            logs: vec![Out {
                id: "0123456789abcdef".repeat(2),
                msg: "subject".into(),
            }],
            ..Default::default()
        }],
        expanded: [0].into(),
        pos: 1,
        ..Default::default()
    };
    (panel, context)
}

#[test]
fn push_commit_copy_is_limited_to_commit_rows_and_reports_read_failures() {
    let (mut panel, context) = fixture();
    for (focus, pos) in [(0, 0), (1, 1), (2, 1), (0, 99)] {
        panel.focus = focus;
        panel.pos = pos;
        for suffix in ['y', 'm', 'a'] {
            assert!(matches!(press(&mut panel, &context, 'y'), Action::None));
            assert!(panel.key_sequence().is_none());
            assert!(matches!(press(&mut panel, &context, suffix), Action::None));
        }
    }
    panel.focus = 0;
    panel.pos = 1;
    for suffix in ['m', 'a'] {
        press(&mut panel, &context, 'y');
        assert!(
            matches!(press(&mut panel, &context, suffix), Action::Notice(NoticeLevel::Error, message) if message.starts_with("Push: "))
        );
        assert!(panel.key_sequence().is_none());
    }
    press(&mut panel, &context, 'y');
    assert!(matches!(
        press(&mut panel, &context, 'y'),
        Action::Copy { .. }
    ));
}

#[test]
fn push_commit_copy_cancels_on_context_changes_and_reprocesses_invalid_keys() {
    use crossterm::event::MouseEvent;

    for interrupt in [
        Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        Event::Resize(80, 24),
        Event::FocusLost,
        Event::Paste("y".into()),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }),
    ] {
        let (mut panel, context) = fixture();
        press(&mut panel, &context, 'y');
        assert!(panel.key_sequence().is_some());
        panel.event(&interrupt, &context);
        assert!(panel.key_sequence().is_none(), "{interrupt:?}");
        assert!(!matches!(
            press(&mut panel, &context, 'a'),
            Action::Copy { .. }
        ));
    }
    let (mut panel, context) = fixture();
    press(&mut panel, &context, 'y');
    press(&mut panel, &context, 'k');
    assert_eq!(panel.pos, 0);
    assert!(!panel.copy_pending);
    panel.pos = 1;
    press(&mut panel, &context, 'y');
    panel.focus_pane(PaneId::PushFiles);
    assert!(!panel.copy_pending);
    panel.focus_pane(PaneId::Outgoing);
    press(&mut panel, &context, 'y');
    panel.refresh(&context).unwrap();
    assert!(!panel.copy_pending);
}

#[test]
fn push_commit_copy_uses_selected_repository_without_preview() {
    let workspace = tempfile::tempdir().unwrap();
    let mut expected = Vec::new();
    for name in ["one", "two"] {
        let path = workspace.path().join(name);
        std::fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]).unwrap();
        git(&path, &["config", "user.name", &format!("作者 {name}")]).unwrap();
        git(&path, &["config", "user.email", &format!("{name}@invalid")]).unwrap();
        git(&path, &["config", "commit.gpgSign", "false"]).unwrap();
        git(&path, &["config", "core.hooksPath", "/dev/null"]).unwrap();
        let message = format!("Subject {name}\n\n正文 $HOME `literal`\n  indented body");
        std::fs::write(path.join("file"), "committed\n").unwrap();
        git(&path, &["add", "file"]).unwrap();
        git(&path, &["commit", "-qm", &message]).unwrap();
        let oid = git(&path, &["rev-parse", "HEAD"]).unwrap();
        expected.push((oid, message, format!("作者 {name} <{name}@invalid>")));
        git(&path, &["config", "user.name", "Another author"]).unwrap();
        git(&path, &["commit", "--allow-empty", "-qm", "Newer commit"]).unwrap();
        std::fs::write(path.join("file"), "staged\n").unwrap();
        git(&path, &["add", "file"]).unwrap();
        std::fs::write(path.join("file"), "working tree\n").unwrap();
    }
    let context = Context::discover(workspace.path()).unwrap();
    let mut panel = PushPanel {
        entries: expected
            .iter()
            .enumerate()
            .map(|(idx, (oid, _, _))| Entry {
                idx,
                n: 1,
                logs: vec![Out {
                    id: oid.clone(),
                    msg: "Only the subject is cached".into(),
                }],
                ..Default::default()
            })
            .collect(),
        expanded: [0, 1].into(),
        checked: [0].into(),
        ..Default::default()
    };
    panel.preview.summary = "Stale preview from another commit".into();
    panel.preview_load.request();
    for (index, (oid, message, author)) in expected.iter().enumerate() {
        panel.pos = index * 2 + 1;
        let repo = &context.repos[index].path;
        let before = (
            git(repo, &["rev-parse", "HEAD"]).unwrap(),
            git(repo, &["ls-files", "--stage"]).unwrap(),
            git(repo, &["status", "--porcelain"]).unwrap(),
        );
        for (suffix, value) in [('y', oid), ('m', message), ('a', author)] {
            assert!(matches!(press(&mut panel, &context, 'y'), Action::None));
            match press(&mut panel, &context, suffix) {
                Action::Copy { text, .. } => assert_eq!(&text, value),
                _ => panic!("y{suffix} did not copy the selected commit"),
            }
            assert_eq!(panel.pos, index * 2 + 1);
            assert_eq!(panel.checked, [0].into());
            assert_eq!(panel.focus, 0);
        }
        assert_eq!(
            before,
            (
                git(repo, &["rev-parse", "HEAD"]).unwrap(),
                git(repo, &["ls-files", "--stage"]).unwrap(),
                git(repo, &["status", "--porcelain"]).unwrap(),
            )
        );
        assert_eq!(std::fs::read(repo.join("file")).unwrap(), b"working tree\n");
    }
}
