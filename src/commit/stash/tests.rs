use super::*;
use crate::core::{git, Repo};
use crossterm::event::KeyEvent;
use ratatui::{backend::TestBackend, Terminal};

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = vec![];
    for name in ["app", "api"] {
        let path = dir.path().join(name);
        fs::create_dir(&path).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Fixture"],
            &["config", "user.email", "fixture@invalid.test"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.hooksPath", ".git/hooks"],
        ] {
            git(&path, args).unwrap();
        }
        fs::write(path.join("mixed.txt"), "base\n").unwrap();
        fs::write(path.join("unchecked.txt"), "base\n").unwrap();
        git(&path, &["add", "."]).unwrap();
        git(&path, &["commit", "-qm", "base"]).unwrap();
        fs::write(path.join("mixed.txt"), "staged\n").unwrap();
        fs::write(path.join("unchecked.txt"), "unchecked staged\n").unwrap();
        git(&path, &["add", "."]).unwrap();
        fs::write(path.join("mixed.txt"), "working\n").unwrap();
        fs::write(path.join("untracked.txt"), "new\n").unwrap();
        repos.push(Repo {
            path,
            name: name.into(),
            branch: "main".into(),
            parent: None,
            submodule: false,
        });
    }
    let c = Context {
        workspace: dir.path().into(),
        state_dir: dir.path().join("state"),
        repos,
        scope: [0, 1].into(),
    };
    let mut p = CommitPanel::default();
    for (root, repo) in c.repos.iter().enumerate() {
        for change in backend::status(&repo.path).unwrap() {
            let key = format!("{}:{}", rid(&c, root), keypart(&change.path));
            p.checks.insert(key.clone(), false);
            p.files.push(File {
                key,
                root,
                change,
                group: DEF.into(),
            });
        }
    }
    p.saved.message = Some("keep this commit draft".into());
    p.sync(&c);
    (dir, c, p)
}

fn key(p: &mut CommitPanel, c: &Context, key: K) -> Action {
    p.event(&Event::Key(KeyEvent::new(key, Mod::NONE)), c)
}

fn review(p: &mut CommitPanel, c: &Context) -> Action {
    key(p, c, K::Char('s'));
    p.buf = "shared stash 中文".into();
    p.caret = p.buf.len();
    key(p, c, K::Enter)
}

fn count(c: &Context, root: usize) -> usize {
    git(&c.repos[root].path, &["stash", "list"])
        .unwrap()
        .lines()
        .count()
}

#[test]
fn stash_shortcut_reviews_all_repositories_and_ignores_checkboxes() {
    let (_dir, c, mut p) = fixture();
    let draft = p.saved.message.clone();
    let checks = p.checks.clone();
    let Action::Confirm { body, token, .. } = review(&mut p, &c) else {
        panic!("review");
    };
    assert_eq!(token, "commit-stash");
    for name in [
        "app",
        "api",
        "unchecked.txt",
        "untracked.txt",
        "shared stash 中文",
    ] {
        assert!(body.contains(name), "{name}");
    }
    assert_eq!(p.stash.plans.len(), 2);
    assert_eq!(p.checks, checks);
    assert_eq!(count(&c, 0), 0);
    let Action::Refresh(NoticeLevel::Success, results) = p.confirm(&token, &c) else {
        panic!("stash");
    };
    assert!(results.contains("SAVED · app"));
    assert!(results.contains("SAVED · api"));
    for root in 0..2 {
        let repo = &c.repos[root].path;
        assert!(backend::status(repo).unwrap().is_empty());
        assert!(git(repo, &["stash", "list"])
            .unwrap()
            .contains("shared stash 中文"));
        assert_eq!(
            git(repo, &["show", "stash@{0}^2:unchecked.txt"]).unwrap(),
            "unchecked staged"
        );
        assert_eq!(
            git(repo, &["show", "stash@{0}:mixed.txt"]).unwrap(),
            "working"
        );
        assert_eq!(
            git(repo, &["show", "stash@{0}^3:untracked.txt"]).unwrap(),
            "new"
        );
    }
    assert_eq!(p.saved.message, draft);
    assert!(matches!(p.confirm(&token, &c), Action::None));
    assert_eq!(count(&c, 0), 1);
}

#[test]
fn stash_scope_is_reviewed_and_rechecked_before_execution() {
    let (_dir, mut c, mut p) = fixture();
    c.scope = [0].into();
    assert!(matches!(review(&mut p, &c), Action::Confirm { .. }));
    assert_eq!(p.stash.plans.len(), 1);
    c.scope = [1].into();
    let Action::Refresh(NoticeLevel::Error, results) = p.confirm("commit-stash", &c) else {
        panic!("results");
    };
    assert!(results.contains("outside Roots"));
    assert_eq!(count(&c, 0), 0);
    assert_eq!(count(&c, 1), 0);
    review(&mut p, &c);
    p.confirm("commit-stash", &c);
    assert_eq!(count(&c, 0), 0);
    assert_eq!(count(&c, 1), 1);
}

