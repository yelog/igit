use super::*;
use crate::core::git;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::{fs, path::Path};
use unicode_width::UnicodeWidthStr;

mod navigation;
mod presentation;
mod tracking;

fn folder_index(panel: &BranchPanel, id: &str) -> usize {
    panel
        .tree
        .rows
        .iter()
        .position(|r| r.id == id && matches!(r.kind, Kind::Folder { .. }))
        .unwrap_or_else(|| panic!("Missing folder: {id}"))
}

fn run(path: &Path, args: &[&str]) -> String {
    git(path, args).unwrap()
}

fn reload(panel: &mut BranchPanel, c: &Context) {
    panel.refresh(c).unwrap();
    panel.finish_loading(c);
}

fn fixture() -> (tempfile::TempDir, Context, BranchPanel) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["api", "web"] {
        let repo = temp.path().join(name);
        fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Branch tester"),
            ("user.email", "branch@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            run(&repo, &["config", key, value]);
        }
        fs::write(repo.join("file"), "base\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "base"]);
    }
    let mut c = Context::discover(temp.path()).unwrap();
    c.state_dir = temp.path().join("state");
    let mut panel = BranchPanel {
        icons: IconMode::Ascii,
        ..BranchPanel::default()
    };
    reload(&mut panel, &c);
    (temp, c, panel)
}

fn source(c: &Context, name: &str) -> Branch {
    Snapshot::read(&c.repos[0]).unwrap().branches[name].clone()
}

fn review(c: &Context, name: &str, operation: Operation, target: &str) -> Review {
    Review::prepare(c, 0, &source(c, name), operation, target).unwrap()
}

fn select(panel: &mut BranchPanel, root: usize, reference: &str) {
    panel.tree.cursor = panel
        .tree
        .rows
        .iter()
        .position(|r| {
            r.root == root && matches!(&r.kind, Kind::Branch(b) if b.reference == reference)
        })
        .unwrap();
}

fn key(panel: &mut BranchPanel, c: &Context, code: K) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, Mods::NONE)), c)
}

fn render(panel: &mut BranchPanel, c: &Context, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), c)).unwrap();
    terminal
}

fn hit_rect(p: &BranchPanel, matches: impl Fn(&Hit) -> bool) -> Rect {
    p.hits
        .iter()
        .find_map(|(rect, hit)| matches(hit).then_some(*rect))
        .unwrap()
}

fn row_cells(terminal: &Terminal<TestBackend>, rect: Rect) -> String {
    let buffer = terminal.backend().buffer();
    let mut line = String::new();
    let mut x = rect.x;
    while x < rect.right() {
        let symbol = buffer[(x, rect.y)].symbol();
        line.push_str(symbol);
        x += symbol.width().max(1) as u16;
    }
    line
}

