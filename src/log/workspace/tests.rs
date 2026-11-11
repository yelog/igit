use super::*;
use crate::log::tests::{commit, git, init};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn snapshot(repo: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                walk(root, &path, files);
            } else if metadata.is_symlink() {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read_link(&path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                );
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(repo, repo, &mut files);
    files
}

fn key(panel: &mut LogPanel, code: K, context: &Context) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, Mods::NONE)), context)
}

fn finish(panel: &mut LogPanel, context: &Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while panel.preview_wakeup().is_some() {
        panel.poll_preview(context).unwrap();
        assert!(
            Instant::now() < deadline,
            "workspace comparison did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn source(diff: &mut DiffView, side: usize) -> String {
    let view = diff.comparison.as_mut().expect("text comparison");
    while view.side() != side {
        view.event(&Event::Key(KeyEvent::new(K::Tab, Mods::NONE)));
    }
    match view.event(&Event::Key(KeyEvent::new(K::Char('y'), Mods::NONE))) {
        crate::merge::RevisionEvent::Copy(text) => text,
        _ => panic!("copy revision"),
    }
}

fn render(panel: &mut LogPanel, context: &Context, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), context))
        .unwrap();
    (0..height)
        .map(|row| {
            (0..width)
                .map(|column| terminal.backend().buffer()[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn workspace_compare_includes_committed_staged_unstaged_and_untracked_changes_without_writes() {
    let temp = init();
    let repo = temp.path();
    for name in [
        "mixed",
        "staged",
        "deleted",
        "recreated",
        "unchanged",
        "committed",
        "index-only",
    ] {
        fs::write(repo.join(name), "base\n").unwrap();
    }
    fs::write(repo.join(".gitignore"), "*.ignored\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "base"]);
    let oid = git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "committed", "committed later");
    for name in ["mixed", "staged", "index-only"] {
        fs::write(repo.join(name), "staged\n").unwrap();
        git(repo, &["add", name]);
    }
    fs::write(repo.join("mixed"), "working\n").unwrap();
    fs::write(repo.join("index-only"), "base\n").unwrap();
    fs::remove_file(repo.join("deleted")).unwrap();
    git(repo, &["rm", "-q", "recreated"]);
    fs::write(repo.join("recreated"), "base\n").unwrap();
    fs::write(repo.join("新文件 [x].txt"), "untracked\n").unwrap();
    fs::write(repo.join("skip.ignored"), "ignored\n").unwrap();
    fs::write(repo.join("new-then-deleted"), "staged\n").unwrap();
    git(repo, &["add", "new-then-deleted"]);
    fs::remove_file(repo.join("new-then-deleted")).unwrap();
    let before = snapshot(repo);
    let files = backend::files(repo, &oid).unwrap();
    let paths: Vec<_> = files.iter().map(|file| model::plain(&file.raw)).collect();
    assert_eq!(
        paths,
        ["committed", "deleted", "mixed", "staged", "新文件 [x].txt"]
    );
    for (name, expected_before, expected_after) in [
        ("committed", "base\n", "committed later"),
        ("mixed", "base\n", "working\n"),
        ("staged", "base\n", "staged\n"),
        ("deleted", "base\n", ""),
        ("新文件 [x].txt", "", "untracked\n"),
    ] {
        let file = files
            .iter()
            .find(|file| file.raw == name.as_bytes())
            .unwrap();
        let mut diff = backend::diff(repo, &oid, file).unwrap();
        assert_eq!(source(&mut diff, 0), expected_before);
        assert_eq!(source(&mut diff, 1), expected_after);
    }
    assert_eq!(snapshot(repo), before);
}

#[test]
fn workspace_compare_shortcut_preserves_highlight_filters_tabs_checks_and_git_state() {
    let temp = init();
    commit(temp.path(), "file", "base content\n");
    let base = git(temp.path(), &["rev-parse", "HEAD"]);
    commit(temp.path(), "file", "later committed\n");
    fs::write(temp.path().join("file"), "current working\n").unwrap();
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    panel.set_pick_mode(true);
    panel.toggle_pick(&context).unwrap();
    let picked = panel.picks[0].oid.clone();
    let selected = panel
        .visible
        .iter()
        .position(|&index| panel.hist.revs[index].oid == base)
        .unwrap();
    panel.select_row(selected);
    let before = snapshot(temp.path());
    assert!(matches!(
        key(&mut panel, K::Char('D'), &context),
        Action::None
    ));
    assert!(panel.comparing_workspace());
    finish(&mut panel, &context);
    assert_eq!(
        panel.workspace_comparison.as_ref().unwrap().target.oid,
        base
    );
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        panel.workspace_comparison.as_mut().unwrap().panel.icons = icons;
        for (width, height) in [(44, 12), (80, 25), (120, 36), (184, 48)] {
            let text = render(&mut panel, &context, width, height);
            assert!(text.contains("Working tree"), "{text}");
            assert!(text.contains("file"), "{text}");
            if width >= 80 {
                assert!(
                    text.contains("Commit") && text.contains("current working"),
                    "{text}"
                );
            }
            assert_eq!(panel.panes.nodes().len(), 2);
            assert!(panel
                .panes
                .nodes()
                .iter()
                .all(|(_, rect)| rect.right() <= width && rect.bottom() <= height));
        }
    }
    assert_eq!(panel.focused_pane(), PaneId::Files);
    let copied = panel.path_copy_target(&context).unwrap();
    assert!(matches!(copied.copy('r'), Action::Copy { text, .. } if text == "file"));
    key(&mut panel, K::Enter, &context);
    assert_eq!(panel.focused_pane(), PaneId::Info);
    assert!(matches!(
        key(&mut panel, K::Char('y'), &context),
        Action::Copy { text, .. } if text == "current working\n"
    ));
    key(&mut panel, K::Char('['), &context);
    assert!(matches!(
        key(&mut panel, K::Char('y'), &context),
        Action::Copy { text, .. } if text == "base content\n"
    ));
    for code in [
        K::Char('A'),
        K::Char('L'),
        K::Char('C'),
        K::Char('d'),
        K::Char('u'),
        K::Char('R'),
        K::F(2),
    ] {
        assert!(!matches!(
            key(&mut panel, code, &context),
            Action::Confirm { .. } | Action::Refresh(..)
        ));
    }
    key(&mut panel, K::Esc, &context);
    assert!(!panel.comparing_workspace());
    assert_eq!(panel.rev().unwrap().oid, base);
    assert_eq!(panel.cursor, selected);
    assert_eq!(panel.picks[0].oid, picked);
    assert!(panel.pick_mode);
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
fn workspace_compare_uppercase_jk_scrolls_preview_without_moving_file_selection() {
    let temp = init();
    let before: String = (0..80)
        .map(|line| format!("ALPHA-{line:03} before\n"))
        .collect();
    commit(temp.path(), "alpha.txt", &before);
    commit(temp.path(), "beta.txt", "base\n");
    let after: String = (0..80)
        .map(|line| format!("ALPHA-{line:03} after\n"))
        .collect();
    fs::write(temp.path().join("alpha.txt"), after).unwrap();
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    key(&mut panel, K::Char('D'), &context);
    finish(&mut panel, &context);
    let cursor = panel
        .workspace_comparison
        .as_ref()
        .unwrap()
        .panel
        .file_cursor();
    let text = render(&mut panel, &context, 120, 36);
    assert!(text.contains("ALPHA-000 after"), "{text}");

    for _ in 0..24 {
        key(&mut panel, K::Char('J'), &context);
    }
    let text = render(&mut panel, &context, 120, 36);
    assert!(!text.contains("ALPHA-000 after"), "{text}");
    assert_eq!(
        panel
            .workspace_comparison
            .as_ref()
            .unwrap()
            .panel
            .file_cursor(),
        cursor
    );

    key(&mut panel, K::Tab, &context);
    for _ in 0..24 {
        key(&mut panel, K::Char('K'), &context);
    }
    let text = render(&mut panel, &context, 120, 36);
    assert!(text.contains("ALPHA-000 after"), "{text}");
    assert_eq!(
        panel
            .workspace_comparison
            .as_ref()
            .unwrap()
            .panel
            .file_cursor(),
        cursor
    );
}

#[test]
fn workspace_compare_context_menu_captures_the_clicked_commit_and_returns_to_log() {
    let temp = init();
    commit(temp.path(), "file", "base\n");
    let base = git(temp.path(), &["rev-parse", "HEAD"]);
    commit(temp.path(), "file", "tip\n");
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    render(&mut panel, &context, 120, 36);
    let row = panel
        .visible
        .iter()
        .position(|&index| panel.hist.revs[index].oid == base)
        .unwrap();
    let rect = panel
        .hits
        .iter()
        .find_map(|(rect, hit)| matches!(hit, Hit::Commit(index) if *index == row).then_some(*rect))
        .unwrap();
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Right),
            column: rect.x,
            row: rect.y,
            modifiers: Mods::NONE,
        }),
        &context,
    );
    assert_eq!(panel.rev().unwrap().oid, base);
    key(&mut panel, K::End, &context);
    let menu = render(&mut panel, &context, 80, 25);
    assert!(menu.contains("Compare with Working Tree"), "{menu}");
    panel.select_row(0);
    key(&mut panel, K::Enter, &context);
    finish(&mut panel, &context);
    assert_eq!(
        panel.workspace_comparison.as_ref().unwrap().target.oid,
        base
    );
    assert_eq!(
        panel
            .workspace_comparison
            .as_ref()
            .unwrap()
            .panel
            .details
            .files
            .len(),
        1
    );
    key(&mut panel, K::Esc, &context);
    assert!(!panel.comparing_workspace());
}

