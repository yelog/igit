use super::*;
use crate::core::{git, Repo};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn fixture() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("app");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    for (path, body) in [
        ("src/界面/model/page.rs", "model body"),
        ("src/界面/view/page.rs", "view body"),
    ] {
        let file = repo.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, body).unwrap();
    }
    git(&repo, &["add", "--", "src"]).unwrap();
    let c = Context {
        workspace: dir.path().into(),
        state_dir: dir.path().join("state"),
        scope: [0].into(),
        repos: vec![Repo {
            path: repo,
            name: "app".into(),
            branch: "main".into(),
            parent: None,
            submodule: false,
        }],
    };
    (dir, c)
}
fn draw(p: &mut CommitPanel, t: &mut Terminal<TestBackend>, c: &Context) {
    t.draw(|f| p.draw(f, f.area(), c)).unwrap();
}
fn read(buf: &Buffer, r: Rect) -> String {
    (r.y..r.bottom())
        .map(|y| {
            let mut line = String::new();
            let mut x = r.x;
            while x < r.right() {
                let symbol = buf[(x, y)].symbol();
                line.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn key(p: &mut CommitPanel, c: &Context, k: K) {
    p.event(&Event::Key(KeyEvent::new(k, Mod::NONE)), c);
}
fn click(p: &mut CommitPanel, c: &Context, x: u16, y: u16) {
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: Mod::NONE,
        }),
        c,
    );
}
fn row_area(p: &CommitPanel, name: &str) -> Rect {
    p.hits
        .iter()
        .find_map(|(a, h)| match h {
            Hit::Row(i) if p.view[*i].label == name => Some(*a),
            _ => None,
        })
        .unwrap()
}

#[test]
fn compacted_directories_keep_their_children_selection_and_hit_targets() {
    let (_d, c) = fixture();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    p.refresh(&c).unwrap();
    key(&mut p, &c, K::Char('t'));
    key(&mut p, &c, K::Char('v'));
    draw(&mut p, &mut t, &c);
    let a = row_area(&p, "src/界面");
    // Clicking indentation or the name only browses; the disclosure and checkbox are separate.
    click(&mut p, &c, a.x, a.y);
    draw(&mut p, &mut t, &c);
    assert_eq!(p.active().unwrap().label, "src/界面");
    assert!(p.view.iter().any(|r| r.label == "model"));
    assert!(p.view.iter().any(|r| r.label == "view"));
    let overview = read(t.backend().buffer(), p.diff);
    assert!(overview.contains("2 files · 2 selected") && overview.contains("view/page.rs"));
    assert!(!overview.contains("view body"));
    let a = row_area(&p, "view");
    let view = p.view.iter().find(|r| r.label == "view").unwrap();
    let (_, check) = p.row_controls(view, a);
    click(&mut p, &c, check.x, check.y);
    assert_eq!(p.selection(&c).len(), 1);
    assert_eq!(p.active().unwrap().label, "src/界面");
    assert_eq!(p.check_counts(p.active().unwrap()), (1, 2));
    draw(&mut p, &mut t, &c);
    let a = row_area(&p, "src/界面");
    let (arrow, _) = p.row_controls(p.active().unwrap(), a);
    click(&mut p, &c, arrow.x, arrow.y);
    assert!(!p.view.iter().any(|r| r.label == "model"));
    p.refresh(&c).unwrap();
    assert_eq!(p.active().unwrap().label, "src/界面");
    assert_eq!(p.check_counts(p.active().unwrap()), (1, 2));
    assert_eq!(
        git(&c.repos[0].path, &["diff", "--cached", "--numstat"])
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn narrow_preview_wraps_the_actual_path_and_never_checks_a_browsed_file() {
    let (_d, c) = fixture();
    let file = format!("{}/子目录/尾部.txt", "很长的目录".repeat(10));
    let path = c.repos[0].path.join(&file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "only in this draft").unwrap();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    p.refresh(&c).unwrap();
    key(&mut p, &c, K::Char('G'));
    // Unversioned files are visible on first entry, even on a narrow terminal.
    assert_eq!(p.active().unwrap().label, "尾部.txt");
    let cursor = p.cursor;
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty());
    key(&mut p, &c, K::Enter);
    draw(&mut p, &mut t, &c);
    let preview = read(t.backend().buffer(), p.diff);
    assert!(
        preview.contains("尾部.txt") && preview.contains("+only in this draft"),
        "{preview}"
    );
    assert!(p.tree.is_empty());
    key(&mut p, &c, K::F(6)); // Focus the message strip; only one pane fits here.
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty());
    key(&mut p, &c, K::Char('v'));
    draw(&mut p, &mut t, &c);
    assert!(p.tree.is_empty() && !p.diff.is_empty() && p.cursor == cursor);
    let back = p
        .hits
        .iter()
        .find(|(_, h)| matches!(h, Hit::Show(Focus::Tree)))
        .unwrap()
        .0;
    click(&mut p, &c, back.x + 1, back.y);
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty() && p.cursor == cursor);
    assert_eq!(p.check_counts(p.active().unwrap()), (0, 1));
}

