use super::*;
use crate::core::command;
use std::{fs, path::PathBuf};
use tempfile::tempdir;

fn run(path: &Path, args: &[&str], date: Option<i64>) -> String {
    let mut cmd = command(path);
    cmd.env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .env("GIT_CONFIG_NOSYSTEM", "1");
    if let Some(ts) = date {
        let date = format!("@{ts} +0000");
        cmd.env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let output = cmd.args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}
pub(super) fn g(path: &Path, args: &[&str]) -> String {
    run(path, args, None)
}
pub(super) fn repo(parent: &Path, name: &str, email: &str) -> PathBuf {
    let p = parent.join(name);
    fs::create_dir(&p).unwrap();
    g(&p, &["init", "-q", "-b", "main"]);
    g(&p, &["config", "user.name", "Tester"]);
    g(&p, &["config", "user.email", email]);
    p
}
pub(super) fn commit(p: &Path, subject: &str) -> String {
    g(p, &["commit", "--allow-empty", "-qm", subject]);
    g(p, &["rev-parse", "HEAD"])
}
pub(super) fn file(p: &Path, name: &str, subject: &str) -> String {
    fs::write(p.join(name), subject).unwrap();
    g(p, &["--literal-pathspecs", "add", "--", name]);
    commit(p, subject)
}
fn loaded(c: &Context, f: &Filter) -> History {
    let h = load(c, f, 100);
    assert!(h.errors.is_empty(), "{:?}", h.errors);
    h
}

#[test]
fn log_tips_include_tags_remotes_and_detached_head_but_not_stash_snapshots() {
    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    let c = Context::discover(tmp.path()).unwrap();
    assert!(loaded(&c, &Filter::default()).revs.is_empty());
    let base = file(&p, "tracked", "base");
    g(&p, &["checkout", "-q", "--detach"]);
    let tag = commit(&p, "tag only");
    g(&p, &["tag", "-a", "release", "-m", "annotated"]);
    g(&p, &["checkout", "-q", "--detach", "main"]);
    let remote = commit(&p, "remote only");
    g(&p, &["update-ref", "refs/remotes/origin/review", "HEAD"]);
    g(&p, &["checkout", "-q", "--detach", "main"]);
    let detached = commit(&p, "detached");
    fs::write(p.join("tracked"), "dirty").unwrap();
    fs::write(p.join("untracked"), "new").unwrap();
    g(&p, &["stash", "push", "-qu", "-m", "shelved"]);
    let h = loaded(&c, &Filter::default());
    assert_eq!(
        h.revs.iter().map(|r| &r.oid).collect::<HashSet<_>>(),
        [&base, &tag, &remote, &detached].into_iter().collect()
    );
    assert!(h
        .revs
        .iter()
        .all(|r| h.on_head(r) == [base.as_str(), detached.as_str()].contains(&r.oid.as_str())));
    let detached_rev = h.revs.iter().find(|r| r.oid == detached).unwrap();
    assert_eq!(
        details(&p, detached_rev).unwrap().branches,
        [(RefKind::Head, "HEAD".into())]
    );

    // It is reachability, not the stash commit's subject, which decides inclusion.
    let stash_index = g(&p, &["rev-parse", "stash^2"]);
    g(&p, &["tag", "saved-index", &stash_index]);
    assert!(loaded(&c, &Filter::default())
        .revs
        .iter()
        .any(|r| r.oid == stash_index));
    g(&p, &["checkout", "-q", "--orphan", "unborn"]);
    let head = Filter {
        branch: Some("HEAD".into()),
        ..Filter::default()
    };
    assert!(loaded(&c, &head).revs.is_empty());
    let base_row = loaded(&c, &Filter::default())
        .revs
        .into_iter()
        .find(|r| r.oid == base)
        .unwrap();
    assert!(!details(&p, &base_row)
        .unwrap()
        .branches
        .iter()
        .any(|(kind, _)| *kind == RefKind::Head));
}

#[test]
fn shared_branch_me_and_literal_or_repository_paths_respect_scope() {
    let tmp = tempdir().unwrap();
    let a = repo(tmp.path(), "api", "a.li+git@example.invalid");
    let b = repo(tmp.path(), "web", "bee@example.invalid");
    let z = repo(tmp.path(), "worker", "c@example.invalid");
    for p in [&a, &b] {
        file(p, "base", "base by me");
        g(p, &["checkout", "-qb", "feat/shared"]);
        file(p, "web", "feature by me");
        g(
            p,
            &[
                "-c",
                "user.email=aXli+git@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "other author",
            ],
        );
    }
    commit(&z, "no feature branch");
    let mut c = Context::discover(tmp.path()).unwrap();
    let f = Filter {
        branch: Some("refs/heads/feat/shared".into()),
        author: "me".into(),
        ..Filter::default()
    };
    let h = loaded(&c, &f);
    assert_eq!(h.revs.len(), 4);
    assert!(h.revs.iter().all(|r| r.subject.ends_with("by me")));
    assert_eq!(
        h.revs
            .iter()
            .map(|r| &c.repos[r.root].name)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    // The editor resolves a typed root once; the Git layer only receives literal paths.
    let mut panel = crate::log::LogPanel {
        filter: f.clone(),
        ..Default::default()
    };
    panel.reload(&c, false).unwrap();
    panel.apply_path_input(&c, "web".into()).unwrap();
    assert_eq!(panel.visible.len(), 2);
    assert!(panel
        .visible
        .iter()
        .all(|&i| c.repos[panel.hist.revs[i].root].name == "web"));
    c.scope
        .remove(&c.repos.iter().position(|r| r.name == "web").unwrap());
    panel.reload(&c, true).unwrap();
    assert!(panel.visible.is_empty());
    let literal = Filter {
        paths: PathFilter::parse("./web", &c).unwrap(),
        ..f
    };
    let h = loaded(&c, &literal);
    assert_eq!(h.revs.len(), 1);
    assert_eq!(h.revs[0].subject, "feature by me");
    let missing = Filter {
        branch: Some("refs/heads/missing".into()),
        ..Filter::default()
    };
    assert!(loaded(&c, &missing).revs.is_empty());
}

#[test]
fn reads_tracking_and_containing_branches_including_ancestors_across_a_filter_gap() {
    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    let base = file(&p, "visible", "common ancestor");
    g(&p, &["tag", "marker"]);
    g(&p, &["checkout", "-qb", "review"]);
    let side = file(&p, "visible", "side only");
    g(&p, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    g(
        &p,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    let c = Context::discover(tmp.path()).unwrap(); // The cached Repo.branch still says review.
    g(&p, &["checkout", "-q", "main"]);
    commit(&p, "filtered intermediate");
    fs::write(p.join("visible"), "main").unwrap();
    g(&p, &["add", "visible"]);
    g(
        &p,
        &[
            "commit",
            "-qm",
            "main only",
            "-m",
            "First line.\nSecond line.",
        ],
    );
    let tip = g(&p, &["rev-parse", "HEAD"]);
    g(&p, &["config", "remote.origin.url", "."]);
    g(
        &p,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    );
    g(&p, &["branch", "--set-upstream-to", "origin/main", "main"]);
    let f = Filter {
        paths: PathFilter::parse("visible", &c).unwrap(),
        ..Filter::default()
    };
    let h = loaded(&c, &f);
    assert_eq!(h.current[&0], "refs/heads/main");
    assert_eq!(h.tracking[&(0, "refs/heads/main".into())], (2, 1));
    assert_eq!(h.remote_upstream(0, "main"), Some("origin/main"));
    assert!(h
        .remote_heads
        .contains(&(0, "refs/remotes/origin/main".into())));
    assert!(!h.branches.iter().any(|b| b.name.ends_with("/HEAD")));
    assert_eq!(h.revs.len(), 3);
    for r in &h.revs {
        assert_eq!(h.on_head(r), r.oid != side);
        let d = details(&p, r).unwrap();
        if r.oid == base {
            assert_eq!(
                d.branches,
                [
                    (RefKind::Head, "HEAD"),
                    (RefKind::Current, "main"),
                    (RefKind::Local, "review"),
                    (RefKind::Remote, "origin/main"),
                ]
                .map(|(kind, name)| (kind, name.to_owned()))
            );
        } else if r.oid == tip {
            assert_eq!(
                d.branches,
                [(RefKind::Head, "HEAD"), (RefKind::Current, "main")]
                    .map(|(kind, name)| (kind, name.to_owned()))
            );
            assert_eq!(d.email, "t@example.invalid");
            assert_eq!(d.body, "First line.\nSecond line.");
        } else {
            assert_eq!(
                d.branches,
                [(RefKind::Local, "review"), (RefKind::Remote, "origin/main")]
                    .map(|(kind, name)| (kind, name.to_owned()))
            );
        }
    }
}

#[test]
fn local_dates_keep_absolute_minutes_and_only_shorten_today_and_yesterday() {
    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    let noon = |relative: &str| -> i64 {
        g(&p, &["rev-parse", &format!("--since={relative} 12:00:00")])
            .strip_prefix("--max-age=")
            .unwrap()
            .parse()
            .unwrap()
    };
    for (ts, subject) in [
        (noon("yesterday") - 86_400, "old"),
        (noon("yesterday"), "yesterday"),
        (noon("today"), "today"),
        (noon("today") + 86_400, "future"),
    ] {
        run(&p, &["commit", "--allow-empty", "-qm", subject], Some(ts));
    }
    let h = loaded(&Context::discover(tmp.path()).unwrap(), &Filter::default());
    for r in &h.revs {
        let (date, hm) = r.date.split_once(", ").unwrap();
        assert!(!date.contains("/0"));
        assert_eq!(hm.len(), 5);
        let displayed = h.display_date(r);
        match r.subject.as_str() {
            "today" => assert_eq!(displayed, format!("Today {hm}")),
            "yesterday" => assert_eq!(displayed, format!("Yesterday {hm}")),
            _ => assert_eq!(displayed, r.date),
        }
    }
    // The two local dates need not be 24 h apart. Compare calendar dates even
    // during the repeated hour at the end of daylight saving time in New York.
    for (offset, ts) in [("-0400", 1793511000), ("-0500", 1793514600)] {
        let ident = format!("unit <t@example.invalid> {ts} {offset}");
        assert_eq!(
            dates_from_ident(&ident).unwrap(),
            ["2026/11/1", "2026/10/31"]
        );
    }
}

#[test]
fn date_presets_filter_real_history_without_losing_recent_ancestors() {
    use std::{collections::BTreeSet, time::SystemTime};

    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // A recent ancestor must survive traversal through a child with an old clock.
    for (ts, subject) in [
        (now - 3600, "recent ancestor"),
        (now - 7 * 86_400 - 7200, "outside week"),
        (now - 7 * 86_400 + 7200, "inside week"),
        (now - 86_400 - 600, "outside day"),
        (now - 86_400 + 600, "inside day"),
        (now + 86_400, "future"),
    ] {
        run(&p, &["commit", "--allow-empty", "-qm", subject], Some(ts));
    }
    let c = Context::discover(tmp.path()).unwrap();
    for (date, expected) in [
        (
            DateFilter::Last24Hours,
            vec!["recent ancestor", "inside day"],
        ),
        (
            DateFilter::Last7Days,
            vec![
                "recent ancestor",
                "inside week",
                "outside day",
                "inside day",
            ],
        ),
    ] {
        let h = loaded(
            &c,
            &Filter {
                date: Some(date),
                ..Default::default()
            },
        );
        assert_eq!(
            h.revs
                .iter()
                .map(|r| r.subject.as_str())
                .collect::<BTreeSet<_>>(),
            expected.into_iter().collect()
        );
        assert!(h.revs.iter().all(|r| h.on_head(r)));
    }
    assert_eq!(loaded(&c, &Filter::default()).revs.len(), 6);
}

#[test]
fn date_range_includes_both_local_day_boundaries() {
    use std::collections::BTreeSet;

    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    let local_ts = |date: &str| -> i64 {
        g(&p, &["rev-parse", &format!("--since={date}")])
            .strip_prefix("--max-age=")
            .unwrap()
            .parse()
            .unwrap()
    };
    for (date, subject) in [
        ("2026-02-28 23:59:59", "before"),
        ("2026-03-01 00:00:00", "first second"),
        ("2026-03-01 12:00:00", "middle"),
        ("2026-03-02 23:59:59", "last second"),
        ("2026-03-03 00:00:00", "after"),
    ] {
        run(
            &p,
            &["commit", "--allow-empty", "-qm", subject],
            Some(local_ts(date)),
        );
    }
    let c = Context::discover(tmp.path()).unwrap();
    for (to, expected) in [
        ("2026-03-02", vec!["first second", "middle", "last second"]),
        ("2026-03-01", vec!["first second", "middle"]),
    ] {
        let h = loaded(
            &c,
            &Filter {
                date: Some(DateFilter::Range {
                    from: "2026-03-01".into(),
                    to: to.into(),
                }),
                ..Default::default()
            },
        );
        assert_eq!(
            h.revs
                .iter()
                .map(|r| r.subject.as_str())
                .collect::<BTreeSet<_>>(),
            expected.into_iter().collect()
        );
    }
}

#[test]
fn reference_badges_keep_namespaces_for_slash_names_and_same_short_names() {
    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "app", "t@example.invalid");
    commit(&p, "tip");
    g(&p, &["branch", "feat/slash"]);
    g(&p, &["branch", "origin/review"]);
    g(&p, &["update-ref", "refs/remotes/origin/review", "HEAD"]);
    g(&p, &["tag", "origin/review"]);
    let rev = revisions(&p, 0, &Filter::default(), &all_tips(), 1)
        .unwrap()
        .remove(0);
    let badges: Vec<_> = rev.decorations().collect();
    assert!(badges.contains(&(RefKind::Current, "main")));
    assert!(badges.contains(&(RefKind::Local, "feat/slash")));
    assert!(badges.contains(&(RefKind::Local, "origin/review")));
    assert!(badges.contains(&(RefKind::Remote, "origin/review")));
    assert!(badges.contains(&(RefKind::Tag, "origin/review")));
    let containing = details(&p, &rev).unwrap().branches;
    assert!(containing.contains(&(RefKind::Local, "heads/origin/review".into())));
    assert!(containing.contains(&(RefKind::Remote, "remotes/origin/review".into())));
    g(&p, &["checkout", "--detach", "-q"]);
    let rev = revisions(&p, 0, &Filter::default(), &all_tips(), 1)
        .unwrap()
        .remove(0);
    assert!(rev
        .decorations()
        .any(|badge| badge == (RefKind::Head, "HEAD")));
    let containing = details(&p, &rev).unwrap().branches;
    assert!(containing.contains(&(RefKind::Head, "HEAD".into())));
    assert!(containing.contains(&(RefKind::Local, "main".into())));
    assert!(!containing.iter().any(|(kind, _)| *kind == RefKind::Current));
}

#[test]
fn graph_colors_follow_real_merge_lanes_and_clone_objects_stay_separate() {
    let tmp = tempdir().unwrap();
    let p = repo(tmp.path(), "one", "t@example.invalid");
    let base = commit(&p, "common");
    g(&p, &["checkout", "-qb", "topic"]);
    let topic = file(&p, "topic", "topic work");
    g(&p, &["checkout", "-q", "main"]);
    let main = file(&p, "main", "main work");
    g(&p, &["merge", "--no-ff", "-qm", "merge", "topic"]);
    let revs = revisions(&p, 0, &Filter::default(), &all_tips(), 100).unwrap();
    let draw = |rows: &[Rev]| {
        let g = Graph::new(rows);
        let w = g.window(0..rows.len(), 0, 80, 0);
        (0..rows.len())
            .map(|i| g.cells(i, w, NodeGlyphs::Unicode, rows[i].is_head_tip()))
            .collect::<Vec<_>>()
    };
    let bands = draw(&revs);
    let row = |oid: &str| revs.iter().position(|r| r.oid == oid).unwrap();
    let dot = |cells: &[(char, usize)]| {
        cells
            .iter()
            .position(|(c, _)| matches!(c, '●' | '◉'))
            .unwrap()
    };
    assert_eq!(bands[0][0], ('◉', 0));
    assert!(bands[0].iter().any(|&(c, color)| c == '╮' && color != 0));
    assert_eq!(bands[row(&main)][dot(&bands[row(&main)])].1, 0);
    assert_ne!(bands[row(&topic)][dot(&bands[row(&topic)])].1, 0);
    assert_eq!(bands[row(&base)][0], ('●', 0));

    let clone = tmp.path().join("two");
    g(
        tmp.path(),
        &[
            "clone",
            "-q",
            "--no-local",
            p.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    let other_base = revisions(&clone, 1, &Filter::default(), &all_tips(), 100)
        .unwrap()
        .into_iter()
        .find(|r| r.oid == base)
        .unwrap();
    let a = revs[row(&main)].clone();
    let b = revs[row(&base)].clone();
    let bands = draw(&[a.clone(), other_base.clone(), b.clone()]);
    assert_ne!(dot(&bands[0]), dot(&bands[1]));
    assert_eq!(bands[1][dot(&bands[0])].0, '│');
    assert_eq!(dot(&bands[0]), dot(&bands[2]));
    let disconnected = draw(&[a, other_base]);
    assert!(disconnected.iter().all(|b| b == &[('●', 0)]));
    g(&p, &["checkout", "-qb", "unmerged"]);
    commit(&p, "ahead of main");
    g(&p, &["checkout", "-q", "main"]);
    let rows = revisions(&p, 0, &Filter::default(), &all_tips(), 100).unwrap();
    let bands = draw(&rows);
    let head = rows.iter().position(|r| r.is_head_tip()).unwrap();
    assert_eq!(dot(&bands[head]), 0);
    // At the merge, this incoming topic joins from above while another parent
    // can leave to the right on the very same row.
    assert!(bands[head].iter().any(|(c, _)| matches!(c, '╯' | '┴')));
}

fn graph_rev(id: &str, parents: &[&str]) -> Rev {
    Rev {
        root: 0,
        oid: id.into(),
        parents: parents.iter().copied().map(String::from).collect(),
        refs: String::new(),
        author: String::new(),
        date: String::new(),
        subject: String::new(),
        ts: 0,
        ..Default::default()
    }
}

fn graph_lines(rows: &[Rev], glyphs: NodeGlyphs) -> Vec<String> {
    let graph = Graph::new(rows);
    let w = graph.window(0..rows.len(), 0, 100, 0);
    (0..rows.len())
        .map(|i| {
            graph
                .cells(i, w, glyphs, rows[i].is_head_tip())
                .iter()
                .map(|v| v.0)
                .collect()
        })
        .collect()
}

#[test]
fn many_tips_pointing_at_one_parent_share_the_run_to_it() {
    let mut rows: Vec<_> = (0..20)
        .map(|i| graph_rev(&i.to_string(), &["base"]))
        .collect();
    rows.push(graph_rev("base", &[]));
    let lines = graph_lines(&rows, NodeGlyphs::Unicode);
    assert_eq!(lines[0], "●  ");
    assert!(lines[1..20].iter().all(|line| line == "├─●"));
    assert_eq!(lines[20], "●  ");
}

#[test]
fn compacting_an_outer_track_draws_the_bend_and_filter_gaps_stay_open() {
    let mut rows = vec![
        graph_rev("H", &["h"]),
        graph_rev("A", &["B"]),
        graph_rev("h", &[]),
        graph_rev("B", &["C"]),
        graph_rev("C", &[]),
    ];
    let lines = graph_lines(&rows, NodeGlyphs::Unicode);
    assert_eq!(&lines[2..], ["● │", "●─╯", "●  "]);

    rows.remove(3); // A's real parent B is not a visible row. C cannot replace B.
    let lines = graph_lines(&rows, NodeGlyphs::Unicode);
    assert_eq!(lines[1], "│ ●");
    assert_eq!(lines[3], "●  ");
}

#[test]
fn a_merge_can_continue_below_its_node_after_its_first_parent_joins_the_trunk() {
    let rows = [
        graph_rev("head", &["base"]),
        graph_rev("merge", &["base", "topic"]),
        graph_rev("topic", &["base"]),
        graph_rev("base", &[]),
    ];
    assert_eq!(
        graph_lines(&rows, NodeGlyphs::Unicode),
        ["●  ", "├─●", "├─●", "●  "]
    );
    // The merge and topic are in the same column, with different real ports:
    // the merge connects left and down, the topic left and up.
    assert_eq!(
        &graph_lines(&rows, NodeGlyphs::Connected)[1..3],
        ["├─\u{f5fe}", "├─\u{f602}"]
    );
}

#[test]
fn straight_commits_sit_on_the_rail_with_only_their_real_vertical_connections() {
    let mut rows = [
        graph_rev("head", &["middle"]),
        graph_rev("middle", &["base"]),
        graph_rev("base", &[]),
    ];
    rows[0].refs = "HEAD".into();
    assert_eq!(graph_lines(&rows, NodeGlyphs::Unicode), ["◉", "●", "●"]);
    // Hollow down, solid through, solid up. Each occupies exactly one cell.
    assert_eq!(
        graph_lines(&rows, NodeGlyphs::Connected),
        ["\u{f5f7}", "\u{f5fa}", "\u{f5f8}"]
    );
    // Filtering out the middle must not leave a stub pointing at the other row.
    assert_eq!(
        graph_lines(&[rows[0].clone(), rows[2].clone()], NodeGlyphs::Connected),
        ["\u{f5ef}", "\u{f5ee}"]
    );
}
