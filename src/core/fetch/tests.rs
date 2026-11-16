use super::*;
use crate::core::{git, git_bytes};
use std::{
    ffi::{OsStr, OsString},
    fs,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

struct Environment {
    saved: Vec<(String, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl Environment {
    fn new(path: &Path) -> Self {
        let mut environment = Self {
            saved: Vec::new(),
            _lock: ENVIRONMENT
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        };
        let config = path.join("global-config");
        fs::write(&config, "").unwrap();
        environment.set("GIT_CONFIG_GLOBAL", Some(config.as_os_str()));
        environment.set("GIT_CONFIG_NOSYSTEM", Some(OsStr::new("1")));
        for key in [
            "GIT_CONFIG",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_SSH_VARIANT",
            "GIT_PROXY_COMMAND",
            "GIT_SSL_NO_VERIFY",
            "GIT_PROXY_SSL_NO_VERIFY",
        ] {
            environment.set(key, None);
        }
        environment
    }

    fn set(&mut self, key: &str, value: Option<&OsStr>) {
        if !self.saved.iter().any(|(name, _)| name == key) {
            self.saved.push((key.to_owned(), std::env::var_os(key)));
        }
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    repo: PathBuf,
    seed: PathBuf,
    remote: PathBuf,
    environment: Environment,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let environment = Environment::new(directory.path());
        let seed = directory.path().join("seed");
        let remote = directory.path().join("fetch.git");
        let repo = directory.path().join("working");
        fs::create_dir(&seed).unwrap();
        git(&seed, &["init", "-q", "-b", "main"]).unwrap();
        git(&seed, &["config", "user.name", "Fetch Test"]).unwrap();
        git(&seed, &["config", "user.email", "fetch@example.invalid"]).unwrap();
        fs::write(seed.join("mixed"), b"base mixed\r\n").unwrap();
        fs::write(seed.join("unrelated"), b"base unrelated\n").unwrap();
        git(&seed, &["add", "--", "mixed", "unrelated"]).unwrap();
        let tree = git(&seed, &["write-tree"]).unwrap();
        let base = git(&seed, &["commit-tree", &tree, "-m", "Initial fixture"]).unwrap();
        git(&seed, &["update-ref", "refs/heads/main", &base]).unwrap();
        git(
            directory.path(),
            &[
                "clone",
                "-q",
                "--bare",
                seed.to_str().unwrap(),
                remote.to_str().unwrap(),
            ],
        )
        .unwrap();
        git(
            directory.path(),
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        )
        .unwrap();
        Self {
            directory,
            repo,
            seed,
            remote,
            environment,
        }
    }

    fn advance_seed(&self) -> String {
        let parent = git(&self.seed, &["rev-parse", "HEAD"]).unwrap();
        fs::write(self.seed.join("remote-change"), format!("after {parent}\n")).unwrap();
        git(&self.seed, &["add", "--", "remote-change"]).unwrap();
        let tree = git(&self.seed, &["write-tree"]).unwrap();
        let oid = git(
            &self.seed,
            &["commit-tree", &tree, "-p", &parent, "-m", "Remote change"],
        )
        .unwrap();
        git(&self.seed, &["update-ref", "refs/heads/main", &oid]).unwrap();
        oid
    }

    fn advance_remote(&self) -> String {
        let oid = self.advance_seed();
        git(
            &self.remote,
            &[
                "fetch",
                "--no-write-fetch-head",
                "--no-tags",
                "--",
                self.seed.to_str().unwrap(),
                "refs/heads/main:refs/heads/main",
            ],
        )
        .unwrap();
        oid
    }

    fn target(&self) -> Target {
        discover(&self.repo, false, &[], &[])
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    }

    fn push_remote(&self) -> PathBuf {
        let remote = self.directory.path().join("push.git");
        git(
            self.directory.path(),
            &[
                "clone",
                "-q",
                "--bare",
                self.seed.to_str().unwrap(),
                remote.to_str().unwrap(),
            ],
        )
        .unwrap();
        git(
            &self.repo,
            &["config", "remote.origin.pushurl", remote.to_str().unwrap()],
        )
        .unwrap();
        remote
    }
}

#[test]
fn ordinary_fetch_preserves_reviewed_bytes_and_only_updates_tracking_refs() {
    let fixture = Fixture::new();
    let original = git(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();
    fs::write(fixture.repo.join("mixed"), b"staged mixed\0content\r\n").unwrap();
    git(&fixture.repo, &["add", "--", "mixed"]).unwrap();
    fs::write(fixture.repo.join("mixed"), b"working mixed\0different\r\n").unwrap();
    fs::write(fixture.repo.join("unrelated"), b"staged unrelated\n").unwrap();
    git(&fixture.repo, &["add", "--", "unrelated"]).unwrap();
    git(&fixture.repo, &["tag", "local-only"]).unwrap();
    git(
        &fixture.repo,
        &["update-ref", "refs/remotes/origin/obsolete", &original],
    )
    .unwrap();
    for (key, value) in [
        ("fetch.prune", "true"),
        ("fetch.pruneTags", "true"),
        ("fetch.all", "true"),
        ("fetch.writeCommitGraph", "true"),
        ("remote.origin.prune", "true"),
        ("remote.origin.pruneTags", "true"),
        ("remote.origin.tagOpt", "--tags"),
        ("submodule.recurse", "true"),
        ("gc.auto", "1"),
        ("maintenance.auto", "true"),
    ] {
        git(&fixture.repo, &["config", key, value]).unwrap();
    }
    let expected = fixture.advance_remote();
    git(
        &fixture.remote,
        &["update-ref", "refs/tags/server-only", &expected],
    )
    .unwrap();
    let directory = fixture.repo.join(".git");
    fs::write(directory.join("FETCH_HEAD"), b"keep fetch head\0bytes\n").unwrap();
    let protected = [
        directory.join("HEAD"),
        directory.join("index"),
        directory.join("config"),
        directory.join("FETCH_HEAD"),
        fixture.repo.join("mixed"),
        fixture.repo.join("unrelated"),
    ]
    .map(|path| {
        let bytes = fs::read(&path).unwrap();
        (path, bytes)
    });
    let heads = git_bytes(
        &fixture.repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads/",
        ],
    )
    .unwrap();
    let tags = git_bytes(
        &fixture.repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/tags/",
        ],
    )
    .unwrap();
    let target = fixture.target();
    let outcome = execute(&target, &ReadControl::default()).unwrap();
    assert!(outcome.changed);
    assert!(outcome.tip.is_none());
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        expected
    );
    assert_eq!(
        git(
            &fixture.repo,
            &["rev-parse", "refs/remotes/origin/obsolete"]
        )
        .unwrap(),
        original
    );
    assert_eq!(
        git_bytes(
            &fixture.repo,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads/"
            ]
        )
        .unwrap(),
        heads
    );
    assert_eq!(
        git_bytes(
            &fixture.repo,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/tags/"
            ]
        )
        .unwrap(),
        tags
    );
    for (path, bytes) in protected {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
    }
    assert!(!execute(&target, &ReadControl::default()).unwrap().changed);
}

