use super::*;
use crate::{core::Repo, ui::Panel};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::mpsc;

fn context() -> Context {
    Context {
        workspace: "/history-test".into(),
        state_dir: "/history-test/state".into(),
        scope: [0, 1].into(),
        repos: ["one", "two"]
            .map(|name| Repo {
                path: format!("/history-test/{name}").into(),
                name: name.into(),
                branch: "main".into(),
                submodule: false,
                parent: None,
            })
            .into(),
    }
}

fn history(label: &str) -> History {
    let mut hist = History::default();
    hist.revs = (0..4)
        .map(|i| model::Rev {
            root: i % 2,
            oid: format!("{i:040x}"),
            subject: format!("{label} {i}"),
            ..Default::default()
        })
        .collect();
    hist.branches = vec![model::Branch {
        root: 0,
        name: "refs/heads/main".into(),
    }];
    hist
}

fn panel(c: &Context) -> LogPanel {
    let mut p = LogPanel {
        hist: history("cached"),
        ..Default::default()
    };
    p.rebuild_visible(None).unwrap();
    p.rebuild_refs(c);
    p.history_load.scope = Some(Scope::new(c));
    p.history_load.request = Some(Request::new(&p));
    p.enable_background_loading();
    p
}

type Reply = mpsc::Sender<Option<Result<Loaded>>>;

fn hold(p: &mut LogPanel) -> Reply {
    p.history_load.task.hold()
}

fn loaded(p: &LogPanel, label: &str) -> Option<Result<Loaded>> {
    Some(Loaded::build(history(label), Projection::new(p)))
}

fn key(p: &mut LogPanel, c: &Context, code: K) -> Action {
    p.event(&Event::Key(KeyEvent::new(code, Mods::NONE)), c)
}

#[test]
fn refresh_queues_reads_and_keeps_cached_rows_browsable_without_preview_reads() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    assert!(p.history_pending());
    assert_eq!(p.visible.len(), 4);
    assert!(p.preview_wakeup().is_none());

    assert!(matches!(key(&mut p, &c, K::Char('j')), Action::None));
    assert_eq!(p.cursor, 1);
    assert!(p.preview_wakeup().is_none());
    let selected = p.history_selection();

    reply.send(loaded(&p, "current")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(!p.history_pending());
    assert_eq!(p.history_selection(), selected);
    assert_eq!(p.cursor, 1);
    assert!(p.rev().unwrap().subject.starts_with("current"));
    assert!(p.preview_wakeup().is_some());
    assert!(p.require_current_history(&c).is_ok());
}

#[test]
fn compare_suspension_keeps_one_worker_until_obsolete_history_is_drained() {
    let context = context();
    let mut panel = panel(&context);
    panel.refresh(&context).unwrap();
    let reply = hold(&mut panel);
    let obsolete = loaded(&panel, "obsolete comparison");
    for _ in 0..5 {
        panel.suspend_history();
        assert!(panel.history_pending());
        assert!(!panel.history_load.task.needs_start());
    }
    reply.send(obsolete).unwrap();
    panel.drain_suspended_history();
    assert!(!panel.history_pending());
    assert!(panel.rev().unwrap().subject.starts_with("cached"));
    panel.refresh(&context).unwrap();
    let reply = hold(&mut panel);
    reply.send(loaded(&panel, "fresh comparison")).unwrap();
    assert!(panel.poll_history(&context).unwrap());
    assert!(panel.rev().unwrap().subject.starts_with("fresh comparison"));
}

#[test]
fn superseded_success_and_error_cannot_replace_the_latest_generation() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    let old_result = loaded(&p, "obsolete");
    p.filter.author = "latest author".into();
    p.reload(&c, false).unwrap();
    old.send(old_result).unwrap();
    // Drain the old generation before holding the replacement; completion order is explicit.
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    current.send(loaded(&p, "latest")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.rev().unwrap().subject.starts_with("latest"));

    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    p.refresh(&c).unwrap();
    old.send(Some(Err(anyhow::anyhow!("obsolete failure"))))
        .unwrap();
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    current.send(loaded(&p, "retry")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.history_status().is_none());
    assert!(p.rev().unwrap().subject.starts_with("retry"));
}