#[test]
fn workspace_compare_mouse_search_refresh_and_empty_state_follow_working_content() {
    let temp = init();
    commit(temp.path(), "file", "base\n");
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    key(&mut panel, K::Char('D'), &context);
    finish(&mut panel, &context);
    assert!(render(&mut panel, &context, 120, 36).contains("No differences between this commit"));
    fs::write(temp.path().join("another"), "local text\n").unwrap();
    fs::write(temp.path().join("file"), "working text\n").unwrap();
    key(&mut panel, K::F(5), &context);
    finish(&mut panel, &context);
    render(&mut panel, &context, 120, 36);
    key(&mut panel, K::Char('/'), &context);
    panel.event(&Event::Paste("file".into()), &context);
    key(&mut panel, K::Enter, &context);
    finish(&mut panel, &context);
    let comparison = panel.workspace_comparison.as_ref().unwrap();
    assert_eq!(comparison.path().as_deref(), Some(b"file".as_slice()));
    key(&mut panel, K::Esc, &context);
    assert!(panel.comparing_workspace());
    render(&mut panel, &context, 120, 36);
    let comparison = panel.workspace_comparison.as_ref().unwrap();
    let rect = comparison
        .panel
        .hits
        .iter()
        .find_map(|(rect, hit)| {
            matches!(hit, Hit::File(row) if comparison.panel.file_is_leaf(*row)
            && *row < comparison.panel.file_cursor())
            .then_some(*rect)
        })
        .unwrap();
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: Mods::NONE,
        }),
        &context,
    );
    finish(&mut panel, &context);
    key(&mut panel, K::Enter, &context);
    key(&mut panel, K::Char('/'), &context);
    panel.event(&Event::Paste("local".into()), &context);
    key(&mut panel, K::Enter, &context);
    key(&mut panel, K::Esc, &context);
    assert!(panel.comparing_workspace());
    key(&mut panel, K::Esc, &context);
    assert!(!panel.comparing_workspace());
}