#[test]
fn safe_custom_and_negative_refspecs_keep_the_configured_scope() {
    let fixture = Fixture::new();
    let original = git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap();
    let expected = fixture.advance_remote();
    for reference in ["refs/heads/private/hidden", "refs/custom/review"] {
        git(&fixture.remote, &["update-ref", reference, &expected]).unwrap();
    }
    git(
        &fixture.repo,
        &[
            "config",
            "--replace-all",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/custom/*",
        ],
    )
    .unwrap();
    for refspec in [
        "^refs/heads/private/*",
        "+refs/custom/*:refs/remotes/review/*",
    ] {
        git(
            &fixture.repo,
            &["config", "--add", "remote.origin.fetch", refspec],
        )
        .unwrap();
    }
    let config = fs::read(fixture.repo.join(".git/config")).unwrap();
    assert!(
        execute(&fixture.target(), &ReadControl::default())
            .unwrap()
            .changed
    );
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "refs/remotes/custom/main"]).unwrap(),
        expected
    );
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "refs/remotes/review/review"]).unwrap(),
        expected
    );
    assert!(git(
        &fixture.repo,
        &["show-ref", "--verify", "refs/remotes/custom/private/hidden"]
    )
    .is_err());
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        original
    );
    assert_eq!(fs::read(fixture.repo.join(".git/config")).unwrap(), config);
}

#[test]
fn unsafe_or_implicit_refspecs_are_rejected_without_fetching() {
    let fixture = Fixture::new();
    let original = git(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();
    for refspec in [
        "+refs/heads/*:refs/heads/*",
        "+refs/tags/*:refs/tags/*",
        "+refs/*:refs/*",
        "refs/heads/main",
        "refs/heads/main:FETCH_HEAD",
        "^refs/heads/private/*",
        "+refs/heads/*:refs/remotes/../heads/*",
        "+refs/heads/**:refs/remotes/origin/**",
        "tag release",
        "",
    ] {
        git(
            &fixture.repo,
            &["config", "--replace-all", "remote.origin.fetch", refspec],
        )
        .unwrap();
        let report = discover_with_warnings(&fixture.repo, false, &[], &[]).unwrap();
        assert!(report.targets.is_empty(), "{refspec}");
        assert!(!report.warnings.is_empty(), "{refspec}");
        assert!(discover(&fixture.repo, false, &[], &[]).is_err());
        assert_eq!(
            git(&fixture.repo, &["rev-parse", "HEAD"]).unwrap(),
            original
        );
    }
}

#[test]
fn unsupported_remote_does_not_hide_an_independent_safe_target() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &[
            "remote",
            "add",
            "unsafe",
            "ext::unsafe-command-with-private-token",
        ],
    )
    .unwrap();
    let report = discover_with_warnings(&fixture.repo, true, &[], &[]).unwrap();
    assert_eq!(report.targets.len(), 1);
    assert_eq!(report.targets[0].key.remote, "origin");
    assert_eq!(report.warnings.len(), 1);
    assert!(!report.warnings[0].contains("private-token"));
    assert_eq!(discover(&fixture.repo, true, &[], &[]).unwrap().len(), 1);
}

#[test]
fn malformed_refspecs_in_one_remote_do_not_break_other_remotes_or_push_capture() {
    let fixture = Fixture::new();
    let push_remote = fixture.push_remote();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.broken.url",
            fixture.remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.broken.pushurl",
            push_remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.broken.fetch",
            "+refs/heads/**:refs/remotes/broken/**",
        ],
    )
    .unwrap();
    let report = discover_with_warnings(
        &fixture.repo,
        true,
        &[],
        &[("broken".into(), "main".into())],
    )
    .unwrap();
    assert_eq!(report.targets.len(), 2);
    assert_eq!(report.warnings.len(), 1);
    assert!(report
        .targets
        .iter()
        .any(|target| { target.key.remote == "origin" && target.key.branch.is_none() }));
    assert!(report.targets.iter().any(|target| {
        target.key.remote == "broken" && target.key.branch.as_deref() == Some("main")
    }));
    assert_eq!(
        push_target(&fixture.repo, "broken", "main")
            .unwrap()
            .endpoint,
        push_remote.to_str().unwrap()
    );
}

#[test]
fn endpoint_resolution_preserves_configuration_in_nested_worktrees_and_bare_repositories() {
    let fixture = Fixture::new();
    let push_remote = fixture.push_remote();
    let nested = fixture.repo.join("nested");
    fs::create_dir(&nested).unwrap();
    git(
        &fixture.remote,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    )
    .unwrap();
    git(
        &fixture.remote,
        &[
            "config",
            "remote.origin.pushurl",
            push_remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    for (path, config, fetch_endpoint) in [
        (&nested, fixture.repo.join(".git/config"), &fixture.remote),
        (
            &fixture.remote,
            fixture.remote.join("config"),
            &fixture.seed,
        ),
    ] {
        let before = fs::read(&config).unwrap();
        let report =
            discover_with_warnings(path, false, &[], &[("origin".into(), "main".into())]).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.targets.len(), 2);
        for target in report.targets {
            let expected = if target.key.branch.is_some() {
                &push_remote
            } else {
                fetch_endpoint
            };
            assert_eq!(target.endpoint, expected.to_str().unwrap());
        }
        assert_eq!(
            push_target(path, "origin", "main").unwrap().endpoint,
            push_remote.to_str().unwrap()
        );
        assert_eq!(fs::read(&config).unwrap(), before);
    }
}

#[test]
fn upstream_then_origin_then_unique_remote_are_selected() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &[
            "remote",
            "add",
            "upstream",
            fixture.remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    git(&fixture.repo, &["config", "branch.main.remote", "upstream"]).unwrap();
    assert_eq!(fixture.target().key.remote, "upstream");
    git(&fixture.repo, &["config", "--unset", "branch.main.remote"]).unwrap();
    assert_eq!(fixture.target().key.remote, "origin");
    git(&fixture.repo, &["remote", "remove", "origin"]).unwrap();
    assert_eq!(fixture.target().key.remote, "upstream");
    git(&fixture.repo, &["remote", "remove", "upstream"]).unwrap();
    assert!(discover(&fixture.repo, false, &[], &[]).unwrap().is_empty());
}

#[test]
fn exclusions_and_skip_fetch_all_apply_to_ordinary_and_automatic_push_targets() {
    let fixture = Fixture::new();
    let push = [("origin".into(), "main".into())];
    assert!(discover(&fixture.repo, true, &["origin".into()], &push)
        .unwrap()
        .is_empty());
    for value in ["true", "yes", "on", "1", "2"] {
        git(
            &fixture.repo,
            &["config", "remote.origin.skipFetchAll", value],
        )
        .unwrap();
        assert!(discover(&fixture.repo, true, &[], &push)
            .unwrap()
            .is_empty());
        let manual = push_target(&fixture.repo, "origin", "main").unwrap();
        assert_eq!(
            execute(&manual, &ReadControl::default()).unwrap().tip,
            Some(git(&fixture.remote, &["rev-parse", "refs/heads/main"]).unwrap())
        );
    }
    assert!(push_target(&fixture.repo, "missing", "main").is_err());
}

#[test]
fn push_endpoint_tip_and_deletion_do_not_overwrite_fetch_endpoint_tracking() {
    let fixture = Fixture::new();
    let original = git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap();
    let expected = fixture.advance_seed();
    let push_remote = fixture.push_remote();
    let fetch_head = fixture.repo.join(".git/FETCH_HEAD");
    fs::write(&fetch_head, b"preserved\n").unwrap();
    let refs = tracking_refs(&fixture.repo).unwrap();
    let targets = discover(
        &fixture.repo,
        false,
        &[],
        &[("origin".into(), "main".into())],
    )
    .unwrap();
    assert_eq!(targets.len(), 2);
    let target = targets
        .iter()
        .find(|target| target.key.branch.is_some())
        .unwrap();
    assert_ne!(fixture.target().endpoint, target.endpoint);
    let outcome = execute(target, &ReadControl::default()).unwrap();
    assert!(!outcome.changed);
    assert_eq!(outcome.tip, Some(expected.clone()));
    assert_eq!(tracking_refs(&fixture.repo).unwrap(), refs);
    assert_eq!(fs::read(&fetch_head).unwrap(), b"preserved\n");
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap(),
        original
    );
    git(
        &fixture.repo,
        &["cat-file", "-e", &format!("{expected}^{{commit}}")],
    )
    .unwrap();
    git(&push_remote, &["update-ref", "-d", "refs/heads/main"]).unwrap();
    assert!(execute(target, &ReadControl::default())
        .unwrap()
        .tip
        .is_none());
    assert_eq!(tracking_refs(&fixture.repo).unwrap(), refs);
}

#[test]
fn ambiguous_push_urls_only_exclude_the_push_target() {
    let fixture = Fixture::new();
    let push_remote = fixture.push_remote();
    git(
        &fixture.repo,
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            push_remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    let report = discover_with_warnings(
        &fixture.repo,
        false,
        &[],
        &[("origin".into(), "main".into())],
    )
    .unwrap();
    assert_eq!(report.targets.len(), 1);
    assert!(report.targets[0].key.branch.is_none());
    assert_eq!(report.warnings.len(), 1);
    assert!(push_target(&fixture.repo, "origin", "main").is_err());
}

#[test]
fn multiple_fetch_urls_keep_the_first_fetchable_endpoint_but_reject_ambiguous_push() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &[
            "config",
            "--add",
            "remote.origin.url",
            "ext::unsupported-private-token",
        ],
    )
    .unwrap();
    let report = discover_with_warnings(
        &fixture.repo,
        false,
        &[],
        &[("origin".into(), "main".into())],
    )
    .unwrap();
    assert_eq!(report.targets.len(), 1);
    assert_eq!(report.targets[0].endpoint, fixture.remote.to_str().unwrap());
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("multiple push URLs"));
    assert!(!report.warnings[0].contains("private-token"));
}

#[test]
fn configuration_changes_invalidate_old_targets_and_change_fingerprints() {
    let fixture = Fixture::new();
    let old = fixture.target();
    fixture.advance_remote();
    let original = git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap();
    git(&fixture.repo, &["config", "fetch.prune", "true"]).unwrap();
    let current = fixture.target();
    assert_ne!(old.key.fingerprint, current.key.fingerprint);
    assert!(execute(&old, &ReadControl::default())
        .unwrap_err()
        .to_string()
        .contains("changed"));
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap(),
        original
    );
    let manual = push_target(&fixture.repo, "origin", "main").unwrap();
    fixture.push_remote();
    assert!(execute(&manual, &ReadControl::default()).is_err());
    let next = push_target(&fixture.repo, "origin", "refs/heads/main").unwrap();
    assert_ne!(manual.key.fingerprint, next.key.fingerprint);
    assert_eq!(next.key.branch.as_deref(), Some("main"));
    let previous = fixture.target();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.url",
            fixture.seed.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert!(execute(&previous, &ReadControl::default()).is_err());
    assert_ne!(previous.key.fingerprint, fixture.target().key.fingerprint);
}

#[test]
fn linked_worktrees_share_canonical_common_directory_keys() {
    let fixture = Fixture::new();
    let linked = fixture.directory.path().join("linked");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked-branch",
            linked.to_str().unwrap(),
        ],
    )
    .unwrap();
    let first = fixture.target();
    let second = discover(&linked, false, &[], &[]).unwrap().remove(0);
    assert_eq!(
        first.key.common_dir,
        fs::canonicalize(fixture.repo.join(".git")).unwrap()
    );
    assert_eq!(first.key, second.key);
    assert_ne!(first.git_dir, second.git_dir);
    let mut replaced = first.clone();
    replaced.path = second.path.clone();
    assert!(execute(&replaced, &ReadControl::default())
        .unwrap_err()
        .to_string()
        .contains("Git directory changed"));
    let expected = fixture.advance_remote();
    execute(&second, &ReadControl::default()).unwrap();
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap(),
        expected
    );
}

#[test]
fn symbolic_tracking_refs_cannot_write_local_branches_even_when_dangling() {
    let fixture = Fixture::new();
    let expected = fixture.advance_remote();
    for reference in ["refs/heads/main", "refs/heads/unborn"] {
        git(
            &fixture.repo,
            &["symbolic-ref", "refs/remotes/origin/danger", reference],
        )
        .unwrap();
        let before = git(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();
        let error = execute(&fixture.target(), &ReadControl::default()).unwrap_err();
        assert!(error.to_string().contains("symbolic ref"));
        assert_eq!(git(&fixture.repo, &["rev-parse", "HEAD"]).unwrap(), before);
        assert_ne!(before, expected);
    }
}

#[test]
fn endpoint_debug_and_discovery_diagnostics_do_not_expose_credentials() {
    let fixture = Fixture::new();
    let endpoint = "https://user:private-token@example.invalid/repo?credential=hidden";
    git(&fixture.repo, &["config", "remote.origin.url", endpoint]).unwrap();
    let target = fixture.target();
    for text in [format!("{:?}", target.key), format!("{target:?}")] {
        assert!(!text.contains("private-token"));
        assert!(!text.contains("credential=hidden"));
    }
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/private-token:refs/heads/secret",
        ],
    )
    .unwrap();
    let error = discover(&fixture.repo, false, &[], &[]).unwrap_err();
    let text = format!("{error:#}");
    assert!(!text.contains("private-token"));
    assert!(!text.contains("credential=hidden"));
}

#[test]
fn unsupported_interactive_authentication_is_skipped_without_network_access() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &["config", "remote.origin.url", "git@example.invalid:repo"],
    )
    .unwrap();
    git(
        &fixture.repo,
        &["config", "core.sshCommand", "custom-ssh private-token"],
    )
    .unwrap();
    let report = discover_with_warnings(&fixture.repo, false, &[], &[]).unwrap();
    assert!(report.targets.is_empty());
    assert!(report.warnings[0].contains("custom SSH"));
    assert!(!report.warnings[0].contains("private-token"));
    git(&fixture.repo, &["config", "--unset", "core.sshCommand"]).unwrap();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo",
        ],
    )
    .unwrap();
    for helper in [
        "!custom-helper private-token",
        "unknown-helper",
        "manager\n",
        "store --file=$(private-token)",
        "cache --socket=/tmp/socket;private-token",
    ] {
        git(
            &fixture.repo,
            &["config", "--replace-all", "credential.helper", helper],
        )
        .unwrap();
        let report = discover_with_warnings(&fixture.repo, false, &[], &[]).unwrap();
        assert!(report.targets.is_empty(), "{helper}");
        assert!(report.warnings[0].contains("credential helper"));
        assert!(!report.warnings[0].contains("private-token"));
    }
    git(&fixture.repo, &["config", "--add", "credential.helper", ""]).unwrap();
    for helper in ["store", "cache --timeout=30", "osxkeychain", "manager"] {
        git(
            &fixture.repo,
            &["config", "--add", "credential.helper", helper],
        )
        .unwrap();
    }
    assert_eq!(discover(&fixture.repo, false, &[], &[]).unwrap().len(), 1);
}

#[test]
fn an_unrelated_hosts_custom_helper_does_not_block_a_safe_http_target() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.url",
            "https://safe.example.invalid/repo",
        ],
    )
    .unwrap();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.pushurl",
            "https://custom.example.invalid/repo",
        ],
    )
    .unwrap();
    git(&fixture.repo, &["config", "credential.helper", "store"]).unwrap();
    git(
        &fixture.repo,
        &[
            "config",
            "credential.https://custom.example.invalid.helper",
            "!custom-helper private-token",
        ],
    )
    .unwrap();
    let report = discover_with_warnings(
        &fixture.repo,
        false,
        &[],
        &[("origin".into(), "main".into())],
    )
    .unwrap();
    assert_eq!(report.targets.len(), 1);
    assert!(report.targets[0].key.branch.is_none());
    assert_eq!(report.warnings.len(), 1);
    assert!(!report.warnings[0].contains("private-token"));
}

#[test]
fn endpoint_rewrites_are_resolved_once_and_ambiguous_chains_are_refused() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &["config", "remote.origin.url", "fixture:fetch"],
    )
    .unwrap();
    let base = fixture.directory.path().to_str().unwrap();
    git(
        &fixture.repo,
        &["config", &format!("url.{base}/.insteadOf"), "fixture:"],
    )
    .unwrap();
    let target = fixture.target();
    assert_eq!(
        target.endpoint,
        fixture.directory.path().join("fetch").to_str().unwrap()
    );
    git(
        &fixture.repo,
        &[
            "config",
            "url.ext::private-token.insteadOf",
            &target.endpoint,
        ],
    )
    .unwrap();
    let report = discover_with_warnings(&fixture.repo, false, &[], &[]).unwrap();
    assert!(report.targets.is_empty());
    assert!(report.warnings[0].contains("chained URL rewrite"));
    assert!(!report.warnings[0].contains("private-token"));
}

#[test]
fn relative_local_urls_resolve_from_the_worktree_root_even_from_a_subdirectory() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &["config", "remote.origin.url", "../fetch.git"],
    )
    .unwrap();
    let nested = fixture.repo.join("nested");
    fs::create_dir(&nested).unwrap();
    let target = discover(&nested, false, &[], &[]).unwrap().remove(0);
    assert_eq!(
        fs::canonicalize(&target.endpoint).unwrap(),
        fs::canonicalize(&fixture.remote).unwrap()
    );
    let expected = fixture.advance_remote();
    execute(&target, &ReadControl::default()).unwrap();
    assert_eq!(
        git(&fixture.repo, &["rev-parse", "origin/main"]).unwrap(),
        expected
    );
}

#[test]
fn commands_disable_prompts_without_weakening_tls_or_host_keys() {
    let mut fixture = Fixture::new();
    git(
        &fixture.repo,
        &[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo",
        ],
    )
    .unwrap();
    git(
        &fixture.repo,
        &["config", "http.https://example.invalid/.sslVerify", "false"],
    )
    .unwrap();
    fixture
        .environment
        .set("GIT_SSL_NO_VERIFY", Some(OsStr::new("1")));
    fixture
        .environment
        .set("GIT_PROXY_SSL_NO_VERIFY", Some(OsStr::new("1")));
    let target = fixture.target();
    let command = transport::fetch_command(&target, fixture.directory.path());
    let arguments: Vec<_> = command
        .get_args()
        .map(|value| value.to_string_lossy())
        .collect();
    for expected in [
        "--no-tags",
        "--no-prune",
        "--no-prune-tags",
        "--no-recurse-submodules",
        "--no-write-fetch-head",
        "--no-auto-maintenance",
        "--atomic",
        "--refmap=",
        "credential.interactive=false",
        "http.sslVerify=true",
        "http.proxySSLVerify=true",
        "http.https://example.invalid/.sslverify=true",
        "fetch.bundleURI=",
        "fetch.followRemoteHEAD=never",
    ] {
        assert!(
            arguments.iter().any(|argument| argument == expected),
            "{expected}"
        );
    }
    let environment: Vec<_> = command.get_envs().collect();
    assert!(environment.contains(&(OsStr::new("GIT_ASKPASS"), Some(OsStr::new("")))));
    assert!(environment.contains(&(OsStr::new("GIT_SSL_NO_VERIFY"), None)));
    assert!(environment.contains(&(OsStr::new("GIT_PROXY_SSL_NO_VERIFY"), None)));
    git(
        &fixture.repo,
        &["config", "remote.origin.url", "git@example.invalid:repo"],
    )
    .unwrap();
    let command = transport::command(&fixture.target(), fixture.directory.path());
    let ssh = command
        .get_envs()
        .find(|(name, _)| *name == OsStr::new("GIT_SSH_COMMAND"))
        .and_then(|(_, value)| value)
        .unwrap()
        .to_string_lossy();
    assert!(ssh.contains("-oBatchMode=yes"));
    assert!(ssh.contains("-oStrictHostKeyChecking=yes"));
    assert!(ssh.contains("-F "));
}

#[test]
fn cancelled_and_expired_execute_reads_never_begin_fetching() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let before = tracking_refs(&fixture.repo).unwrap();
    let cancelled = ReadControl::default();
    cancelled.cancel();
    assert!(execute(&target, &cancelled)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(execute(&target, &ReadControl::new(Duration::ZERO))
        .unwrap_err()
        .to_string()
        .contains("timed out"));
    assert_eq!(tracking_refs(&fixture.repo).unwrap(), before);
}

#[test]
fn authentication_classification_does_not_pause_for_ordinary_network_errors() {
    for message in [
        "fatal: Authentication failed",
        "fatal: could not read Username: terminal prompts disabled",
        "Permission denied (publickey).",
        "The requested URL returned error: 403",
        "Automatic fetch authentication failed; authenticate in a foreground Git command",
    ] {
        assert!(authentication_failed(message), "{message}");
    }
    for message in [
        "Connection timed out",
        "Could not resolve host",
        "Host key verification failed",
        "TLS certificate verification failed",
        "Automatic fetch configuration changed",
        "Git request cancelled",
    ] {
        assert!(!authentication_failed(message), "{message}");
    }
}

#[cfg(unix)]
mod unix;
