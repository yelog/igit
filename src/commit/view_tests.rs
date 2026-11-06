use super::*;
mod inline_message;
mod text_selection;
use crate::core::{git, Repo};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn fixture() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("app");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    for (path, body) in [
        ("src/ｕｉ/model/page.rs", "model body"),
        ("src/ｕｉ/view/page.rs", "view body"),
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
    mouse(p, c, M::Down(MouseButton::Left), x, y);
}
fn mouse(p: &mut CommitPanel, c: &Context, kind: M, x: u16, y: u16) -> Action {
    p.event(
        &Event::Mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: Mod::NONE,
        }),
        c,
    )
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
fn resizing_both_commit_dividers_reveals_message_body_and_preserves_review() {
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let message = "Subject\n\nDetails one\nDetails two\nDetails three";
    p.saved.message = Some(message.into());
    let index = git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap();
    let checks = p.checks.clone();
    let cursor = p.cursor;
    let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
    draw(&mut p, &mut t, &c);
    let old_message = p.panes.rect(PaneId::Message).unwrap();
    assert!(!read(t.backend().buffer(), old_message).contains("Details three"));
    for (side, step) in [(1, 2), (0, 3)] {
        let area = p.panes.rect(PaneId::Message).unwrap();
        let (x, y) = (area.x + 4, area.y - side);
        click(&mut p, &c, x, y);
        mouse(&mut p, &c, M::Drag(MouseButton::Left), x, y - step);
        draw(&mut p, &mut t, &c);
        mouse(&mut p, &c, M::Up(MouseButton::Left), x, y - step);
    }
    let expanded = p.panes.rect(PaneId::Message).unwrap();
    assert_eq!(expanded.height, old_message.height + 5);
    assert!(read(t.backend().buffer(), expanded).contains("Details three"));

    let preview = p.panes.rect(PaneId::Preview).unwrap();
    for side in [1, 0] {
        let area = p.panes.rect(PaneId::Preview).unwrap();
        let (x, y) = (area.x - side, area.y + 2);
        click(&mut p, &c, x, y);
        mouse(&mut p, &c, M::Drag(MouseButton::Left), x - 4, y);
        mouse(&mut p, &c, M::Up(MouseButton::Left), x - 4, y);
        draw(&mut p, &mut t, &c);
    }
    assert_eq!(
        p.panes.rect(PaneId::Preview).unwrap().width,
        preview.width + 8
    );
    assert_eq!(p.cursor, cursor);
    assert_eq!(p.checks, checks);
    assert!(p.edit.is_none() && p.dragged.is_none() && !p.panes.dragging());
    assert_eq!(p.focused_pane(), PaneId::Changes);
    let draft = p
        .hits
        .iter()
        .find(|(_, h)| matches!(h, Hit::Msg))
        .unwrap()
        .0;
    click(&mut p, &c, draft.x, draft.bottom() - 1);
    assert!(matches!(p.edit, Some(Edit::Message)));
    assert_eq!(p.buf, message);
    key(&mut p, &c, K::Esc);
    draw(&mut p, &mut t, &c);
    let review = p
        .hits
        .iter()
        .find(|(_, h)| matches!(h, Hit::Submit(false)))
        .unwrap()
        .0;
    let action = mouse(
        &mut p,
        &c,
        M::Down(MouseButton::Left),
        review.x + 1,
        review.y,
    );
    assert!(matches!(action, Action::Confirm { body, .. } if body.contains("Details three")));
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        index
    );
}