#[test]
fn flat_default_and_mouse_preview_resolve_identical_filenames_to_the_right_patch() {
    let (_d, c) = fixture();
    fs::write(c.repos[0].path.join("notes.txt"), "private draft").unwrap();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    p.refresh(&c).unwrap();
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty() && p.diff_key.is_empty());
    let list = read(t.backend().buffer(), p.tree);
    assert!(list.contains("page.rs app/src/界面/model"));
    assert!(list.contains("page.rs app/src/界面/view"));
    assert!(list.contains("Unversioned Files") && list.contains("notes.txt app"));
    assert_eq!(p.selection(&c).len(), 2);
    let target = p
        .hits
        .iter()
        .find_map(|(r, h)| {
            let Hit::Row(i) = h else {
                return None;
            };
            let f = &p.files[p.view[*i].file()?];
            f.change.path.ends_with("view/page.rs").then_some(*r)
        })
        .unwrap();
    click(&mut p, &c, target.x + 22, target.y);
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty());
    click(&mut p, &c, target.x + 22, target.y);
    draw(&mut p, &mut t, &c);
    let diff = read(t.backend().buffer(), p.diff);
    assert!(diff.contains("view/page.rs") && diff.contains("+view body"));
    assert!(!diff.contains("model body") && !p.tree.is_empty());
    key(&mut p, &c, K::Esc);
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty() && p.tree.width == target.width);
    assert_eq!(p.selection(&c).len(), 2);
    let note = row_area(&p, "notes.txt");
    let untracked = p.view.iter().find(|r| r.label == "notes.txt").unwrap();
    let (_, check) = p.row_controls(untracked, note);
    click(&mut p, &c, check.right() - 1, check.y);
    assert_eq!(p.selection(&c).len(), 3);
    // Undo selection is a local checkbox action. Neither the cursor nor Git's index should move.
    key(&mut p, &c, K::Char('u'));
    assert_eq!(p.active().unwrap().label, "page.rs");
    assert_eq!(p.selection(&c).len(), 2);
    assert!(git(&c.repos[0].path, &["ls-files", "--", "notes.txt"])
        .unwrap()
        .is_empty());
}

#[test]
fn ascii_checkbox_responds_on_both_brackets_and_disappears_when_it_cannot_fit() {
    let (_d, c) = fixture();
    let mut p = CommitPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    draw(&mut p, &mut t, &c);
    let a = row_area(&p, "Changes");
    let line = read(t.backend().buffer(), a);
    let x = a.x + line.find("[x]").expect("ASCII checkbox") as u16;
    click(&mut p, &c, x + 2, a.y);
    assert!(p.selection(&c).is_empty());
    draw(&mut p, &mut t, &c);
    assert!(read(t.backend().buffer(), a).contains("[ ]"));
    click(&mut p, &c, x, a.y);
    assert_eq!(p.selection(&c).len(), 2);

    // The margin and disclosure leave only two cells of the three-cell checkbox.
    let mut t = Terminal::new(TestBackend::new(6, 24)).unwrap();
    draw(&mut p, &mut t, &c);
    let a = row_area(&p, "Changes");
    assert_eq!(read(t.backend().buffer(), a), "v   ");
    click(&mut p, &c, a.right() - 1, a.y);
    assert_eq!(p.selection(&c).len(), 2);
}

