use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::MouseEvent;
use std::{fs, path::Path};

fn g(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn identity(repo: &Path) {
    for (key, value) in [
        ("user.name", "Update tester"),
        ("user.email", "update@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        ("pull.ff", "only"),
    ] {
        g(repo, &["config", key, value]);
    }
}

fn commit(repo: &Path, file: &str, contents: &str) -> String {
    fs::write(repo.join(file), contents).unwrap();
    g(repo, &["add", "--", file]);
    g(repo, &["commit", "-qm", contents.trim()]);
    head(repo)
}

fn head(repo: &Path) -> String {
    g(repo, &["rev-parse", "HEAD"])
}

fn tracking(repo: &Path) -> String {
    g(repo, &["rev-parse", "refs/remotes/origin/main"])
}

fn pair(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let local = dir.join("workspace").join(name);
    let remote = dir.join(format!("{name}.git"));
    let writer = dir.join(format!("{name}-writer"));
    fs::create_dir_all(&local).unwrap();
    g(&local, &["init", "-qb", "main"]);
    identity(&local);
    commit(&local, "base.txt", "base\n");
    g(
        &local,
        &["clone", "--bare", "-q", ".", remote.to_str().unwrap()],
    );
    g(
        &local,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    g(&local, &["fetch", "origin"]);
    g(&local, &["branch", "--set-upstream-to=origin/main"]);
    g(
        dir,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            writer.to_str().unwrap(),
        ],
    );
    identity(&writer);
    (local, writer)
}

fn incoming(writer: &Path, file: &str) -> String {
    let oid = commit(writer, file, "incoming\n");
    g(writer, &["push", "origin", "main"]);
    oid
}

fn workspace(dir: &Path) -> Context {
    let mut ctx = Context::discover(&dir.join("workspace")).unwrap();
    ctx.state_dir = dir.join("state");
    ctx
}

fn press(app: &mut Shell, ch: char) {
    app.dispatch(&key(KeyCode::Char(ch)));
}

fn click(app: &mut Shell, screen: &str, label: &str) {
    let (row, column) = screen
        .lines()
        .enumerate()
        .find_map(|(i, line)| line.find(label).map(|byte| (i, line[..byte].width())))
        .unwrap_or_else(|| panic!("missing {label:?} in {screen}"));
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column as u16,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));
}

fn finish(app: &mut Shell) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.help_content().context() != "Update / Results" {
        app.poll();
        assert!(Instant::now() < deadline, "update timed out: {}", app.msg);
        std::thread::sleep(Duration::from_millis(10));
    }
    screen(app, 120, 30)
}

#[test]
fn workspace_fetch_reviews_scope_and_updates_refs_only_after_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let (active, writer) = pair(dir.path(), "active");
    let (hidden, hidden_writer) = pair(dir.path(), "hidden");
    let next = incoming(&writer, "remote.txt");
    incoming(&hidden_writer, "remote.txt");
    let before = snapshot(&active);
    let old_tracking = tracking(&active);
    let hidden_tracking = tracking(&hidden);
    let mut ctx = workspace(dir.path());
    ctx.scope.retain(|index| ctx.repos[*index].name == "active");
    let mut app = Shell::new(ctx, IconMode::Ascii);
    press(&mut app, 'f');
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("Fetch Repositories")
            && text.contains("active: Ready")
            && text.contains("origin")
    );
    assert!(!text.contains("hidden") && !text.contains("Rebase"));
    app.dispatch(&key(KeyCode::Enter)); // Cancel is initially focused.
    assert!(app.pop.is_none());
    assert_eq!(tracking(&active), old_tracking);

    press(&mut app, 'f');
    let text = screen(&mut app, 44, 12);
    click(&mut app, &text, "y Fetch 1");
    assert!(finish(&mut app).contains("active: OK"));
    assert_eq!(tracking(&active), next);
    assert_eq!(snapshot(&active), before);
    assert_eq!(tracking(&hidden), hidden_tracking);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Commit);
    press(&mut app, '2');
    assert!(screen(&mut app, 120, 30).contains("incoming"));
}

