use super::*;
use crate::{
    core::{Context, Repo},
    merge::{Hit, MergePanel},
    ui::{Action, Panel},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

fn git(repo: &Path, args: &[&str]) -> String {
    String::from_utf8(checked(repo, args).unwrap())
        .unwrap()
        .trim()
        .to_owned()
}

fn fixture(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Recovery tester"),
        ("user.email", "recovery@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        ("rebase.backend", "merge"),
        ("rebase.autostash", "false"),
        ("rebase.updateRefs", "false"),
        ("merge.autostash", "false"),
    ] {
        git(repo, &["config", key, value]);
    }
    fs::write(repo.join("file.txt"), "base\n").unwrap();
    fs::write(repo.join("unrelated.txt"), "baseline\n").unwrap();
    fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "base"]);
    git(repo, &["switch", "-qc", "incoming"]);
    fs::write(repo.join("file.txt"), "incoming\n").unwrap();
    git(repo, &["commit", "-qam", "incoming"]);
    git(repo, &["switch", "-q", "main"]);
    fs::write(repo.join("file.txt"), "local\n").unwrap();
    git(repo, &["commit", "-qam", "local"]);
    let args: &[&str] = match name {
        "merge" => &["merge", "--no-edit", "incoming"],
        "rebase" => &["rebase", "incoming"],
        "cherry-pick" => &["cherry-pick", "incoming"],
        "revert" => &["revert", "--no-edit", "incoming"],
        _ => panic!("Unsupported fixture operation"),
    };
    assert!(!run(repo, args).unwrap().status.success());
    assert_eq!(operation(repo).unwrap().unwrap().name, name);
    assert_eq!(conflicts(0, repo).unwrap().len(), 1);
    dir
}

fn context(roots: &[&Path]) -> Context {
    Context {
        repos: roots
            .iter()
            .enumerate()
            .map(|(id, path)| Repo {
                path: path.to_path_buf(),
                name: format!("repo-{id}"),
                branch: "main".into(),
                submodule: false,
                parent: None,
            })
            .collect(),
        scope: (0..roots.len()).collect(),
        workspace: roots[0].into(),
        state_dir: roots[0].into(),
    }
}

fn key(panel: &mut MergePanel, ctx: &Context, code: KeyCode) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), ctx)
}

#[test]
fn reviewed_abort_restores_each_native_operation() {
    for name in ["merge", "rebase", "cherry-pick", "revert"] {
        let dir = fixture(name);
        let repo = dir.path();
        let main = git(repo, &["rev-parse", "main"]);
        let original = State::read(repo).unwrap();
        let (plan, body) = RecoveryPlan::review(repo, RecoveryAction::Abort).unwrap();
        assert!(body.contains(&format!("git {name} --abort")));
        assert!(body.contains("file.txt") && body.contains("staged changes"));
        assert_eq!(
            State::read(repo).unwrap(),
            original,
            "review must not write"
        );
        let message = plan.execute(repo).unwrap();
        assert!(
            message.contains("completed") && message.contains("Operation ended"),
            "{message}"
        );
        assert!(operation(repo).unwrap().is_none());
        assert!(conflicts(0, repo).unwrap().is_empty());
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), main);
        assert_eq!(git(repo, &["symbolic-ref", "--short", "HEAD"]), "main");
        assert_eq!(fs::read(repo.join("file.txt")).unwrap(), b"local\n");
    }
}

#[test]
fn reviewed_skip_discards_only_current_commit_in_supported_operations() {
    for name in ["rebase", "cherry-pick", "revert"] {
        let dir = fixture(name);
        let repo = dir.path();
        let expected = git(
            repo,
            &[
                "rev-parse",
                if name == "rebase" { "incoming" } else { "main" },
            ],
        );
        let (plan, body) = RecoveryPlan::review(repo, RecoveryAction::Skip).unwrap();
        assert!(body.contains("Discard the current replayed commit"));
        let message = plan.execute(repo).unwrap();
        assert!(
            message.contains("completed") && message.contains("Operation ended"),
            "{message}"
        );
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), expected);
        assert!(conflicts(0, repo).unwrap().is_empty());
        assert!(operation(repo).unwrap().is_none());
    }
}