#[test]
fn stash_partial_failure_retry_only_saves_remaining_repository() {
    let (_dir, c, mut p) = fixture();
    review(&mut p, &c);
    fs::write(c.repos[1].path.join("mixed.txt"), "external edit\n").unwrap();
    let Action::Refresh(NoticeLevel::Warning, results) = p.confirm("commit-stash", &c) else {
        panic!("results");
    };
    assert!(results.contains("SAVED · app"));
    assert!(results.contains("FAILED · api"));
    let first = git(&c.repos[0].path, &["rev-parse", "refs/stash"]).unwrap();
    // Use the same stale rendered list: planning must reread Git, not re-save
    // repositories that succeeded or depend on a background status refresh.
    key(&mut p, &c, K::Char('s'));
    assert_eq!(p.buf, "shared stash 中文");
    assert!(matches!(key(&mut p, &c, K::Enter), Action::Confirm { .. }));
    assert_eq!(p.stash.plans.len(), 1);
    assert_eq!(p.stash.plans[0].root, 1);
    p.confirm("commit-stash", &c);
    assert_eq!(count(&c, 0), 1);
    assert_eq!(count(&c, 1), 1);
    assert_eq!(
        git(&c.repos[0].path, &["rev-parse", "refs/stash"]).unwrap(),
        first
    );
}

#[test]
fn stash_preparation_errors_do_not_stop_other_repositories() {
    let (_dir, c, mut p) = fixture();
    fs::write(
        c.repos[1].path.join(".git/MERGE_HEAD"),
        git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap(),
    )
    .unwrap();
    let Action::Confirm { body, .. } = review(&mut p, &c) else {
        panic!("review");
    };
    assert!(body.contains("SKIP · api"));
    assert_eq!(p.stash.plans.len(), 1);
    let Action::Refresh(NoticeLevel::Warning, results) = p.confirm("commit-stash", &c) else {
        panic!("results");
    };
    assert!(results.contains("SKIP · api"));
    assert_eq!(count(&c, 0), 1);
    assert_eq!(count(&c, 1), 0);
}

#[test]
fn stash_input_cancel_empty_input_and_literal_keys_do_not_write() {
    let (_dir, c, mut p) = fixture();
    let indexes: Vec<_> = c
        .repos
        .iter()
        .map(|r| fs::read(r.path.join(".git/index")).unwrap())
        .collect();
    let draft = p.saved.message.clone();
    key(&mut p, &c, K::Char('s'));
    assert!(matches!(
        key(&mut p, &c, K::Enter),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    key(&mut p, &c, K::Char('s'));
    assert_eq!(p.buf, "s");
    p.event(&Event::Paste("中文\nmessage".into()), &c);
    assert_eq!(p.buf, "s中文 message");
    key(&mut p, &c, K::Esc);
    assert!(!p.editing());
    assert_eq!(p.saved.message, draft);
    key(&mut p, &c, K::Char('i'));
    key(&mut p, &c, K::Char('s'));
    assert!(p.buf.ends_with('s'));
    assert!(matches!(p.edit, Some(Edit::Message)));
    key(&mut p, &c, K::Esc);
    for (root, index) in indexes.iter().enumerate() {
        assert_eq!(
            fs::read(c.repos[root].path.join(".git/index")).unwrap(),
            *index
        );
        assert_eq!(count(&c, root), 0);
    }
}

#[test]
fn stash_message_dialog_fits_and_status_toggle_is_preserved() {
    let (_dir, c, mut p) = fixture();
    key(&mut p, &c, K::Char('T'));
    assert!(p.options.status);
    key(&mut p, &c, K::Char('s'));
    assert!(p.options.status);
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| p.draw(f, f.area(), &c)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Create stash"), "{width}x{height}");
        assert!(text.contains("Review"));
        assert!(text.contains("Cancel"));
        assert!(p.hits.iter().any(|(_, hit)| matches!(hit, Hit::EditSave)));
    }
}

#[cfg(unix)]
#[test]
fn stash_retry_does_not_duplicate_a_published_stash_with_cleanup_failure() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, c, mut p) = fixture();
    let hook = c.repos[0].path.join(".git/hooks/reference-transaction");
    fs::write(&hook, "#!/bin/sh\nif [ \"$1\" = committed ]; then\nwhile read old new ref; do\nif [ \"$ref\" = refs/stash ]; then : > .git/index.lock; fi\ndone\nfi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    review(&mut p, &c);
    let Action::Refresh(NoticeLevel::Warning, results) = p.confirm("commit-stash", &c) else {
        panic!("results");
    };
    assert!(results.contains("cleanup failed"));
    fs::remove_file(c.repos[0].path.join(".git/index.lock")).unwrap();
    let Action::Notice(NoticeLevel::Error, message) = review(&mut p, &c) else {
        panic!("retry must not write");
    };
    assert!(message.contains("already saved"));
    assert_eq!(count(&c, 0), 1);
    assert_eq!(count(&c, 1), 1);
}