#[test]
fn workspace_compare_drops_stale_list_and_preview_results_when_roots_or_files_change() {
    let temp = init();
    commit(temp.path(), "file", "base\n");
    fs::write(temp.path().join("file"), "working\n").unwrap();
    let mut context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    key(&mut panel, K::Char('D'), &context);
    let pending = panel.workspace_comparison.as_mut().unwrap().files.hold();
    context.scope.clear();
    panel.poll_preview(&context).unwrap();
    pending
        .send(Ok(vec![model::File {
            raw: b"obsolete".to_vec(),
            ..Default::default()
        }]))
        .unwrap();
    panel.poll_preview(&context).unwrap();
    let text = render(&mut panel, &context, 80, 25);
    assert!(
        text.contains("outside Roots") && !text.contains("obsolete"),
        "{text}"
    );
    assert!(panel
        .workspace_comparison
        .as_ref()
        .unwrap()
        .panel
        .details
        .files
        .is_empty());
    context.scope.insert(0);
    finish(&mut panel, &context);
    panel.poll_preview(&context).unwrap();
    finish(&mut panel, &context);
    fs::write(temp.path().join("second"), "second contents\n").unwrap();
    key(&mut panel, K::F(5), &context);
    finish(&mut panel, &context);
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    comparison.panel.select_file(1);
    comparison.request_preview(Duration::ZERO);
    let pending = comparison.preview.hold();
    comparison.panel.select_file(0);
    comparison.request_preview(Duration::ZERO);
    assert_eq!(comparison.wakeup(), Some(Duration::from_millis(16)));
    pending
        .send(Ok(DiffView {
            lines: vec!["obsolete preview".into()],
            ..Default::default()
        }))
        .unwrap();
    finish(&mut panel, &context);
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    assert_eq!(comparison.path().as_deref(), Some(b"file".as_slice()));
    assert_eq!(
        source(comparison.panel.diff.as_mut().unwrap(), 1),
        "working\n"
    );
}