#[test]
fn native_recovery_refusal_reports_surviving_conflicts_without_claiming_success() {
    let dir = fixture("merge");
    let repo = dir.path();
    // A native lock is already held, so Git itself must refuse this command
    // and retain the recoverable conflict state.
    fs::write(
        git_path(repo, "index.lock").unwrap(),
        "held by another Git command\n",
    )
    .unwrap();
    let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let working = fs::read(repo.join("file.txt")).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]);
    let (plan, _) = RecoveryPlan::review(repo, RecoveryAction::Abort).unwrap();
    let message = plan.execute(repo).unwrap();
    assert!(
        message.contains("stopped") && message.contains("still pending"),
        "{message}"
    );
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(fs::read(repo.join("file.txt")).unwrap(), working);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
}

#[test]
fn merge_and_stash_conflicts_do_not_offer_skip_or_invent_stash_abort() {
    let dir = fixture("merge");
    let repo = dir.path();
    let before = State::read(repo).unwrap();
    assert!(RecoveryPlan::review(repo, RecoveryAction::Skip).is_err());
    assert_eq!(State::read(repo).unwrap(), before);
    git(repo, &["merge", "--abort"]);
    fs::write(repo.join("file.txt"), "stashed edits\n").unwrap();
    git(repo, &["stash", "push", "-qm", "saved"]);
    let stash = git(repo, &["rev-parse", "refs/stash"]);
    fs::write(repo.join("file.txt"), "new committed edits\n").unwrap();
    git(repo, &["commit", "-qam", "after stash"]);
    assert!(!run(repo, &["stash", "apply"]).unwrap().status.success());
    assert!(operation(repo).unwrap().is_none());
    assert_eq!(conflicts(0, repo).unwrap().len(), 1);
    let index = fs::read(git_path(repo, "index").unwrap()).unwrap();
    let working = fs::read(repo.join("file.txt")).unwrap();
    for action in [RecoveryAction::Abort, RecoveryAction::Skip] {
        assert!(RecoveryPlan::review(repo, action).is_err());
    }
    assert_eq!(fs::read(git_path(repo, "index").unwrap()).unwrap(), index);
    assert_eq!(fs::read(repo.join("file.txt")).unwrap(), working);
    assert_eq!(git(repo, &["rev-parse", "refs/stash"]), stash);
}

#[test]
fn stale_recovery_reviews_preserve_external_content_index_and_operation_metadata() {
    for change in [
        "conflict",
        "unrelated",
        "untracked",
        "ignored",
        "index",
        "index-flags",
        "message",
        "orig-head",
        "branch",
        "head",
        "config",
    ] {
        for action in [RecoveryAction::Abort, RecoveryAction::Skip] {
            let dir = fixture("cherry-pick");
            let repo = dir.path();
            let (plan, _) = RecoveryPlan::review(repo, action).unwrap();
            match change {
                "conflict" => fs::write(repo.join("file.txt"), "external resolution\n").unwrap(),
                "unrelated" => fs::write(repo.join("unrelated.txt"), "external edits\n").unwrap(),
                "untracked" => fs::write(repo.join("new.txt"), "new untracked\n").unwrap(),
                "ignored" => {
                    fs::write(repo.join("ignored.txt"), "ignored external edits\n").unwrap()
                }
                "index" => {
                    fs::write(repo.join("new.txt"), "new staged\n").unwrap();
                    git(repo, &["add", "new.txt"]);
                }
                "index-flags" => {
                    git(
                        repo,
                        &["update-index", "--assume-unchanged", "unrelated.txt"],
                    );
                }
                "message" => {
                    fs::write(git_path(repo, "MERGE_MSG").unwrap(), "edited message\n").unwrap()
                }
                "orig-head" => fs::write(
                    git_path(repo, "ORIG_HEAD").unwrap(),
                    "changed recovery target\n",
                )
                .unwrap(),
                "branch" => {
                    git(repo, &["branch", "same-head"]);
                    git(repo, &["symbolic-ref", "HEAD", "refs/heads/same-head"]);
                }
                "head" => {
                    git(repo, &["update-ref", "HEAD", "incoming"]);
                }
                "config" => {
                    git(repo, &["config", "core.autocrlf", "true"]);
                }
                _ => unreachable!(),
            }
            let external = State::read(repo).unwrap();
            let error = plan.execute(repo).unwrap_err();
            assert!(error.to_string().contains("changed"), "{change}: {error:#}");
            assert_eq!(
                State::read(repo).unwrap(),
                external,
                "{change}: stale execution wrote files"
            );
        }
    }
}

