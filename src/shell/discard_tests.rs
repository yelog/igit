use super::{test_support::*, *};
use crate::{config::keys::parse_sequence, core::git};
use crossterm::event::MouseEvent;
use unicode_width::UnicodeWidthStr;

fn send(app: &mut Shell, keys: &str) {
    for key in keys.split_whitespace() {
        for k in parse_sequence(key).unwrap() {
            app.dispatch(&Event::Key(k));
        }
    }
}

fn click(app: &mut Shell, column: u16, row: u16) {
    app.dispatch(&Event::Mouse(MouseEvent {
        column,
        row,
        kind: MouseEventKind::Down(MouseButton::Left),
        modifiers: KeyModifiers::NONE,
    }));
}

fn click_label(app: &mut Shell, label: &str) {
    let rendered = screen(app, 80, 24);
    let (row, line, index) = rendered
        .lines()
        .enumerate()
        .filter_map(|(row, line)| line.find(label).map(|i| (row, line, i)))
        .last()
        .unwrap_or_else(|| panic!("missing {label:?}\n{rendered}"));
    click(app, line[..index].width() as u16, row as u16);
}

fn focus_commit_file(app: &mut Shell, name: &str) {
    send(app, "1 g");
    let rendered = screen(app, 120, 36);
    let tree = app
        .commit
        .panes
        .rect(crate::ui::panes::PaneId::Changes)
        .unwrap();
    let (row, col) = rendered
        .lines()
        .enumerate()
        .skip(usize::from(tree.y))
        .take(usize::from(tree.height))
        .find_map(|(row, line)| {
            line.find(name)
                .map(|i| (row as u16, line[..i].width() as u16))
        })
        .unwrap_or_else(|| panic!("missing {name:?} in Commit\n{rendered}"));
    click(app, col, row);
}

#[test]
fn discard_changes_group_opens_rollback_dialog() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    send(&mut app, "g d");
    let rendered = screen(&mut app, 80, 24);
    assert!(rendered.contains("Rollback Changes"), "{rendered}");
    assert!(rendered.contains("2 modified"), "{rendered}");
    assert!(
        rendered.contains("Delete local copies of added files"),
        "{rendered}"
    );
    send(&mut app, "Esc");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn discard_confirmation_defaults_to_cancel_and_refreshes_only_after_confirmation() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    focus_commit_file(&mut app, "mixed.txt");
    send(&mut app, "Space");
    for cancel in ["Esc", "Enter", "n"] {
        send(&mut app, "d");
        assert!(matches!(&app.pop, Some(Popup::Rollback(_))), "{}", app.msg);
        for (width, height) in [(120, 36), (80, 24), (44, 12)] {
            let rendered = screen(&mut app, width, height);
            for label in [
                "Rollback Changes",
                "mixed.txt",
                "Delete local copies of added files",
                " Close ",
                " Rollback ",
            ] {
                assert!(
                    rendered.contains(label),
                    "{width}x{height} missing {label}\n{rendered}"
                );
            }
        }
        send(&mut app, cancel);
        assert!(app.pop.is_none());
        assert_eq!(snapshot(&repo), before);
    }
    send(&mut app, "d y");
    assert!(app.pop.is_none());
    assert_eq!(git(&repo, &["show", ":mixed.txt"]).unwrap(), "base");
    assert_eq!(std::fs::read(repo.join("mixed.txt")).unwrap(), b"base\n");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), before[0]);
    assert!(app.msg.contains("rolled back 1 file"), "{}", app.msg);
    // Refresh removed the discarded target from the live list.
    focus_commit_file(&mut app, "other.txt");
    send(&mut app, "d");
    assert!(matches!(&app.pop, Some(Popup::Rollback(_))));
    let rendered = screen(&mut app, 80, 24);
    assert!(
        rendered.contains("other.txt") && !rendered.contains("mixed.txt"),
        "{rendered}"
    );
}

#[test]
fn discard_file_prechecks_only_that_file_in_the_full_tree_and_can_include_a_sibling() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    focus_commit_file(&mut app, "mixed.txt");
    send(&mut app, "d");
    let rendered = screen(&mut app, 80, 24);
    let row = |label: &str| rendered.lines().rfind(|line| line.contains(label)).unwrap();
    assert!(row("mixed.txt").contains("[x]"), "{rendered}");
    assert!(row("other.txt").contains("[ ]"), "{rendered}");
    assert!(
        row("Changes 2 files").contains(app.icons.glyph(Icon::Partial)),
        "{rendered}"
    );
    assert!(rendered.contains("1 modified"), "{rendered}");

    click_label(&mut app, "other.txt");
    send(&mut app, "Space");
    assert!(screen(&mut app, 80, 24).contains("2 modified"));
    click_label(&mut app, "mixed.txt");
    send(&mut app, "Space");
    assert!(screen(&mut app, 80, 24).contains("1 modified"));
    assert_eq!(snapshot(&repo), before);
    let capture = clipboard::CopyCapture::start();
    send(&mut app, "y r");
    assert_eq!(capture.texts(), ["mixed.txt"]);
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "Shift-Tab Enter");

    assert_eq!(git(&repo, &["show", ":other.txt"]).unwrap(), "base");
    assert_eq!(std::fs::read(repo.join("other.txt")).unwrap(), b"base\n");
    assert_eq!(git(&repo, &["show", ":mixed.txt"]).unwrap(), "staged");
    assert_eq!(
        std::fs::read(repo.join("mixed.txt")).unwrap(),
        b"working tree\n"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), before[0]);
}

