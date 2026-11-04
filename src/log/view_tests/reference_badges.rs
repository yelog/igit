use super::*;

const HEAD: Color = Color::Rgb(238, 190, 85);
const LOCAL: Color = Color::Rgb(97, 168, 112);
const REMOTE: Color = Color::Rgb(176, 137, 222);

fn tag_colours(p: &LogPanel, t: &Terminal<TestBackend>, root: usize, oid: &str) -> Vec<Color> {
    let n = p
        .visible
        .iter()
        .position(|&i| p.hist.revs[i].root == root && p.hist.revs[i].oid == oid)
        .unwrap();
    let row = target(p, |hit| matches!(hit, Hit::Commit(i) if *i == n));
    (row.x..row.right())
        .filter_map(|x| {
            let cell = &t.backend().buffer()[(x, row.y)];
            (cell.symbol() == p.icons.glyph(Icon::Tag)).then_some(cell.fg)
        })
        .collect()
}

fn message_only(p: &mut LogPanel) {
    for (column, visible) in &mut p.options.columns {
        *visible = *column == display::Column::Message;
    }
}

#[test]
fn reference_badges_keep_local_and_head_markers_in_each_repository() {
    let d = init();
    commit(d.path(), "file", "local head");
    let child = d.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let child = child.canonicalize().unwrap();
    git(&child, &["init", "-q", "-b", "main"]);
    git(&child, &["config", "user.name", "Log tester"]);
    git(&child, &["config", "user.email", "test@example.invalid"]);
    commit(&child, "file", "tracked head");
    git(&child, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(&child, &["config", "remote.origin.url", "."]);
    git(
        &child,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    );
    git(
        &child,
        &["branch", "--set-upstream-to", "origin/main", "main"],
    );
    let c = Context::discover(d.path()).unwrap();
    assert_eq!(c.repos.len(), 2);
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let mut p = LogPanel {
            icons: mode,
            show_refs: false,
            show_info: false,
            ..Default::default()
        };
        message_only(&mut p);
        p.refresh(&c).unwrap();
        for rev in &mut p.hist.revs {
            rev.subject = "long title ".repeat(30);
        }
        for width in [16, 24, 160] {
            let mut t = Terminal::new(TestBackend::new(width, 12)).unwrap();
            for refs_left in [false, true] {
                for compact in [false, true] {
                    p.options.refs_left = refs_left;
                    p.options.compact_refs = compact;
                    render(&mut p, &mut t, &c);
                    for (root, repo) in c.repos.iter().enumerate() {
                        let oid = git(&repo.path, &["rev-parse", "HEAD"]);
                        let expected = if repo.path == child {
                            vec![REMOTE, LOCAL, HEAD]
                        } else {
                            vec![LOCAL, HEAD]
                        };
                        assert_eq!(
                            tag_colours(&p, &t, root, &oid),
                            expected,
                            "{mode:?}, width={width}, refs_left={refs_left}, compact={compact}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn reference_badges_distinguish_detached_head_local_and_remote_tips() {
    let d = init();
    commit(d.path(), "file", "remote tip");
    let remote = git(d.path(), &["rev-parse", "HEAD"]);
    git(
        d.path(),
        &["update-ref", "refs/remotes/origin/review", "HEAD"],
    );
    commit(d.path(), "file", "local tip");
    let local = git(d.path(), &["rev-parse", "HEAD"]);
    git(d.path(), &["checkout", "--detach", "-q"]);
    commit(d.path(), "file", "detached tip");
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    let c = Context::discover(d.path()).unwrap();
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let mut p = LogPanel {
            icons: mode,
            show_refs: false,
            show_info: false,
            ..Default::default()
        };
        message_only(&mut p);
        p.refresh(&c).unwrap();
        let mut t = Terminal::new(TestBackend::new(160, 12)).unwrap();
        render(&mut p, &mut t, &c);
        assert_eq!(tag_colours(&p, &t, 0, &head), [HEAD]);
        assert_eq!(tag_colours(&p, &t, 0, &local), [LOCAL]);
        assert_eq!(tag_colours(&p, &t, 0, &remote), [REMOTE]);
    }
}
