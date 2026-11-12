use super::*;
use std::{fs, path::Path};

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn remote_urls_map_to_the_hosting_services_commit_page() {
    for (remote, page) in [
        (
            "https://github.com/team/repo.git",
            "https://github.com/team/repo/commit/",
        ),
        (
            "git@github.com:team/repo.git",
            "https://github.com/team/repo/commit/",
        ),
        (
            "git://github.com/team/repo.git/",
            "https://github.com/team/repo/commit/",
        ),
        (
            "ssh://git@ssh.github.com:443/team/repo.git",
            "https://github.com/team/repo/commit/",
        ),
        (
            "ssh://git@github.corp.test:2222/team/repo.git",
            "https://github.corp.test/team/repo/commit/",
        ),
        (
            "https://user:secret@github.com/team/repo.git?access_token=secret#private",
            "https://github.com/team/repo/commit/",
        ),
        (
            "https://gitlab.com/group/subgroup/repo.git",
            "https://gitlab.com/group/subgroup/repo/-/commit/",
        ),
        (
            "git@gitlab.corp.test:group/subgroup/repo.git",
            "https://gitlab.corp.test/group/subgroup/repo/-/commit/",
        ),
        (
            "ssh://git@altssh.gitlab.com:443/group/repo",
            "https://gitlab.com/group/repo/-/commit/",
        ),
        (
            "http://gitlab.corp.test:8080/base/group/repo.git",
            "http://gitlab.corp.test:8080/base/group/repo/-/commit/",
        ),
        (
            "git@bitbucket.org:team/repo.git",
            "https://bitbucket.org/team/repo/commits/",
        ),
        (
            "https://bitbucket.corp.test/base/scm/PROJ/repo.git",
            "https://bitbucket.corp.test/base/projects/PROJ/repos/repo/commits/",
        ),
        (
            "ssh://git@bitbucket.corp.test:7999/PROJ/repo.git",
            "https://bitbucket.corp.test/projects/PROJ/repos/repo/commits/",
        ),
        (
            "https://stash.corp.test/scm/~alice/repo.git",
            "https://stash.corp.test/users/alice/repos/repo/commits/",
        ),
        (
            "git@gitee.com:team/repo.git",
            "https://gitee.com/team/repo/commit/",
        ),
        (
            "https://gitea.com/team/repo.git",
            "https://gitea.com/team/repo/commit/",
        ),
        (
            "git@forgejo.corp.test:team/repo",
            "https://forgejo.corp.test/team/repo/commit/",
        ),
        (
            "git@codeberg.org:team/repo.git",
            "https://codeberg.org/team/repo/commit/",
        ),
        (
            "https://org@dev.azure.com/org/project/_git/repo",
            "https://dev.azure.com/org/project/_git/repo/commit/",
        ),
        (
            "git@ssh.dev.azure.com:v3/org/project/repo",
            "https://dev.azure.com/org/project/_git/repo/commit/",
        ),
        (
            "ssh://git@ssh.dev.azure.com/v3/org/project/repo.git",
            "https://dev.azure.com/org/project/_git/repo/commit/",
        ),
        (
            "https://org.visualstudio.com/DefaultCollection/project/_git/repo",
            "https://org.visualstudio.com/DefaultCollection/project/_git/repo/commit/",
        ),
        (
            "org@vs-ssh.visualstudio.com:v3/org/project/repo",
            "https://dev.azure.com/org/project/_git/repo/commit/",
        ),
        (
            "https://github.com/team/中文 repo.git",
            "https://github.com/team/%E4%B8%AD%E6%96%87%20repo/commit/",
        ),
        (
            "https://github.com/team/already%20escaped.git",
            "https://github.com/team/already%20escaped/commit/",
        ),
        (
            "https://GITHUB.COM/team/repo.git",
            "https://github.com/team/repo/commit/",
        ),
    ] {
        assert_eq!(
            remote_commit_url(remote, OID).unwrap(),
            format!("{page}{OID}"),
            "{remote}"
        );
    }
}

#[test]
fn unsupported_or_malformed_remotes_never_leak_credentials() {
    for remote in [
        "/tmp/repo.git",
        "../repo.git",
        "C:/repos/repo.git",
        "file:///tmp/repo.git",
        "ext::helper secret",
        "https://user:secret@unknown.test/team/repo.git",
        "https://github.com",
        "https:///team/repo",
        "https://github.com:secret/team/repo",
        "https://github.com/../repo",
        "https://github.com\\@other.test/team/repo",
        "https://github.com/team/repo\nsecret",
        "git@ssh.dev.azure.com:v3/org/project",
    ] {
        let error = remote_commit_url(remote, OID).unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
        assert!(error.contains("igit.webCommitUrl"), "{error}");
    }
}

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    for (key, value) in [
        ("user.name", "Browser tester"),
        ("user.email", "browser@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(repo, &["config", key, value]).unwrap();
    }
    fs::write(repo.join("file"), "base\n").unwrap();
    git(repo, &["add", "."]).unwrap();
    git(repo, &["commit", "-qm", "base"]).unwrap();
    directory
}

fn add_remote(repo: &Path, name: &str, url: &str) {
    git(repo, &["remote", "add", name, url]).unwrap();
}