#[test]
fn discard_newly_checked_sibling_uses_its_original_snapshot() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    std::fs::write(repo.join("other.txt"), "reviewed worktree\n").unwrap();
    send(&mut app, "F5");
    focus_commit_file(&mut app, "mixed.txt");
    send(&mut app, "d");
    std::fs::write(repo.join("other.txt"), "changed outside dialog\n").unwrap();
    let before = snapshot(&repo);
    click_label(&mut app, "other.txt");
    send(&mut app, "Space Shift-Tab Enter");
    assert!(app.msg.contains("other.txt changed"), "{}", app.msg);
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn discard_remapping_and_help_open_review_without_writing() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.config = AppConfig::from_toml("[keybindings.commit]\ndiscard=['Ctrl-r']").unwrap();
    focus_commit_file(&mut app, "mixed.txt");
    send(&mut app, "d");
    assert!(app.pop.is_none());
    send(&mut app, "Ctrl-r");
    assert!(matches!(&app.pop, Some(Popup::Rollback(_))));
    send(&mut app, "Esc F1");
    app.dispatch(&Event::Paste(
        "Review rollback for the highlighted scope".into(),
    ));
    assert!(screen(&mut app, 120, 36).contains("Ctrl-r"));
    send(&mut app, "Enter");
    assert!(matches!(&app.pop, Some(Popup::Rollback(_))));
    send(&mut app, "Esc");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn discard_group_mouse_checkboxes_and_help_keep_the_unchecked_index_and_worktree() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    send(&mut app, "g d z");
    let collapsed = screen(&mut app, 80, 24);
    assert!(
        collapsed.contains("2 modified") && !collapsed.contains("other.txt"),
        "{collapsed}"
    );
    send(&mut app, "Space y");
    assert!(matches!(&app.pop, Some(Popup::Rollback(_))));
    assert!(screen(&mut app, 80, 24).contains("No files selected"));
    click_label(&mut app, " Rollback ");
    assert!(matches!(&app.pop, Some(Popup::Rollback(_))));
    assert_eq!(snapshot(&repo), before);

    send(&mut app, "Space Z");
    click_label(&mut app, "other.txt");
    let rendered = screen(&mut app, 80, 24);
    assert!(rendered.contains("2 modified"), "{rendered}");
    let (row, line) = rendered
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("other.txt"))
        .last()
        .unwrap();
    let checkbox_end = line[..line.find("other.txt").unwrap()]
        .rfind("[x]")
        .unwrap()
        + 2;
    click(&mut app, line[..checkbox_end].width() as u16, row as u16);
    assert!(screen(&mut app, 80, 24).contains("1 modified"));
    send(&mut app, "o j Space Esc F1");
    assert_eq!(
        app.pop.as_ref().and_then(|p| match p {
            Popup::Help { state, .. } => Some(state.content.context()),
            _ => None,
        }),
        Some("Rollback / Changes")
    );
    send(&mut app, "Esc");
    assert!(screen(&mut app, 80, 24).contains("1 modified"));
    assert_eq!(snapshot(&repo), before);
    // Revalidation uses the checked snapshot; a newer unchecked edit stays.
    std::fs::write(repo.join("other.txt"), "external edit\n").unwrap();
    click_label(&mut app, " Rollback ");
    assert!(app.pop.is_none(), "{}", app.msg);
    assert_eq!(git(&repo, &["show", ":mixed.txt"]).unwrap(), "base");
    assert_eq!(std::fs::read(repo.join("mixed.txt")).unwrap(), b"base\n");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(
        std::fs::read(repo.join("other.txt")).unwrap(),
        b"external edit\n"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), before[0]);
}

#[test]
fn discard_added_copy_is_kept_unless_the_option_is_selected() {
    for delete_copy in [false, true] {
        let (_dir, mut app) = fixture();
        let repo = app.ctx.repos[0].path.clone();
        let new = repo.join("new.txt");
        std::fs::write(&new, "staged copy\n").unwrap();
        git(&repo, &["add", "new.txt"]).unwrap();
        std::fs::write(&new, "local copy\n").unwrap();
        send(&mut app, "F5");
        focus_commit_file(&mut app, "new.txt");
        send(&mut app, "d");
        let rendered = screen(&mut app, 80, 24);
        assert!(
            rendered.contains("1 added") && rendered.contains("[ ] Delete local copies"),
            "{rendered}"
        );
        if delete_copy {
            click_label(&mut app, "Delete local copies of added files");
            assert!(screen(&mut app, 80, 24).contains("[x] Delete local copies"));
        }
        click_label(&mut app, " Rollback ");
        assert!(app.pop.is_none(), "{}", app.msg);
        assert_eq!(git(&repo, &["ls-files", "--", "new.txt"]).unwrap(), "");
        assert_eq!(
            git(&repo, &["show", ":other.txt"]).unwrap(),
            "unselected staged"
        );
        if delete_copy {
            assert!(!new.exists());
        } else {
            assert_eq!(std::fs::read(new).unwrap(), b"local copy\n");
        }
    }
}