#[test]
fn roots_and_worktree_path_changes_reject_reads_even_without_explicit_refresh() {
    let mut c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    c.scope.remove(&0);
    assert!(p.take_history(&c).unwrap());
    assert!(p.visible.is_empty());
    assert!(p.rev().is_none());
    assert!(p.require_current_history(&c).is_err());
    old.send(Some(Err(anyhow::anyhow!("old Roots failed"))))
        .unwrap();
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    let mut hist = history("narrowed");
    hist.revs.retain(|r| c.scope.contains(&r.root));
    current
        .send(Some(Loaded::build(hist, Projection::new(&p))))
        .unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert_eq!(p.visible.len(), 2);
    assert!(p.visible.iter().all(|&i| p.hist.revs[i].root == 1));

    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    c.repos[1].path = "/replacement-worktree".into();
    assert!(p.take_history(&c).unwrap());
    assert!(p.visible.is_empty());
    old.send(loaded(&p, "wrong path")).unwrap();
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    current
        .send(Some(Loaded::build(History::default(), Projection::new(&p))))
        .unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.require_current_history(&c).is_ok());
}

#[test]
fn tab_restore_preserves_the_target_selection_and_never_saves_cached_selection() {
    let c = context();
    let mut p = panel(&c);
    p.select_row(3);
    let saved = p.history_selection();
    p.new_tab(&c).unwrap();
    assert!(p.history_selection().is_none());
    assert!(p.snapshot().selected.is_none());
    let old = hold(&mut p);
    p.switch_tab(0, &c).unwrap();
    assert_eq!(p.history_selection(), saved);
    assert!(p.views[1].selected.is_none());
    old.send(loaded(&p, "wrong tab")).unwrap();
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    current.send(loaded(&p, "restored")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert_eq!(p.cursor, 3);
    assert_eq!(p.history_selection(), saved);
}

#[test]
fn filters_reset_selection_and_a_later_refresh_does_not_reuse_the_cached_oid() {
    let c = context();
    let mut p = panel(&c);
    p.select_row(3);
    p.filter.date = Some(DateFilter::Last7Days);
    p.reload(&c, false).unwrap();
    assert!(p.history_selection().is_none());
    let old = hold(&mut p);
    p.refresh(&c).unwrap();
    assert!(p.history_selection().is_none());
    old.send(loaded(&p, "old date")).unwrap();
    assert!(!p.take_history(&c).unwrap());
    let current = hold(&mut p);
    current.send(loaded(&p, "date")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert_eq!(p.cursor, 0);
}

#[test]
fn text_and_local_roots_changed_during_loading_reproject_the_returned_page() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    let original = loaded(&p, "loaded");
    p.filter.query = "loaded 3".into();
    p.filter.roots = Some([1].into());
    p.rebuild_visible(None).unwrap();
    assert!(p.visible.is_empty());
    reply.send(original).unwrap();

    // The history page is reusable even though its old projection must not be installed.
    assert!(!p.take_history(&c).unwrap());
    assert!(p.history_load.project.is_some());
    let reply = hold(&mut p);
    // A real worker receives the page; invalid fixture worktrees would report errors if it
    // accidentally queried Git instead of reusing that page.
    let read = p.history_read(&c);
    reply
        .send(
            std::thread::spawn(move || read.run(|| false))
                .join()
                .unwrap(),
        )
        .unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert_eq!(p.visible.len(), 1);
    assert_eq!(p.rev().unwrap().root, 1);
    assert_eq!(p.rev().unwrap().subject, "loaded 3");
    assert!(p.hist.errors.is_empty());
}

#[test]
fn loading_disables_keyboard_mouse_and_confirmation_actions_but_keeps_navigation() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let _reply = hold(&mut p);
    for code in [K::Char('a'), K::F(2), K::Enter] {
        assert!(
            matches!(key(&mut p, &c, code), Action::Notice(NoticeLevel::Warning, message) if message.contains("History is not current"))
        );
        assert!(p.commit_dialog.is_none());
    }
    assert!(p.click(Hit::CommitActions, &c).is_err());
    p.hits = vec![(Rect::new(1, 1, 30, 1), Hit::Commit(2))];
    let action = p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Right),
            column: 2,
            row: 1,
            modifiers: Mods::NONE,
        }),
        &c,
    );
    assert!(matches!(action, Action::Notice(_, _)));
    assert_eq!(p.cursor, 2);
    assert!(p.commit_dialog.is_none());
    assert!(matches!(
        p.confirm(REVIEW_TOKEN, &c),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    assert!(matches!(key(&mut p, &c, K::Char('j')), Action::None));
    assert_eq!(p.cursor, 3);
    assert!(p.preview_wakeup().is_none());
}

#[test]
fn a_failed_reader_keeps_cache_unavailable_for_writes_until_a_successful_retry() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    drop(reply);
    assert!(p.poll_history(&c).is_err());
    assert!(!p.history_pending());
    assert_eq!(p.visible.len(), 4);
    assert!(p
        .history_status()
        .unwrap()
        .contains("Could not load history"));
    assert!(p.require_current_history(&c).is_err());
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    reply.send(loaded(&p, "retry")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.require_current_history(&c).is_ok());
}

