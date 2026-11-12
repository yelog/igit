use super::*;

fn switch_at(repo: &Path, name: &str, date: &str) {
    let output = crate::core::command(repo)
        .args(["switch", "--no-guess", name])
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn leaves(panel: &BranchPanel, remote: bool) -> Vec<String> {
    panel
        .tree
        .rows
        .iter()
        .filter_map(|row| match &row.kind {
            Kind::Branch(branch) if branch.remote == remote => Some(branch.name.clone()),
            _ => None,
        })
        .collect()
}

fn set_sort(panel: &mut BranchPanel, sort: Sort) {
    let repo = panel.repository_index().map(|index| &panel.repos[index]);
    panel.tree.set_sort(sort, repo);
}

#[test]
fn default_sort_places_the_previous_branch_below_the_current_branch() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in ["client-plugin-batch-update", "develop", "unused"] {
        run(repo, &["branch", name]);
    }
    for (name, date) in [
        ("client-plugin-batch-update", "2020-01-01T00:00:00Z"),
        ("develop", "2020-01-02T00:00:00Z"),
        ("main", "2020-01-03T00:00:00Z"),
    ] {
        switch_at(repo, name, date);
    }
    reload(&mut panel, &context);
    assert_eq!(panel.tree.sort(), Sort::RecentlyUsed);
    assert_eq!(
        leaves(&panel, false),
        ["main", "develop", "client-plugin-batch-update", "unused"]
    );
}

#[test]
fn consecutive_checkouts_update_usage_without_reloading_or_changing_name_sort() {
    for grouped in [true, false] {
        let (_temp, context, mut panel) = fixture();
        let repo = &context.repos[0].path;
        for name in ["client-plugin-batch-update", "develop", "unused"] {
            run(repo, &["branch", name]);
        }
        switch_at(repo, "client-plugin-batch-update", "2020-01-01T00:00:00Z");
        switch_at(repo, "main", "2020-01-02T00:00:00Z");
        reload(&mut panel, &context);
        set_sort(&mut panel, Sort::RecentlyUsed);
        if !grouped {
            panel.tree.toggle_grouping(panel.repos.first());
        }
        let other_usage = panel.repos[1].metadata.last_used.clone();
        let remote = leaves(&panel, true);
        for name in ["develop", "main"] {
            select(&mut panel, 0, &format!("refs/heads/{name}"));
            assert!(matches!(
                key(&mut panel, &context, K::Enter),
                Action::Notice(NoticeLevel::Info, _)
            ));
            finish_checkout(&mut panel);
            assert!(!panel.reads_pending());
            assert_eq!(panel.selection().unwrap().1.name, name);
        }
        assert_eq!(
            leaves(&panel, false),
            ["main", "develop", "client-plugin-batch-update", "unused"]
        );
        assert_eq!(panel.repos[1].metadata.last_used, other_usage);
        assert_eq!(leaves(&panel, true), remote);
        let (_, metadata) = Snapshot::read_for_display(&context.repos[0]).unwrap();
        assert_eq!(panel.repos[0].metadata.last_used, metadata.last_used);

        set_sort(&mut panel, Sort::Name);
        select(&mut panel, 0, "refs/heads/unused");
        key(&mut panel, &context, K::Enter);
        finish_checkout(&mut panel);
        select(&mut panel, 0, "refs/heads/main");
        key(&mut panel, &context, K::Enter);
        finish_checkout(&mut panel);
        assert_eq!(panel.tree.sort(), Sort::Name);
        assert_eq!(
            leaves(&panel, false),
            ["main", "client-plugin-batch-update", "develop", "unused"]
        );
        set_sort(&mut panel, Sort::RecentlyUsed);
        let expected = ["main", "unused", "develop", "client-plugin-batch-update"];
        assert_eq!(leaves(&panel, false), expected);
        assert!(!panel.reads_pending());
        reload(&mut panel, &context);
        assert_eq!(leaves(&panel, false), expected);
    }
}

