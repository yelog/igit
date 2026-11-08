use super::*;
use crate::{
    core::Context,
    merge::MergePanel,
    ui::{Action, Panel},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const FILE: &str = "src/配置 file.ts";
const OTHER: &str = "unrelated.txt";
const BASE: &str = "base\na\nb\nshared\nc\nd\nbase\n";
const CURRENT: &str = "current only\na\nb\ncurrent\nc\nd\nbase\n";
const STASHED: &str = "base\na\nb\nstashed\nc\nd\nstash only\n";
const RESOLVED: &str = "current only\na\nb\nstashed\nc\nd\nstash only\n";

fn stash_fixture(rule: &str, autocrlf: &str, style: &str, action: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    g(repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Conflict tester"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        ("core.autocrlf", autocrlf),
        ("core.eol", "lf"),
        ("core.safecrlf", "false"),
        ("merge.conflictStyle", style),
        ("stash.index", "false"),
    ] {
        g(repo, &["config", key, value]);
    }
    fs::create_dir(repo.join("src")).unwrap();
    fs::write(repo.join(".gitattributes"), format!("*.ts {rule}\n")).unwrap();
    fs::write(repo.join(FILE), BASE).unwrap();
    fs::write(repo.join(OTHER), "original\n").unwrap();
    g(repo, &["add", "."]);
    g(repo, &["commit", "-qm", "base"]);
    fs::write(repo.join(FILE), STASHED).unwrap();
    g(repo, &["stash", "push", "-qm", "incoming", "--", FILE]);
    fs::write(repo.join(FILE), CURRENT).unwrap();
    g(repo, &["add", "--", FILE]);
    g(repo, &["commit", "-qm", "current"]);
    let outcome = run(repo, &["stash", action]).unwrap();
    assert_eq!(outcome.status.code(), Some(1));
    assert_eq!(conflicts(0, repo).unwrap().len(), 1);
    assert!(operation(repo).unwrap().is_none());
    fs::write(repo.join(OTHER), "staged separately\n").unwrap();
    g(repo, &["add", "--", OTHER]);
    fs::write(repo.join(OTHER), "unstaged separately\n").unwrap();
    dir
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    working: Vec<u8>,
    index: Vec<u8>,
    head: String,
    stash: String,
    other: Vec<u8>,
}

fn snapshot(repo: &Path) -> Snapshot {
    Snapshot {
        working: fs::read(repo.join(FILE)).unwrap(),
        index: checked(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        head: rev(repo, "HEAD"),
        stash: g(repo, &["stash", "list", "--format=%H %gs"]),
        other: fs::read(repo.join(OTHER)).unwrap(),
    }
}

fn key(panel: &mut MergePanel, ctx: &Context, code: KeyCode) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), ctx)
}

