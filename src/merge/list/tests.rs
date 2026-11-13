use super::*;
use crate::{
    core::git,
    ui::{icons::IconMode, key_sequence::KeySequence, Panel},
};
use crossterm::event::{
    Event, KeyCode as K, KeyEvent, KeyModifiers as M, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::fs;

const FILE: &str = "src/example.js";
const BASE: &str = "base\na\nb\nshared\nc\nd\nbase\n";
const CURRENT: &str = "current only\na\nb\ncurrent\nc\nd\nbase\n";
const INCOMING: &str = "base\na\nb\nincoming\nc\nd\nincoming only\n";

fn repo(path: &Path) {
    let run = |args: &[&str]| git(path, args).unwrap();
    for dir in ["src", "tools"] {
        fs::create_dir_all(path.join(dir)).unwrap();
    }
    run(&["init", "-q", "-b", "main"]);
    for (name, value) in [
        ("user.name", "Conflict tester"),
        ("user.email", "conflict@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        run(&["config", name, value]);
    }
    let readme = path.join("tools/README.md");
    fs::write(path.join(FILE), BASE).unwrap();
    fs::write(&readme, "base\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "base"]);
    run(&["switch", "-qc", "incoming"]);
    fs::write(path.join(FILE), INCOMING).unwrap();
    fs::write(&readme, "updated\n").unwrap();
    fs::write(path.join("z.yml"), "incoming\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "incoming"]);
    run(&["switch", "-q", "main"]);
    fs::write(path.join(FILE), CURRENT).unwrap();
    fs::remove_file(readme).unwrap();
    fs::write(path.join("z.yml"), "current\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "current"]);
    assert!(git(path, &["merge", "--no-edit", "incoming"]).is_err());
    assert_eq!(conflicts(0, path).unwrap().len(), 3);
}

fn panel(ctx: &Context) -> MergePanel {
    let mut panel = MergePanel {
        icons: IconMode::Ascii,
        ..MergePanel::default()
    };
    panel.refresh(ctx).unwrap();
    panel
}

fn render(panel: &mut MergePanel, ctx: &Context, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), ctx)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn screen(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| row(buffer, y))
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(panel: &mut MergePanel, ctx: &Context, code: K) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, M::NONE)), ctx)
}

fn click(panel: &mut MergePanel, ctx: &Context, hit: impl Fn(&Hit) -> bool) -> Action {
    let cell = panel.hits.iter().find(|(_, h)| hit(h)).unwrap().0;
    panel.event(
        &Event::Mouse(MouseEvent {
            column: cell.right() - 1,
            row: cell.y,
            kind: MouseEventKind::Down(MouseButton::Left),
            modifiers: M::NONE,
        }),
        ctx,
    )
}

fn finish_background(panel: &mut MergePanel, ctx: &Context) -> Option<String> {
    panel.finish_loading(ctx)
}

#[test]
fn conflict_list_shows_git_sides_and_keeps_controls_available_in_small_terminals() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("app");
    repo(&repo_path);
    let ctx = Context::discover(&repo_path).unwrap();
    let mut panel = panel(&ctx);
    let buffer = render(&mut panel, &ctx, 120, 24);
    let text = screen(&buffer);
    assert!(row(&buffer, 0).contains("Prepare Simple Drafts"), "{text}");
    assert!(!text.contains("Merging incoming into main"), "{text}");
    for label in [
        "Yours (main)",
        "Theirs (incoming)",
        "Unresolved  3 files",
        "Resolve Manually",
    ] {
        assert!(text.contains(label), "{text}");
    }
    let find = |name| text.lines().find(|line| line.contains(name)).unwrap();
    assert_eq!(find("example.js").matches("Modified").count(), 2);
    assert_eq!(find("example.js").matches("Accept").count(), 2);
    assert!(find("README.md").contains("Deleted"));
    assert_eq!(find("z.yml").matches("Added").count(), 2);
    assert!(!panel.hits.iter().any(|(_, h)| matches!(h, Hit::Continue)));

    click(&mut panel, &ctx, |hit| matches!(hit, Hit::TogglePaths));
    let text = screen(&render(&mut panel, &ctx, 80, 24));
    assert!(text.contains("tools/README.md"), "{text}");
    key(&mut panel, &ctx, K::Char('j')); // A delete/modify conflict.
    panel.refresh(&ctx).unwrap();
    assert_eq!(
        panel.list.selected_file().unwrap().path,
        Path::new("tools/README.md")
    );
    let text = screen(&render(&mut panel, &ctx, 44, 9));
    assert!(text.contains("Yours") && text.contains("Theirs"), "{text}");
    assert!(
        text.contains("Close") && text.contains("Resolve Manually"),
        "{text}"
    );
    assert!(!panel
        .hits
        .iter()
        .any(|(_, h)| matches!(h, Hit::AcceptFile(..))));
    click(&mut panel, &ctx, |hit| matches!(hit, Hit::FoldUnresolved));
    let text = screen(&render(&mut panel, &ctx, 44, 9));
    assert!(
        text.contains("Unresolved") && !text.contains("README.md"),
        "{text}"
    );
    key(&mut panel, &ctx, K::Enter);
    assert!(panel.doc.is_none());
    assert!(panel.list.selected_file().is_none());
    let text = screen(&render(&mut panel, &ctx, 44, 9));
    assert!(text.contains("README.md"), "{text}");
}