#[test]
fn workspace_compare_reports_binary_and_oversized_limits_without_writes() {
    let temp = init();
    commit(temp.path(), "base", "base\n");
    let oid = git(temp.path(), &["rev-parse", "HEAD"]);
    fs::write(temp.path().join("binary"), b"binary\0content").unwrap();
    fs::write(temp.path().join("large"), vec![b'x'; 96 * 1024 + 1]).unwrap();
    fs::write(temp.path().join("tracked-large"), vec![b'x'; 128 * 1024]).unwrap();
    git(temp.path(), &["add", "tracked-large"]);
    let before = snapshot(temp.path());
    let files = backend::files(temp.path(), &oid).unwrap();
    for (name, explanation) in [("binary", "Binary"), ("large", "96 KiB")] {
        let file = files
            .iter()
            .find(|file| file.raw == name.as_bytes())
            .unwrap();
        let diff = backend::diff(temp.path(), &oid, file).unwrap();
        assert!(diff.comparison.is_none());
        assert!(
            diff.lines.iter().any(|line| line.contains(explanation)),
            "{:?}",
            diff.lines
        );
        assert!(!diff
            .lines
            .iter()
            .any(|line| line.contains("showing the Git patch")));
    }
    let file = files
        .iter()
        .find(|file| file.raw == b"tracked-large")
        .unwrap();
    let mut diff = backend::diff(temp.path(), &oid, file).unwrap();
    assert_eq!(source(&mut diff, 1).len(), 128 * 1024);
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
fn workspace_compare_handles_literal_and_unicode_paths_independently() {
    let temp = init();
    let names = vec![
        PathBuf::from("literal [x].txt"),
        PathBuf::from("日本語 中文.txt"),
    ];
    #[cfg(unix)]
    let names = {
        let mut names = names;
        names.push(PathBuf::from(":(glob)pattern"));
        names.push(PathBuf::from("line\nbreak"));
        names
    };
    for (index, name) in names.iter().enumerate() {
        fs::write(temp.path().join(name), format!("base {index}\n")).unwrap();
    }
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "literal paths"]);
    let oid = git(temp.path(), &["rev-parse", "HEAD"]);
    for (index, name) in names.iter().enumerate() {
        fs::write(temp.path().join(name), format!("working {index}\n")).unwrap();
    }
    let before = snapshot(temp.path());
    let files = backend::files(temp.path(), &oid).unwrap();
    assert_eq!(files.len(), names.len());
    for (index, name) in names.iter().enumerate() {
        let file = files
            .iter()
            .find(|file| file.raw == name.as_os_str().as_encoded_bytes())
            .unwrap();
        let mut diff = backend::diff(temp.path(), &oid, file).unwrap();
        assert_eq!(source(&mut diff, 0), format!("base {index}\n"));
        assert_eq!(source(&mut diff, 1), format!("working {index}\n"));
    }
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
#[cfg(unix)]
fn workspace_compare_preserves_historical_native_byte_path_identity() {
    use std::{io::Write, process::Stdio};

    let temp = init();
    commit(temp.path(), "file", "base\n");
    let object = |args: &[&str], bytes: &[u8]| {
        let mut child = crate::core::command(temp.path())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let raw = b"native-\xff";
    let blob = object(&["hash-object", "-w", "--stdin"], b"native contents\n");
    let mut entry = format!("100644 blob {blob}\t").into_bytes();
    entry.extend(raw);
    entry.push(0);
    let tree = object(&["mktree", "-z"], &entry);
    let oid = git(temp.path(), &["commit-tree", &tree, "-m", "native path"]);
    let before = snapshot(temp.path());
    let files = backend::files(temp.path(), &oid).unwrap();
    let file = files.iter().find(|file| file.raw == raw).unwrap();
    let path = model::path_from_bytes(raw).unwrap();
    assert_eq!(
        crate::core::revision::source(temp.path(), &oid, path)
            .unwrap()
            .text,
        "native contents\n"
    );
    let mut diff = backend::diff(temp.path(), &oid, file).unwrap();
    if diff.comparison.is_some() {
        assert_eq!(source(&mut diff, 0), "native contents\n");
        assert_eq!(source(&mut diff, 1), "");
    } else {
        assert!(diff
            .lines
            .iter()
            .any(|line| line.contains("Cannot inspect working file")));
    }
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
#[cfg(unix)]
fn workspace_compare_reads_link_text_and_rejects_parent_symlinks_without_touching_targets() {
    use std::os::unix::fs::symlink;

    let temp = init();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("directory")).unwrap();
    fs::write(temp.path().join("directory/file"), "base\n").unwrap();
    fs::write(outside.path().join("file"), "outside contents\n").unwrap();
    symlink("old-target", temp.path().join("link")).unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "links"]);
    let oid = git(temp.path(), &["rev-parse", "HEAD"]);
    fs::remove_file(temp.path().join("link")).unwrap();
    symlink("new-target", temp.path().join("link")).unwrap();
    fs::remove_file(temp.path().join("directory/file")).unwrap();
    fs::remove_dir(temp.path().join("directory")).unwrap();
    symlink(outside.path(), temp.path().join("directory")).unwrap();
    let before = snapshot(temp.path());
    let files = backend::files(temp.path(), &oid).unwrap();
    let link = files.iter().find(|file| file.raw == b"link").unwrap();
    let mut diff = backend::diff(temp.path(), &oid, link).unwrap();
    assert_eq!(source(&mut diff, 0), "old-target");
    assert_eq!(source(&mut diff, 1), "new-target");
    let child = files
        .iter()
        .find(|file| file.raw == b"directory/file")
        .unwrap();
    let diff = backend::diff(temp.path(), &oid, child).unwrap();
    assert!(diff.comparison.is_none());
    assert!(diff
        .lines
        .iter()
        .any(|line| line.contains("symlink parents")));
    assert!(!diff
        .lines
        .iter()
        .any(|line| line.contains("outside contents")));
    assert_eq!(
        fs::read_to_string(outside.path().join("file")).unwrap(),
        "outside contents\n"
    );
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
fn workspace_compare_resizes_through_mouse_and_keyboard_without_changing_sources() {
    let temp = init();
    commit(temp.path(), "file", "base\n");
    fs::write(temp.path().join("file"), "working\n").unwrap();
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    key(&mut panel, K::Char('D'), &context);
    finish(&mut panel, &context);
    render(&mut panel, &context, 120, 36);
    let original = panel.panes.rect(PaneId::Files).unwrap();
    let press = MouseEvent {
        kind: M::Down(MouseButton::Left),
        column: original.right() - 1,
        row: original.y + 2,
        modifiers: Mods::NONE,
    };
    panel.event(&Event::Mouse(press), &context);
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Drag(MouseButton::Left),
            column: press.column + 8,
            ..press
        }),
        &context,
    );
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Up(MouseButton::Left),
            column: press.column + 8,
            ..press
        }),
        &context,
    );
    render(&mut panel, &context, 120, 36);
    let enlarged = panel.panes.rect(PaneId::Files).unwrap();
    assert_eq!(enlarged.width, original.width + 8);
    assert!(panel.panes.resize(enlarged, Axis::Width, -4));
    render(&mut panel, &context, 120, 36);
    assert_eq!(
        panel.panes.rect(PaneId::Files).unwrap().width,
        original.width + 4
    );
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    assert_eq!(
        source(comparison.panel.diff.as_mut().unwrap(), 1),
        "working\n"
    );
}