#[test]
fn loading_indicator_keeps_cached_commit_rows_and_initial_loading_has_no_empty_state() {
    let c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let _reply = hold(&mut p);
    let mut terminal = Terminal::new(TestBackend::new(160, 24)).unwrap();
    terminal.draw(|f| p.draw(f, f.area(), &c)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Refreshing history"));
    assert!(text.contains("cached 0"));

    p.hist = History::default();
    p.visible.clear();
    terminal.draw(|f| p.draw(f, f.area(), &c)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Loading history"));
    assert!(!text.contains("No commits in this scope"));
}

#[test]
fn a_real_worker_reads_history_graph_and_optional_presentation_before_installation() {
    use crate::log::tests::{commit, git, init};
    let dir = init();
    commit(dir.path(), "file", "worker history");
    let before = git(dir.path(), &["status", "--porcelain"]);
    let c = Context::discover(dir.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    p.enable_background_loading();
    p.options
        .columns
        .iter_mut()
        .find(|(col, _)| *col == display::Column::Signature)
        .unwrap()
        .1 = true;
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    let read = p.history_read(&c);
    let ui_thread = std::thread::current().id();
    let result = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), ui_thread);
        read.run(|| false)
    })
    .join()
    .unwrap();
    reply.send(result).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.hist.errors.is_empty(), "{:?}", p.hist.errors);
    assert_eq!(p.rev().unwrap().subject, "worker history");
    assert!(p.hist.is_mine(p.rev().unwrap()));
    assert_eq!(p.hist.signature(p.rev().unwrap()), ("Unsigned", 'N'));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), before);
}

