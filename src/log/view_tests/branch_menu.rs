use super::*;
use crate::log::branches::{Row, Target};
use std::collections::BTreeSet;

fn fixture() -> (tempfile::TempDir, Context, LogPanel) {
    let d = tempfile::tempdir().unwrap();
    let mut cdir = d.path().join("workspace");
    std::fs::create_dir(&cdir).unwrap();
    for name in ["one", "two"] {
        cdir.push(name);
        std::fs::create_dir(&cdir).unwrap();
        git(&cdir, &["init", "-q", "-b", "master"]);
        git(&cdir, &["config", "user.name", "Dev"]);
        git(&cdir, &["config", "user.email", "dev@example.invalid"]);
        commit(&cdir, "base", &format!("{name} base"));
        git(&cdir, &["remote", "add", "origin", "."]);
        git(&cdir, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
        git(&cdir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(
            &cdir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/master",
            ],
        );
        if name == "one" {
            git(&cdir, &["checkout", "-qb", "topic"]);
            commit(&cdir, "topic", "topic only");
            git(
                &cdir,
                &["update-ref", "refs/remotes/origin/antv-version", "HEAD"],
            );
            for n in ["v10", "v2"] {
                git(
                    &cdir,
                    &[
                        "update-ref",
                        &format!("refs/remotes/origin/api/{n}"),
                        "HEAD",
                    ],
                );
            }
            git(&cdir, &["tag", "topic"]);
            git(&cdir, &["remote", "add", "team/mirror", "."]);
            git(
                &cdir,
                &["update-ref", "refs/remotes/team/mirror/feat/ui", "HEAD"],
            );
            for i in 0..18 {
                git(
                    &cdir,
                    &[
                        "update-ref",
                        &format!("refs/remotes/origin/feat/{i:02}"),
                        "HEAD",
                    ],
                );
            }
            git(&cdir, &["checkout", "-q", "master"]);
        }
        cdir.pop();
    }
    let mut c = Context::discover(&cdir).unwrap();
    c.state_dir = d.path().join("state");
    let mut p = LogPanel {
        icons: IconMode::Unicode,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    (d, c, p)
}
fn item(p: &LogPanel, t: &Terminal<TestBackend>, text: &str, level: fn(Row) -> bool) -> Rect {
    p.hits
        .iter()
        .find_map(|(r, h)| match h {
            Hit::Dropdown(DropdownHit::Branch(Target::Row(row)))
                if level(*row) && row_text(t, *r).contains(text) =>
            {
                Some(*r)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing rendered row: {text}"))
}
fn main_row(r: Row) -> bool {
    matches!(r, Row::Main(_))
}
fn child_row(r: Row) -> bool {
    matches!(r, Row::Child(_))
}
fn subjects(p: &LogPanel) -> BTreeSet<&str> {
    p.visible
        .iter()
        .map(|&i| p.hist.revs[i].subject.as_str())
        .collect()
}
fn paste(p: &mut LogPanel, s: &str, c: &Context) {
    p.event(&Event::Paste(s.into()), c);
}

#[test]
fn hover_cascades_star_is_independent_of_filter_and_survives_reopening() {
    let (_d, c, mut p) = fixture();
    let mut t = Terminal::new(TestBackend::new(160, 30)).unwrap();
    filter_key(&mut p, 'b', &c);
    render(&mut p, &mut t, &c);
    let labels: Vec<_> = p
        .hits
        .iter()
        .filter(|(_, h)| {
            matches!(
                h,
                Hit::Dropdown(DropdownHit::Branch(Target::Row(Row::Main(_))))
            )
        })
        .map(|(r, _)| {
            row_text(&t, *r)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(
        labels,
        [
            "Select...",
            "Favorites",
            "★ HEAD",
            "★ master",
            "★ origin/main",
            "★ origin/master",
            "Local ›",
            "origin/... ›",
            "team/mirror/... ›"
        ]
    );
    let remote = item(&p, &t, "origin/...", main_row);
    assert!(p.hover_menu_at(remote.x + 4, remote.y));
    render(&mut p, &mut t, &c);
    let leaf = item(&p, &t, "origin/antv-version", child_row);
    assert!(
        item(&p, &t, "origin/api/v2", child_row).y < item(&p, &t, "origin/api/v10", child_row).y
    );
    assert!(leaf.x >= remote.right());
    assert!(row_text(&t, leaf).contains("☆"));
    assert_eq!(t.backend().buffer()[(leaf.x + 1, leaf.y)].fg, GOLD);
    let star = target(&p, |h| {
        matches!(
            h,
            Hit::Dropdown(DropdownHit::Branch(Target::Star(Row::Child(0))))
        )
    });
    click_at(&mut p, star, &c);
    render(&mut p, &mut t, &c);
    assert!(p.filter.branch.is_none());
    assert_eq!(item(&p, &t, "origin/antv-version", child_row), leaf);
    assert!(row_text(&t, leaf).contains('★'));
    let user = target(&p, |h| matches!(h, Hit::Filter(Field::Author)));
    click_at(&mut p, user, &c);
    assert!(matches!(p.editor, Some((Field::Author, _))));
    let mut reopened = LogPanel::default();
    reopened.refresh(&c).unwrap();
    filter_key(&mut reopened, 'b', &c);
    render(&mut reopened, &mut t, &c);
    let fav = item(&reopened, &t, "Favorites", main_row);
    assert!(item(&reopened, &t, "origin/antv-version", main_row).y > fav.y);
    click_at(&mut reopened, fav, &c);
    assert_eq!(
        subjects(&reopened),
        ["one base", "two base", "topic only"].into()
    );
    assert!(reopened.filter.single_branch().is_none() && reopened.filter.branch.is_some());
    assert!(!reopened.editing());
}

#[test]
fn select_combines_exact_refs_regex_and_exclusions_per_repository() {
    let (_d, c, mut p) = fixture();
    for (text, expected) in [
        (
            "origin/antv-version | master",
            vec!["one base", "two base", "topic only"],
        ),
        ("master..origin/antv-version", vec!["topic only"]),
        ("refs/heads/topic..master", vec![]),
        ("origin/feat/.*\n-master", vec!["topic only"]),
        ("-refs/heads/topic", vec!["two base"]),
        ("refs/tags/topic", vec!["one base", "topic only"]),
    ] {
        filter_key(&mut p, 'b', &c);
        key(&mut p, K::Enter, Mods::NONE, &c);
        key(&mut p, K::Char('u'), Mods::CONTROL, &c);
        paste(&mut p, text, &c);
        assert!(matches!(
            key(&mut p, K::Enter, Mods::CONTROL, &c),
            Action::None
        ));
        assert!(!p.editing(), "{text}");
        assert_eq!(subjects(&p), expected.into_iter().collect(), "{text}");
    }
    filter_key(&mut p, 'b', &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    paste(&mut p, "topic", &c);
    assert!(matches!(
        key(&mut p, K::Enter, Mods::NONE, &c),
        Action::Notice(_)
    ));
    assert!(p.editing());
    assert_eq!(p.filter.single_branch(), Some("refs/tags/topic"));
    assert_eq!(
        git(&c.repos[0].path, &["symbolic-ref", "HEAD"]),
        "refs/heads/master"
    );
    assert!(git(&c.repos[0].path, &["status", "--porcelain"]).is_empty());
}

#[test]
fn narrow_keyboard_and_mouse_back_reach_last_remote_and_respect_paths() {
    let (_d, c, mut p) = fixture();
    let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
    filter_key(&mut p, 'b', &c);
    key(&mut p, K::End, Mods::NONE, &c); // team/mirror
    key(&mut p, K::Up, Mods::NONE, &c); // origin
    key(&mut p, K::Right, Mods::NONE, &c);
    key(&mut p, K::End, Mods::NONE, &c);
    render(&mut p, &mut t, &c);
    let last = item(&p, &t, "origin/master", child_row);
    assert!(last.y < 12 && last.right() <= 40);
    let back = target(&p, |h| {
        matches!(h, Hit::Dropdown(DropdownHit::Branch(Target::Back)))
    });
    click_at(&mut p, back, &c);
    render(&mut p, &mut t, &c);
    assert!(item(&p, &t, "origin/...", main_row).width > 0);
    key(&mut p, K::Right, Mods::NONE, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(
        p.filter.single_branch(),
        Some("refs/remotes/origin/antv-version")
    );
    assert_eq!(subjects(&p), ["one base", "topic only"].into());
    p.filter.roots = Some([1].into());
    filter_key(&mut p, 'b', &c);
    key(&mut p, K::End, Mods::NONE, &c);
    render(&mut p, &mut t, &c);
    let remote = item(&p, &t, "origin/...", main_row);
    click_at(&mut p, remote, &c);
    render(&mut p, &mut t, &c);
    assert!(item(&p, &t, "origin/main", child_row).width > 0);
    assert_eq!(
        p.hits
            .iter()
            .filter(|(_, h)| matches!(
                h,
                Hit::Dropdown(DropdownHit::Branch(Target::Row(Row::Child(_))))
            ))
            .count(),
        2
    );
}

#[test]
fn left_side_cascade_keeps_the_hovered_row_under_the_pointer_in_all_icon_modes() {
    let (_d, c, mut p) = fixture();
    let mut t = Terminal::new(TestBackend::new(96, 16)).unwrap();
    let anchor = Rect::new(82, 1, 8, 1);
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        p.icons = icons;
        filter_key(&mut p, 'b', &c);
        let paint = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            p.hits.clear();
            t.draw(|f| {
                p.branch_menu
                    .as_mut()
                    .unwrap()
                    .draw(f, f.area(), anchor, p.icons, &mut p.hits)
            })
            .unwrap();
        };
        paint(&mut p, &mut t);
        let group = item(&p, &t, "origin/...", main_row);
        assert!(p.hover_menu_at(group.x + 3, group.y));
        paint(&mut p, &mut t);
        let (n, row) = p
            .hits
            .iter()
            .filter_map(|(r, h)| match h {
                Hit::Dropdown(DropdownHit::Branch(Target::Row(Row::Child(n)))) => Some((n, *r)),
                _ => None,
            })
            .next_back()
            .unwrap();
        assert!(*n >= 3 && row.right() <= group.x);
        let label = row_text(&t, row).trim().to_owned();
        assert!(p.hover_menu_at(row.x + 4, row.y));
        paint(&mut p, &mut t);
        assert_eq!(item(&p, &t, &label, child_row), row);
        click_at(&mut p, Rect::new(row.x + 4, row.y, 1, 1), &c);
        assert_eq!(
            p.filter.single_branch(),
            Some(format!("refs/remotes/{label}").as_str())
        );
    }
}
