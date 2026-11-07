use super::*;
use crate::log::tests::{commit, finish_preview, git, init};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::fs;

fn key(p: &mut LogPanel, c: &Context, code: K) -> Action {
    p.event(&Event::Key(KeyEvent::new(code, Mods::NONE)), c)
}

fn render(p: &mut LogPanel, c: &Context, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            p.draw_content(f, Rect::new(0, 1, width, height.saturating_sub(1)), c);
            p.draw_tabs(f, Rect::new(0, 0, width, 1), c);
            p.draw_overlays(f, Rect::new(0, 1, width, height.saturating_sub(1)), c);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x = x.saturating_add(cells(symbol).max(1));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn copy_revision(p: &mut LogPanel, c: &Context) -> String {
    p.focus = Pane::Info;
    match key(p, c, K::Char('y')) {
        Action::Copy { text, .. } => text,
        _ => panic!("expected a revision copy action"),
    }
}

#[test]
fn file_history_tabs_keep_literal_file_scope_and_original_log_selection() {
    let tmp = init();
    let file = "[x] 文件 name.js";
    commit(tmp.path(), file, "const original = 1;\n");
    let original = git(tmp.path(), &["rev-parse", "HEAD"]);
    commit(tmp.path(), "x 文件 name.js", "unrelated\n");
    let head = git(tmp.path(), &["rev-parse", "HEAD"]);
    let c = Context::discover(tmp.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    p.set_file_history(&c, &tmp.path().join(file)).unwrap();
    assert_eq!(p.views.len(), 2);
    assert!(p.hist.revs.is_empty()); // Startup setup does no history read.
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    assert_eq!(p.hist.revs.len(), 1);
    assert_eq!(p.rev().unwrap().oid, original);
    assert_eq!(copy_revision(&mut p, &c), "const original = 1;\n");
    let screen = render(&mut p, &c, 220, 32);
    assert!(screen.contains("History: [x] 文件 name.js"), "{screen}");
    assert!(screen.contains("Branch: HEAD"));
    assert!(screen.contains("Before") && screen.contains("After"));
    assert!(!screen.contains("Repository Diff:"));
    assert!(!p.editing());
    p.switch_tab(0, &c).unwrap();
    assert!(p.filter.file_history.is_none());
    assert_eq!(p.rev().unwrap().oid, head);
    p.switch_tab(1, &c).unwrap();
    assert_eq!(p.rev().unwrap().oid, original);
    p.click(Hit::Reset, &c).unwrap();
    assert!(p.filter.file_history.is_some());
    assert_eq!(p.hist.revs.len(), 1);
    p.close_tab(&c).unwrap();
    assert_eq!(p.views.len(), 1);
    assert!(p.filter.file_history.is_none());
}

#[test]
fn merge_file_history_renders_three_revisions_and_updates_on_commit_click() {
    let (_tmp, path, rev) = model::merge_diff::tests::fixture();
    fs::write(path.join("example.js"), "staged unrelated\n").unwrap();
    git(&path, &["add", "example.js"]);
    fs::write(path.join("example.js"), "working unrelated\n").unwrap();
    let index = fs::read(path.join(".git/index")).unwrap();
    let c = Context::discover(&path).unwrap();
    let mut p = LogPanel::default();
    p.set_file_history(&c, &path.join("example.js")).unwrap();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    assert_eq!(p.rev().unwrap().oid, rev.oid);
    assert_eq!(p.rev().unwrap().parents, rev.parents);
    assert_eq!(p.hist.revs.iter().filter(|r| r.oid == rev.oid).count(), 1);
    assert!(p
        .history_diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap()
        .is_three_way());
    let screen = render(&mut p, &c, 260, 34);
    for value in [
        "History: example.js",
        "Parent 1",
        "Parent 2",
        "Result",
        "No conflicts",
        "resolved from both",
    ] {
        assert!(screen.contains(value), "Missing {value}:\n{screen}");
    }
    assert!(!screen.contains("README.md"));
    assert!(!screen.contains("1/1 files"));
    assert_eq!(p.panes.nodes().len(), 2);
    assert_eq!(copy_revision(&mut p, &c), "resolved from both\n");
    key(&mut p, &c, K::Char('['));
    assert_eq!(copy_revision(&mut p, &c), "left\n");
    key(&mut p, &c, K::Esc);
    assert_eq!(p.focus, Pane::Graph);
    let area = p
        .hits
        .iter()
        .find_map(|(rect, hit)| matches!(hit, Hit::Commit(1)).then_some(*rect))
        .unwrap();
    key(&mut p, &c, K::Tab);
    assert_eq!(p.focus, Pane::Info);
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: Mods::NONE,
        }),
        &c,
    );
    assert_eq!(p.focus, Pane::Graph);
    assert_eq!(p.cursor, 1);
    assert!(p.history_diff.is_none());
    finish_preview(&mut p, &c);
    assert!(!p
        .history_diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap()
        .is_three_way());
    for (width, height) in [(160, 32), (120, 24), (80, 16), (44, 12), (12, 4), (1, 1)] {
        render(&mut p, &c, width, height);
    }
    assert_eq!(index, fs::read(path.join(".git/index")).unwrap());
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), rev.oid);
    assert_eq!(
        fs::read_to_string(path.join("example.js")).unwrap(),
        "working unrelated\n"
    );
}