#[test]
fn workspace_compare_refuses_stale_history_and_reports_an_unavailable_commit_without_writes() {
    let temp = init();
    commit(temp.path(), "file", "base\n");
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    panel.enable_background_loading();
    panel.reload(&context, true).unwrap();
    assert!(matches!(
        key(&mut panel, K::Char('D'), &context),
        Action::Notice(_, _)
    ));
    assert!(!panel.comparing_workspace());
    let deadline = Instant::now() + Duration::from_secs(10);
    while panel.history_pending() {
        panel.poll_history(&context).unwrap();
        assert!(Instant::now() < deadline, "history refresh timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    key(&mut panel, K::Char('D'), &context);
    let before = snapshot(temp.path());
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    comparison.target.oid = "f".repeat(40);
    comparison.refresh(&context);
    finish(&mut panel, &context);
    let text = render(&mut panel, &context, 120, 36);
    assert!(text.contains("F5 retry"), "{text}");
    assert!(panel
        .workspace_comparison
        .as_ref()
        .unwrap()
        .panel
        .details
        .files
        .is_empty());
    key(&mut panel, K::Esc, &context);
    assert!(!panel.comparing_workspace());
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
fn workspace_compare_uses_the_highlighted_repository_when_two_roots_share_a_commit() {
    let source_repo = init();
    commit(source_repo.path(), "file", "shared\n");
    let temp = tempfile::tempdir().unwrap();
    for name in ["left", "right"] {
        let path = temp.path().join(name);
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                source_repo.path().to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        fs::write(path.join("file"), format!("{name} working\n")).unwrap();
        fs::write(path.join(name), name).unwrap();
    }
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    let root = context
        .repos
        .iter()
        .position(|repo| repo.name == "right")
        .unwrap();
    let row = panel
        .visible
        .iter()
        .position(|&index| panel.hist.revs[index].root == root)
        .unwrap();
    panel.select_row(row);
    let before = snapshot(temp.path());
    key(&mut panel, K::Char('D'), &context);
    finish(&mut panel, &context);
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    assert_eq!(comparison.repo.name, "right");
    let paths: Vec<_> = comparison
        .panel
        .details
        .files
        .iter()
        .map(|file| model::plain(&file.raw))
        .collect();
    assert_eq!(paths, ["file", "right"]);
    assert_eq!(
        source(comparison.panel.diff.as_mut().unwrap(), 1),
        "right working\n"
    );
    assert_eq!(snapshot(temp.path()), before);
}
