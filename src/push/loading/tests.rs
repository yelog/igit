use super::*;
use std::sync::mpsc;

fn fixture() -> (tempfile::TempDir, Context, PushPanel) {
    let dir = tempfile::tempdir().unwrap();
    crate::push::tests::make(dir.path(), "app");
    crate::push::tests::make(dir.path(), "tools");
    let c = Context::discover(dir.path()).unwrap();
    let mut panel = PushPanel::default();
    panel.refresh(&c).unwrap();
    assert!(panel.entries.is_empty(), "refresh must only queue reads");
    assert!(panel.load.loading() && !panel.busy());
    panel.finish_loading(&c);
    (dir, c, panel)
}

fn hold_entries(panel: &mut PushPanel) -> mpsc::Sender<Vec<Entry>> {
    panel.load.hold()
}

#[test]
fn refresh_keeps_cached_rows_but_blocks_remote_actions_and_rejects_old_roots() {
    let (_dir, mut c, mut panel) = fixture();
    let old = panel.entries.clone();
    panel.refresh(&c).unwrap();
    let release = hold_entries(&mut panel);
    assert_eq!(panel.entries, old);
    assert!(matches!(panel.review(&c, false), Action::Notice(_)));
    assert!(matches!(panel.review(&c, true), Action::Notice(_)));
    assert!(matches!(panel.start(&c, true), Action::Notice(_)));
    assert!(!panel.busy());

    // Roots may change while Push is inactive, without calling its refresh first.
    c.scope = BTreeSet::from([1]);
    panel.poll_loading(&c).unwrap();
    assert!(panel.entries.iter().all(|e| e.idx == 1));
    release.send(old).unwrap();
    panel.finish_loading(&c);
    assert_eq!(panel.entries.len(), 1);
    assert_eq!(panel.entries[0].idx, 1);
    assert_eq!(
        panel.checked,
        BTreeSet::from([0]),
        "hidden checks must not select another root"
    );
    assert!(matches!(panel.review(&c, false), Action::Notice(_)));

    panel.reseed = Some(vec![1]);
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert_eq!(panel.checked, BTreeSet::from([1]));
    assert_eq!(panel.selected(), Some((1, None)));
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
}

#[test]
fn target_change_during_load_discards_the_previous_target() {
    let (_dir, c, mut panel) = fixture();
    let old = panel.entries.clone();
    panel.refresh(&c).unwrap();
    let release = hold_entries(&mut panel);
    panel.targets.insert(
        0,
        Target {
            remote: "origin".into(),
            dst: "new-target".into(),
        },
    );
    panel.refresh(&c).unwrap();
    release.send(old).unwrap();
    panel.finish_loading(&c);
    let e = panel.entries.iter().find(|e| e.idx == 0).unwrap();
    assert_eq!(e.t.dst, "new-target");
    assert!(e.tracking.is_none());
    assert!(panel.preview.files.iter().all(|f| f.status == b'A'));
}

#[test]
fn refresh_preserves_current_commit_by_oid_and_user_checks() {
    let (_dir, mut c, mut panel) = fixture();
    panel.pos = 1;
    panel.inspect(&c);
    panel.finish_loading(&c);
    let selected = panel.selected_identity();
    panel.checked = BTreeSet::from([1]);
    git(
        &c.repos[0].path,
        &["commit", "--allow-empty", "-qm", "new newest commit"],
    )
    .unwrap();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert_eq!(panel.selected_identity(), selected);
    assert_eq!(
        panel.pos, 2,
        "new commits must not silently change preview identity"
    );
    assert_eq!(panel.checked, BTreeSet::from([1]));

    c.scope.remove(&1);
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert_eq!(
        panel.selected_identity(),
        selected,
        "changing other Roots keeps the current commit"
    );
    c.scope.insert(1);
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);

    // Use the user's latest cursor, not the cursor captured when a scan started.
    panel.refresh(&c).unwrap();
    panel.pos = panel.rows().iter().position(|r| *r == (1, None)).unwrap();
    panel.inspect(&c);
    panel.finish_loading(&c);
    assert_eq!(panel.selected(), Some((1, None)));
}

#[test]
fn unchanged_refresh_retains_file_diff_scroll_and_checks() {
    let (_dir, c, mut panel) = fixture();
    let row = panel
        .preview
        .tree
        .visible
        .iter()
        .position(|&n| matches!(panel.preview.tree.nodes[n].kind, Kind::File(_)))
        .unwrap();
    panel.activate_file(row, false, &c);
    panel.finish_loading(&c);
    panel.detail_off = 7;
    let before = (
        panel.preview.body.clone(),
        panel.preview.tree.cursor,
        panel.checked.clone(),
    );
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert_eq!(
        (
            panel.preview.body.clone(),
            panel.preview.tree.cursor,
            panel.checked.clone()
        ),
        before
    );
    assert_eq!(panel.detail_off, 7);
}