#[test]
fn refresh_discards_an_existing_review_and_confirmation_never_creates_its_ref() {
    use crate::log::{
        operations::{Kind, Review, Target},
        tests::{commit, git, init},
    };
    let dir = init();
    commit(dir.path(), "file", "review target");
    let c = Context::discover(dir.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let (review, _) = Review::prepare(
        &c,
        Target::new(&c, p.rev().unwrap()).unwrap(),
        Kind::Branch,
        "must-not-be-created".into(),
    )
    .unwrap();
    p.commit_review = Some(review);
    let before = git(dir.path(), &["for-each-ref", "--format=%(refname)"]);
    p.enable_background_loading();
    p.refresh(&c).unwrap();
    let reply = hold(&mut p);
    assert!(p.commit_review.is_none());
    assert!(matches!(
        p.confirm(REVIEW_TOKEN, &c),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    let read = p.history_read(&c);
    reply
        .send(
            std::thread::spawn(move || read.run(|| false))
                .join()
                .unwrap(),
        )
        .unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(matches!(p.confirm(REVIEW_TOKEN, &c), Action::None));
    assert_eq!(
        git(dir.path(), &["for-each-ref", "--format=%(refname)"]),
        before
    );
}

#[test]
fn a_cancelled_history_read_stops_before_querying_any_worktree() {
    let c = context();
    // These worktrees deliberately do not exist; an accidental Git read produces an error.
    let hist = model::load_with_cancel(&c, &Filter::default(), 160, &|| true);
    assert!(hist.revs.is_empty());
    assert!(hist.branches.is_empty());
    assert!(hist.errors.is_empty());
}

#[test]
fn every_reload_entry_queues_history_and_keeps_the_existing_page() {
    let c = context();
    for entry in [
        "refresh",
        "author",
        "date",
        "path",
        "branch",
        "signature",
        "cherry",
        "new tab",
        "restore tab",
        "more",
        "reset",
    ] {
        let mut p = panel(&c);
        match entry {
            "refresh" => p.click(Hit::Refresh, &c).unwrap(),
            "author" => {
                p.editor = Some((Field::Author, "someone".into()));
                p.apply(&c).unwrap();
            }
            "date" => {
                p.editor = Some((Field::Date, String::new()));
                p.accept_choice(2, &c).unwrap();
            }
            "path" => {
                p.editor = Some((Field::Path, "./src".into()));
                p.apply(&c).unwrap();
            }
            "branch" => {
                p.refs.reset_cursor();
                p.activate_ref(&c).unwrap();
            }
            "signature" => {
                p.view_menu = Some(display::Menu::default());
                p.view_menu_hit(display::MenuHit::Column(display::Column::Signature), &c)
                    .unwrap();
            }
            "cherry" => {
                p.filter.branch = Some("refs/heads/other".into());
                p.toggle_view_item(Item::NotCherryPicked, &c).unwrap();
            }
            "new tab" => p.new_tab(&c).unwrap(),
            "restore tab" => {
                p.views.push(ViewState::default());
                p.switch_tab(1, &c).unwrap();
            }
            "more" => p.click(Hit::More, &c).unwrap(),
            "reset" => p.click(Hit::Reset, &c).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            p.history_load.task.needs_start(),
            "{entry} did not queue history"
        );
        assert_eq!(
            p.visible.len(),
            4,
            "{entry} replaced the cached page synchronously"
        );
        assert!(
            p.preview_wakeup().is_none(),
            "{entry} queued a stale preview"
        );
        assert!(
            p.require_current_history(&c).is_err(),
            "{entry} allowed a stale action"
        );
    }
}

#[test]
fn delivered_results_are_checked_against_current_filters_and_workspace_before_installation() {
    let mut c = context();
    let mut p = panel(&c);
    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    old.send(loaded(&p, "wrong filter")).unwrap();
    p.filter.author = "changed before polling".into();
    assert!(p.take_history(&c).unwrap());
    assert!(p.rev().unwrap().subject.starts_with("cached"));
    let reply = hold(&mut p);
    reply.send(loaded(&p, "current filter")).unwrap();
    assert!(p.poll_history(&c).unwrap());

    p.refresh(&c).unwrap();
    let old = hold(&mut p);
    old.send(loaded(&p, "wrong workspace")).unwrap();
    c.workspace = "/another-workspace".into();
    assert!(p.take_history(&c).unwrap());
    assert!(p.visible.is_empty());
    let reply = hold(&mut p);
    reply.send(loaded(&p, "current workspace")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.rev().unwrap().subject.starts_with("current workspace"));
}

#[test]
fn closing_and_recreating_the_same_tab_index_still_rejects_the_old_generation() {
    let c = context();
    let mut p = panel(&c);
    p.new_tab(&c).unwrap();
    let old = hold(&mut p);
    let old_result = loaded(&p, "closed tab");
    p.close_tab(&c).unwrap();
    p.new_tab(&c).unwrap();
    assert_eq!(p.view_index, 1);
    old.send(old_result).unwrap();
    assert!(!p.take_history(&c).unwrap());
    let reply = hold(&mut p);
    reply.send(loaded(&p, "replacement tab")).unwrap();
    assert!(p.poll_history(&c).unwrap());
    assert!(p.rev().unwrap().subject.starts_with("replacement tab"));
}