#[test]
fn conflict_list_enter_toggles_the_focused_unresolved_group() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    let background = |buffer: &Buffer, label: &str| {
        let y = (0..buffer.area.height)
            .find(|&y| row(buffer, y).contains(label))
            .unwrap();
        buffer[(1, y)].bg
    };
    let buffer = render(&mut panel, &ctx, 80, 24);
    assert_eq!(
        background(&buffer, "Unresolved"),
        background(&buffer, "README.md")
    );

    key(&mut panel, &ctx, K::Up);
    panel.refresh(&ctx).unwrap();
    let buffer = render(&mut panel, &ctx, 80, 24);
    assert_eq!(background(&buffer, "Unresolved"), SEL);
    assert!(!panel
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::Open | Hit::AcceptFile(..) | Hit::PrepareSimple)));
    key(&mut panel, &ctx, K::Char('Y'));
    assert!(panel.doc.is_none());

    for expanded in [false, true] {
        key(&mut panel, &ctx, K::Enter);
        assert!(panel.doc.is_none());
        let buffer = render(&mut panel, &ctx, 80, 24);
        assert_eq!(background(&buffer, "Unresolved"), SEL);
        let text = screen(&buffer);
        assert_eq!(text.contains("example.js"), expanded, "{text}");
    }

    key(&mut panel, &ctx, K::Char('j'));
    key(&mut panel, &ctx, K::Enter);
    assert_eq!(panel.doc.as_ref().unwrap().file.path, Path::new(FILE));
}

#[test]
fn conflict_list_uses_gg_for_first_and_uppercase_g_for_last() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    key(&mut panel, &ctx, K::Char('j'));
    let before = panel.list.focus;
    key(&mut panel, &ctx, K::Char('g'));
    assert!(panel.list.focus == before);
    assert_eq!(panel.key_sequence(), Some(KeySequence::FirstItem));
    key(&mut panel, &ctx, K::Char('g'));
    assert!(panel.list.focus == Some(Row::Unresolved));
    key(&mut panel, &ctx, K::Char('G'));
    let last = panel.list.focus;
    assert!(last != Some(Row::Unresolved));
    key(&mut panel, &ctx, K::Home);
    assert!(panel.list.focus == Some(Row::Unresolved));
    key(&mut panel, &ctx, K::End);
    assert!(panel.list.focus == last);
}

#[test]
fn conflict_list_accept_previews_the_entire_selected_side_before_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    repo(path);
    fs::write(path.join("unrelated.txt"), "staged separately\n").unwrap();
    git(path, &["add", "unrelated.txt"]).unwrap();
    let git = |args: &[&str]| git(path, args).unwrap();
    let head = git(&["rev-parse", "HEAD"]);
    let index = git(&["ls-files", "--stage"]);
    let unrelated = git(&["ls-files", "--stage", "--", "unrelated.txt"]);
    let original = fs::read(path.join(FILE)).unwrap();
    let ctx = Context::discover(path).unwrap();
    let mut panel = panel(&ctx);
    render(&mut panel, &ctx, 120, 24);
    let notice = click(&mut panel, &ctx, |hit| {
        matches!(hit, Hit::AcceptFile(_, FileSide::Incoming))
    });
    assert!(
        matches!(notice, Action::Notice(NoticeLevel::Info, message) if message.contains("entire Theirs"))
    );
    let doc = panel.doc.as_ref().unwrap();
    let preview: String = doc
        .chunks
        .iter()
        .map(|chunk| chunk.result.as_str())
        .collect();
    assert_eq!(preview, INCOMING);
    assert_eq!(doc.pending(), 0);
    let Action::Confirm { token, body, .. } = key(&mut panel, &ctx, K::Char('A')) else {
        panic!("Apply file must open a review");
    };
    assert!(body.contains(FILE));
    assert_eq!(fs::read(path.join(FILE)).unwrap(), original);
    assert_eq!(git(&["ls-files", "--stage"]), index);
    assert!(matches!(
        panel.confirm(&token, &ctx),
        Action::Refresh(NoticeLevel::Warning, message)
            if message.contains("staged resolved")
                && message.contains("Continue is a separate action")
                && message.contains("Next file:")
                && message.contains("Added/deleted/renamed conflict")
    ));
    assert_eq!(fs::read_to_string(path.join(FILE)).unwrap(), INCOMING);
    assert_eq!(
        git(&["ls-files", "--stage", "--", "unrelated.txt"]),
        unrelated
    );
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert!(!git(&["rev-parse", "MERGE_HEAD"]).is_empty());
    assert_eq!(conflicts(0, path).unwrap().len(), 2);
}