#[test]
fn stale_file_and_patch_reads_cannot_replace_the_latest_selection() {
    let (_dir, c, mut panel) = fixture();
    panel.preview_load.request();
    let send = panel.preview_load.hold();
    panel.pos = panel.rows().iter().position(|r| *r == (1, None)).unwrap();
    panel.inspect(&c);
    send.send(preview::Loaded::Files {
        files: vec![],
        summary: "obsolete files".into(),
    })
    .ok()
    .unwrap();
    panel.finish_loading(&c);
    assert!(!panel.preview.body.contains("obsolete"));
    assert!(!panel.preview.files.is_empty());

    let row = panel
        .preview
        .tree
        .visible
        .iter()
        .position(|&n| matches!(panel.preview.tree.nodes[n].kind, Kind::File(_)))
        .unwrap();
    panel.activate_file(row, false, &c);
    let send = panel.preview_load.hold();
    panel.preview.tree.cursor = 0; // Repository summary, while the patch is still reading.
    panel.inspect_file(&c);
    send.send(preview::Loaded::Patch {
        body: "obsolete diff".into(),
        is_patch: true,
    })
    .ok()
    .unwrap();
    panel.finish_loading(&c);
    assert_eq!(panel.preview.body, panel.preview.summary);
    assert!(!panel.preview.is_patch);
}

#[test]
fn batched_config_keeps_remote_precedence_case_and_last_value() {
    let (_dir, c, _) = fixture();
    let repo = &c.repos[0];
    let g = |args: &[&str]| git(&repo.path, args).unwrap();
    g(&["branch", "-m", "Topic.Case"]);
    assert_eq!(
        get_t(repo, "Topic.Case"),
        Target {
            remote: "origin".into(),
            dst: "main".into()
        }
    );
    g(&["remote", "add", "backup", "unused"]);
    g(&["config", "remote.pushDefault", "backup"]);
    assert_eq!(
        get_t(repo, "Topic.Case"),
        Target {
            remote: "backup".into(),
            dst: "Topic.Case".into()
        }
    );
    g(&["config", "branch.Topic.Case.pushRemote", "backup"]);
    g(&["config", "--add", "branch.Topic.Case.pushRemote", "origin"]);
    assert_eq!(
        get_t(repo, "Topic.Case"),
        Target {
            remote: "origin".into(),
            dst: "main".into()
        }
    );
    g(&["config", "--unset-all", "branch.Topic.Case.pushRemote"]);
    g(&["config", "--unset", "remote.pushDefault"]);
    g(&["config", "--unset", "branch.Topic.Case.remote"]);
    assert_eq!(get_t(repo, "Topic.Case").remote, "origin");
    g(&["remote", "remove", "origin"]);
    assert_eq!(get_t(repo, "Topic.Case").remote, "backup");
}

#[test]
fn entry_with_no_outgoing_commits_has_no_history_or_preview() {
    let (_dir, c, mut panel) = fixture();
    git(&c.repos[0].path, &["push", "-q", "origin", "main"]).unwrap();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    let e = panel.entries.iter().find(|e| e.idx == 0).unwrap();
    assert_eq!(e.n, 0);
    assert!(e.logs.is_empty() && e.error.is_empty());
    assert!(panel.preview.files.is_empty() && panel.preview.body.is_empty());
}

#[test]
fn failed_reader_retains_snapshot_disables_writes_and_can_refresh_again() {
    let (_dir, c, mut panel) = fixture();
    let before = panel.entries.clone();
    panel.refresh(&c).unwrap();
    drop(hold_entries(&mut panel));
    assert!(panel.poll_loading(&c).is_err());
    assert_eq!(panel.entries, before);
    assert!(panel.read_status().unwrap().contains("refresh to retry"));
    assert!(matches!(panel.review(&c, false), Action::Notice(_)));
    assert!(matches!(panel.start(&c, true), Action::Notice(_)));
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert!(panel.read_status().is_none());
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
}

#[test]
fn loading_controls_fit_and_disable_mouse_writes_in_all_icon_modes() {
    use ratatui::{backend::TestBackend, Terminal};
    let (_dir, c, mut panel) = fixture();
    panel.refresh(&c).unwrap();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (w, h) in [(44, 12), (80, 24), (120, 36)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| panel.draw(f, f.area(), &c)).unwrap();
            assert!(!panel.hits.iter().any(|(_, hit)| matches!(
                hit,
                Hit::Send | Hit::Fetch | Hit::Retry | Hit::EditTarget
            )));
            assert!(panel
                .hits
                .iter()
                .any(|(r, hit)| matches!(hit, Hit::Cancel) && !r.is_empty()));
        }
    }
}
