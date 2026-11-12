use super::*;
use crate::core::git;
use std::{fs, io::Write};

fn run(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn init(path: &Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Remote tester"],
        &["config", "user.email", "remote@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["config", "core.hooksPath", "/dev/null"],
    ] {
        run(path, args);
    }
    fs::write(path.join("tracked.txt"), "base\n").unwrap();
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "base"]);
}

#[test]
fn load_lists_every_endpoint_and_isolates_a_broken_repository() {
    let dir = tempfile::tempdir().unwrap();
    let api = dir.path().join("api");
    let web = dir.path().join("web");
    fs::create_dir(&api).unwrap();
    fs::create_dir(&web).unwrap();
    init(&api);
    init(&web);

    let fetch_one = "https://alice:s3cret@example.invalid/a b.git";
    let fetch_two = "../relative path/仓库 [literal].git";
    let push_one = "ssh://bob:token@example.invalid/push [x].git";
    let push_two = "git@example.invalid:team/push path.git";
    run(&api, &["remote", "add", "origin", fetch_one]);
    run(&api, &["remote", "set-url", "--add", "origin", fetch_two]);
    run(
        &api,
        &["remote", "set-url", "--add", "--push", "origin", push_one],
    );
    run(
        &api,
        &["remote", "set-url", "--add", "--push", "origin", push_two],
    );
    run(
        &api,
        &[
            "config",
            "remote.empty.fetch",
            "+refs/heads/*:refs/remotes/empty/*",
        ],
    );
    let included = api.join(".git/included-remotes.conf");
    fs::write(
        &included,
        "[remote \"included\"]\n\turl = ../included\n\tfetch = +refs/heads/*:refs/remotes/included/*\n",
    )
    .unwrap();
    run(&api, &["config", "include.path", "included-remotes.conf"]);

    let mut context = Context::discover(dir.path()).unwrap();
    context.state_dir = dir.path().join("state");
    let api_root = context
        .repos
        .iter()
        .position(|repo| repo.path.ends_with("api"))
        .unwrap();
    let web_root = context
        .repos
        .iter()
        .position(|repo| repo.path.ends_with("web"))
        .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(web.join(".git/config"))
        .unwrap()
        .write_all(b"\n[broken\n")
        .unwrap();

    let loaded = load(&context);
    let api_group = loaded.iter().find(|group| group.root == api_root).unwrap();
    assert!(api_group.error.is_none());
    assert_eq!(
        api_group
            .remotes
            .iter()
            .map(|remote| remote.name.as_str())
            .collect::<Vec<_>>(),
        vec!["empty", "included", "origin"]
    );
    let origin = api_group
        .remotes
        .iter()
        .find(|remote| remote.name == "origin")
        .unwrap();
    assert_eq!(
        origin.urls,
        vec![fetch_one.to_owned(), fetch_two.to_owned()]
    );
    assert_eq!(
        origin.push_urls,
        vec![push_one.to_owned(), push_two.to_owned()]
    );
    let empty = api_group
        .remotes
        .iter()
        .find(|remote| remote.name == "empty")
        .unwrap();
    assert!(empty.urls.is_empty());
    assert!(empty.push_urls.is_empty());
    assert_eq!(
        read(&api).unwrap(),
        api_group.remotes.clone(),
        "read and load must use the same configured URLs"
    );

    let web_group = loaded.iter().find(|group| group.root == web_root).unwrap();
    assert!(web_group.remotes.is_empty());
    assert!(web_group.error.as_deref().is_some_and(|error| {
        error.to_ascii_lowercase().contains("config")
            && !error.contains("s3cret")
            && !error.contains('\n')
    }));
}

#[test]
fn label_redacts_url_userinfo_and_escapes_controls_without_hiding_unicode() {
    let text = "https://alice:s3cret@example.invalid/a b?x=1\nssh://bob@example.invalid/r\u{1b} 仓库 git@example.invalid:团队/repo.git";
    let rendered = label(text);
    for secret in ["alice", "s3cret", "bob", "\u{1b}"] {
        assert!(!rendered.contains(secret), "{rendered}");
    }
    assert!(
        rendered.contains("<redacted>@example.invalid"),
        "{rendered}"
    );
    assert!(rendered.contains("仓库"), "{rendered}");
    assert!(
        rendered.contains("git@example.invalid:团队/repo.git"),
        "{rendered}"
    );
    assert!(rendered.contains("\\n"), "{rendered}");
    assert!(!rendered.chars().any(char::is_control), "{rendered}");
}
