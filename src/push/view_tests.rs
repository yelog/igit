use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use unicode_width::UnicodeWidthStr;

fn fixture(icons: IconMode) -> (PushPanel, Context) {
    let names = [
        "moss-web",
        "aps-hudong",
        "dcc",
        "devicemate",
        "界面-ui",
        "maas",
        "mes",
        "mes-isg",
        "mes-mbg",
        "sep",
        "qds",
    ];
    let c = Context {
        workspace: "/workspace".into(),
        state_dir: "/state".into(),
        scope: (0..names.len()).collect(),
        repos: names
            .iter()
            .enumerate()
            .map(|(i, name)| Repo {
                path: Path::new("/workspace").join(name),
                name: name.to_string(),
                branch: "master".into(),
                submodule: i > 0,
                parent: (i > 0).then_some(0),
            })
            .collect(),
    };
    let files = [
        ("dev/iframe-proxy/compose.yaml", b'A'),
        ("dev/iframe-proxy/nginx.conf", b'D'),
        ("dev/iframe-proxy/README.md", b'A'),
        ("src/layouts/customer-service/index.vue", b'M'),
        ("src/locales/en_US/system.ts", b'M'),
        ("src/locales/zh_CN/system.ts", b'A'),
        ("package.json", b'T'),
        ("scripts/renamed.mjs", b'R'),
    ]
    .into_iter()
    .map(|(path, status)| Change {
        path: path.into(),
        previous: None,
        status,
    })
    .collect::<Vec<_>>();
    let p = PushPanel {
        icons,
        entries: names
            .iter()
            .enumerate()
            .map(|(idx, _)| Entry {
                idx,
                source: "master".into(),
                t: Target {
                    remote: "origin".into(),
                    dst: "master".into(),
                },
                n: if idx == 0 {
                    2
                } else if idx == 7 {
                    123
                } else {
                    0
                },
                ..Default::default()
            })
            .collect(),
        checked: [0].into(),
        preview: Preview {
            tree: FileTree::new(&files),
            files,
            body: "master → origin:master".into(),
            summary: "master → origin:master".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    (p, c)
}

fn draw(p: &mut PushPanel, c: &Context, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| p.draw(f, f.area(), c)).unwrap();
    term.backend().buffer().clone()
}

fn text_at(buf: &Buffer, row: Rect) -> String {
    let mut x = row.x;
    let mut out = String::new();
    while x < row.right() {
        let symbol = buf[(x, row.y)].symbol();
        out.push_str(symbol);
        x += symbol.width().max(1) as u16;
    }
    out
}

fn find_text(buf: &Buffer, row: Rect, needle: &str) -> u16 {
    let text = text_at(buf, row);
    let i = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} missing in {text:?}"));
    row.x + text[..i].width() as u16
}

fn key(p: &mut PushPanel, c: &Context, code: KeyCode) {
    p.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), c);
}

fn click(p: &mut PushPanel, c: &Context, row: Rect) {
    p.event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: row.right() - 1,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        }),
        c,
    );
}

#[test]
fn push_columns_align_sources_targets_counts_and_submodules_in_terminal_cells() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for width in [120, 180, 300] {
            let (mut p, c) = fixture(icons);
            p.results.insert(0, Ok(()));
            p.results.insert(3, Ok(()));
            p.results.insert(7, Err("offline".into()));
            let buf = draw(&mut p, &c, width, 36);
            let rows: Vec<_> = p
                .hits
                .iter()
                .filter_map(|(r, h)| matches!(h, Hit::Row(_)).then_some(*r))
                .collect();
            let positions = |needle: &str, rows: &[Rect]| {
                rows.iter()
                    .map(|&r| find_text(&buf, r, needle))
                    .collect::<BTreeSet<_>>()
            };
            assert_eq!(positions("master", &rows).len(), 1, "{icons:?} {width}");
            assert_eq!(positions("origin:", &rows).len(), 1, "{icons:?} {width}");
            if width >= 180 {
                assert_eq!(positions("submodule", &rows[1..]).len(), 1);
                find_text(&buf, rows[3], "devicemate");
            }
            if width >= 180 || icons != IconMode::Ascii {
                assert_eq!(positions(icons.glyph(Icon::Ahead), &rows).len(), 1);
            }
            let target = find_text(&buf, rows[0], "origin:");
            assert_eq!(buf[(target, rows[0].y)].fg, ACCENT);
            assert_eq!(buf[(target, rows[1].y)].fg, MUTED);
        }
    }
}

