use igit::core::{git, Context};
use std::path::Path;
fn g(p: &Path, a: &[&str]) {
    git(p, a).unwrap();
}
#[test]
fn independent_worktrees_submodule_and_bare_remote() {
    let td = tempfile::tempdir().unwrap();
    let base = td.path();
    for name in ["web", "api"] {
        let p = base.join(name);
        std::fs::create_dir(&p).unwrap();
        g(&p, &["init", "-q", "-b", "main"]);
        g(
            &p,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@invalid",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "base",
            ],
        );
    }
    let web = base.join("web");
    g(
        &web,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            base.join("api").to_str().unwrap(),
            "deps/sdk",
        ],
    );
    g(
        &web,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            base.join("review-web").to_str().unwrap(),
        ],
    );
    g(base, &["init", "--bare", "-q", "origin.git"]);
    let c = Context::discover(base).unwrap();
    assert_eq!(c.repos.len(), 4);
    let sub = c.repos.iter().find(|r| r.name == "sdk").unwrap();
    assert!(sub.submodule);
    assert_eq!(c.repos[sub.parent.unwrap()].name, "web");
    assert!(c
        .repos
        .iter()
        .find(|r| r.name == "review-web")
        .unwrap()
        .branch
        .starts_with("detached@"));
    assert_eq!(c.scope.len(), 4);
}
