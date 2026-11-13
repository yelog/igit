use super::*;
use crate::log::tests::{commit, git, init};
use crossterm::event::KeyEvent;
use ratatui::{backend::TestBackend, Terminal};

fn key(panel: &mut FileDiff, code: KeyCode) -> bool {
    panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn render(panel: &mut FileDiff, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), IconMode::Unicode))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn original() -> String {
    (1..=70)
        .map(|line| format!("source line {line}\n"))
        .collect()
}

fn fixture() -> (tempfile::TempDir, Context, String) {
    let temporary = init();
    let filename = "[literal] 文件 name.txt";
    commit(temporary.path(), filename, &original());
    commit(temporary.path(), "sibling.txt", "original sibling\n");
    let staged = original().replace("source line 4\n", "staged four\n");
    fs::write(temporary.path().join(filename), staged).unwrap();
    fs::write(temporary.path().join("sibling.txt"), "staged sibling\n").unwrap();
    git(temporary.path(), &["add", "--all"]);
    let working = original()
        .replace("source line 4\n", "working four\n")
        .replace("source line 60\n", "working sixty\n");
    fs::write(temporary.path().join(filename), working).unwrap();
    let context = Context::discover(temporary.path()).unwrap();
    (temporary, context, filename.into())
}

fn git_state(repo: &Path) -> (String, Vec<u8>, Vec<u8>) {
    (
        git(repo, &["rev-parse", "HEAD"]),
        fs::read(repo.join(".git/index")).unwrap(),
        crate::core::git_bytes(repo, &["diff", "--cached", "--binary"]).unwrap(),
    )
}

#[test]
fn head_file_diff_restores_only_the_reviewed_hunk_and_preserves_the_real_index() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(&filename);
    let state = git_state(temporary.path());
    let mut panel = FileDiff::open(&context, &file, 4, None).unwrap();
    let screen = render(&mut panel, 150, 32);
    assert!(screen.contains("HEAD"), "{screen}");
    assert!(screen.contains("Local"), "{screen}");
    assert!(screen.contains("working four"), "{screen}");
    assert!(!screen.contains("staged four"), "{screen}");
    assert_eq!(panel.comparison.change_count(), 2);
    assert!(!key(&mut panel, KeyCode::Char('L')));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original().replace("source line 60\n", "working sixty\n")
    );
    assert_eq!(panel.comparison.change_count(), 1);
    assert_eq!(git_state(temporary.path()), state);
    assert!(!key(&mut panel, KeyCode::Char('U')));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original()
            .replace("source line 4\n", "working four\n")
            .replace("source line 60\n", "working sixty\n")
    );
    assert_eq!(panel.comparison.change_count(), 2);
    assert_eq!(git_state(temporary.path()), state);
    assert!(!key(&mut panel, KeyCode::Char('U')));
    assert!(panel.notice.is_some());
    assert_eq!(
        fs::read_to_string(temporary.path().join("sibling.txt")).unwrap(),
        "staged sibling\n"
    );
    assert!(key(&mut panel, KeyCode::Esc));
}

#[test]
fn head_file_diff_uses_the_unsaved_snapshot_and_never_writes_the_working_file() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(&filename);
    let disk = fs::read(&file).unwrap();
    let state = git_state(temporary.path());
    let snapshot_directory = tempfile::tempdir().unwrap();
    let snapshot = snapshot_directory.path().join("buffer.txt");
    let unsaved = original()
        .replace("source line 4\n", "unsaved four\n")
        .replace("source line 60\n", "unsaved sixty\n");
    fs::write(&snapshot, &unsaved).unwrap();
    let mut panel = FileDiff::open(&context, &file, 4, Some(&snapshot)).unwrap();
    let screen = render(&mut panel, 150, 32);
    assert!(screen.contains("unsaved four"), "{screen}");
    assert!(!screen.contains("working four"), "{screen}");
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::read_to_string(&snapshot).unwrap(),
        original().replace("source line 60\n", "unsaved sixty\n")
    );
    assert!(!key(&mut panel, KeyCode::Char('u')));
    assert!(panel.notice.is_none());
    assert_eq!(fs::read_to_string(&snapshot).unwrap(), unsaved);
    assert_eq!(panel.comparison.change_count(), 2);
    assert_eq!(fs::read(&file).unwrap(), disk);
    assert_eq!(git_state(temporary.path()), state);
}