#[test]
fn long_repo_and_branch_columns_clip_without_moving_controls_or_targets() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
            let (mut p, mut c) = fixture(icons);
            c.repos[0].name = "界面👩‍💻-a-very-long-repository-name".repeat(3);
            p.entries[0].source = "feature/客户-service-with-a-long-branch".into();
            p.entries[0].t.dst = "release/a-very-long-target-branch".into();
            let buf = draw(&mut p, &c, width, height);
            for (r, _) in &p.hits {
                assert!(
                    r.right() <= width && r.bottom() <= height,
                    "{icons:?}: {r:?}"
                );
            }
            let targets: Vec<_> = p
                .hits
                .iter()
                .filter_map(|(r, h)| match h {
                    Hit::Target(id) if r.y < p.zones[0].bottom() => Some((*id, *r)),
                    _ => None,
                })
                .collect();
            assert!(
                targets
                    .iter()
                    .map(|(_, r)| r.x)
                    .collect::<BTreeSet<_>>()
                    .len()
                    <= 1
            );
            if width >= 120 {
                assert!(!targets.is_empty());
                assert!(text_at(&buf, targets[0].1).contains('…'));
            }
            let check = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::Check(0)))
                .unwrap()
                .0;
            assert_eq!(text_at(&buf, check), icons.check(true));
            click(&mut p, &c, check);
            assert!(p.checked.is_empty());
            assert_eq!(p.selected(), Some((0, None)));
        }
    }
}

#[test]
fn push_file_tree_uses_change_colours_and_independent_file_type_icons() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let (mut p, c) = fixture(icons);
        let buf = draw(&mut p, &c, 180, 36);
        assert!(!p.show_details);
        assert_eq!(p.zones[1].height, p.zones[0].height);
        for (i, file) in p.preview.files.iter().enumerate() {
            let visible = p
                .preview
                .tree
                .visible
                .iter()
                .position(
                    |&n| matches!(p.preview.tree.nodes[n].kind, Kind::File(file) if file == i),
                )
                .unwrap();
            let row = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::File(v) if *v == visible))
                .unwrap()
                .0;
            let name = file.path.file_name().unwrap().to_str().unwrap();
            let x = find_text(&buf, row, name);
            let color = match file.status {
                b'A' => GREEN,
                b'D' => RED,
                b'R' | b'T' => crate::ui::idea::CONFLICT,
                _ => ACCENT,
            };
            assert_eq!(buf[(x, row.y)].fg, color, "{name}");
            let (glyph, tint) = icons.file(&file.path);
            let x = find_text(&buf, row, glyph);
            assert_eq!(buf[(x, row.y)].fg, tint, "{name}");
            assert!(text_at(&buf, row).contains(&format!(" {}", char::from(file.status))));
        }
        assert!((0..buf.area.height)
            .any(|y| text_at(&buf, Rect::new(90, y, 90, 1)).contains("dev/iframe-proxy 3 files")));
    }
}

#[test]
fn file_tree_mouse_keyboard_scrolling_and_pane_cycle_keep_push_selection() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let (mut p, c) = fixture(icons);
        p.preview.files.extend((0..30).map(|i| Change {
            path: format!("src/locales/extra-{i}/system.ts").into(),
            previous: None,
            status: b'M',
        }));
        p.preview.tree = FileTree::new(&p.preview.files);
        draw(&mut p, &c, 80, 24);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 1);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 0);
        let arrow = p
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::FileFold(0)))
            .unwrap()
            .0;
        click(&mut p, &c, arrow);
        assert_eq!(p.preview.tree.visible, [0]);
        assert_eq!(p.checked, BTreeSet::from([0]));
        assert!(!p.show_details);
        key(&mut p, &c, KeyCode::Char('l'));
        assert_eq!(p.preview.tree.visible.len(), p.preview.tree.nodes.len());
        key(&mut p, &c, KeyCode::Char('['));
        assert_eq!(p.preview.tree.visible, [0]);
        key(&mut p, &c, KeyCode::Char(']'));
        let len = p.preview.tree.visible.len();
        for _ in 0..len + 5 {
            key(&mut p, &c, KeyCode::Char('j'));
        }
        draw(&mut p, &c, 80, 24);
        assert_eq!(p.preview.tree.cursor, len - 1);
        assert!(p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::File(v) if *v == len - 1)));
        assert!(p.preview.tree.offset > 0);
        let collapse = p
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::FoldAll(true)))
            .unwrap()
            .0;
        click(&mut p, &c, collapse);
        assert_eq!(p.preview.tree.visible, [0]);
        key(&mut p, &c, KeyCode::Char('d'));
        assert!(p.show_details);
        assert_eq!(p.preview.body, p.preview.summary);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 2);
        key(&mut p, &c, KeyCode::Esc);
        assert_eq!(p.focus, 1);
        assert!(!p.show_details);
        assert_eq!(p.checked, BTreeSet::from([0]));
    }
}