fn screen(panel: &mut BranchPanel, c: &Context, width: u16, height: u16) -> String {
    let terminal = render(panel, c, width, height);
    (0..height)
        .map(|y| row_cells(&terminal, Rect::new(0, y, width, 1)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn state(path: &Path) -> Vec<String> {
    [
        vec!["rev-parse", "HEAD"],
        vec!["symbolic-ref", "HEAD"],
        vec!["ls-files", "--stage"],
        vec!["diff", "--binary"],
        vec!["for-each-ref", "--format=%(refname) %(objectname)"],
    ]
    .iter()
    .map(|args| run(path, args))
    .collect()
}

fn tracking(c: &Context, local: &str, remote: &str) {
    let repo = &c.repos[0].path;
    run(repo, &["remote", "add", "origin", "../unused-local-remote"]);
    run(
        repo,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{remote}"),
            "HEAD",
        ],
    );
    run(
        repo,
        &[
            "branch",
            &format!("--set-upstream-to=origin/{remote}"),
            local,
        ],
    );
}

#[test]
fn branch_prioritizes_current_and_default_paths_with_all_groups_open() {
    let (_temp, c, mut p) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "team/active/current"]);
    run(repo, &["branch", "team/pending/other"]);
    run(repo, &["branch", "release/stable"]);
    run(repo, &["branch", "topic/other"]);
    tracking(&c, "release/stable", "release/stable");
    run(repo, &["update-ref", "refs/remotes/origin/aaa", "HEAD"]);
    run(
        repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release/stable",
        ],
    );
    let leaves = |p: &BranchPanel| {
        p.tree
            .rows
            .iter()
            .filter_map(|row| match &row.kind {
                Kind::Branch(branch) => Some(branch.name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    reload(&mut p, &c);
    assert_eq!(
        leaves(&p),
        [
            "team/active/current",
            "team/pending/other",
            "release/stable",
            "main",
            "topic/other",
            "origin/aaa",
            "origin/release/stable",
        ]
    );

    run(repo, &["branch", "-d", "release/stable"]);
    reload(&mut p, &c);
    let visible = leaves(&p);
    assert_eq!(
        &visible[visible.len() - 2..],
        ["origin/release/stable", "origin/aaa"]
    );
}

#[test]
fn tree_pins_slash_current_groups_remotes_and_shows_real_upstreams() {
    let (_temp, mut c, mut p) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "feature/z/current"]);
    for name in ["aaa", "feature/a", "feature/z/aaa", "fix/中文"] {
        run(repo, &["branch", name]);
    }
    tracking(&c, "feature/z/current", "release/next");
    run(
        repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release/next",
        ],
    );
    reload(&mut p, &c);
    let leaves: Vec<_> = p
        .tree
        .rows
        .iter()
        .filter(|r| r.root == 0)
        .filter_map(|r| {
            if let Kind::Branch(b) = &r.kind {
                Some(b)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(leaves[0].name, "feature/z/current");
    assert!(leaves[0].current);
    assert_eq!(leaves[0].upstream, "refs/remotes/origin/release/next");
    assert!(!leaves.iter().any(|b| b.name == "origin/HEAD"));
    assert!(p
        .tree
        .rows
        .iter()
        .any(|r| r.label == "origin" && matches!(r.kind, Kind::Folder { .. })));
    assert!(p
        .tree
        .rows
        .iter()
        .any(|r| r.label == "release" && r.depth == 2));
    select(&mut p, 0, "refs/heads/feature/z/current");
    let rendered = screen(&mut p, &c, 120, 36);
    assert!(
        rendered.contains("current (current) -> origin/release/next"),
        "{rendered}"
    );
    assert!(rendered.contains("api") && rendered.contains("web"));

    run(
        repo,
        &["update-ref", "-d", "refs/remotes/origin/release/next"],
    );
    reload(&mut p, &c);
    assert!(screen(&mut p, &c, 120, 36).contains("origin/release/next [gone]"));
    c.scope.remove(&1);
    reload(&mut p, &c);
    assert!(p.tree.rows.iter().all(|r| r.root == 0));
}

#[test]
fn empty_unborn_and_detached_repositories_are_browsable() {
    let (_temp, c, mut p) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "--detach"]);
    reload(&mut p, &c);
    assert!(screen(&mut p, &c, 80, 25).contains("detached@"));
    assert!(!Snapshot::read(&c.repos[0])
        .unwrap()
        .branches
        .values()
        .any(|b| b.current));
    assert!(Review::prepare(&c, 0, &source(&c, "refs/heads/main"), Operation::Merge, "").is_err());
    review(&c, "refs/heads/main", Operation::Checkout, "")
        .execute(&c)
        .unwrap();

    let empty = tempfile::tempdir().unwrap();
    run(empty.path(), &["init", "-q", "-b", "feature/first"]);
    let mut c = Context::discover(empty.path()).unwrap();
    c.state_dir = empty.path().join("state");
    p = BranchPanel::default();
    reload(&mut p, &c);
    let branch = source(&c, "refs/heads/feature/first");
    assert!(branch.current && branch.oid.is_empty());
    assert!(screen(&mut p, &c, 44, 12).contains("[no commits]"));
    let t = render(&mut p, &c, 120, 36);
    let row = hit_rect(&p, |hit| matches!(hit, Hit::Row(i) if *i == p.tree.cursor));
    assert!(row_cells(&t, row).trim_end().ends_with("[no commits]"));
    assert!(Review::prepare(&c, 0, &branch, Operation::Create, "new").is_err());
}

#[test]
fn browsing_folding_mouse_and_cancelled_unicode_input_do_not_write() {
    let (_temp, c, mut p) = fixture();
    let before = state(&c.repos[0].path);
    let initial = p.selection().unwrap().1;
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        let text = screen(&mut p, &c, w, h);
        assert!(text.contains("Local Branches"));
        assert!(text.contains("Remote Branches"));
        let (rect, _) = p
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, Hit::Row(i) if *i == p.tree.cursor))
            .unwrap();
        p.event(
            &Event::Mouse(MouseEvent {
                kind: M::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: Mods::NONE,
            }),
            &c,
        );
        assert_eq!(p.selection().unwrap().1, initial);
    }
    key(&mut p, &c, K::Char('a'));
    p.event(&Event::Paste("feature/中文\n".into()), &c);
    assert_eq!(p.input.as_ref().unwrap().name, "feature/中文");
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        assert!(screen(&mut p, &c, w, h).contains("Enter Review"));
    }
    key(&mut p, &c, K::Esc);
    assert!(!p.editing());
    key(&mut p, &c, K::Char('h'));
    let folder = p.tree.rows[p.tree.cursor].id.clone();
    key(&mut p, &c, K::Char('h'));
    reload(&mut p, &c);
    assert_eq!(p.tree.rows[p.tree.cursor].id, folder);
    assert!(matches!(
        p.tree.rows[p.tree.cursor].kind,
        Kind::Folder { open: false, .. }
    ));
    key(&mut p, &c, K::Char('l'));
    assert_eq!(state(&c.repos[0].path), before);
}

