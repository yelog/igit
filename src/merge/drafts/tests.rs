use super::*;
use crate::{
    core::Context,
    merge::{Hit, MergePanel},
    ui::{Action, NoticeLevel, Panel},
};

const FILE: &str = "file with spaces.txt";

fn git(repo: &Path, args: &[&str]) -> String {
    String::from_utf8(checked(repo, args).unwrap())
        .unwrap()
        .trim()
        .into()
}

fn fixture() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Draft tester"),
        ("user.email", "draft@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&repo, &["config", key, value]);
    }
    fs::write(repo.join(FILE), "start\nbase\nend\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    git(&repo, &["checkout", "-qb", "incoming"]);
    fs::write(repo.join(FILE), "start\nincoming\nend\n").unwrap();
    git(&repo, &["commit", "-qam", "incoming"]);
    git(&repo, &["checkout", "-q", "main"]);
    fs::write(repo.join(FILE), "start\ncurrent\nend\n").unwrap();
    git(&repo, &["commit", "-qam", "current"]);
    assert_eq!(
        run(&repo, &["merge", "--no-edit", "incoming"])
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    let mut ctx = Context::discover(&repo).unwrap();
    ctx.state_dir = dir.path().join("state");
    (dir, ctx)
}

fn open(ctx: &Context) -> MergePanel {
    let mut panel = MergePanel::default();
    panel.refresh(ctx).unwrap();
    let file = conflicts(0, &ctx.repos[0].path).unwrap().remove(0);
    panel.choose_file(ctx, file).unwrap();
    panel
}

fn decisions(document: &Document) -> Vec<(String, [bool; 2])> {
    document
        .chunks
        .iter()
        .map(|chunk| {
            (
                chunk.result.clone(),
                FileSide::ALL.map(|side| chunk.pending(side)),
            )
        })
        .collect()
}

fn update_index(repo: &Path, entries: &[u8]) {
    let mut command = crate::core::command(repo);
    let mut child = command
        .args(["update-index", "-z", "--index-info"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(entries).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn persistent_merge_draft_survives_panel_restart_and_preserves_partial_decisions() {
    for accept in [false, true] {
        let (_dir, ctx) = fixture();
        let repo = &ctx.repos[0].path;
        let before = fs::read(repo.join(FILE)).unwrap();
        let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
        let mut panel = open(&ctx);
        let document = panel.doc.as_mut().unwrap();
        let conflict = document
            .chunks
            .iter()
            .position(|chunk| chunk.kind == Kind::Conflict)
            .unwrap();
        document.chunks[conflict].decide(FileSide::Current, accept);
        document.chunks[0].resolve("manually edited candidate\n".into());
        let expected: Vec<_> = document.chunks.iter().map(Decision::new).collect();
        let mut continuation = document.chunks[conflict].clone();
        continuation.decide(FileSide::Incoming, true);
        assert!(matches!(
            panel.act(Hit::Save, &ctx),
            Action::Notice(NoticeLevel::Success, _)
        ));
        assert!(panel.doc.is_none());
        drop(panel);
        let mut restarted = open(&ctx);
        let loaded = restarted.doc.as_mut().unwrap();
        for (chunk, expected) in loaded.chunks.iter().zip(&expected) {
            assert_eq!(chunk.result, expected.result);
            assert_eq!(
                FileSide::ALL.map(|side| chunk.pending(side)),
                expected.pending
            );
        }
        loaded.chunks[conflict].decide(FileSide::Incoming, true);
        assert_eq!(loaded.chunks[conflict].result, continuation.result);
        assert_eq!(fs::read(repo.join(FILE)).unwrap(), before);
        assert_eq!(
            checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
            index
        );
    }
}

#[test]
fn persistent_merge_draft_opens_fresh_for_changed_stages_and_preserves_the_saved_record() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Current), &ctx);
    let expected = decisions(panel.doc.as_ref().unwrap());
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    assert!(matches!(
        panel.act(Hit::List, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let path = location(&ctx.state_dir, &identity).unwrap();
    let saved = fs::read(&path).unwrap();
    let original_index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let source = repo.parent().unwrap().join("new-incoming");
    fs::write(&source, "different incoming\n").unwrap();
    let oid = git(repo, &["hash-object", "-w", source.to_str().unwrap()]);
    update_index(repo, format!("100644 {oid} 3\t{FILE}\0").as_bytes());
    let base = checked(repo, &["show", &format!(":1:{FILE}")]).unwrap();
    fs::write(repo.join(FILE), &base).unwrap();
    let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let file = conflicts(0, repo).unwrap().remove(0);
    let fresh = Document::open(repo, file.clone()).unwrap();
    let mut restarted = MergePanel::default();
    restarted.choose_file(&ctx, file).unwrap();
    assert_eq!(
        decisions(restarted.doc.as_ref().unwrap()),
        decisions(&fresh)
    );
    assert!(restarted.doc.as_ref().unwrap().pending() > 0);
    let new_identity = restarted.draft_identity.as_ref().unwrap();
    assert_ne!(new_identity, &identity);
    assert_ne!(location(&ctx.state_dir, new_identity).unwrap(), path);
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), base);
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    update_index(repo, &original_index);
    let restored = open(&ctx);
    assert_eq!(restored.draft_identity.as_ref().unwrap(), &identity);
    assert_eq!(decisions(restored.doc.as_ref().unwrap()), expected);
    assert_eq!(fs::read(&path).unwrap(), saved);
}

#[test]
fn persistent_merge_draft_opens_fresh_for_changed_operation_without_overwriting_working_content() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Incoming), &ctx);
    let expected = decisions(panel.doc.as_ref().unwrap());
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    assert!(matches!(
        panel.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let path = location(&ctx.state_dir, &identity).unwrap();
    let saved = fs::read(&path).unwrap();
    let merge_head = git_path(repo, "MERGE_HEAD").unwrap();
    let original_operation = fs::read(&merge_head).unwrap();
    fs::remove_file(&merge_head).unwrap();
    let base = checked(repo, &["show", &format!(":1:{FILE}")]).unwrap();
    fs::write(repo.join(FILE), &base).unwrap();
    let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let file = conflicts(0, repo).unwrap().remove(0);
    let fresh = Document::open(repo, file.clone()).unwrap();
    let mut restarted = MergePanel::default();
    restarted.choose_file(&ctx, file).unwrap();
    assert_eq!(
        decisions(restarted.doc.as_ref().unwrap()),
        decisions(&fresh)
    );
    assert!(restarted.doc.as_ref().unwrap().pending() > 0);
    let new_identity = restarted.draft_identity.as_ref().unwrap();
    assert_ne!(new_identity, &identity);
    assert_ne!(location(&ctx.state_dir, new_identity).unwrap(), path);
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), base);
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    fs::write(&merge_head, original_operation).unwrap();
    let restored = open(&ctx);
    assert_eq!(restored.draft_identity.as_ref().unwrap(), &identity);
    assert_eq!(decisions(restored.doc.as_ref().unwrap()), expected);
    assert_eq!(fs::read(&path).unwrap(), saved);
}

