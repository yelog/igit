use super::*;
use crate::core::{git, Repo};

pub(super) fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = vec![];
    for name in ["app", "api"] {
        let path = dir.path().join(name);
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]).unwrap();
        for (key, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@invalid.test"),
            ("commit.gpgSign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            git(&path, &["config", key, value]).unwrap();
        }
        for file in ["mixed.txt", "unselected.txt"] {
            fs::write(path.join(file), "base\n").unwrap();
        }
        git(&path, &["add", "."]).unwrap();
        git(&path, &["commit", "-qm", "base"]).unwrap();
        fs::write(path.join("mixed.txt"), format!("{name} staged\n")).unwrap();
        fs::write(path.join("unselected.txt"), "unselected staged marker\n").unwrap();
        git(&path, &["add", "."]).unwrap();
        fs::write(path.join("mixed.txt"), "unstaged private marker\n").unwrap();
        fs::write(path.join("untracked.txt"), "untracked private marker\n").unwrap();
        repos.push(Repo {
            path,
            name: name.into(),
            branch: "main".into(),
            parent: None,
            submodule: false,
        });
    }
    let context = Context {
        workspace: dir.path().into(),
        state_dir: dir.path().join("state"),
        repos,
        scope: [0, 1].into(),
    };
    let mut panel = CommitPanel::default();
    panel.refresh(&context).unwrap();
    for file in &panel.files {
        panel
            .checks
            .insert(file.key.clone(), file.change.path == Path::new("mixed.txt"));
    }
    panel.saved.message = Some("feat: shared subject\n\nShared body 中文".into());
    (dir, context, panel)
}

#[test]
fn shared_message_commits_each_repo_and_preserves_unselected_index_and_worktree() {
    let (_dir, c, mut panel) = fixture();
    let message = panel.saved.message.clone().unwrap();
    let Action::Committed {
        repos,
        level: NoticeLevel::Success,
        ..
    } = panel.submit(&c, false).unwrap()
    else {
        panic!("commit");
    };
    assert_eq!(repos, [0, 1]);
    for repo in &c.repos {
        assert_eq!(
            git(&repo.path, &["log", "-1", "--format=%B"])
                .unwrap()
                .trim(),
            message
        );
        assert_eq!(
            git(&repo.path, &["show", "HEAD:mixed.txt"]).unwrap(),
            format!("{} staged", repo.name)
        );
        assert_eq!(
            git(&repo.path, &["show", "HEAD:unselected.txt"]).unwrap(),
            "base"
        );
        assert_eq!(
            git(&repo.path, &["diff", "--cached", "--name-only"])
                .unwrap()
                .trim(),
            "unselected.txt"
        );
        assert_eq!(
            git(&repo.path, &["show", ":unselected.txt"]).unwrap(),
            "unselected staged marker"
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("mixed.txt")).unwrap(),
            "unstaged private marker\n"
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("untracked.txt")).unwrap(),
            "untracked private marker\n"
        );
    }
    assert_eq!(panel.saved.message.as_deref(), Some(""));
}

#[test]
fn partial_failure_keeps_shared_message_and_retry_skips_successful_repository() {
    let (_dir, c, mut panel) = fixture();
    let message = panel.saved.message.clone();
    let plans = panel.prepare_submission(&c).unwrap();
    git(&c.repos[1].path, &["add", "mixed.txt"]).unwrap(); // Invalidate only the second snapshot.
    let Action::Committed {
        repos,
        level: NoticeLevel::Warning,
        ..
    } = panel.execute_submission(&c, plans, true)
    else {
        panic!("commit");
    };
    assert_eq!(repos, [0]);
    assert_eq!(panel.saved.message, message);
    let successful_head = git(&c.repos[0].path, &["rev-parse", "HEAD"]).unwrap();
    panel.refresh(&c).unwrap();
    assert_eq!(panel.checked_repos(&c), [1]);
    let Action::Committed {
        repos,
        level: NoticeLevel::Success,
        ..
    } = panel.submit(&c, false).unwrap()
    else {
        panic!("retry");
    };
    assert_eq!(repos, [1]);
    assert_eq!(
        git(&c.repos[1].path, &["log", "-1", "--format=%B"]).unwrap(),
        message.unwrap()
    );
    assert_eq!(
        git(&c.repos[0].path, &["rev-parse", "HEAD"]).unwrap(),
        successful_head
    );
    assert_eq!(
        git(&c.repos[1].path, &["rev-list", "--count", "HEAD"])
            .unwrap()
            .trim(),
        "2"
    );
}

#[test]
fn shared_message_scope_excludes_hidden_repository_and_preserves_its_selection() {
    let (_dir, mut c, mut panel) = fixture();
    let hidden_head = git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap();
    c.scope = [0].into();
    assert!(
        matches!(panel.submit(&c, false).unwrap(), Action::Committed { repos, .. } if repos == [0])
    );
    assert_eq!(
        git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap(),
        hidden_head
    );
    assert!(!panel.saved.message.as_deref().unwrap().is_empty());
    c.scope.insert(1);
    assert_eq!(panel.checked_repos(&c), [1]);
}

#[test]
fn legacy_drafts_are_migrated_once_and_retained_for_recovery() {
    let (_dir, c, mut panel) = fixture();
    panel.saved.message = None;
    panel.saved.drafts = [
        (rid(&c, 0), "legacy app".into()),
        (rid(&c, 1), "legacy api".into()),
    ]
    .into();
    panel.save(&c).unwrap();
    let mut reopened = CommitPanel::default();
    reopened.refresh(&c).unwrap();
    assert_eq!(reopened.saved.message.as_deref(), Some("legacy app"));
    assert_eq!(reopened.saved.drafts.len(), 2);
    reopened.saved.message = Some(String::new());
    reopened.save(&c).unwrap();
    let mut again = CommitPanel::default();
    again.refresh(&c).unwrap();
    assert_eq!(again.saved.message.as_deref(), Some(""));
    assert_eq!(again.saved.drafts, reopened.saved.drafts);
}

#[test]
fn amend_requires_an_unambiguous_highlighted_repository() {
    let (_dir, c, mut panel) = fixture();
    assert!(matches!(
        panel.launch(Hit::Amend, &c),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    assert!(panel.amend.is_empty());
    let checks = panel.checks.clone();
    panel.set_all_expanded(false, &c);
    panel.browse_repo(true, &c);
    assert_eq!(panel.highlighted_repo(), Some(1));
    assert_eq!(panel.checks, checks);
    panel.launch(Hit::Amend, &c);
    assert_eq!(panel.amend, [1].into());
    let plans = panel.prepare_submission(&c).unwrap();
    assert!(!plans[0].amend && plans[1].amend);
}