#[test]
fn compact_commit_keeps_the_only_tree_row_clickable_and_frames_the_visible_focus() {
    use crate::ui::{ACCENT, LINE};
    let (_dir, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    // Four body rows are available in the top half of a 44×12 workspace.
    let mut t = Terminal::new(TestBackend::new(44, 4)).unwrap();
    draw(&mut p, &mut t, &c);
    let row = row_area(&p, "Changes");
    assert_eq!(p.tree.height, 1);
    let (_, check) = p.row_controls(p.active().unwrap(), row);
    click(&mut p, &c, check.x, check.y);
    assert!(p.selection(&c).is_empty());
    let tree = p.panes.rect(PaneId::Changes).unwrap();
    let msg = p.panes.rect(PaneId::Message).unwrap();
    click(&mut p, &c, msg.x, msg.y); // The frame focuses the draft without editing it.
    draw(&mut p, &mut t, &c);
    assert_eq!(p.focused_pane(), PaneId::Message);
    assert!(p.edit.is_none());
    let border = |t: &Terminal<TestBackend>, r: Rect| t.backend().buffer()[(r.x, r.y)].fg;
    assert_eq!(border(&t, tree), LINE);
    assert_eq!(border(&t, msg), ACCENT);
    p.panes.active = false;
    draw(&mut p, &mut t, &c);
    assert_eq!(border(&t, msg), LINE);

    p.panes.active = true;
    key(&mut p, &c, K::Char('v'));
    draw(&mut p, &mut t, &c);
    assert_eq!(
        p.panes.nodes(),
        &[
            (PaneId::Changes, Rect::default()),
            (PaneId::Preview, t.backend().buffer().area),
            (PaneId::Message, Rect::default())
        ]
    );
    p.focus_pane(PaneId::Message);
    draw(&mut p, &mut t, &c);
    assert_eq!(p.panes.rect(PaneId::Preview), Some(Rect::default()));
    assert_eq!(border(&t, msg), ACCENT);
    assert!(p.selection(&c).is_empty());

    // At these heights only the list has a bottom edge (drawn or compact).
    // The first message row must remain editable across its entire draft.
    for h in [7, 10] {
        let mut t = Terminal::new(TestBackend::new(44, h)).unwrap();
        draw(&mut p, &mut t, &c);
        let draft = p
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, Hit::Msg))
            .unwrap()
            .0;
        assert_eq!(draft.y, p.panes.rect(PaneId::Message).unwrap().y);
        click(&mut p, &c, draft.x, draft.y);
        assert!(matches!(p.edit, Some(Edit::Message)));
        assert!(!p.panes.dragging());
        key(&mut p, &c, K::Esc);
    }
}

#[test]
fn view_options_menu_keeps_checks_and_blocks_underlying_actions_in_all_icon_modes() {
    use super::display::Item;
    use crate::ui::idea::{CHECK, DIM, SELECT};
    use ratatui::style::Modifier;

    let (_d, c) = fixture();
    let item_area = |p: &CommitPanel, item| {
        p.hits
            .iter()
            .find_map(|(r, h)| matches!(h, Hit::ViewItem(i) if *i == item).then_some(*r))
            .unwrap()
    };
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let mut p = CommitPanel {
                icons: mode,
                ..Default::default()
            };
            p.refresh(&c).unwrap();
            let checks = p.checks.clone();
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            draw(&mut p, &mut t, &c);
            let eye = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::ViewOptions))
                .unwrap()
                .0;
            assert_eq!(
                read(t.backend().buffer(), eye).trim(),
                mode.glyph(Icon::Preview)
            );
            click(&mut p, &c, eye.x + 1, eye.y);
            draw(&mut p, &mut t, &c);
            assert!(p.editing());
            let menu = p.view_menu.as_ref().unwrap().area;
            assert!(menu.right() <= width && menu.bottom() <= height);
            let buffer = t.backend().buffer();
            let text = read(buffer, menu);
            for label in ["Group By", "Show", "Ignored Files", "Git Status"] {
                assert!(text.contains(label), "{width}x{height}: {text}");
            }
            let directory = item_area(&p, Item::Directory);
            let module = item_area(&p, Item::Module);
            for (row, label, checked, color) in [
                (directory, "Directory", true, CHECK),
                (module, "Module", false, DIM),
            ] {
                assert!(read(buffer, row).contains(&format!("{} {label}", mode.check(checked))));
                let mark = &buffer[(row.x + 1, row.y)];
                assert_eq!(mark.fg, color);
                assert_eq!(mark.modifier.contains(Modifier::BOLD), checked);
            }
            assert_eq!(buffer[(directory.x + 1, directory.y)].bg, SELECT);
            // The menu border sits over tree rows and must not browse or toggle them.
            let cursor = p.cursor;
            click(&mut p, &c, menu.x, menu.y + 1);
            key(&mut p, &c, K::Char('c'));
            assert_eq!(p.cursor, cursor);
            assert!(p.plans.is_empty() && p.edit.is_none());
            key(&mut p, &c, K::Char(' '));
            assert!(!p.view.iter().any(|r| matches!(r.kind, Kind::Directory(_))));
            draw(&mut p, &mut t, &c);
            let module = item_area(&p, Item::Module);
            assert!(p.hover_at(module.x + 2, module.y));
            key(&mut p, &c, K::Enter);
            assert!(p.view.iter().any(|r| matches!(r.kind, Kind::Repo(_))));
            draw(&mut p, &mut t, &c);
            key(&mut p, &c, K::Esc);
            assert!(!p.editing());
            assert_eq!(p.checks, checks);
        }
    }
}

