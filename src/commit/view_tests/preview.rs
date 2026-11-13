use super::*;

fn commit_base(c: &Context) {
    git(
        &c.repos[0].path,
        &[
            "-c",
            "user.name=Preview tester",
            "-c",
            "user.email=preview@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-qm",
            "base",
        ],
    )
    .unwrap();
}

fn select_file(p: &mut CommitPanel, path: &str) {
    p.cursor = p
        .view
        .iter()
        .position(|row| {
            row.file()
                .is_some_and(|i| p.files[i].change.path == Path::new(path))
        })
        .unwrap();
}

fn copied(p: &mut CommitPanel, c: &Context) -> String {
    match p.event(&Event::Key(KeyEvent::new(K::Char('y'), Mod::NONE)), c) {
        Action::Copy { text, .. } => text,
        _ => panic!("expected source revision copy"),
    }
}

#[test]
fn escape_keeps_changes_and_message_focus_without_hiding_preview() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    p.saved.message = Some("Keep the shared draft".into());
    let checks = p.checks.clone();
    let cursor = p.cursor;
    for width in [44, 80, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
        for preview_open in [false, true] {
            p.preview_open = preview_open;
            for pane in [PaneId::Changes, PaneId::Message] {
                p.focus_pane(pane);
                draw(&mut p, &mut terminal, &c);
                key(&mut p, &c, K::Esc);
                draw(&mut p, &mut terminal, &c);
                assert_eq!(p.focused_pane(), pane);
                assert_eq!(p.preview_open, preview_open);
                assert_eq!(p.cursor, cursor);
                assert_eq!(p.checks, checks);
                assert_eq!(p.saved.message.as_deref(), Some("Keep the shared draft"));
            }
        }
    }
}

#[test]
fn file_preview_shares_history_split_rendering_and_preserves_commit_candidates() {
    let (_dir, c) = fixture();
    commit_base(&c);
    let repo = &c.repos[0].path;
    let selected = "src/ｕｉ/model/page.rs";
    let sibling = "src/ｕｉ/view/page.rs";
    let staged = "fn model() {\n\tprintln!(\"staged 中文\");\n}\n";
    let working = "later unstaged edits\n";
    fs::write(repo.join(selected), staged).unwrap();
    fs::write(repo.join(sibling), "unchecked staged sibling\n").unwrap();
    git(repo, &["add", "--", "src"]).unwrap();
    fs::write(repo.join(selected), working).unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let mut p = CommitPanel {
            icons: mode,
            ..Default::default()
        };
        p.refresh(&c).unwrap();
        let other = p
            .files
            .iter()
            .find(|f| f.change.path == Path::new(sibling))
            .unwrap();
        p.checks.insert(other.key.clone(), false);
        p.saved.message = Some("Keep the shared draft".into());
        let checks = p.checks.clone();
        select_file(&mut p, selected);
        for width in [180, 120] {
            let mut t = Terminal::new(TestBackend::new(width, 36)).unwrap();
            draw(&mut p, &mut t, &c);
            let text = read(t.backend().buffer(), p.diff);
            assert!(text.contains("Before") && text.contains("After"), "{text}");
            assert!(
                text.contains("Index") && !text.contains(working.trim()),
                "{text}"
            );
            assert!(text.contains("Unfold") && text.contains("Zoom"), "{text}");
            assert!(p.diff_comparison.is_some());
        }
        p.focus_pane(PaneId::Preview);
        assert_eq!(copied(&mut p, &c), staged);
        key(&mut p, &c, K::Char('['));
        assert_eq!(copied(&mut p, &c), "model body");
        key(&mut p, &c, K::Char(']'));
        assert_eq!(copied(&mut p, &c), staged);
        key(&mut p, &c, K::Tab);
        assert_eq!(p.focused_pane(), PaneId::Message);
        assert_eq!(p.saved.message.as_deref(), Some("Keep the shared draft"));
        assert_eq!(p.checks, checks);
    }
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(fs::read_to_string(repo.join(selected)).unwrap(), working);
    assert_eq!(
        git(repo, &["show", &format!(":{sibling}")]).unwrap(),
        "unchecked staged sibling"
    );
}

