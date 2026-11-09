use super::*;
use crate::ui::{
    icons::Icon,
    idea::{BLUE, LOCAL_REF, TAG},
};

fn commit_on(repo: &Path, parent: &str, message: &str) -> String {
    run(
        repo,
        &["commit-tree", "HEAD^{tree}", "-p", parent, "-m", message],
    )
}

fn diverged_fixture() -> (tempfile::TempDir, Context, BranchPanel) {
    let (temp, c, mut p) = fixture();
    let repo = &c.repos[0].path;
    tracking(&c, "main", "release");
    let base = run(repo, &["rev-parse", "HEAD"]);
    let local = commit_on(repo, &base, "local one");
    let local = commit_on(repo, &local, "local two");
    let remote = commit_on(repo, &base, "remote one");
    let remote = commit_on(repo, &remote, "remote two");
    let remote = commit_on(repo, &remote, "remote three");
    run(repo, &["update-ref", "refs/heads/main", &local]);
    run(
        repo,
        &["update-ref", "refs/remotes/origin/release", &remote],
    );
    // A same-name remote ref must not override the configured upstream.
    run(repo, &["update-ref", "refs/remotes/origin/main", &local]);
    run(repo, &["branch", "ahead", &local]);
    run(repo, &["update-ref", "refs/remotes/origin/base", &base]);
    run(repo, &["branch", "--set-upstream-to=origin/base", "ahead"]);
    run(repo, &["branch", "behind", &base]);
    run(
        repo,
        &["branch", "--set-upstream-to=origin/release", "behind"],
    );
    run(repo, &["branch", "synced", &remote]);
    run(
        repo,
        &["branch", "--set-upstream-to=origin/release", "synced"],
    );
    run(repo, &["branch", "untracked", &local]);
    run(repo, &["branch", "gone", &local]);
    run(repo, &["update-ref", "refs/remotes/origin/deleted", &base]);
    run(
        repo,
        &["branch", "--set-upstream-to=origin/deleted", "gone"],
    );
    run(repo, &["update-ref", "-d", "refs/remotes/origin/deleted"]);
    reload(&mut p, &c);
    (temp, c, p)
}

fn row_text(p: &mut BranchPanel, c: &Context, reference: &str, width: u16, height: u16) -> String {
    select(p, p.repo_root.unwrap(), reference);
    let t = render(p, c, width, height);
    let rect = hit_rect(p, |hit| matches!(hit, Hit::Row(i) if *i == p.tree.cursor));
    row_cells(&t, rect)
}

fn repository_text(t: &Terminal<TestBackend>, p: &BranchPanel, index: usize) -> String {
    let rect = hit_rect(p, |hit| matches!(hit, Hit::Repository(i) if *i == index));
    row_cells(t, rect)
}

#[test]
fn branch_tracking_counts_follow_each_configured_upstream_and_refresh_without_writes() {
    let (_temp, c, mut p) = diverged_fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("file"), "staged\n").unwrap();
    fs::write(repo.join("other"), "also staged\n").unwrap();
    run(repo, &["add", "."]);
    fs::write(repo.join("file"), "unstaged\n").unwrap();
    let before = state(repo);
    let web = &c.repos[1].path;
    run(web, &["switch", "-c", "web-live"]);
    let other = state(web);
    reload(&mut p, &c);
    let cached = p.repository().unwrap();
    let snapshot = cached.state.as_ref().unwrap();
    assert_eq!(*snapshot, Snapshot::read(&c.repos[0]).unwrap());
    for (name, counts) in [
        ("main", (2, 3)),
        ("ahead", (2, 0)),
        ("behind", (0, 3)),
        ("synced", (0, 0)),
    ] {
        assert_eq!(
            cached.metadata.tracking.get(&format!("refs/heads/{name}")),
            Some(&counts)
        );
    }
    assert!(!cached.metadata.tracking.contains_key("refs/heads/gone"));
    assert!(!cached
        .metadata
        .tracking
        .contains_key("refs/heads/untracked"));
    assert!(!cached
        .metadata
        .tracking
        .keys()
        .any(|name| name.starts_with("refs/remotes/")));
    for (name, ahead, behind) in [
        ("main", Some("^2"), Some("v3")),
        ("ahead", Some("^2"), None),
        ("behind", None, Some("v3")),
        ("synced", None, None),
        ("untracked", None, None),
        ("gone", None, None),
    ] {
        let reference = format!("refs/heads/{name}");
        let row = row_text(&mut p, &c, &reference, 120, 36);
        for count in ["^2", "v3"] {
            assert_eq!(
                row.contains(count),
                ahead == Some(count) || behind == Some(count),
                "{name}: {row}"
            );
        }
        assert!(!row.contains("^0") && !row.contains("v0"), "{row}");
        if name == "gone" {
            assert!(row.contains("[gone]"), "{row}");
        }
    }
    p.select_repository(1);
    let row = row_text(&mut p, &c, "refs/heads/web-live", 120, 36);
    assert!(!row.contains("^2") && !row.contains("v3"), "{row}");
    let t = render(&mut p, &c, 120, 36);
    let api = repository_text(&t, &p, 0);
    let web_row = repository_text(&t, &p, 1);
    assert!(
        api.starts_with(" R api") && api.ends_with("main ^2 v3 >"),
        "{api}"
    );
    assert!(
        web_row.starts_with(" R web") && web_row.ends_with("web-live >"),
        "{web_row}"
    );
    let repo_row = hit_rect(&p, |hit| matches!(hit, Hit::Repository(0)));
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: repo_row.right() - 1,
            row: repo_row.y,
            modifiers: Mods::NONE,
        }),
        &c,
    );
    assert_eq!(p.repo_root, Some(0));
    assert_eq!(state(repo), before);
    assert_eq!(state(web), other);

    run(repo, &["update-ref", "refs/remotes/origin/release", "main"]);
    run(web, &["switch", "--detach"]);
    let before = state(repo);
    reload(&mut p, &c);
    let row = row_text(&mut p, &c, "refs/heads/main", 120, 36);
    assert!(!row.contains("^2") && !row.contains("v3"), "{row}");
    let t = render(&mut p, &c, 120, 36);
    assert!(repository_text(&t, &p, 0).ends_with("main >"));
    assert!(repository_text(&t, &p, 1).contains("detached@"));
    assert_eq!(state(repo), before);
}