#[test]
fn conflict_list_prepares_drafts_only_in_the_highlighted_repository() {
    let dir = tempfile::tempdir().unwrap();
    let roots = [dir.path().join("app"), dir.path().join("lib")];
    for root in &roots {
        repo(root);
    }
    let snapshot = || {
        roots.each_ref().map(|root| {
            (
                git(root, &["ls-files", "--stage"]).unwrap(),
                fs::read(root.join(FILE)).unwrap(),
            )
        })
    };
    let before = snapshot();
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    let text = screen(&render(&mut panel, &ctx, 120, 24));
    assert!(text.contains("Unresolved  6 files"), "{text}");
    assert!(
        text.contains("Current") && text.contains("Incoming"),
        "{text}"
    );
    assert!(matches!(
        click(&mut panel, &ctx, |h| matches!(h, Hit::PrepareSimple)),
        Action::Notice(NoticeLevel::Warning, msg) if msg.contains("app: prepared 1") && msg.contains("2 skipped")
    ));
    assert_eq!(snapshot(), before);
    let text = screen(&render(&mut panel, &ctx, 120, 24));
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("example.js") && line.contains("Draft"))
            .count(),
        1
    );
    key(&mut panel, &ctx, K::Enter);
    let doc = panel.doc.as_ref().unwrap();
    assert_eq!(
        doc.pending(),
        1,
        "the overlapping change remains for manual review"
    );
    let preview: String = doc
        .chunks
        .iter()
        .map(|chunk| chunk.result.as_str())
        .collect();
    assert_eq!(preview, "current only\na\nb\nshared\nc\nd\nincoming only\n");
    assert_eq!(snapshot(), before);
}

#[test]
fn background_refresh_retains_cursor_and_folds_for_the_same_roots() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    panel.enable_background_loading();

    key(&mut panel, &ctx, K::Char('j'));
    let focus = panel.list.focus;
    let selected = panel.list.selected_file().unwrap().path.clone();
    panel.refresh(&ctx).unwrap();
    assert!(panel.reads_pending());
    assert_eq!(panel.list.focus, focus);
    assert_eq!(panel.list.selected_file().unwrap().path, selected);
    assert!(finish_background(&mut panel, &ctx).is_none());
    assert_eq!(panel.list.focus, focus);
    assert_eq!(panel.list.selected_file().unwrap().path, selected);

    key(&mut panel, &ctx, K::Char(' '));
    assert!(panel.list.folded);
    assert_eq!(panel.list.focus, Some(Row::Unresolved));
    panel.refresh(&ctx).unwrap();
    assert!(finish_background(&mut panel, &ctx).is_none());
    assert!(panel.list.folded);
    assert_eq!(panel.list.focus, Some(Row::Unresolved));
}

#[test]
fn background_scope_change_clears_old_rows_immediately() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let mut ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    panel.enable_background_loading();
    assert!(!panel.list.is_empty());

    ctx.scope.clear();
    panel.refresh(&ctx).unwrap();
    assert!(panel.list.is_empty());
    assert!(!panel.has_pending_work());
    assert!(finish_background(&mut panel, &ctx).is_none());
}

#[test]
fn failed_repository_read_keeps_known_rows_and_reports_the_error() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["app", "lib"] {
        repo(&dir.path().join(name));
    }
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    panel.enable_background_loading();
    let failed = ctx
        .repos
        .iter()
        .position(|repo| repo.name == "app")
        .unwrap();
    let other = ctx
        .repos
        .iter()
        .position(|repo| repo.name == "lib")
        .unwrap();
    assert_eq!(panel.list.files_in(failed).count(), 3);
    assert_eq!(panel.list.files_in(other).count(), 3);

    let offline = dir.path().join("app-offline");
    fs::rename(&ctx.repos[failed].path, &offline).unwrap();
    panel.refresh(&ctx).unwrap();
    let error = finish_background(&mut panel, &ctx).expect("failed read is reported");
    assert!(error.contains("app"), "{error}");
    assert!(panel.list.repo_failed(failed));
    assert!(!panel.list.repo_failed(other));
    assert_eq!(panel.list.files_in(failed).count(), 3);
    assert_eq!(panel.list.files_in(other).count(), 3);
    assert!(panel.list.error().unwrap().contains("app"));
    let text = screen(&render(&mut panel, &ctx, 120, 24));
    assert!(
        text.contains("Refresh failed") && text.contains("app"),
        "{text}"
    );
    fs::rename(offline, &ctx.repos[failed].path).unwrap();
}