#[test]
fn preview_requires_gg_for_start_and_uppercase_g_for_end() {
    let (_dir, c) = fixture();
    commit_base(&c);
    let repo = &c.repos[0].path;
    let file = "src/long-preview.txt";
    let body = (0..80)
        .map(|line| format!("line {line:02}\n"))
        .collect::<String>();
    let path = repo.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &body).unwrap();
    git(repo, &["add", "--", file]).unwrap();

    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    select_file(&mut p, file);
    p.launch(Hit::Show(Focus::Diff), &c);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    draw(&mut p, &mut terminal, &c);
    assert!(p.comparison_visible());

    key(&mut p, &c, K::Char('G'));
    draw(&mut p, &mut terminal, &c);
    let bottom = read(terminal.backend().buffer(), p.diff);
    assert!(
        bottom.contains("line 79") && !bottom.contains("line 00"),
        "{bottom}"
    );

    key(&mut p, &c, K::Char('g'));
    assert_eq!(p.key_sequence(), Some(KeySequence::FirstItem));
    draw(&mut p, &mut terminal, &c);
    let waiting = read(terminal.backend().buffer(), p.diff);
    assert!(
        waiting.contains("line 79") && !waiting.contains("line 00"),
        "{waiting}"
    );

    key(&mut p, &c, K::Char('g'));
    assert!(p.key_sequence().is_none());
    draw(&mut p, &mut terminal, &c);
    let top = read(terminal.backend().buffer(), p.diff);
    assert!(top.contains("line 00") && !top.contains("line 79"), "{top}");
}

#[test]
fn narrow_file_preview_switches_revisions_and_container_overview_stays_available() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let checks = p.checks.clone();
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    let original = p.cursor;
    p.launch(Hit::Show(Focus::Diff), &c);
    let mut t = Terminal::new(TestBackend::new(44, 12)).unwrap();
    draw(&mut p, &mut t, &c);
    let text = read(t.backend().buffer(), p.diff);
    assert!(
        text.contains("After") && text.contains("model body"),
        "{text}"
    );
    assert!(!text.contains("Before"), "{text}");
    key(&mut p, &c, K::Char('['));
    draw(&mut p, &mut t, &c);
    let before = read(t.backend().buffer(), p.diff);
    assert!(
        before.contains("Before") && !before.contains("After"),
        "{before}"
    );
    key(&mut p, &c, K::Char(']'));
    assert_eq!(copied(&mut p, &c), "model body");
    key(&mut p, &c, K::Esc);
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty() && p.cursor == original);
    key(&mut p, &c, K::Home);
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    p.launch(Hit::Show(Focus::Diff), &c);
    draw(&mut p, &mut t, &c);
    assert!(read(t.backend().buffer(), Rect::new(0, 0, 120, 36)).contains("Overview"));
    assert!(p.diff_comparison.is_none());
    assert_eq!(p.checks, checks);
}

#[test]
fn unsupported_file_previews_keep_the_explanation_and_selectable_patch() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("binary.dat"), b"binary\0content").unwrap();
    git(repo, &["add", "--", "binary.dat"]).unwrap();
    fs::write(repo.join("large.txt"), "large\n".repeat(20_000)).unwrap();
    fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(repo.join("ignored.txt"), "ignored content\n").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let mut p = CommitPanel {
        options: display::Options {
            ignored: true,
            ..Default::default()
        },
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    p.focus_pane(PaneId::Preview);
    let mut t = Terminal::new(TestBackend::new(180, 36)).unwrap();
    for (path, explanation, patch) in [
        ("binary.dat", "Binary or non-UTF-8", "Binary files"),
        ("large.txt", "96 KiB", "Preview truncated at 96 KiB"),
        ("ignored.txt", "Ignored by Git", "Excluded from commits"),
    ] {
        select_file(&mut p, path);
        draw(&mut p, &mut t, &c);
        assert!(p.diff_comparison.is_none(), "{path}");
        assert!(p.diff_text.contains(explanation), "{}", p.diff_text);
        assert!(p.diff_text.contains(patch), "missing patch for {path}");
        assert!(
            read(t.backend().buffer(), p.diff).contains(explanation),
            "missing visible explanation for {path}"
        );
        p.event(&Event::Key(KeyEvent::new(K::Char('a'), Mod::CONTROL)), &c);
        assert!(p.selected_text().unwrap().ends_with(&p.diff_text));
    }
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(repo.join("binary.dat")).unwrap(),
        b"binary\0content"
    );
}