#[test]
fn checkout_completion_keeps_same_second_usage_and_group_folds_current() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in ["aa/older", "zz/recent", "folded/unused"] {
        run(repo, &["branch", name]);
    }
    let date = "2020-01-01T00:00:00Z";
    switch_at(repo, "aa/older", date);
    switch_at(repo, "main", date);
    reload(&mut panel, &context);
    set_sort(&mut panel, Sort::RecentlyUsed);
    let folded = "0:refs/heads/folded/";
    panel.tree.cursor = folder_index(&panel, folded);
    key(&mut panel, &context, K::Char('h'));

    for name in ["zz/recent", "main"] {
        let prepared = review(
            &context,
            &format!("refs/heads/{name}"),
            Operation::Checkout,
            "",
        );
        switch_at(repo, name, date);
        let done = prepared.checkout_outcome().unwrap();
        assert!(panel.checkout_completed(&done));
        assert_eq!(panel.tree.rows[panel.tree.cursor].id, folded);
        assert!(!panel.reads_pending());
    }
    assert_eq!(leaves(&panel, false), ["main", "zz/recent", "aa/older"]);
    let usage = &panel.repos[0].metadata.last_used;
    assert_eq!(
        usage["refs/heads/zz/recent"].timestamp,
        usage["refs/heads/aa/older"].timestamp
    );
    assert!(usage["refs/heads/zz/recent"] > usage["refs/heads/aa/older"]);
    assert!(matches!(
        panel.tree.rows[folder_index(&panel, folded)].kind,
        Kind::Folder { open: false, .. }
    ));
}

#[test]
fn tracking_checkout_records_the_new_local_branch_without_a_ref_scan() {
    let (_temp, context, mut panel) = fixture();
    tracking(&context, "main", "zz/tracked");
    run(&context.repos[0].path, &["branch", "alpha"]);
    reload(&mut panel, &context);
    set_sort(&mut panel, Sort::RecentlyUsed);
    panel.tree.toggle_grouping(panel.repos.first());
    select(&mut panel, 0, "refs/remotes/origin/zz/tracked");
    key(&mut panel, &context, K::Enter);
    key(&mut panel, &context, K::Enter);
    finish_checkout(&mut panel);
    assert!(panel.repos[0]
        .metadata
        .last_used
        .contains_key("refs/heads/zz/tracked"));
    select(&mut panel, 0, "refs/heads/main");
    key(&mut panel, &context, K::Enter);
    finish_checkout(&mut panel);
    assert_eq!(leaves(&panel, false), ["main", "zz/tracked", "alpha"]);
    assert!(!panel.reads_pending());
}

#[test]
fn recent_usage_reads_switch_times_and_same_second_order_across_rename() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in ["alpha", "zulu", "unused"] {
        run(repo, &["branch", name]);
    }
    switch_at(repo, "alpha", "2020-01-01T00:00:00Z");
    switch_at(repo, "zulu", "2020-01-01T00:00:00Z");
    run(repo, &["branch", "-m", "feature/中文"]);
    switch_at(repo, "main", "2020-01-02T00:00:00Z");
    run(repo, &["commit", "--allow-empty", "-qm", "later commit"]);
    let reviewed = Snapshot::read(&context.repos[0]).unwrap();
    let before = state(repo);
    let reflog = run(repo, &["reflog", "show", "--format=%gD %gs", "HEAD"]);
    reload(&mut panel, &context);
    let used = &panel.repos[0].metadata.last_used;
    assert_eq!(used.len(), 3);
    assert_eq!(used["refs/heads/alpha"].timestamp, 1_577_836_800);
    assert_eq!(used["refs/heads/feature/中文"].timestamp, 1_577_836_800);
    assert_eq!(used["refs/heads/main"].timestamp, 1_577_923_200);
    assert!(used["refs/heads/feature/中文"] > used["refs/heads/alpha"]);
    assert_eq!(panel.repos[0].state.as_ref().unwrap(), &reviewed);
    set_sort(&mut panel, Sort::RecentlyUsed);
    assert_eq!(
        leaves(&panel, false),
        ["main", "feature/中文", "alpha", "unused"]
    );
    assert_eq!(state(repo), before);
    assert_eq!(
        run(repo, &["reflog", "show", "--format=%gD %gs", "HEAD"]),
        reflog
    );
    assert!(!context.state_dir.exists());
}