#[test]
fn workspace_pull_integrates_diverged_branches_with_the_reviewed_strategy() {
    for rebase in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (local, writer) = pair(dir.path(), "project");
        let local_head = commit(&local, "local.txt", "local\n");
        let incoming_head = incoming(&writer, "remote.txt");
        let mut app = Shell::new(workspace(dir.path()), IconMode::Ascii);
        if rebase {
            app.go(Push);
        }
        press(&mut app, 'p');
        let text = screen(&mut app, 44, 12);
        assert!(text.contains("Update Project") && text.contains("main <- origin:main"));
        assert!(text.contains("(o) m Merge"));
        assert_eq!(head(&local), local_head);
        if rebase {
            click(&mut app, &text, "r Rebase");
            assert!(screen(&mut app, 44, 12).contains("(o) r Rebase"));
            app.dispatch(&key(KeyCode::Tab));
            app.dispatch(&key(KeyCode::Enter));
        } else {
            press(&mut app, 'y');
        }
        assert!(finish(&mut app).contains("project: OK"));
        let parents = g(&local, &["show", "-s", "--format=%P"]);
        if rebase {
            assert_eq!(parents, incoming_head);
            assert_ne!(head(&local), local_head);
        } else {
            assert_eq!(parents, format!("{local_head} {incoming_head}"));
        }
        assert_eq!(
            fs::read_to_string(local.join("local.txt")).unwrap(),
            "local\n"
        );
        assert_eq!(
            fs::read_to_string(local.join("remote.txt")).unwrap(),
            "incoming\n"
        );
        assert!(g(&local, &["status", "--porcelain"]).is_empty());
    }
}

#[test]
fn workspace_pull_keeps_conflicts_and_refuses_a_branch_switched_after_review() {
    let dir = tempfile::tempdir().unwrap();
    let (clean, clean_writer) = pair(dir.path(), "a-clean");
    let (conflict, conflict_writer) = pair(dir.path(), "b-conflict");
    let (stale, stale_writer) = pair(dir.path(), "c-stale");
    let (detached, _) = pair(dir.path(), "d-detached");
    let clean_next = incoming(&clean_writer, "remote.txt");
    let conflict_next = incoming(&conflict_writer, "base.txt");
    let conflict_head = commit(&conflict, "base.txt", "local conflict\n");
    incoming(&stale_writer, "remote.txt");
    g(&detached, &["switch", "--detach"]);
    let stale_before = snapshot(&stale);
    let stale_tracking = tracking(&stale);
    let mut app = Shell::new(workspace(dir.path()), IconMode::Ascii);
    press(&mut app, 'p');
    let text = screen(&mut app, 120, 30);
    assert!(text.lines().any(|line| line.contains("d-detached")
        && line.contains("Skipped")
        && line.contains("Detached HEAD")));
    g(&stale, &["switch", "-c", "same-head"]);
    g(&stale, &["branch", "--set-upstream-to=origin/main"]);
    press(&mut app, 'y');
    let text = finish(&mut app);
    assert!(text.contains("1 succeeded · 2 failed · 1 skipped"));
    for (name, status) in [
        ("a-clean", "OK"),
        ("b-conflict", "Failed"),
        ("c-stale", "Failed"),
        ("d-detached", "Skipped"),
    ] {
        assert!(
            text.lines()
                .any(|line| line.contains(name) && line.contains(status)),
            "missing {name}: {status} in {text}"
        );
    }
    click(&mut app, &text, "c-stale");
    assert!(screen(&mut app, 120, 30).contains("review again"));
    assert_eq!(head(&clean), clean_next);
    assert_eq!(head(&conflict), conflict_head);
    assert_eq!(g(&conflict, &["rev-parse", "MERGE_HEAD"]), conflict_next);
    assert!(!g(&conflict, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(snapshot(&stale), stale_before);
    assert_eq!(tracking(&stale), stale_tracking);
    app.dispatch(&key(KeyCode::Esc));
    assert!(screen(&mut app, 120, 30)
        .lines()
        .next()
        .unwrap()
        .contains("Conflicts"));
    press(&mut app, '6');
    assert_eq!(app.tab, Conflicts);
    assert!(screen(&mut app, 120, 30).contains("base.txt"));

    let conflict_path = conflict.canonicalize().unwrap();
    let root = app
        .ctx
        .repos
        .iter()
        .position(|r| r.path == conflict_path)
        .unwrap();
    press(&mut app, 'r');
    for _ in 0..root {
        app.dispatch(&key(KeyCode::Down));
    }
    press(&mut app, ' ');
    app.dispatch(&key(KeyCode::Enter));
    assert!(!app.ctx.scope.contains(&root));
    assert_eq!(app.tab, Commit);
    screen(&mut app, 120, 30);
    assert!(app.regions.tab(Conflicts).is_empty());

    press(&mut app, 'r');
    press(&mut app, 'a');
    app.dispatch(&key(KeyCode::Enter));
    screen(&mut app, 120, 30);
    assert!(!app.regions.tab(Conflicts).is_empty());
    assert!(!g(&conflict, &["ls-files", "--unmerged"]).is_empty());
}