#[test]
fn stash_eol_lf_conflicts_open_review_and_apply() {
    for (rule, autocrlf, style, action) in [
        ("eol=lf", "false", "merge", "pop"),
        ("text eol=lf", "false", "diff3", "pop"),
        ("text=auto eol=lf", "true", "zdiff3", "pop"),
        ("eol=lf", "input", "diff3", "apply"),
        ("text=auto eol=lf", "false", "merge", "apply"),
    ] {
        let dir = stash_fixture(rule, autocrlf, style, action);
        let repo = dir.path();
        let ctx = Context::discover(repo).unwrap();
        let mut panel = MergePanel::default();
        panel.refresh(&ctx).unwrap();
        let before = snapshot(repo);
        let unrelated_index = checked(repo, &["ls-files", "--stage", "-z", "--", OTHER]).unwrap();
        if let Action::Notice(error) = key(&mut panel, &ctx, KeyCode::Enter) {
            panic!("Enter refused stash {action} with {rule}: {error}");
        }
        let doc = panel.doc.as_ref().expect("Enter opens the merge editor");
        assert!(doc.op.is_none());
        assert!(doc.pending() > 0);
        assert_eq!(snapshot(repo), before);
        key(&mut panel, &ctx, KeyCode::Esc);
        assert!(panel.doc.is_none());
        assert_eq!(snapshot(repo), before);
        key(&mut panel, &ctx, KeyCode::Enter);
        key(&mut panel, &ctx, KeyCode::Char('a'));
        key(&mut panel, &ctx, KeyCode::Char('L'));
        let doc = panel.doc.as_ref().unwrap();
        assert_eq!(doc.pending(), 0);
        assert_eq!(
            doc.chunks
                .iter()
                .map(|c| c.result.as_str())
                .collect::<String>(),
            RESOLVED
        );
        let Action::Confirm { token, body, .. } = key(&mut panel, &ctx, KeyCode::Char('A')) else {
            panic!("Apply must require review");
        };
        assert!(body.contains(FILE));
        assert_eq!(snapshot(repo), before);
        let Action::Refresh(message) = panel.confirm(&token, &ctx) else {
            panic!("Confirmed Apply must succeed");
        };
        assert!(message.contains("Review in Commit"), "{message}");
        assert!(!message.contains("Continue"), "{message}");
        assert_eq!(fs::read(repo.join(FILE)).unwrap(), RESOLVED.as_bytes());
        assert_eq!(
            checked(repo, &["show", &format!(":0:{FILE}")]).unwrap(),
            RESOLVED.as_bytes()
        );
        assert_eq!(
            checked(repo, &["show", &format!("HEAD:{FILE}")]).unwrap(),
            CURRENT.as_bytes()
        );
        assert!(conflicts(0, repo).unwrap().is_empty());
        assert_eq!(
            checked(repo, &["ls-files", "--stage", "-z", "--", OTHER]).unwrap(),
            unrelated_index
        );
        let after = snapshot(repo);
        assert_eq!(after.head, before.head);
        assert_eq!(after.stash, before.stash);
        assert!(!after.stash.is_empty());
        assert_eq!(after.other, before.other);
        assert!(operation(repo).unwrap().is_none());
        assert!(continue_plan(repo).is_err());
        panel.refresh(&ctx).unwrap();
        assert!(!panel.has_pending_work());
    }
}

#[test]
fn eol_rules_still_require_identical_checkout_bytes() {
    for rule in ["text eol=crlf", "text=auto eol=crlf"] {
        let dir = stash_fixture(rule, "false", "merge", "pop");
        let repo = dir.path();
        let before = snapshot(repo);
        let file = conflicts(0, repo).unwrap().pop().unwrap();
        let error = Document::open(repo, file).err().unwrap().to_string();
        assert!(
            error.contains("line endings transform this file"),
            "{error}"
        );
        assert_eq!(snapshot(repo), before);
    }
    // An explicit -text makes eol inert; do not reject the attribute by name.
    let dir = stash_fixture("-text eol=crlf", "true", "merge", "pop");
    let repo = dir.path();
    let before = snapshot(repo);
    let file = conflicts(0, repo).unwrap().pop().unwrap();
    assert!(Document::open(repo, file).is_ok());
    assert_eq!(snapshot(repo), before);
}

#[test]
fn eol_allowlist_keeps_custom_conversion_guards() {
    let dir = stash_fixture("text eol=lf", "false", "merge", "pop");
    let repo = dir.path();
    let before = snapshot(repo);
    let file = conflicts(0, repo).unwrap().pop().unwrap();
    for attr in [
        "merge=custom",
        "-merge",
        "filter=custom",
        "working-tree-encoding=UTF-16",
        "ident",
        "eol=unknown",
    ] {
        fs::write(
            repo.join(".gitattributes"),
            format!("*.ts text eol=lf {attr}\n"),
        )
        .unwrap();
        let error = Document::open(repo, file.clone())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Path uses"), "{attr}: {error}");
        assert_eq!(snapshot(repo), before);
    }
}

#[test]
fn eol_changes_after_open_block_apply_without_writes() {
    for change_attribute in [true, false] {
        let dir = stash_fixture("text eol=lf", "false", "merge", "pop");
        let repo = dir.path();
        let file = conflicts(0, repo).unwrap().pop().unwrap();
        let mut doc = Document::open(repo, file).unwrap();
        for chunk in &mut doc.chunks {
            chunk.use_side(FileSide::Incoming);
        }
        let before = snapshot(repo);
        if change_attribute {
            fs::write(repo.join(".gitattributes"), "*.ts text eol=crlf\n").unwrap();
        } else {
            g(repo, &["config", "core.autocrlf", "true"]);
        }
        let error = doc.apply().unwrap_err().to_string();
        assert!(error.contains("Git attributes/config changed"), "{error}");
        assert_eq!(snapshot(repo), before);
    }
}