#[test]
fn remote_choice_prefers_upstream_then_origin_then_the_only_remote() {
    let directory = fixture();
    let repo = directory.path();
    let oid = git(repo, &["rev-parse", "HEAD"]).unwrap();
    assert!(commit_url(repo, &oid)
        .unwrap_err()
        .to_string()
        .contains("No remote"));
    add_remote(repo, "mirror", "git@gitlab.com:team/mirror.git");
    assert!(commit_url(repo, &oid).unwrap().contains("/team/mirror/"));
    add_remote(repo, "origin", "git@github.com:team/origin.git");
    assert!(commit_url(repo, &oid).unwrap().contains("/team/origin/"));
    git(repo, &["update-ref", "refs/remotes/mirror/main", &oid]).unwrap();
    git(repo, &["branch", "--set-upstream-to=mirror/main"]).unwrap();
    assert!(commit_url(repo, &oid).unwrap().contains("/team/mirror/"));
    git(repo, &["switch", "--detach", "-q"]).unwrap();
    git(repo, &["update-ref", "refs/remotes/origin/main", &oid]).unwrap();
    assert!(commit_url(repo, &oid).unwrap().contains("/team/origin/"));
}

#[test]
fn commit_containment_takes_priority_over_the_current_branches_remote() {
    let directory = fixture();
    let repo = directory.path();
    let base = git(repo, &["rev-parse", "HEAD"]).unwrap();
    add_remote(repo, "origin", "git@github.com:team/fork.git");
    add_remote(repo, "team/mirror", "git@gitlab.com:group/upstream.git");
    git(repo, &["update-ref", "refs/remotes/origin/main", &base]).unwrap();
    git(repo, &["branch", "--set-upstream-to=origin/main"]).unwrap();
    fs::write(repo.join("file"), "next\n").unwrap();
    git(repo, &["commit", "-qam", "next"]).unwrap();
    let oid = git(repo, &["rev-parse", "HEAD"]).unwrap();
    git(repo, &["update-ref", "refs/remotes/team/mirror/main", &oid]).unwrap();
    assert_eq!(
        commit_url(repo, &oid).unwrap(),
        format!("https://gitlab.com/group/upstream/-/commit/{oid}")
    );
}

#[test]
fn explicit_remote_overrides_containment_and_ambiguity_is_reported() {
    let directory = fixture();
    let repo = directory.path();
    let oid = git(repo, &["rev-parse", "HEAD"]).unwrap();
    add_remote(repo, "left", "git@github.com:team/left.git");
    add_remote(repo, "right", "git@gitlab.com:team/right.git");
    assert!(commit_url(repo, &oid)
        .unwrap_err()
        .to_string()
        .contains("igit.webRemote"));
    git(repo, &["config", "igit.webRemote", "right"]).unwrap();
    assert!(commit_url(repo, &oid).unwrap().contains("/team/right/"));
    git(repo, &["config", "igit.webRemote", "missing"]).unwrap();
    assert!(commit_url(repo, &oid).is_err());
}

#[test]
fn fetch_url_rewrites_are_honored_without_using_the_push_url_or_writing_git_state() {
    let directory = fixture();
    let repo = directory.path();
    let oid = git(repo, &["rev-parse", "HEAD"]).unwrap();
    add_remote(repo, "origin", "company:team/repo.git");
    git(
        repo,
        &["config", "url.https://gitlab.com/.insteadOf", "company:"],
    )
    .unwrap();
    git(
        repo,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            "git@github.com:other/push.git",
        ],
    )
    .unwrap();
    fs::write(repo.join("file"), "staged\n").unwrap();
    git(repo, &["add", "."]).unwrap();
    fs::write(repo.join("file"), "unstaged\n").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let config = fs::read(repo.join(".git/config")).unwrap();
    let cached = git(repo, &["diff", "--cached", "--binary"]).unwrap();
    let working = git(repo, &["diff", "--binary"]).unwrap();
    assert_eq!(
        commit_url(repo, &oid).unwrap(),
        format!("https://gitlab.com/team/repo/-/commit/{oid}")
    );
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), oid);
    assert_eq!(
        git(repo, &["diff", "--cached", "--binary"]).unwrap(),
        cached
    );
    assert_eq!(git(repo, &["diff", "--binary"]).unwrap(), working);
}

#[test]
fn custom_templates_support_arbitrary_hosts_and_require_safe_web_urls() {
    let directory = fixture();
    let repo = directory.path();
    for oid in [OID.to_owned(), "a".repeat(64)] {
        git(
            repo,
            &[
                "config",
                "igit.webCommitUrl",
                "http://[::1]:8080/forge/team/repo/commit/{commit}?view=diff",
            ],
        )
        .unwrap();
        assert_eq!(
            commit_url(repo, &oid).unwrap(),
            format!("http://[::1]:8080/forge/team/repo/commit/{oid}?view=diff")
        );
    }
    git(
        repo,
        &[
            "config",
            "igit.webCommitUrl",
            "https://forge.test/?p=repo;a=commit;h={commit}",
        ],
    )
    .unwrap();
    assert_eq!(
        commit_url(repo, OID).unwrap(),
        format!("https://forge.test/?p=repo;a=commit;h={OID}")
    );
    for template in [
        "",
        "https://forge.test/team/repo",
        "javascript:alert('{commit}')",
        "file:///tmp/{commit}",
        "https://user:secret@forge.test/team/repo/{commit}",
        "https://forge.test/team/repo/{commit}\nsecret",
        "https://forge.test/team/repo/{unknown}/{commit}",
    ] {
        git(repo, &["config", "igit.webCommitUrl", template]).unwrap();
        let error = commit_url(repo, OID).unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
        assert!(error.contains("igit.webCommitUrl"), "{error}");
    }
    assert!(commit_url(repo, "HEAD").is_err());
}