#[test]
fn head_file_diff_reviews_apply_all_separately_and_defaults_to_cancel() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(&filename);
    let disk = fs::read(&file).unwrap();
    let state = git_state(temporary.path());
    let mut panel = FileDiff::open(&context, &file, 4, None).unwrap();
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::Char('A'));
    assert!(panel.reviewing);
    let screen = render(&mut panel, 120, 32);
    assert!(screen.contains("Restore all 2 changed blocks"), "{screen}");
    key(&mut panel, KeyCode::Enter);
    assert!(!panel.reviewing);
    assert_eq!(fs::read(&file).unwrap(), disk);
    key(&mut panel, KeyCode::Char('A'));
    render(&mut panel, 32, 8);
    key(&mut panel, KeyCode::Char('y'));
    assert!(panel.reviewing);
    assert_eq!(fs::read(&file).unwrap(), disk);
    render(&mut panel, 44, 12);
    key(&mut panel, KeyCode::Char('y'));
    assert!(!panel.reviewing);
    assert!(panel.notice.is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), original());
    assert_eq!(git_state(temporary.path()), state);
    assert!(!key(&mut panel, KeyCode::Char('U')));
    assert!(panel.notice.is_none());
    assert_eq!(fs::read(&file).unwrap(), disk);
    assert_eq!(git_state(temporary.path()), state);
}

#[test]
fn head_file_diff_undoes_successive_restores_in_reverse_order() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(filename);
    let working = original()
        .replace("source line 4\n", "working four\n")
        .replace("source line 60\n", "working sixty\n");
    let after_first = original().replace("source line 60\n", "working sixty\n");
    let state = git_state(temporary.path());
    let mut panel = FileDiff::open(&context, &file, 4, None).unwrap();
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::Char('L'));
    assert_eq!(fs::read_to_string(&file).unwrap(), after_first);
    assert_eq!(panel.comparison.change_count(), 1);
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::Char('L'));
    assert_eq!(fs::read_to_string(&file).unwrap(), original());
    assert_eq!(panel.comparison.change_count(), 0);
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::Char('u'));
    assert_eq!(fs::read_to_string(&file).unwrap(), after_first);
    assert_eq!(panel.comparison.change_count(), 1);
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::Char('u'));
    assert_eq!(fs::read_to_string(&file).unwrap(), working);
    assert_eq!(panel.comparison.change_count(), 2);
    assert_eq!(git_state(temporary.path()), state);
}

#[test]
fn head_file_diff_rejects_changed_head_disk_attributes_and_buffer_snapshots() {
    for external in ["HEAD", "disk", "attributes", "snapshot"] {
        let (temporary, context, filename) = fixture();
        let file = temporary.path().join(&filename);
        let snapshot_directory = tempfile::tempdir().unwrap();
        let snapshot = snapshot_directory.path().join("buffer.txt");
        fs::write(&snapshot, fs::read(&file).unwrap()).unwrap();
        let mut panel = FileDiff::open(
            &context,
            &file,
            4,
            (external == "snapshot").then_some(snapshot.as_path()),
        )
        .unwrap();
        render(&mut panel, 120, 32);
        match external {
            "HEAD" => {
                git(
                    temporary.path(),
                    &["commit", "--allow-empty", "-qm", "external"],
                );
            }
            "disk" => fs::write(&file, "external working content\n").unwrap(),
            "attributes" => fs::write(
                temporary.path().join(".gitattributes"),
                "*.txt text eol=crlf\n",
            )
            .unwrap(),
            "snapshot" => fs::write(&snapshot, "external snapshot\n").unwrap(),
            _ => unreachable!(),
        }
        let state = git_state(temporary.path());
        let disk = fs::read(&file).unwrap();
        let snapshot_bytes = fs::read(&snapshot).unwrap();
        key(&mut panel, KeyCode::Char('L'));
        assert!(panel.notice.is_some(), "{external}");
        assert_eq!(fs::read(&file).unwrap(), disk, "{external}");
        assert_eq!(fs::read(&snapshot).unwrap(), snapshot_bytes, "{external}");
        assert_eq!(git_state(temporary.path()), state, "{external}");
    }
}

#[test]
fn head_file_diff_undo_rechecks_the_target_after_a_restore() {
    for snapshot_mode in [false, true] {
        let (temporary, context, filename) = fixture();
        let file = temporary.path().join(filename);
        let snapshots = tempfile::tempdir().unwrap();
        let snapshot = snapshots.path().join("buffer.txt");
        fs::write(&snapshot, fs::read(&file).unwrap()).unwrap();
        let state = git_state(temporary.path());
        let mut panel = FileDiff::open(
            &context,
            &file,
            4,
            snapshot_mode.then_some(snapshot.as_path()),
        )
        .unwrap();
        render(&mut panel, 120, 32);
        key(&mut panel, KeyCode::Char('L'));
        assert!(panel.notice.is_none());
        let target = if snapshot_mode { &snapshot } else { &file };
        fs::write(target, "external change\n").unwrap();
        key(&mut panel, KeyCode::Char('u'));
        assert!(panel.notice.is_some());
        assert_eq!(fs::read_to_string(target).unwrap(), "external change\n");
        assert_eq!(git_state(temporary.path()), state);
    }
}