#[test]
fn failed_repository_read_preserves_a_known_ready_row() {
    let repo = Repo {
        path: std::path::PathBuf::from("/igit-test/unavailable"),
        name: "ready".into(),
        branch: "main".into(),
        parent: None,
        submodule: false,
    };
    let operation = Operation {
        name: "rebase",
        head: "head".into(),
        other: "other".into(),
    };
    let mut list = ConflictList::default();
    list.install(vec![RepoSnapshot {
        root: 0,
        rows: Ok(RepoRows {
            files: Vec::new(),
            ready: true,
            sources: Some(Sources::new(&repo, Some(&operation))),
        }),
    }]);
    assert!(matches!(list.items.as_slice(), [Entry::Ready(0)]));
    assert!(matches!(list.open(), Hit::Continue));

    list.install(vec![RepoSnapshot {
        root: 0,
        rows: Err("ready: offline".into()),
    }]);
    assert!(matches!(list.items.as_slice(), [Entry::Ready(0)]));
    assert!(list.repo_failed(0));
    assert_eq!(list.error(), Some("ready: offline"));
}

#[test]
fn source_identity_tracks_stages_and_operation() {
    let repo = Repo {
        path: std::path::PathBuf::from("/igit-test/source"),
        name: "source".into(),
        branch: "main".into(),
        parent: None,
        submodule: false,
    };
    let operation = Operation {
        name: "rebase",
        head: "head".into(),
        other: "other".into(),
    };
    let changed_operation = Operation {
        other: "changed".into(),
        ..operation.clone()
    };
    let file = Conflict {
        root: 0,
        path: "file".into(),
        stages: Stages::default(),
    };
    let mut list = ConflictList::default();
    list.install(vec![RepoSnapshot {
        root: 0,
        rows: Ok(RepoRows {
            files: vec![file.clone()],
            ready: false,
            sources: Some(Sources::new(&repo, Some(&operation))),
        }),
    }]);
    assert!(list.source_matches(&file, &Some(operation.clone())));

    let mut changed_stages = file.clone();
    changed_stages.stages.0[0] = Some(("100644".into(), "base".into()));
    assert!(!list.source_matches(&changed_stages, &Some(operation.clone())));
    assert!(!list.source_matches(&file, &Some(changed_operation)));
}

#[test]
fn changed_sources_keep_the_document_and_draft_and_show_a_stale_notice() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    panel.enable_background_loading();

    key(&mut panel, &ctx, K::Char('S'));
    let draft = (0, std::path::PathBuf::from(FILE));
    assert!(panel.drafts.contains_key(&draft));
    key(&mut panel, &ctx, K::Enter);
    assert_eq!(panel.doc.as_ref().unwrap().file.path, Path::new(FILE));

    git(dir.path(), &["add", "--", FILE]).unwrap();
    panel.refresh(&ctx).unwrap();
    assert!(finish_background(&mut panel, &ctx).is_none());
    assert_eq!(panel.doc.as_ref().unwrap().file.path, Path::new(FILE));
    assert!(panel.doc_stale);
    assert!(panel.drafts.contains_key(&draft));
    let text = screen(&render(&mut panel, &ctx, 120, 24));
    assert!(text.contains("Sources changed"), "{text}");
    assert!(matches!(
        key(&mut panel, &ctx, K::Char('A')),
        Action::Notice(NoticeLevel::Warning, message) if message.contains("Sources changed")
    ));
}

#[test]
fn refresh_waits_for_editor_and_menus() {
    let dir = tempfile::tempdir().unwrap();
    repo(dir.path());
    let ctx = Context::discover(dir.path()).unwrap();
    let mut panel = panel(&ctx);
    key(&mut panel, &ctx, K::Enter);
    assert!(panel.refresh_allowed());
    key(&mut panel, &ctx, K::Char('i'));
    assert!(!panel.refresh_allowed());
    key(&mut panel, &ctx, K::Esc);
    assert!(panel.refresh_allowed());
    key(&mut panel, &ctx, K::Char('v'));
    assert!(!panel.refresh_allowed());
    key(&mut panel, &ctx, K::Esc);
    assert!(panel.refresh_allowed());
}