#[test]
fn running_commit_allows_revision_controls_and_keeps_message_and_write_guards() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    p.focus_pane(PaneId::Preview);
    p.saved.message = Some("Keep the running batch message".into());
    let mut t = Terminal::new(TestBackend::new(180, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    let index = fs::read(c.repos[0].path.join(".git/index")).unwrap();
    let checks = p.checks.clone();
    let finish = p.hold_execution();
    for code in ['f', 'z', 'n', 'p', '[', ']'] {
        assert!(matches!(
            p.event(&Event::Key(KeyEvent::new(K::Char(code), Mod::NONE)), &c),
            Action::None
        ));
    }
    assert_eq!(copied(&mut p, &c), "model body");
    for code in ['c', 'i', 'd', 'P', 'M'] {
        let action = p.event(&Event::Key(KeyEvent::new(K::Char(code), Mod::NONE)), &c);
        assert!(
            matches!(action, Action::Notice(NoticeLevel::Warning, message) if message.contains("Commit running")),
            "{code} passed a running commit guard"
        );
        assert!(p.edit.is_none());
    }
    assert_eq!(
        p.saved.message.as_deref(),
        Some("Keep the running batch message")
    );
    assert_eq!(p.checks, checks);
    assert_eq!(fs::read(c.repos[0].path.join(".git/index")).unwrap(), index);
    finish();
}

#[test]
fn revision_search_survives_source_refresh_overview_and_patch_fallback() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("binary.dat"), b"binary\0content").unwrap();
    git(repo, &["add", "--", "binary.dat"]).unwrap();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let checks = p.checks.clone();
    let index = fs::read(repo.join(".git/index")).unwrap();
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    p.focus_pane(PaneId::Preview);
    let mut t = Terminal::new(TestBackend::new(180, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    key(&mut p, &c, K::Char('/'));
    p.event(&Event::Paste("body".into()), &c);
    key(&mut p, &c, K::Enter);
    let prompt = |p: &CommitPanel| {
        let mut t = Terminal::new(TestBackend::new(80, 1)).unwrap();
        t.draw(|f| p.focused_search().unwrap().draw(f, f.area()))
            .unwrap();
        read(t.backend().buffer(), Rect::new(0, 0, 80, 1))
    };
    assert!(prompt(&p).contains("body") && prompt(&p).contains("1/1"));
    select_file(&mut p, "src/ｕｉ/view/page.rs");
    draw(&mut p, &mut t, &c);
    assert!(prompt(&p).contains("body") && !prompt(&p).contains("1/1"));
    key(&mut p, &c, K::Char('n'));
    assert!(prompt(&p).contains("1/1"));
    p.refresh(&c).unwrap();
    draw(&mut p, &mut t, &c);
    assert!(prompt(&p).contains("body") && !prompt(&p).contains("1/1"));
    key(&mut p, &c, K::Char('n'));
    assert!(prompt(&p).contains("1/1"));
    select_file(&mut p, "binary.dat");
    draw(&mut p, &mut t, &c);
    assert!(p.diff_comparison.is_none() && prompt(&p).contains("body"));
    p.cursor = 0;
    draw(&mut p, &mut t, &c);
    assert!(p.focused_search().is_none());
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    draw(&mut p, &mut t, &c);
    assert!(prompt(&p).contains("body") && !prompt(&p).contains("1/1"));
    key(&mut p, &c, K::Char('n'));
    assert!(prompt(&p).contains("1/1"));
    assert_eq!(p.checks, checks);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn dragging_a_file_through_revision_preview_keeps_the_changelist_gesture() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.saved.groups.push("Later".into());
    p.sync(&c);
    select_file(&mut p, "src/ｕｉ/model/page.rs");
    let file = p.active().unwrap().file().unwrap();
    let file_key = p.files[file].key.clone();
    let checks = p.checks.clone();
    let index = fs::read(c.repos[0].path.join(".git/index")).unwrap();
    let mut t = Terminal::new(TestBackend::new(180, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    let row = p
        .hits
        .iter()
        .find_map(|(area, hit)| {
            matches!(hit, Hit::Row(i) if p.view[*i].file() == Some(file)).then_some(*area)
        })
        .unwrap();
    let (_, checkbox) = p.row_controls(p.active().unwrap(), row);
    click(&mut p, &c, checkbox.right() + 3, row.y);
    assert!(matches!(p.dragged, Some((i, false)) if i == file));
    mouse(&mut p, &c, M::Drag(MouseButton::Left), 150, 8);
    assert!(matches!(p.dragged, Some((i, true)) if i == file));
    assert_eq!(p.focused_pane(), PaneId::Changes);
    let later = row_area(&p, "Later");
    mouse(&mut p, &c, M::Up(MouseButton::Left), later.x + 5, later.y);
    assert!(p.dragged.is_none());
    assert_eq!(
        p.saved.membership.get(&file_key).map(String::as_str),
        Some("Later")
    );
    assert_eq!(p.checks, checks);
    assert_eq!(fs::read(c.repos[0].path.join(".git/index")).unwrap(), index);
}

#[test]
fn tree_motion_loads_only_the_preview_for_the_drawn_row() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
    let files: Vec<_> = p
        .view
        .iter()
        .enumerate()
        .filter_map(|(i, row)| row.file().map(|_| i))
        .collect();
    p.cursor = files[0];
    draw(&mut p, &mut t, &c);
    let initial_preview = p.diff_key.clone();
    let checks = p.checks.clone();

    for _ in files[0]..files[1] {
        key(&mut p, &c, K::Char('j'));
        assert_eq!(p.diff_key, initial_preview);
    }
    assert_eq!(p.cursor, files[1]);
    assert_eq!(p.checks, checks);

    draw(&mut p, &mut t, &c);
    assert_eq!(p.diff_key, p.view[files[1]].id);
    assert!(p.comparison_visible());
}
