use super::*;
use std::fs;

fn screen(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| text_at(buf, Rect::new(0, y, buf.area.width, 1)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn changed_files_diff_uses_the_workspace_and_returns_to_the_reviewed_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(dir.path(), "app");
    let git = |args: &[&str]| git(&repo, args).unwrap();
    let script = |value: &str| -> String {
        (0..30)
            .map(|i| {
                if i == 12 {
                    format!("const item = '{value}';\n")
                } else {
                    format!("// line {i}\n")
                }
            })
            .collect()
    };
    fs::write(repo.join("example.js"), script("old")).unwrap();
    git(&["add", "example.js"]);
    git(&["commit", "-qm", "Add script"]);
    git(&["push", "-q", "origin", "main"]);
    fs::write(repo.join("example.js"), script("new")).unwrap();
    fs::write(repo.join("binary.bin"), b"binary\0content").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "Review outgoing files"]);

    let c = Context::discover(dir.path()).unwrap();
    let mut p = PushPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    p.finish_loading(&c);
    let buf = draw(&mut p, &c, 120, 36);
    let file = p
        .hits
        .iter()
        .find(|(row, hit)| {
            matches!(hit, Hit::File(_)) && text_at(&buf, *row).contains("example.js")
        })
        .unwrap()
        .0;
    click(&mut p, &c, file);
    let file_cursor = p.preview.tree.cursor;
    let loading = screen(&draw(&mut p, &c, 120, 36));
    assert!(loading.contains("Loading diff…"));
    assert!(p.editing() && p.hits.is_empty());
    key(&mut p, &c, KeyCode::Char('P'));
    assert!(p.review.is_empty());

    p.finish_loading(&c);
    let buf = draw(&mut p, &c, 120, 36);
    let rendered = screen(&buf);
    for label in [
        "Repository Diff: example.js",
        "2/2 files",
        "Before",
        "After",
        "const item = 'old';",
        "const item = 'new';",
        "unchanged lines",
    ] {
        assert!(rendered.contains(label), "Missing {label}:\n{rendered}");
    }
    assert!(!rendered.contains("Outgoing commits"));
    let x = find_text(&buf, Rect::new(0, 1, 120, 1), " < ");
    click(&mut p, &c, Rect::new(x + 1, 1, 1, 1));
    p.finish_loading(&c);
    let fallback = screen(&draw(&mut p, &c, 120, 36));
    assert!(fallback.contains("1/2 files"));
    assert!(fallback.contains("Binary or non-UTF-8 content"));
    assert!(fallback.contains("Binary files /dev/null and b/binary.bin differ"));
    key(&mut p, &c, KeyCode::Char(']'));
    p.finish_loading(&c);

    let narrow = screen(&draw(&mut p, &c, 44, 12));
    assert!(narrow.contains("After") && !narrow.contains("Before"));
    key(&mut p, &c, KeyCode::Tab);
    let narrow = screen(&draw(&mut p, &c, 44, 12));
    assert!(narrow.contains("Before") && !narrow.contains("After"));

    let buf = draw(&mut p, &c, 120, 36);
    let x = find_text(&buf, Rect::new(0, 0, 120, 1), "Close");
    click(&mut p, &c, Rect::new(x, 0, 1, 1));
    assert!(!p.editing());
    assert_eq!(p.focused_pane(), PaneId::PushFiles);
    assert_eq!(p.preview.tree.cursor, file_cursor);
    key(&mut p, &c, KeyCode::Enter);
    assert!(p.editing());
    key(&mut p, &c, KeyCode::Esc);
    assert!(!p.editing());
    let buf = draw(&mut p, &c, 120, 36);
    assert!(screen(&buf).contains("Cancel   Push "));
    let details = p
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::Details))
        .unwrap()
        .0;
    click(&mut p, &c, details);
    assert!(p.editing());
    key(&mut p, &c, KeyCode::Esc);
    key(&mut p, &c, KeyCode::Char('k'));
    key(&mut p, &c, KeyCode::Char('k'));
    key(&mut p, &c, KeyCode::Char('d'));
    assert!(p.show_details && !p.editing());
    key(&mut p, &c, KeyCode::Char('j'));
    key(&mut p, &c, KeyCode::Char('d'));
    assert!(!p.show_details && !p.editing());
    key(&mut p, &c, KeyCode::Char('d'));
    assert!(p.editing());
    key(&mut p, &c, KeyCode::Char('d'));
    assert!(!p.editing());
    assert_eq!(p.checked, BTreeSet::from([0]));
    assert!(p.review.is_empty() && !p.busy());
}