#[test]
fn branch_tracking_arrows_keep_colors_and_click_identity_in_all_icon_modes_and_sizes() {
    let (_temp, mut c, mut p) = diverged_fixture();
    for single_repo in [false, true] {
        if single_repo {
            c.scope.remove(&1);
            reload(&mut p, &c);
        }
        for mode in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            p.icons = mode;
            for (width, height) in [(120, 36), (80, 25), (44, 12)] {
                select(&mut p, 0, "refs/heads/main");
                let terminal = render(&mut p, &c, width, height);
                let rect = hit_rect(&p, |hit| matches!(hit, Hit::Row(i) if *i == p.tree.cursor));
                let buffer = terminal.backend().buffer();
                let mut rows = vec![(rect, SEL)];
                if !single_repo {
                    rows.push((hit_rect(&p, |h| matches!(h, Hit::Repository(0))), TAG));
                }
                for (row, bg) in rows {
                    for (icon, count, color) in
                        [(Icon::Ahead, "2", LOCAL_REF), (Icon::Behind, "3", BLUE)]
                    {
                        let glyph = mode.glyph(icon);
                        let x = (row.x..row.right())
                            .find(|&x| buffer[(x, row.y)].symbol() == glyph)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Missing {icon:?} at {width}x{height}, {mode:?}: {}",
                                    row_cells(&terminal, row)
                                )
                            });
                        assert_eq!(buffer[(x, row.y)].fg, color);
                        assert_eq!(buffer[(x, row.y)].bg, bg);
                        assert_eq!(buffer[(x + glyph.width() as u16, row.y)].symbol(), count);
                    }
                }
                p.event(
                    &Event::Mouse(MouseEvent {
                        kind: M::Down(MouseButton::Left),
                        column: rect.right() - 1,
                        row: rect.y,
                        modifiers: Mods::NONE,
                    }),
                    &c,
                );
                assert_eq!(p.selection().unwrap().1.reference, "refs/heads/main");
                assert!(p.input.is_none() && p.review.is_none());
            }
        }
    }
}

#[test]
fn branch_tracking_counts_survive_long_unicode_names_in_grouped_and_flat_details() {
    let (_temp, mut c, mut p) = diverged_fixture();
    c.scope.remove(&1);
    let name = "feature/中文分支名称-long-long-long-long-long-long-long-long-long-long";
    let reference = format!("refs/heads/{name}");
    let repo = &c.repos[0].path;
    run(repo, &["branch", name]);
    run(repo, &["branch", "--set-upstream-to=origin/release", name]);
    reload(&mut p, &c);
    for grouped in [true, false] {
        if !grouped {
            key(&mut p, &c, K::Char('o'));
            key(&mut p, &c, K::Enter);
            key(&mut p, &c, K::Esc);
        }
        for (width, height) in [(120, 36), (80, 25), (44, 12)] {
            let row = row_text(&mut p, &c, &reference, width, height);
            assert!(row.contains('…') && row.contains("^2 v3"), "{row}");
            let text = screen(&mut p, &c, width, height);
            assert!(
                text.lines()
                    .rev()
                    .take(3)
                    .any(|line| line.contains("^2 v3")),
                "{text}"
            );
        }
    }
}

#[test]
fn branch_tracking_counts_reserve_upstream_space_even_when_their_folder_is_folded() {
    let (_temp, mut c, mut p) = diverged_fixture();
    c.scope.remove(&1);
    let name = "feature/branch-with-counts";
    let reference = format!("refs/heads/{name}");
    let repo = &c.repos[0].path;
    run(repo, &["branch", name]);
    run(repo, &["branch", "--set-upstream-to=origin/release", name]);
    reload(&mut p, &c);
    let row = row_text(&mut p, &c, &reference, 120, 36);
    assert!(row.contains("branch-with-counts ^2 v3"), "{row}");
    assert!(row.contains("-> B origin/release"), "{row}");
    let column = p.branch_columns(p.viewport.area.width).upstream;
    key(&mut p, &c, K::Char('h'));
    key(&mut p, &c, K::Char('h'));
    assert!(!p
        .tree
        .rows
        .iter()
        .any(|row| row.id == format!("0:{reference}")));
    screen(&mut p, &c, 120, 36);
    assert_eq!(p.branch_columns(p.viewport.area.width).upstream, column);

    key(&mut p, &c, K::Char('o'));
    key(&mut p, &c, K::Enter);
    key(&mut p, &c, K::Esc);
    let row = row_text(&mut p, &c, &reference, 120, 36);
    assert!(row.contains("feature/branch-with-counts ^2 v3"), "{row}");
    assert!(row.contains("-> B origin/release"), "{row}");
}