#[test]
fn recent_usage_is_scoped_to_each_worktrees_head_reflog() {
    let (temp, context, _panel) = fixture();
    let repo = &context.repos[0];
    for name in ["parent-only", "linked-only"] {
        run(&repo.path, &["branch", name]);
    }
    switch_at(&repo.path, "parent-only", "2020-01-01T00:00:00Z");
    switch_at(&repo.path, "main", "2020-01-02T00:00:00Z");
    let linked = temp.path().join("linked");
    run(
        &repo.path,
        &[
            "worktree",
            "add",
            "-b",
            "linked-start",
            linked.to_str().unwrap(),
        ],
    );
    switch_at(&linked, "linked-only", "2020-01-03T00:00:00Z");
    let linked_repo = crate::core::Repo {
        path: linked,
        ..repo.clone()
    };
    let (_, parent) = Snapshot::read_for_display(repo).unwrap();
    let (_, child) = Snapshot::read_for_display(&linked_repo).unwrap();
    assert!(parent.last_used.contains_key("refs/heads/parent-only"));
    assert!(!parent.last_used.contains_key("refs/heads/linked-only"));
    assert!(child.last_used.contains_key("refs/heads/linked-only"));
    assert!(!child.last_used.contains_key("refs/heads/parent-only"));
}

