use super::*;
use crate::{
    core::Context,
    merge::MergePanel,
    ui::{Action, Panel},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Stdio};

fn add_native_conflict(repo: &Path, stages: &Stages, name: &[u8]) -> PathBuf {
    let mut input = Vec::new();
    for (stage, entry) in stages.0.iter().enumerate() {
        let (mode, oid) = entry.as_ref().unwrap();
        input.extend(format!("{mode} {oid} {}\t", stage + 1).as_bytes());
        input.extend(name);
        input.push(0);
    }
    let mut child = crate::core::command(repo)
        .args(["update-index", "-z", "--index-info"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(OsString::from_vec(name.to_vec()))
}

#[test]
fn non_utf8_index_paths_keep_ordinary_conflicts_and_native_operation_visible() {
    let dir = fixture();
    let repo = dir.path();
    let ordinary = conflicts(0, repo).unwrap().pop().unwrap();
    let paths = [b"nonutf-\xff.txt".as_slice(), b"nonutf-\xfe.txt".as_slice()]
        .map(|bytes| add_native_conflict(repo, &ordinary.stages, bytes));
    let invalid: Vec<_> = conflicts(0, repo)
        .unwrap()
        .into_iter()
        .filter(|file| file.path.to_str().is_none())
        .collect();
    assert_eq!(
        invalid.len(),
        2,
        "lossy display names must not merge native identities"
    );
    for path in &paths {
        assert!(invalid.iter().any(|file| &file.path == path));
    }
    let raw_index = fs::read(git_path(repo, "index").unwrap()).unwrap();
    let raw_stages = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let working = fs::read(repo.join(&ordinary.path)).unwrap();
    let head = g(repo, &["rev-parse", "HEAD"]);
    let ctx = Context::discover(repo).unwrap();
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    assert!(panel.has_pending_work());
    assert_eq!(panel.list.files_in(0).count(), 3);
    let mut terminal = Terminal::new(TestBackend::new(120, 18)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), &ctx)).unwrap();
    let buffer = terminal.backend().buffer();
    let screen = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        screen.contains("3 files") && screen.contains("External"),
        "{screen}"
    );
    assert!(!screen.contains("No conflicts"), "{screen}");
    for file in &invalid {
        assert!(file.stage_error().unwrap().contains("external Git tool"));
        panel.list.select_file(file);
        let action = panel.event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &ctx,
        );
        assert!(matches!(action, Action::Notice(_, message) if message.contains("Non-UTF-8 path")));
        assert!(panel.doc.is_none());
        assert!(matches!(
            panel.event(
                &Event::Key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::NONE)),
                &ctx
            ),
            Action::Confirm { .. }
        ));
    }
    panel.list.select_file(&ordinary);
    assert!(matches!(
        panel.event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &ctx
        ),
        Action::None
    ));
    assert_eq!(panel.doc.as_ref().unwrap().file, ordinary);
    assert_eq!(
        fs::read(git_path(repo, "index").unwrap()).unwrap(),
        raw_index
    );
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        raw_stages
    );
    assert_eq!(fs::read(repo.join(&ordinary.path)).unwrap(), working);
    assert_eq!(g(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(operation(repo).unwrap().unwrap().name, "merge");
}

#[test]
fn applying_an_ordinary_file_preserves_every_non_utf8_index_stage() {
    let dir = fixture();
    let repo = dir.path();
    let ordinary = conflicts(0, repo).unwrap().pop().unwrap();
    add_native_conflict(repo, &ordinary.stages, b"nonutf-\xff.txt");
    let unsupported = conflicts(0, repo)
        .unwrap()
        .into_iter()
        .find(|file| file.path.to_str().is_none())
        .unwrap();
    let mut document = Document::open(repo, ordinary).unwrap();
    for chunk in &mut document.chunks {
        chunk.use_side(FileSide::Current);
    }
    document.apply().unwrap();
    assert_eq!(conflicts(0, repo).unwrap(), vec![unsupported]);
    assert!(operation(repo).unwrap().is_some());
    assert!(
        continue_plan(repo).is_err(),
        "unsupported stages still block Continue"
    );
}