#[test]
fn showing_ignored_files_preserves_checks_undo_and_git_contents() {
    use super::display::Item;
    let (_d, c) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join(".gitignore"), "cache/\n*.log\n").unwrap();
    fs::create_dir(repo.join("cache")).unwrap();
    fs::write(repo.join("cache/private.txt"), "never commit this marker").unwrap();
    fs::write(repo.join("app.log"), "never commit this marker").unwrap();
    let before = git(repo, &["ls-files", "--stage"]).unwrap();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.toggle(0);
    let checks = p.checks.clone();
    let undo = p.last_check.clone();
    p.toggle_view_item(Item::Ignored, &c).unwrap();
    assert_eq!(p.checks, checks);
    assert_eq!(p.last_check, undo);
    let i = p.view.iter().position(|r| r.label == IGNORED).unwrap();
    assert_eq!(p.check_counts(&p.view[i]), (0, 0));
    for &f in &p.view[i].files {
        p.checks.insert(p.files[f].key.clone(), true); // A stale historical check is still excluded.
    }
    p.toggle(i);
    assert!(p.selection(&c).is_empty());
    p.cursor = i;
    key(&mut p, &c, K::Char('m'));
    assert!(p.edit.is_none());
    p.toggle_view_item(Item::Ignored, &c).unwrap();
    assert!(!p.view.iter().any(|r| r.label == IGNORED));
    key(&mut p, &c, K::Char('u'));
    assert_eq!(p.selection(&c).len(), 2);
    assert_eq!(git(repo, &["ls-files", "--stage"]).unwrap(), before);
    assert_eq!(
        fs::read_to_string(repo.join("cache/private.txt")).unwrap(),
        "never commit this marker"
    );
}