#[test]
fn head_file_diff_handles_unsaved_new_files_and_uniform_crlf() {
    let (temporary, context, _) = fixture();
    let snapshots = tempfile::tempdir().unwrap();
    let snapshot = snapshots.path().join("buffer.txt");
    let new_file = temporary.path().join("new 文件.txt");
    fs::write(&snapshot, "new unsaved file\n").unwrap();
    let state = git_state(temporary.path());
    let mut panel = FileDiff::open(&context, &new_file, 1, Some(&snapshot)).unwrap();
    render(&mut panel, 120, 24);
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_none());
    assert_eq!(fs::read(&snapshot).unwrap(), b"");
    assert!(!new_file.exists());
    assert_eq!(git_state(temporary.path()), state);

    let crlf = temporary.path().join("crlf.txt");
    commit(temporary.path(), "crlf.txt", "first\nsecond\nthird\n");
    fs::write(&crlf, "first\r\nsecond\r\nthird\r\n").unwrap();
    fs::write(&snapshot, "first\r\nunsaved\r\nthird\r\n").unwrap();
    let mut panel = FileDiff::open(&context, &crlf, 2, Some(&snapshot)).unwrap();
    render(&mut panel, 120, 24);
    assert_eq!(panel.comparison.change_count(), 1);
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::read(&snapshot).unwrap(),
        b"first\r\nsecond\r\nthird\r\n"
    );
}

#[test]
fn head_file_diff_rejects_binary_content_and_snapshots_inside_worktrees() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(filename);
    assert!(FileDiff::open(&context, &file, 1, Some(&file)).is_err());
    fs::write(&file, b"binary\0content").unwrap();
    assert!(FileDiff::open(&context, &file, 1, None).is_err());
}

#[test]
fn head_file_diff_help_search_and_small_view_preserve_review_boundaries() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(filename);
    let disk = fs::read(&file).unwrap();
    let mut panel = FileDiff::open(&context, &file, 4, None).unwrap();
    render(&mut panel, 32, 8);
    key(&mut panel, KeyCode::Char('L'));
    key(&mut panel, KeyCode::Char('A'));
    assert_eq!(fs::read(&file).unwrap(), disk);
    assert!(!panel.reviewing);
    render(&mut panel, 120, 32);
    key(&mut panel, KeyCode::F(1));
    assert!(panel.help.is_some());
    let screen = render(&mut panel, 120, 32);
    assert!(
        screen.contains("Restore the selected block from HEAD"),
        "{screen}"
    );
    assert!(screen.contains("Undo the last restore"), "{screen}");
    assert!(!key(&mut panel, KeyCode::Esc));
    assert!(panel.help.is_none());
    key(&mut panel, KeyCode::Char('/'));
    for character in "working".chars() {
        key(&mut panel, KeyCode::Char(character));
    }
    key(&mut panel, KeyCode::Enter);
    assert!(!key(&mut panel, KeyCode::Esc));
    assert_eq!(fs::read(&file).unwrap(), disk);
    assert!(key(&mut panel, KeyCode::Esc));
}

#[test]
fn head_file_diff_centering_unchanged_context_does_not_restore_an_offscreen_hunk() {
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(filename);
    let disk = fs::read(&file).unwrap();
    let mut panel = FileDiff::open(&context, &file, 35, None).unwrap();
    let screen = render(&mut panel, 120, 24);
    assert!(screen.contains("source line 35"), "{screen}");
    assert!(!screen.contains("working four"), "{screen}");
    assert!(!screen.contains("working sixty"), "{screen}");
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_some());
    assert_eq!(fs::read(&file).unwrap(), disk);
    key(&mut panel, KeyCode::Char('n'));
    assert!(render(&mut panel, 120, 24).contains("working sixty"));
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original().replace("source line 4\n", "working four\n")
    );
}

#[cfg(unix)]
#[test]
fn head_file_diff_rejects_symlink_snapshots_and_preserves_file_permissions() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let (temporary, context, filename) = fixture();
    let file = temporary.path().join(filename);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    let mut panel = FileDiff::open(&context, &file, 4, None).unwrap();
    render(&mut panel, 120, 24);
    key(&mut panel, KeyCode::Char('L'));
    assert!(panel.notice.is_none());
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let snapshots = tempfile::tempdir().unwrap();
    let linked = snapshots.path().join("linked.txt");
    symlink(&file, &linked).unwrap();
    assert!(FileDiff::open(&context, &file, 1, Some(&linked)).is_err());
}