#[test]
fn renamed_and_deleted_files_preview_each_historical_path_and_actual_parent() {
    let tmp = init();
    commit(tmp.path(), "old.js", "const kept = 1;\n");
    let first = git(tmp.path(), &["rev-parse", "HEAD"]);
    git(tmp.path(), &["mv", "old.js", "new 名.js"]);
    git(tmp.path(), &["commit", "-qm", "rename"]);
    let renamed = git(tmp.path(), &["rev-parse", "HEAD"]);
    commit(tmp.path(), "new 名.js", "const kept = 2;\n");
    git(tmp.path(), &["rm", "new 名.js"]);
    git(tmp.path(), &["commit", "-qm", "delete"]);
    let c = Context::discover(tmp.path()).unwrap();
    let mut p = LogPanel::default();
    p.set_file_history(&c, &tmp.path().join("new 名.js"))
        .unwrap();
    p.refresh(&c).unwrap();
    assert_eq!(p.hist.revs.len(), 4);
    finish_preview(&mut p, &c);
    assert_eq!(copy_revision(&mut p, &c), "");
    key(&mut p, &c, K::Char('['));
    assert_eq!(copy_revision(&mut p, &c), "const kept = 2;\n");
    let row = p
        .visible
        .iter()
        .position(|&i| p.hist.revs[i].oid == renamed)
        .unwrap();
    p.select_row(row);
    finish_preview(&mut p, &c);
    assert_eq!(
        p.rev().unwrap().parents.as_slice(),
        std::slice::from_ref(&first)
    );
    assert_eq!(copy_revision(&mut p, &c), "const kept = 1;\n");
    key(&mut p, &c, K::Char('['));
    assert_eq!(copy_revision(&mut p, &c), "const kept = 1;\n");
    p.select_row(3);
    finish_preview(&mut p, &c);
    assert_eq!(p.rev().unwrap().oid, first);
    assert_eq!(p.rev().unwrap().file.as_ref().unwrap().raw, b"old.js");
    assert_eq!(copy_revision(&mut p, &c), "const kept = 1;\n");
    key(&mut p, &c, K::Char('['));
    assert_eq!(copy_revision(&mut p, &c), "");
}

#[test]
fn history_scope_never_includes_another_repository_or_reenables_hidden_root() {
    let outer = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        let repo = outer.path().join(name);
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-qb", "main"]);
        git(&repo, &["config", "user.name", "Fixture"]);
        git(&repo, &["config", "user.email", "fixture@invalid"]);
        commit(&repo, "same.js", name);
    }
    let mut c = Context::discover(outer.path()).unwrap();
    let mut p = LogPanel::default();
    p.set_file_history(&c, &outer.path().join("two/same.js"))
        .unwrap();
    let target = p.filter.file_history.as_ref().unwrap().root;
    let other = c
        .scope
        .iter()
        .copied()
        .find(|root| *root != target)
        .unwrap();
    c.repos[other].path = outer.path().join("unavailable-other-repository");
    p.refresh(&c).unwrap();
    assert_eq!(p.hist.revs.len(), 1);
    assert!(p.hist.errors.is_empty(), "{:?}", p.hist.errors);
    assert_eq!(p.rev().unwrap().root, target);
    assert_eq!(p.rev().unwrap().subject, "two");
    c.scope.remove(&target);
    p.refresh(&c).unwrap();
    assert!(p.hist.revs.is_empty());
    assert!(p.history_diff.is_none());
    assert!(!c.scope.contains(&target));
    assert!(p
        .set_file_history(&c, &outer.path().join("outside.js"))
        .is_err());
}

#[test]
fn empty_history_and_binary_preview_remain_read_only_and_closeable() {
    let tmp = init();
    let c = Context::discover(tmp.path()).unwrap();
    let mut p = LogPanel::default();
    p.set_file_history(&c, &tmp.path().join("missing/deleted.js"))
        .unwrap();
    p.refresh(&c).unwrap();
    assert!(p.visible.is_empty());
    assert!(render(&mut p, &c, 120, 24).contains("History: deleted.js"));
    p.close_tab(&c).unwrap();
    fs::write(tmp.path().join("binary"), [0, 1, 2]).unwrap();
    git(tmp.path(), &["add", "binary"]);
    git(tmp.path(), &["commit", "-qm", "binary"]);
    p.set_file_history(&c, &tmp.path().join("binary")).unwrap();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    assert!(p.history_diff.as_ref().unwrap().comparison.is_none());
    assert!(render(&mut p, &c, 200, 24).contains("Binary or non-UTF-8"));
    p.close_tab(&c).unwrap();
    assert!(p.filter.file_history.is_none());
}