#[test]
fn skip_reports_the_next_native_conflict_and_retains_sequencer_state() {
    let dir = fixture("merge");
    let repo = dir.path();
    git(repo, &["merge", "--abort"]);
    git(repo, &["switch", "-q", "incoming"]);
    let first = git(repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join("file.txt"), "second incoming\n").unwrap();
    git(repo, &["commit", "-qam", "second"]);
    let second = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "-q", "main"]);
    assert!(!run(repo, &["cherry-pick", &first, &second])
        .unwrap()
        .status
        .success());
    let (plan, _) = RecoveryPlan::review(repo, RecoveryAction::Skip).unwrap();
    let message = plan.execute(repo).unwrap();
    assert!(
        message.contains("stopped") && message.contains("still pending"),
        "{message}"
    );
    assert!(message.contains("1 unresolved paths"), "{message}");
    assert_eq!(operation(repo).unwrap().unwrap().other, second);
    assert_eq!(conflicts(0, repo).unwrap().len(), 1);
    assert!(git_path(repo, "sequencer").unwrap().is_dir());
}

#[test]
fn rebase_original_branch_and_replay_metadata_changes_expire_abort_and_skip() {
    for change in ["original-branch", "todo", "original-head"] {
        for action in [RecoveryAction::Abort, RecoveryAction::Skip] {
            let dir = fixture("rebase");
            let repo = dir.path();
            let (plan, _) = RecoveryPlan::review(repo, action).unwrap();
            match change {
                "original-branch" => {
                    git(repo, &["update-ref", "refs/heads/main", "incoming"]);
                }
                "todo" => {
                    let todo = git_path(repo, "rebase-merge/git-rebase-todo").unwrap();
                    let mut content = fs::read(&todo).unwrap();
                    content.extend(b"\n# externally edited remaining sequence\n");
                    fs::write(todo, content).unwrap();
                }
                "original-head" => {
                    fs::write(
                        git_path(repo, "rebase-merge/orig-head").unwrap(),
                        git(repo, &["rev-parse", "incoming"]),
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }
            let before = State::read(repo).unwrap();
            assert!(plan
                .execute(repo)
                .unwrap_err()
                .to_string()
                .contains("changed"));
            assert_eq!(State::read(repo).unwrap(), before);
        }
    }
}

#[cfg(unix)]
#[test]
fn working_modes_symlinks_and_untracked_content_are_bound_to_recovery_review() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for change in ["mode", "symlink", "untracked-content"] {
        let dir = fixture("cherry-pick");
        let repo = dir.path();
        let native = repo.join("untracked.txt");
        fs::write(&native, "native content\n").unwrap();
        let (plan, _) = RecoveryPlan::review(repo, RecoveryAction::Abort).unwrap();
        match change {
            "mode" => fs::set_permissions(
                repo.join("unrelated.txt"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap(),
            "symlink" => {
                fs::remove_file(repo.join("unrelated.txt")).unwrap();
                symlink("file.txt", repo.join("unrelated.txt")).unwrap();
            }
            "untracked-content" => fs::write(&native, "external edits\n").unwrap(),
            _ => unreachable!(),
        }
        let before = State::read(repo).unwrap();
        assert!(plan
            .execute(repo)
            .unwrap_err()
            .to_string()
            .contains("changed"));
        assert_eq!(State::read(repo).unwrap(), before);
    }
}

// macOS accepts synthetic non-UTF-8 index entries (covered by engine::paths)
// but rejects the corresponding working-tree filename with EILSEQ.
#[cfg(target_os = "linux")]
#[test]
fn native_non_utf8_working_content_is_bound_without_lossy_git_arguments() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let dir = fixture("cherry-pick");
    let repo = dir.path();
    let native = repo.join(OsString::from_vec(b"untracked-\xff.txt".to_vec()));
    fs::write(&native, "native content\n").unwrap();
    let (plan, _) = RecoveryPlan::review(repo, RecoveryAction::Abort).unwrap();
    fs::write(&native, "external native edits\n").unwrap();
    let before = State::read(repo).unwrap();
    assert!(plan
        .execute(repo)
        .unwrap_err()
        .to_string()
        .contains("changed"));
    assert_eq!(State::read(repo).unwrap(), before);
}

#[test]
fn canceled_reviews_and_changed_roots_cannot_authorize_recovery() {
    let a = fixture("cherry-pick");
    let b = fixture("cherry-pick");
    let mut ctx = context(&[a.path(), b.path()]);
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    let before_a = State::read(a.path()).unwrap();
    let before_b = State::read(b.path()).unwrap();
    for action_key in ['B', 'N'] {
        let Action::Confirm { token, .. } = key(&mut panel, &ctx, KeyCode::Char(action_key)) else {
            panic!("recovery should require confirmation");
        };
        key(&mut panel, &ctx, KeyCode::Esc);
        assert!(matches!(panel.confirm(&token, &ctx), Action::Notice(_)));
        assert_eq!(State::read(a.path()).unwrap(), before_a);
        assert_eq!(State::read(b.path()).unwrap(), before_b);
    }
    let Action::Confirm { token, .. } = key(&mut panel, &ctx, KeyCode::Char('B')) else {
        panic!()
    };
    ctx.scope.remove(&0);
    assert!(
        matches!(panel.confirm(&token, &ctx), Action::Notice(message) if message.contains("out of scope"))
    );
    ctx.scope.insert(0);
    let Action::Confirm { token, .. } = key(&mut panel, &ctx, KeyCode::Char('B')) else {
        panic!()
    };
    ctx.repos.swap(0, 1);
    assert!(
        matches!(panel.confirm(&token, &ctx), Action::Refresh(message) if message.contains("changed"))
    );
    assert_eq!(State::read(a.path()).unwrap(), before_a);
    assert_eq!(State::read(b.path()).unwrap(), before_b);
}

#[test]
fn recovery_buttons_remain_reachable_and_only_write_the_highlighted_repository() {
    let a = fixture("rebase");
    let b = fixture("rebase");
    let ctx = context(&[a.path(), b.path()]);
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    key(&mut panel, &ctx, KeyCode::Char('j'));
    let before_a = State::read(a.path()).unwrap();
    for width in [44, 80, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        terminal.draw(|f| panel.draw(f, f.area(), &ctx)).unwrap();
        for matches in [
            (|hit: &Hit| matches!(hit, Hit::Abort)) as fn(&Hit) -> bool,
            |hit| matches!(hit, Hit::Skip),
        ] {
            assert!(panel
                .hits
                .iter()
                .any(|(area, hit)| area.width > 0 && matches(hit)));
        }
    }
    let Action::Confirm { token, title, .. } = key(&mut panel, &ctx, KeyCode::Char('N')) else {
        panic!()
    };
    assert!(title.contains("repo-1"));
    assert!(
        matches!(panel.confirm(&token, &ctx), Action::Refresh(message) if message.contains("completed"))
    );
    assert_eq!(State::read(a.path()).unwrap(), before_a);
    assert!(operation(b.path()).unwrap().is_none());
    assert!(matches!(panel.confirm(&token, &ctx), Action::Notice(_)));
}

#[test]
fn abort_mouse_control_opens_a_review_without_writing_the_repository() {
    let dir = fixture("merge");
    let ctx = context(&[dir.path()]);
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    let before = State::read(dir.path()).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(44, 12)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), &ctx)).unwrap();
    assert!(!panel.hits.iter().any(|(_, hit)| matches!(hit, Hit::Skip)));
    let area = panel
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::Abort))
        .unwrap()
        .0;
    let action = panel.event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.right() - 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        }),
        &ctx,
    );
    assert!(matches!(action, Action::Confirm { title, .. } if title.contains("Abort merge")));
    assert_eq!(State::read(dir.path()).unwrap(), before);
}

#[test]
fn merge_abort_preserves_unrelated_unstaged_and_untracked_working_content() {
    let dir = fixture("merge");
    let repo = dir.path();
    fs::write(repo.join("unrelated.txt"), "keep these edits\n").unwrap();
    fs::write(repo.join("new.txt"), "keep untracked\n").unwrap();
    fs::write(repo.join("ignored.txt"), "keep ignored\n").unwrap();
    let unrelated_index =
        checked(repo, &["ls-files", "--stage", "-z", "--", "unrelated.txt"]).unwrap();
    let (plan, _) = RecoveryPlan::review(repo, RecoveryAction::Abort).unwrap();
    assert!(plan.execute(repo).unwrap().contains("completed"));
    assert_eq!(
        fs::read(repo.join("unrelated.txt")).unwrap(),
        b"keep these edits\n"
    );
    assert_eq!(fs::read(repo.join("new.txt")).unwrap(), b"keep untracked\n");
    assert_eq!(
        fs::read(repo.join("ignored.txt")).unwrap(),
        b"keep ignored\n"
    );
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z", "--", "unrelated.txt"]).unwrap(),
        unrelated_index
    );
}