#[test]
fn regrouped_commit_keeps_the_selected_index_version_and_unselected_staged_file() {
    use super::display::Item;
    let (_d, c) = fixture();
    let repo = &c.repos[0].path;
    git(repo, &["config", "user.name", "Fixture"]).unwrap();
    git(repo, &["config", "user.email", "fixture@invalid"]).unwrap();
    git(repo, &["config", "core.hooksPath", "/dev/null"]).unwrap();
    git(repo, &["config", "commit.gpgSign", "false"]).unwrap();
    git(
        repo,
        &[
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
    let selected = "src/ｕｉ/model/page.rs";
    let unselected = "src/ｕｉ/view/page.rs";
    fs::write(repo.join(selected), "selected index version\n").unwrap();
    fs::write(repo.join(unselected), "unselected index version\n").unwrap();
    git(repo, &["add", "--", selected, unselected]).unwrap();
    fs::write(repo.join(selected), "selected working version\n").unwrap();
    fs::write(repo.join(".gitignore"), "cache/\n").unwrap();
    fs::create_dir(repo.join("cache")).unwrap();
    fs::write(repo.join("cache/private.txt"), "ignored marker\n").unwrap();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let unselected_row = p
        .view
        .iter()
        .position(|r| {
            r.file()
                .is_some_and(|i| p.files[i].change.path == Path::new(unselected))
        })
        .unwrap();
    p.toggle(unselected_row);
    for item in [Item::Directory, Item::Module, Item::Ignored, Item::Status] {
        p.toggle_view_item(item, &c).unwrap();
    }
    p.saved.message = Some("selected".into());
    let Action::Confirm { body, .. } = p.review(&c, false).unwrap() else {
        panic!("expected review");
    };
    assert!(body.contains("selected index version"));
    assert!(
        !body.contains("selected working version")
            && !body.contains("unselected index version")
            && !body.contains("ignored marker")
    );
    assert!(matches!(p.confirm("commit", &c), Action::Committed { repos, .. } if repos == [0]));
    assert_eq!(
        git(repo, &["show", &format!("HEAD:{selected}")]).unwrap(),
        "selected index version"
    );
    assert_eq!(
        git(repo, &["show", &format!(":{unselected}")]).unwrap(),
        "unselected index version"
    );
    assert_eq!(
        fs::read_to_string(repo.join(selected)).unwrap(),
        "selected working version\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("cache/private.txt")).unwrap(),
        "ignored marker\n"
    );
    assert_eq!(
        git(
            repo,
            &[
                "-c",
                "core.quotepath=false",
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                "HEAD"
            ]
        )
        .unwrap(),
        selected
    );
}

#[test]
fn directory_preview_uses_the_repo_root_when_launched_in_a_subdirectory() {
    let (_d, mut c) = fixture();
    let directory = Path::new("src").join("ｕｉ");
    c.workspace = c.repos[0].path.join(&directory);
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    p.cursor = p
        .view
        .iter()
        .position(|r| matches!(&r.kind, Kind::Directory(path) if path == &directory))
        .unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    draw(&mut p, &mut t, &c);
    let preview: String = read(t.backend().buffer(), p.diff)
        .split_whitespace()
        .collect();
    assert!(preview.contains(&format!(
        "Workspace{}",
        c.repos[0].path.join(&directory).display()
    )));
    assert_eq!(p.selection(&c).len(), 2);
}

#[test]
fn compacted_directories_keep_their_children_selection_and_hit_targets() {
    let (_d, c) = fixture();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    p.refresh(&c).unwrap();
    draw(&mut p, &mut t, &c);
    let directory = Path::new("app")
        .join("src")
        .join("ｕｉ")
        .display()
        .to_string();
    let a = row_area(&p, &directory);
    // Clicking indentation or the name only browses; the disclosure and checkbox are separate.
    click(&mut p, &c, a.x, a.y);
    draw(&mut p, &mut t, &c);
    assert_eq!(p.active().unwrap().label, directory);
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
    assert_eq!(p.active().unwrap().label, directory);
    assert_eq!(p.check_counts(p.active().unwrap()), (1, 2));
    draw(&mut p, &mut t, &c);
    let a = row_area(&p, &directory);
    let (arrow, _) = p.row_controls(p.active().unwrap(), a);
    click(&mut p, &c, arrow.x, arrow.y);
    assert!(!p.view.iter().any(|r| r.label == "model"));
    p.refresh(&c).unwrap();
    assert_eq!(p.active().unwrap().label, directory);
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
    let file = format!("{}/subdir/tail.txt", "ｌｏｎｇ".repeat(10));
    let path = c.repos[0].path.join(&file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "only in this draft").unwrap();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    p.refresh(&c).unwrap();
    key(&mut p, &c, K::Char('G'));
    // Unversioned files are visible on first entry, even on a narrow terminal.
    assert_eq!(p.active().unwrap().label, "tail.txt");
    let cursor = p.cursor;
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty());
    key(&mut p, &c, K::Enter);
    draw(&mut p, &mut t, &c);
    let preview = read(t.backend().buffer(), p.diff);
    assert!(
        preview.contains("tail.txt") && preview.contains("+only in this draft"),
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
fn flat_layout_and_mouse_preview_resolve_identical_filenames_to_the_right_patch() {
    let (_d, c) = fixture();
    fs::write(c.repos[0].path.join("notes.txt"), "private draft").unwrap();
    let mut p = CommitPanel::default();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    p.refresh(&c).unwrap();
    key(&mut p, &c, K::Char('v')); // Hide the default preview to exercise double-click opening.
    key(&mut p, &c, K::Char('t'));
    draw(&mut p, &mut t, &c);
    assert!(p.diff.is_empty() && p.diff_key.is_empty());
    let list = read(t.backend().buffer(), p.tree);
    assert!(list.contains("page.rs app/src/ｕｉ/model"));
    assert!(list.contains("page.rs app/src/ｕｉ/view"));
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
fn shared_draft_uses_the_full_width_without_repository_names() {
    let (_d, mut c) = fixture();
    c.repos[0].name = "a-very-long-repository-name".into();
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let mut p = CommitPanel {
            icons: mode,
            ..Default::default()
        };
        p.refresh(&c).unwrap();
        p.saved.message = Some("Shared subject".into());
        for width in [16, 32, 80, 120] {
            let mut t = Terminal::new(TestBackend::new(width, 24)).unwrap();
            draw(&mut p, &mut t, &c);
            let composer = p.panes.rect(PaneId::Message).unwrap();
            let message = p
                .hits
                .iter()
                .find(|(_, hit)| matches!(hit, Hit::Msg))
                .unwrap()
                .0;
            assert!(message.width >= composer.width.saturating_sub(2));
            let text = read(t.backend().buffer(), composer);
            assert!(text.contains("Shared subject"), "{text}");
            assert!(!text.contains(&c.repos[0].name));
        }
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
    panel.ai_settings.endpoint = "http://127.0.0.1:1/mock".into();
    panel.ai_settings.model = "test".into();
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
    let [ai::Patch { content: patch, .. }] = panel.ai_request.as_ref().unwrap().patches.as_slice()
    else {
        panic!("expected one repository's patch");
    };
    assert!(patch.contains("-tracked contents"));
    assert!(!patch.contains("private.txt") && !patch.contains("unchecked child marker"));
}