#[test]
fn recent_sort_groups_by_latest_descendant_and_preserves_remote_order() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in [
        "aa/default",
        "aa/unknown",
        "bb/older",
        "zz/newest",
        "zz/unused",
        "zz/deep/old",
        "zz/deep/new",
        "unused-b",
        "unused-a",
    ] {
        run(repo, &["branch", name]);
    }
    tracking(&context, "aa/default", "aa/default");
    run(repo, &["update-ref", "refs/remotes/origin/zzz", "HEAD"]);
    run(
        repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/aa/default",
        ],
    );
    for (name, date) in [
        ("bb/older", "2020-01-01T00:00:00Z"),
        ("zz/deep/old", "2020-01-02T00:00:00Z"),
        ("zz/deep/new", "2020-01-03T00:00:00Z"),
        ("zz/newest", "2020-01-04T00:00:00Z"),
        ("main", "2020-01-05T00:00:00Z"),
    ] {
        switch_at(repo, name, date);
    }
    reload(&mut panel, &context);
    let remote = leaves(&panel, true);
    select(&mut panel, 0, "refs/heads/bb/older");
    set_sort(&mut panel, Sort::RecentlyUsed);
    let expected = [
        "main",
        "zz/newest",
        "zz/deep/new",
        "zz/deep/old",
        "zz/unused",
        "bb/older",
        "aa/default",
        "aa/unknown",
        "unused-a",
        "unused-b",
    ];
    assert_eq!(leaves(&panel, false), expected);
    assert_eq!(panel.selection().unwrap().1.name, "bb/older");
    assert_eq!(leaves(&panel, true), remote);
    panel.tree.cursor = folder_index(&panel, "0:refs/heads/zz/");
    key(&mut panel, &context, K::Char('h'));
    reload(&mut panel, &context);
    assert!(matches!(
        panel.tree.rows[folder_index(&panel, "0:refs/heads/zz/")].kind,
        Kind::Folder { open: false, .. }
    ));
    let unfolded = panel.tree.unfolded_rows(panel.repos.first()).unwrap();
    let unfolded: Vec<_> = unfolded
        .iter()
        .filter_map(|row| match &row.kind {
            Kind::Branch(branch) if !branch.remote => Some(branch.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(unfolded, expected);
    panel.tree.toggle_grouping(panel.repos.first());
    assert_eq!(
        leaves(&panel, false),
        [
            "main",
            "zz/newest",
            "zz/deep/new",
            "zz/deep/old",
            "bb/older",
            "aa/default",
            "aa/unknown",
            "unused-a",
            "unused-b",
            "zz/unused",
        ]
    );
    set_sort(&mut panel, Sort::Name);
    assert_eq!(
        leaves(&panel, false),
        [
            "main",
            "aa/default",
            "aa/unknown",
            "bb/older",
            "unused-a",
            "unused-b",
            "zz/deep/new",
            "zz/deep/old",
            "zz/newest",
            "zz/unused",
        ]
    );
}

#[test]
fn missing_history_falls_back_to_names_with_current_branch_first() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in ["z/current", "aaa", "bbb"] {
        run(repo, &["branch", name]);
    }
    switch_at(repo, "z/current", "2020-01-01T00:00:00Z");
    run(repo, &["reflog", "expire", "--expire=all", "--all"]);
    reload(&mut panel, &context);
    assert!(panel.repos[0].metadata.last_used.is_empty());
    set_sort(&mut panel, Sort::RecentlyUsed);
    assert_eq!(leaves(&panel, false), ["z/current", "aaa", "bbb", "main"]);
    panel.tree.toggle_grouping(panel.repos.first());
    assert_eq!(leaves(&panel, false), ["z/current", "aaa", "bbb", "main"]);
    run(repo, &["switch", "--detach", "HEAD"]);
    reload(&mut panel, &context);
    assert!(panel.repos[0].metadata.last_used.is_empty());
    assert_eq!(leaves(&panel, false), ["aaa", "bbb", "main", "z/current"]);
    fs::remove_file(repo.join(".git/logs/HEAD")).unwrap();
    reload(&mut panel, &context);
    assert!(panel.repos[0].state.is_ok());
    assert!(panel.repos[0].metadata.last_used.is_empty());
    let missing = crate::core::Repo {
        path: repo.join("missing"),
        ..context.repos[0].clone()
    };
    assert!(recent::read(
        &missing.path,
        panel.repos[0]
            .state
            .as_ref()
            .unwrap()
            .branches
            .keys()
            .map(String::as_str)
    )
    .is_empty());
}

#[test]
fn sort_menu_preserves_selection_and_latest_mode_during_background_refresh() {
    let (_temp, mut context, mut panel) = fixture();
    let repo = context.repos[0].path.clone();
    for name in ["alpha", "zulu"] {
        run(&repo, &["branch", name]);
    }
    switch_at(&repo, "zulu", "2020-01-01T00:00:00Z");
    switch_at(&repo, "main", "2020-01-02T00:00:00Z");
    reload(&mut panel, &context);
    select(&mut panel, 0, "refs/heads/alpha");
    let before = state(&repo);
    panel.refresh(&context).unwrap();
    let release = panel.hold_loading(&context);
    key(&mut panel, &context, K::Char('o'));
    key(&mut panel, &context, K::Char('j'));
    key(&mut panel, &context, K::Down);
    key(&mut panel, &context, K::Enter);
    assert_eq!(panel.tree.sort(), Sort::RecentlyUsed);
    assert!(screen(&mut panel, &context, 44, 12).contains("[x] Recently Used"));
    key(&mut panel, &context, K::Esc);
    release();
    panel.finish_loading(&context);
    assert_eq!(leaves(&panel, false), ["main", "zulu", "alpha"]);
    assert_eq!(panel.selection().unwrap().1.name, "alpha");
    panel.select_repository(1);
    assert_eq!(panel.tree.sort(), Sort::RecentlyUsed);
    panel.select_repository(0);
    assert_eq!(panel.selection().unwrap().1.name, "alpha");
    context.scope.remove(&0);
    reload(&mut panel, &context);
    context.scope.insert(0);
    reload(&mut panel, &context);
    panel.select_repository(0);
    assert_eq!(panel.tree.sort(), Sort::RecentlyUsed);
    assert_eq!(leaves(&panel, false), ["main", "zulu", "alpha"]);

    key(&mut panel, &context, K::Char('o'));
    for (width, height) in [(120, 36), (80, 25), (44, 12), (28, 6)] {
        screen(&mut panel, &context, width, height);
        let rect = hit_rect(&panel, |hit| matches!(hit, Hit::Sort(Sort::Name)));
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: M::Down(MouseButton::Left),
                column: rect.x + 1,
                row: rect.y,
                modifiers: Mods::NONE,
            }),
            &context,
        );
        assert_eq!(panel.tree.sort(), Sort::Name);
        assert_eq!(leaves(&panel, false), ["main", "alpha", "zulu"]);
        key(&mut panel, &context, K::Char('j'));
        key(&mut panel, &context, K::Char(' '));
        assert_eq!(panel.tree.sort(), Sort::RecentlyUsed);
    }
    key(&mut panel, &context, K::Esc);
    assert_eq!(state(&repo), before);
    assert!(!context.state_dir.exists());
}

#[test]
fn recent_sort_refreshes_after_external_and_panel_switches() {
    let (_temp, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    for name in ["alpha", "zulu"] {
        run(repo, &["branch", name]);
    }
    reload(&mut panel, &context);
    set_sort(&mut panel, Sort::RecentlyUsed);
    switch_at(repo, "zulu", "2020-01-01T00:00:00Z");
    reload(&mut panel, &context);
    assert_eq!(leaves(&panel, false), ["zulu", "alpha", "main"]);
    select(&mut panel, 0, "refs/heads/alpha");
    assert!(matches!(
        key(&mut panel, &context, K::Enter),
        Action::Notice(NoticeLevel::Info, _)
    ));
    finish_checkout(&mut panel);
    reload(&mut panel, &context);
    assert_eq!(leaves(&panel, false), ["alpha", "zulu", "main"]);
    assert_eq!(panel.selection().unwrap().1.name, "alpha");
}