#[test]
fn persistent_merge_draft_new_identity_saves_without_overwriting_the_old_record() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Current), &ctx);
    let original_decisions = decisions(panel.doc.as_ref().unwrap());
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    assert!(matches!(
        panel.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let path = location(&ctx.state_dir, &identity).unwrap();
    let saved = fs::read(&path).unwrap();
    let merge_head = git_path(repo, "MERGE_HEAD").unwrap();
    let original_operation = fs::read(&merge_head).unwrap();
    fs::remove_file(&merge_head).unwrap();
    let base = checked(repo, &["show", &format!(":1:{FILE}")]).unwrap();
    fs::write(repo.join(FILE), &base).unwrap();
    let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let mut fresh = open(&ctx);
    assert!(fresh.doc.as_ref().unwrap().pending() > 0);
    fresh.act(Hit::UseFile(FileSide::Incoming), &ctx);
    let new_decisions = decisions(fresh.doc.as_ref().unwrap());
    let new_identity = fresh.draft_identity.as_ref().unwrap().clone();
    let new_path = location(&ctx.state_dir, &new_identity).unwrap();
    assert_ne!(new_path, path);
    assert!(matches!(
        fresh.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let new_saved = fs::read(&new_path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), saved);
    drop(fresh);
    drop(panel);
    let restarted = open(&ctx);
    assert_eq!(restarted.draft_identity.as_ref().unwrap(), &new_identity);
    assert_eq!(decisions(restarted.doc.as_ref().unwrap()), new_decisions);
    fs::write(&merge_head, original_operation).unwrap();
    let restored = open(&ctx);
    assert_eq!(restored.draft_identity.as_ref().unwrap(), &identity);
    assert_eq!(
        decisions(restored.doc.as_ref().unwrap()),
        original_decisions
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(fs::read(&new_path).unwrap(), new_saved);
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), base);
    assert_eq!(
        checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
}

#[test]
fn persistent_merge_draft_refuses_corrupt_or_mismatched_matching_records() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Current), &ctx);
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    assert!(matches!(
        panel.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let path = location(&ctx.state_dir, &identity).unwrap();
    let saved = fs::read(&path).unwrap();
    let mut mismatched: Draft = serde_json::from_slice(&saved).unwrap();
    mismatched.identity.head.push(0);
    let mut corrupt_chunks: Draft = serde_json::from_slice(&saved).unwrap();
    corrupt_chunks.chunks[0].base.push_str("changed source");
    let working = fs::read(repo.join(FILE)).unwrap();
    let index = checked(repo, &["ls-files", "--stage", "-z"]).unwrap();
    for bytes in [
        b"{".to_vec(),
        serde_json::to_vec(&mismatched).unwrap(),
        serde_json::to_vec(&corrupt_chunks).unwrap(),
    ] {
        fs::write(&path, &bytes).unwrap();
        let file = conflicts(0, repo).unwrap().remove(0);
        let mut restarted = MergePanel::default();
        assert!(restarted.choose_file(&ctx, file).is_err());
        assert!(restarted.doc.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(repo.join(FILE)).unwrap(), working);
        assert_eq!(
            checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
            index
        );
    }
}

#[test]
fn persistent_merge_draft_save_failure_keeps_the_editor_and_unsaved_decisions() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Current), &ctx);
    let before = fs::read(repo.join(FILE)).unwrap();
    let candidate: String = panel
        .doc
        .as_ref()
        .unwrap()
        .chunks
        .iter()
        .map(|chunk| chunk.result.as_str())
        .collect();
    fs::create_dir_all(&ctx.state_dir).unwrap();
    fs::write(ctx.state_dir.join("merge-drafts"), "blocked").unwrap();
    assert!(matches!(
        panel.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Error, message) if message.contains("Editor kept open")
    ));
    let retained: String = panel
        .doc
        .as_ref()
        .unwrap()
        .chunks
        .iter()
        .map(|chunk| chunk.result.as_str())
        .collect();
    assert_eq!(retained, candidate);
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), before);
}