#[test]
fn create_from_focused_commit_then_rename_delete_and_switch_preserve_other_state() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    let other = state(&c.repos[1].path);
    run(repo, &["switch", "-c", "topic"]);
    fs::write(repo.join("file"), "topic\n").unwrap();
    run(repo, &["commit", "-am", "topic"]);
    let topic = run(repo, &["rev-parse", "HEAD"]);
    run(repo, &["switch", "main"]);
    review(&c, "refs/heads/topic", Operation::Create, "feature/中文")
        .execute(&c)
        .unwrap();
    assert_eq!(run(repo, &["rev-parse", "feature/中文"]), topic);
    assert_eq!(run(repo, &["branch", "--show-current"]), "main");
    review(
        &c,
        "refs/heads/feature/中文",
        Operation::Rename,
        "feature/renamed",
    )
    .execute(&c)
    .unwrap();
    review(&c, "refs/heads/feature/renamed", Operation::Checkout, "")
        .execute(&c)
        .unwrap();
    assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "topic\n");
    review(
        &c,
        "refs/heads/feature/renamed",
        Operation::Rename,
        "feature/current",
    )
    .execute(&c)
    .unwrap();
    assert_eq!(run(repo, &["branch", "--show-current"]), "feature/current");
    review(&c, "refs/heads/topic", Operation::Delete, "")
        .execute(&c)
        .unwrap();
    assert!(git(repo, &["show-ref", "--verify", "refs/heads/topic"]).is_err());
    assert_eq!(state(&c.repos[1].path), other);
}

#[test]
fn create_and_rename_keep_staged_and_unstaged_bytes_and_upstream() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    tracking(&c, "main", "primary");
    fs::write(repo.join("file"), "staged\n").unwrap();
    fs::write(repo.join("other"), "other staged\n").unwrap();
    run(repo, &["add", "."]);
    fs::write(repo.join("file"), "unstaged\n").unwrap();
    let index = run(repo, &["ls-files", "--stage"]);
    let diff = run(repo, &["diff", "--binary"]);
    review(&c, "refs/heads/main", Operation::Create, "copy")
        .execute(&c)
        .unwrap();
    review(&c, "refs/heads/main", Operation::Rename, "renamed")
        .execute(&c)
        .unwrap();
    assert_eq!(
        source(&c, "refs/heads/renamed").upstream,
        "refs/remotes/origin/primary"
    );
    assert_eq!(run(repo, &["ls-files", "--stage"]), index);
    assert_eq!(run(repo, &["diff", "--binary"]), diff);
    assert!(source(&c, "refs/heads/copy").upstream.is_empty());
}

#[test]
fn unsafe_delete_names_and_other_worktree_operations_are_refused() {
    let (temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    let main = source(&c, "refs/heads/main");
    for name in ["", "main", "-force", "bad..name", "@{-1}", "bad name"] {
        assert!(
            Review::prepare(&c, 0, &main, Operation::Create, name).is_err(),
            "{name}"
        );
    }
    assert!(Review::prepare(&c, 0, &main, Operation::Delete, "").is_err());
    run(repo, &["switch", "-c", "unmerged"]);
    fs::write(repo.join("file"), "unique\n").unwrap();
    run(repo, &["commit", "-am", "unique"]);
    run(repo, &["switch", "main"]);
    let before = state(repo);
    assert!(review(&c, "refs/heads/unmerged", Operation::Delete, "")
        .execute(&c)
        .is_err());
    assert_eq!(state(repo), before);
    run(
        repo,
        &[
            "worktree",
            "add",
            "-b",
            "other-worktree",
            temp.path().join("linked").to_str().unwrap(),
            "main",
        ],
    );
    let branch = source(&c, "refs/heads/other-worktree");
    for op in [Operation::Delete, Operation::Rename, Operation::Checkout] {
        assert!(Review::prepare(&c, 0, &branch, op, "renamed").is_err());
    }
}

#[test]
fn reviewed_operations_reject_changed_source_head_scope_and_workspace() {
    let (_temp, mut c, _) = fixture();
    let repo = c.repos[0].path.clone();
    let plan = review(&c, "refs/heads/main", Operation::Create, "stale");
    run(&repo, &["commit", "--allow-empty", "-qm", "external"]);
    assert!(plan
        .execute(&c)
        .unwrap_err()
        .to_string()
        .contains("changed"));
    let plan = review(&c, "refs/heads/main", Operation::Create, "hidden");
    c.scope.remove(&0);
    assert!(plan.execute(&c).is_err());
    c.scope.insert(0);
    let plan = review(&c, "refs/heads/main", Operation::Create, "elsewhere");
    c.repos.swap(0, 1);
    assert!(plan.execute(&c).is_err());
    assert_eq!(
        run(
            &repo,
            &["for-each-ref", "--format=%(refname)", "refs/heads/"]
        ),
        "refs/heads/main"
    );
}

#[test]
fn checkout_preserves_local_edits_and_refuses_overwrite_even_after_review() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "topic"]);
    fs::write(repo.join("file"), "topic\n").unwrap();
    run(repo, &["commit", "-am", "topic"]);
    run(repo, &["switch", "main"]);
    let plan = review(&c, "refs/heads/topic", Operation::Checkout, "");
    fs::write(repo.join("file"), "keep this\n").unwrap();
    let before = state(repo);
    assert!(plan.execute(&c).is_err());
    assert_eq!(state(repo), before);
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        "keep this\n"
    );
}