#[test]
fn clipped_draft_names_do_not_overlap_repo_navigation() {
    let (_d, mut c) = fixture();
    c.repos[0].name = "很长的仓库名称".into();
    c.repos.push(Repo {
        name: "long-submodule-name".into(),
        submodule: true,
        ..c.repos[0].clone()
    });
    c.scope.insert(1);
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let mut p = CommitPanel {
            icons: mode,
            ..Default::default()
        };
        let mut t = Terminal::new(TestBackend::new(32, 16)).unwrap();
        draw(&mut p, &mut t, &c);
        let tabs: Vec<_> = p
            .hits
            .iter()
            .filter_map(|(r, h)| match h {
                Hit::Repo(_) => Some(*r),
                _ => None,
            })
            .collect();
        assert_eq!(tabs.len(), 3);
        assert!(tabs.windows(2).all(|pair| pair[0].right() < pair[1].x));
        let next = tabs[2];
        assert_eq!(
            read(t.backend().buffer(), next).trim(),
            mode.glyph(Icon::Next)
        );
        click(&mut p, &c, next.x + 1, next.y);
        assert_eq!(p.repo, 1);

        let mut t = Terminal::new(TestBackend::new(16, 16)).unwrap();
        draw(&mut p, &mut t, &c);
        let tabs: Vec<_> = p
            .hits
            .iter()
            .filter_map(|(_, h)| match h {
                Hit::Repo(r) => Some(*r),
                _ => None,
            })
            .collect();
        assert_eq!(tabs, [1]); // Only the current name fits; no invisible arrow can switch roots.
    }
}

#[test]
fn group_toggle_ignores_conflicted_files_even_with_an_old_check() {
    let (_d, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.files[1].change.conflict = true; // An already checked path became conflicted.
    assert_eq!(p.check_counts(p.active().unwrap()), (1, 1));
    p.toggle(p.cursor);
    assert!(p.selection(&c).is_empty());
    p.toggle(p.cursor);
    assert_eq!(p.selection(&c).len(), 1);
    p.files[0].change.conflict = true;
    p.toggle(p.cursor);
    assert_eq!(p.check_counts(p.active().unwrap()), (0, 0));
    assert!(p.selection(&c).is_empty());
}

#[test]
fn ai_review_of_a_replaced_file_never_includes_the_unchecked_child() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let node = repo.join("node");
    fs::write(&node, "tracked contents\n").unwrap();
    git(repo, &["add", "--", "node"]).unwrap();
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    fs::remove_file(&node).unwrap();
    fs::create_dir(&node).unwrap();
    fs::write(node.join("private.txt"), "unchecked child marker\n").unwrap();

    let mut panel = CommitPanel::default();
    panel.refresh(&c).unwrap();
    panel.saved.endpoint = "http://127.0.0.1:1/mock".into();
    panel.saved.model = "test".into();
    assert_eq!(panel.selection(&c).len(), 1);
    let Action::Confirm { body, token, .. } =
        panel.event(&Event::Key(KeyEvent::new(K::Char('A'), Mod::NONE)), &c)
    else {
        panic!("expected the AI review for the selected deletion");
    };
    assert_eq!(token, "commit-ai");
    assert!(body.lines().any(|line| line.trim() == "node"));
    assert!(!body.contains("private.txt"));
    // Inspect the patch queued for the model without sending anything to a service.
    let [(_, patch)] = panel.ai_payload.as_slice() else {
        panic!("expected one repository's patch");
    };
    assert!(patch.contains("-tracked contents"));
    assert!(!patch.contains("private.txt") && !patch.contains("unchecked child marker"));
}