#[test]
fn persistent_merge_draft_successful_apply_removes_only_the_matching_saved_draft() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Current), &ctx);
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    assert!(matches!(
        panel.act(Hit::Save, &ctx),
        Action::Notice(NoticeLevel::Success, _)
    ));
    let file = conflicts(0, repo).unwrap().remove(0);
    panel.choose_file(&ctx, file).unwrap();
    assert!(matches!(
        panel.confirm("merge:apply", &ctx),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert!(!location(&ctx.state_dir, &identity).unwrap().exists());
    assert!(conflicts(0, repo).unwrap().is_empty());
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), b"start\ncurrent\nend\n");
}

#[test]
fn persistent_merge_draft_saves_stale_editor_work_without_applying_it() {
    let (_dir, ctx) = fixture();
    let repo = &ctx.repos[0].path;
    let mut panel = open(&ctx);
    panel.act(Hit::UseFile(FileSide::Incoming), &ctx);
    let identity = panel.draft_identity.as_ref().unwrap().clone();
    let expected = decisions(panel.doc.as_ref().unwrap());
    let before = fs::read(repo.join(FILE)).unwrap();
    let merge_head = git_path(repo, "MERGE_HEAD").unwrap();
    let original_operation = fs::read(&merge_head).unwrap();
    fs::remove_file(&merge_head).unwrap();
    panel.refresh(&ctx).unwrap();
    assert!(panel.doc_stale);
    assert!(matches!(
        panel.act(Hit::List, &ctx),
        Action::Notice(NoticeLevel::Warning, message) if message.contains("Outdated merge draft saved")
    ));
    assert!(panel.doc.is_none());
    let path = location(&ctx.state_dir, &identity).unwrap();
    let saved = fs::read(&path).unwrap();
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), before);
    let base = checked(repo, &["show", &format!(":1:{FILE}")]).unwrap();
    fs::write(repo.join(FILE), &base).unwrap();
    let file = conflicts(0, repo).unwrap().remove(0);
    let fresh = Document::open(repo, file.clone()).unwrap();
    panel.choose_file(&ctx, file).unwrap();
    assert_eq!(decisions(panel.doc.as_ref().unwrap()), decisions(&fresh));
    assert!(panel.doc.as_ref().unwrap().pending() > 0);
    assert_ne!(panel.draft_identity.as_ref().unwrap(), &identity);
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(fs::read(repo.join(FILE)).unwrap(), base);
    fs::write(&merge_head, original_operation).unwrap();
    let restored = open(&ctx);
    assert_eq!(restored.draft_identity.as_ref().unwrap(), &identity);
    assert_eq!(decisions(restored.doc.as_ref().unwrap()), expected);
    assert_eq!(fs::read(&path).unwrap(), saved);
}
