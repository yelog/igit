use super::*;
use crate::ui::{
    icons::Icon,
    idea::{CONFLICT, LOCAL_REF, REMOTE_REF, TAG},
};

fn render(p: &mut BranchPanel, c: &Context, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| p.draw(f, f.area(), c)).unwrap();
    t
}

fn branch_rect(p: &BranchPanel, reference: &str) -> Rect {
    p.hits.iter().find_map(|(rect, hit)| match hit {
        Hit::Row(i) if matches!(&p.tree.rows[*i].kind, Kind::Branch(b) if b.reference == reference) => Some(*rect),
        _ => None,
    }).unwrap()
}

fn symbol_x(t: &Terminal<TestBackend>, rect: Rect, symbol: &str) -> u16 {
    (rect.x..rect.right())
        .find(|&x| t.backend().buffer()[(x, rect.y)].symbol() == symbol)
        .unwrap()
}

#[test]
fn branch_glyphs_colors_counts_and_upstream_columns_survive_unicode_and_icon_modes() {
    let (_temp, c, mut p) = fixture();
    tracking(&c, "main", "main");
    let repo = &c.repos[0].path;
    for name in ["short", "feature/中文", "feature/deep/longer-branch-name"] {
        run(repo, &["branch", name]);
        run(repo, &["branch", "--set-upstream-to=origin/main", name]);
    }
    run(repo, &["branch", "untracked"]);
    reload(&mut p, &c);
    expand_remotes(&mut p, &c, 0);
    for mode in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        p.icons = mode;
        select(&mut p, 0, "refs/heads/feature/中文");
        let t = render(&mut p, &c, 120, 36);
        let mut columns = Vec::new();
        for name in [
            "main",
            "short",
            "feature/中文",
            "feature/deep/longer-branch-name",
        ] {
            let r = branch_rect(&p, &format!("refs/heads/{name}"));
            let icon_x = symbol_x(&t, r, mode.glyph(Icon::Branch));
            assert_eq!(t.backend().buffer()[(icon_x, r.y)].fg, LOCAL_REF);
            let upstream_icon = (icon_x + 1..r.right())
                .find(|&x| t.backend().buffer()[(x, r.y)].symbol() == mode.glyph(Icon::Branch))
                .unwrap();
            assert_eq!(t.backend().buffer()[(upstream_icon, r.y)].fg, REMOTE_REF);
            columns.push(upstream_icon);
        }
        assert!(columns.iter().all(|x| *x == columns[0]), "{columns:?}");
        let r = branch_rect(&p, "refs/remotes/origin/main");
        let x = symbol_x(&t, r, mode.glyph(Icon::Branch));
        assert_eq!(t.backend().buffer()[(x, r.y)].fg, REMOTE_REF);
        let text = screen(&mut p, &c, 120, 36);
        assert!(text.contains("Local Branches  5") && text.contains("Remote Branches  1"));
        assert!(text.contains("No upstream"));
        let repo_y = p
            .hits
            .iter()
            .find_map(|(r, h)| {
                matches!(h, Hit::Row(i) if p.tree.rows[*i].depth == 0).then_some(r.y)
            })
            .unwrap();
        assert_eq!(t.backend().buffer()[(p.viewport.x, repo_y)].bg, TAG);
        let selected = branch_rect(&p, "refs/heads/feature/中文");
        assert_eq!(
            t.backend().buffer()[(selected.right() - 1, selected.y)].bg,
            SEL
        );
    }
}

#[test]
fn long_names_keep_gone_status_and_narrow_details_and_clicks_target_the_same_branch() {
    let (_temp, c, mut p) = fixture();
    let name = "feature/中文分支名称-long-long-long-long-long-long-long-long-long-long-long";
    run(&c.repos[0].path, &["branch", name]);
    tracking(&c, name, "release/missing-remote-with-a-long-name");
    run(
        &c.repos[0].path,
        &[
            "update-ref",
            "-d",
            "refs/remotes/origin/release/missing-remote-with-a-long-name",
        ],
    );
    reload(&mut p, &c);
    let reference = format!("refs/heads/{name}");
    select(&mut p, 0, &reference);
    for (w, h) in [(120, 36), (80, 25), (44, 18)] {
        let t = render(&mut p, &c, w, h);
        let text = screen(&mut p, &c, w, h);
        assert!(text.contains('…'), "{text}");
        assert!(text.contains("(current)"));
        let rect = branch_rect(&p, &reference);
        if w >= 80 {
            let offset = p.upstream_column(rect.width).unwrap();
            let x = symbol_x(
                &t,
                Rect {
                    x: rect.x + offset,
                    width: rect.width - offset,
                    ..rect
                },
                "[",
            );
            assert_eq!(t.backend().buffer()[(x, rect.y)].fg, CONFLICT);
            assert!(text.contains("[gone]"));
        } else {
            assert!(text.contains("Upstream: origin/release/"), "{text}");
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
        assert_eq!(p.selection().unwrap().1.reference, reference);
        assert!(p.input.is_none() && p.review.is_none());
    }
}

#[test]
fn action_buttons_only_offer_operations_for_the_focused_branch() {
    let (_temp, c, mut p) = fixture();
    tracking(&c, "main", "main");
    reload(&mut p, &c);
    let operations = |p: &mut BranchPanel| {
        screen(p, &c, 120, 36);
        p.hits
            .iter()
            .filter_map(|(_, h)| {
                if let Hit::Operation(op) = h {
                    Some(*op)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    select(&mut p, 0, "refs/heads/main");
    assert_eq!(operations(&mut p), [Operation::Create, Operation::Rename]);
    p.tree.cursor = 0;
    assert!(operations(&mut p).is_empty());
    expand_remotes(&mut p, &c, 0);
    select(&mut p, 0, "refs/remotes/origin/main");
    assert_eq!(
        operations(&mut p),
        [Operation::Create, Operation::Checkout, Operation::Merge]
    );
}