#[test]
fn remote_checkout_creates_tracking_branch_and_reuses_it_without_remote_writes() {
    let (_temp, c, mut p) = fixture();
    let repo = &c.repos[0].path;
    tracking(&c, "main", "feature/remote");
    let remote = run(repo, &["rev-parse", "refs/remotes/origin/feature/remote"]);
    reload(&mut p, &c);
    select(&mut p, 0, "refs/remotes/origin/feature/remote");
    key(&mut p, &c, K::Enter);
    assert_eq!(p.input.as_ref().unwrap().name, "feature/remote");
    assert!(matches!(key(&mut p, &c, K::Enter), Action::Confirm { .. }));
    assert!(matches!(p.confirm("branch:apply", &c), Action::Refresh(_)));
    assert_eq!(run(repo, &["branch", "--show-current"]), "feature/remote");
    assert_eq!(
        source(&c, "refs/heads/feature/remote").upstream,
        "refs/remotes/origin/feature/remote"
    );
    run(repo, &["switch", "main"]);
    reload(&mut p, &c);
    select(&mut p, 0, "refs/remotes/origin/feature/remote");
    assert!(matches!(key(&mut p, &c, K::Enter), Action::Confirm { .. }));
    assert!(p.input.is_none());
    p.confirm("branch:apply", &c);
    assert_eq!(run(repo, &["branch", "--show-current"]), "feature/remote");
    assert_eq!(
        run(repo, &["rev-parse", "refs/remotes/origin/feature/remote"]),
        remote
    );
    assert!(Review::prepare(
        &c,
        0,
        &source(&c, "refs/remotes/origin/feature/remote"),
        Operation::Delete,
        ""
    )
    .is_err());
}

#[test]
fn merge_fast_forward_and_conflicts_preserve_destination_and_pending_operation() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "incoming"]);
    fs::write(repo.join("file"), "incoming\n").unwrap();
    run(repo, &["commit", "-am", "incoming"]);
    let incoming = run(repo, &["rev-parse", "HEAD"]);
    run(repo, &["switch", "main"]);
    review(&c, "refs/heads/incoming", Operation::Merge, "")
        .execute(&c)
        .unwrap();
    assert_eq!(run(repo, &["rev-parse", "main"]), incoming);
    assert_eq!(run(repo, &["branch", "--show-current"]), "main");
    run(repo, &["switch", "incoming"]);
    fs::write(repo.join("file"), "new incoming\n").unwrap();
    run(repo, &["commit", "-am", "incoming again"]);
    run(repo, &["switch", "main"]);
    fs::write(repo.join("file"), "current\n").unwrap();
    run(repo, &["commit", "-am", "current"]);
    let head = run(repo, &["rev-parse", "HEAD"]);
    let error = review(&c, "refs/heads/incoming", Operation::Merge, "")
        .execute(&c)
        .unwrap_err()
        .to_string();
    assert!(error.contains("CONFLICT"), "{error}");
    assert_eq!(run(repo, &["rev-parse", "HEAD"]), head);
    assert!(!run(repo, &["ls-files", "--unmerged"]).is_empty());
    assert!(Review::prepare(
        &c,
        0,
        &source(&c, "refs/heads/incoming"),
        Operation::Checkout,
        ""
    )
    .is_err());
}

#[test]
fn merge_rechecks_dirty_index_and_does_not_autostash() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["branch", "topic"]);
    run(repo, &["config", "merge.autoStash", "true"]);
    let plan = review(&c, "refs/heads/topic", Operation::Merge, "");
    fs::write(repo.join("file"), "dirty\n").unwrap();
    run(repo, &["add", "file"]);
    let before = state(repo);
    assert!(plan.execute(&c).is_err());
    assert_eq!(state(repo), before);
    assert!(run(repo, &["stash", "list"]).is_empty());
    assert!(Review::prepare(&c, 0, &source(&c, "refs/heads/topic"), Operation::Merge, "").is_err());
}
