use super::*;
use crate::core::{command, git, git_bytes, revpath};
use std::fs;

fn run(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn init(repo: &Path, marker: &str) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Remote tester"],
        &["config", "user.email", "remote@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["config", "core.hooksPath", "/dev/null"],
    ] {
        run(repo, args);
    }
    fs::write(repo.join("tracked.txt"), format!("{marker}\n")).unwrap();
    fs::write(repo.join("中文 file.txt"), format!("{marker} 内容\n")).unwrap();
    run(repo, &["add", "."]);
    run(repo, &["commit", "-qm", "base"]);
}

fn fixture() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["api", "web"] {
        let repo = dir.path().join(name);
        fs::create_dir(&repo).unwrap();
        init(&repo, name);
    }
    let mut context = Context::discover(dir.path()).unwrap();
    context.state_dir = dir.path().join("state");
    (dir, context)
}

fn root(c: &Context, name: &str) -> usize {
    c.repos
        .iter()
        .position(|repo| repo.path.ends_with(name))
        .unwrap()
}

fn configured(repo: &Path, name: &str) -> Remote {
    model::read(repo)
        .unwrap()
        .into_iter()
        .find(|remote| remote.name == name)
        .unwrap()
}

fn state(repo: &Path) -> Vec<Vec<u8>> {
    let git_dir = revpath(repo, "--absolute-git-dir").unwrap();
    let mut state = vec![
        fs::read(git_dir.join("HEAD")).unwrap(),
        fs::read(git_dir.join("index")).unwrap(),
        git_bytes(repo, &["rev-parse", "HEAD"]).unwrap(),
        git_bytes(
            repo,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ],
        )
        .unwrap(),
    ];
    for file in ["tracked.txt", "中文 file.txt"] {
        state.push(fs::read(repo.join(file)).unwrap());
    }
    state
}

fn local_config(repo: &Path) -> String {
    run(repo, &["config", "--local", "--list"])
}

fn optional(repo: &Path, args: &[&str]) -> Option<String> {
    let output = command(repo).args(args).output().unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
}

fn prepare(c: &Context, root: usize, operation: Operation) -> Review {
    Review::prepare(c, root, operation).unwrap()
}

fn repo_at(path: &Path) -> Repo {
    Repo {
        path: fs::canonicalize(path).unwrap(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        branch: String::new(),
        submodule: false,
        parent: None,
    }
}

#[test]
fn identity_uses_the_selected_repository_config_not_the_process_workspace() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let api = c.repos[api_root].path.clone();
    let expected_common = fs::canonicalize(api.join(".git")).unwrap();
    let identity = Identity::read(&c.repos[api_root]).unwrap();
    assert_eq!(identity.git_dir, expected_common);
    assert_eq!(identity.common_dir, expected_common);
    assert_eq!(identity.config, expected_common.join("config"));
}

#[test]
fn linked_worktree_identity_uses_the_main_repository_config() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let api = c.repos[api_root].path.clone();
    let holder = tempfile::tempdir().unwrap();
    let linked = holder.path().join("linked");
    run(
        &api,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    let identity = Identity::read(&repo_at(&linked)).unwrap();
    let expected_common = fs::canonicalize(api.join(".git")).unwrap();
    assert_eq!(identity.worktree, fs::canonicalize(&linked).unwrap());
    assert_eq!(identity.common_dir, expected_common);
    assert_eq!(identity.config, expected_common.join("config"));
}

#[test]
fn submodule_identity_uses_the_submodule_git_directory_config() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let web_root = root(&c, "web");
    let api = c.repos[api_root].path.clone();
    let web = c.repos[web_root].path.clone();
    run(
        &web,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            api.to_str().unwrap(),
            "module",
        ],
    );
    run(&web, &["commit", "-qm", "add submodule"]);
    let submodule = web.join("module");
    let identity = Identity::read(&repo_at(&submodule)).unwrap();
    let expected_common = fs::canonicalize(web.join(".git/modules/module")).unwrap();
    assert_eq!(identity.git_dir, expected_common);
    assert_eq!(identity.common_dir, expected_common);
    assert_eq!(identity.config, expected_common.join("config"));
    assert_ne!(
        identity.config,
        fs::canonicalize(web.join(".git/config")).unwrap()
    );
}

#[test]
fn endpoint_updates_reject_an_identity_from_another_repository() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let web_root = root(&c, "web");
    let api = c.repos[api_root].path.clone();
    let web = c.repos[web_root].path.clone();
    run(&api, &["remote", "add", "target", "../target"]);
    let mut identity = Identity::read(&c.repos[api_root]).unwrap();
    identity.config = fs::canonicalize(web.join(".git/config")).unwrap();
    let review = Review {
        root: api_root,
        repository: "api".into(),
        path: api.clone(),
        identity,
        config: ConfigSnapshot::read(&api).unwrap(),
        refs: read_refs(&api).unwrap(),
        operation: Operation::Add(Remote {
            name: "target".into(),
            urls: vec!["../other".into()],
            push_urls: vec![],
        }),
    };
    let api_config = fs::read(api.join(".git/config")).unwrap();
    let web_config = fs::read(web.join(".git/config")).unwrap();
    let error = review
        .update_endpoints(
            &api,
            &review.identity,
            &review.config,
            &review.refs,
            &Remote {
                name: "target".into(),
                urls: vec!["../other".into()],
                push_urls: vec![],
            },
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Git directory") || error.contains("Repository"),
        "{error}"
    );
    assert_eq!(fs::read(api.join(".git/config")).unwrap(), api_config);
    assert_eq!(fs::read(web.join(".git/config")).unwrap(), web_config);
    assert!(!api.join(".git/config.lock").exists());
    assert!(!web.join(".git/config.lock").exists());
}

#[test]
fn lifecycle_changes_one_root_and_preserves_other_roots_settings_upstreams_and_files() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let web_root = root(&c, "web");
    let api = c.repos[api_root].path.clone();
    let web = c.repos[web_root].path.clone();
    let initial_state = state(&api);

    run(&web, &["remote", "add", "shared", "../web-only"]);
    run(&web, &["config", "remote.shared.uploadpack", "web-upload"]);
    let web_config = local_config(&web);
    let web_refs = run(
        &web,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/remotes/",
        ],
    );

    let fetch_one = "https://alice:s3cret@example.invalid/relative [x](y)+$.git";
    let fetch_two = "../本地 path/仓库 [literal].git";
    let push_one = "file:///tmp/push [x](y)+$.git";
    let push_two = "C:\\push 目录\\repo[1].git";
    let added = Remote {
        name: "shared".into(),
        urls: vec![fetch_one.into(), fetch_two.into()],
        push_urls: vec![push_one.into(), push_two.into()],
    };
    let review = prepare(&c, api_root, Operation::Add(added.clone()));
    assert_eq!(review.title(), "Add remote");
    assert_eq!(review.destination(), (api_root, Some("shared".into())));
    let body = review.body();
    assert!(body.contains("<redacted>@example.invalid"), "{body}");
    assert!(!body.contains("s3cret"), "{body}");
    assert!(body.contains(fetch_two), "{body}");
    assert!(body.contains(push_one), "{body}");
    review.execute(&c).unwrap();
    assert_eq!(configured(&api, "shared"), added);
    assert_eq!(state(&api), initial_state);
    assert_eq!(local_config(&web), web_config);
    assert_eq!(
        run(
            &web,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/remotes/"
            ]
        ),
        web_refs
    );

    run(
        &api,
        &[
            "config",
            "--add",
            "remote.shared.fetch",
            "+refs/heads/review/*:refs/remotes/shared/review/*",
        ],
    );
    run(
        &api,
        &["config", "remote.shared.uploadpack", "custom-upload"],
    );
    run(&api, &["config", "remote.shared.proxy", "custom-proxy"]);
    run(&api, &["config", "branch.main.remote", "shared"]);
    run(&api, &["config", "branch.main.merge", "refs/heads/main"]);
    run(&api, &["config", "branch.main.pushremote", "shared"]);
    run(&api, &["config", "remote.pushdefault", "shared"]);
    let oid = run(&api, &["rev-parse", "HEAD"]);
    run(&api, &["update-ref", "refs/remotes/shared/main", &oid]);
    run(
        &api,
        &["update-ref", "refs/remotes/shared/review/topic", &oid],
    );
    let refs_before_edit = run(
        &api,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/remotes/",
        ],
    );

    let before = configured(&api, "shared");
    let edited = Remote {
        name: "shared".into(),
        urls: vec![
            "../edited [one](two)+$.git".into(),
            "ssh://carol:token@example.invalid/团队 repo.git".into(),
        ],
        push_urls: vec![],
    };
    let review = prepare(
        &c,
        api_root,
        Operation::Edit {
            before: before.clone(),
            after: edited.clone(),
        },
    );
    assert_eq!(review.title(), "Edit remote");
    let body = review.body();
    assert!(body.contains("Push URLs are cleared"), "{body}");
    assert!(body.contains("<redacted>@example.invalid"), "{body}");
    assert!(!body.contains("token"), "{body}");
    review.execute(&c).unwrap();
    assert_eq!(configured(&api, "shared"), edited);
    assert!(configured(&api, "shared").push_urls.is_empty());
    let fetch = run(&api, &["config", "--get-all", "remote.shared.fetch"]);
    assert!(fetch.contains("refs/remotes/shared/review/*"), "{fetch}");
    assert!(fetch.contains("refs/remotes/shared/*"), "{fetch}");
    assert_eq!(
        run(&api, &["config", "--get", "remote.shared.uploadpack"]),
        "custom-upload"
    );
    assert_eq!(
        run(&api, &["config", "--get", "remote.shared.proxy"]),
        "custom-proxy"
    );
    assert_eq!(
        run(&api, &["config", "--get", "branch.main.remote"]),
        "shared"
    );
    assert_eq!(
        run(
            &api,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/remotes/"
            ]
        ),
        refs_before_edit
    );
    assert_eq!(state(&api), initial_state);

    let renamed = Remote {
        name: "共享-remote".into(),
        urls: edited.urls.clone(),
        push_urls: vec![push_one.into(), push_two.into()],
    };
    let review = prepare(
        &c,
        api_root,
        Operation::Edit {
            before: edited.clone(),
            after: renamed.clone(),
        },
    );
    assert_eq!(review.title(), "Rename remote");
    assert_eq!(review.destination(), (api_root, Some("共享-remote".into())));
    review.execute(&c).unwrap();
    assert!(model::read(&api)
        .unwrap()
        .iter()
        .all(|remote| remote.name != "shared"));
    assert_eq!(configured(&api, "共享-remote"), renamed);
    let fetch = run(&api, &["config", "--get-all", "remote.共享-remote.fetch"]);
    assert!(fetch.contains("refs/remotes/共享-remote/*"), "{fetch}");
    assert!(
        fetch.contains("refs/remotes/共享-remote/review/*"),
        "{fetch}"
    );
    assert!(!fetch.contains("refs/remotes/shared/"), "{fetch}");
    assert_eq!(
        run(&api, &["config", "--get", "remote.共享-remote.uploadpack"]),
        "custom-upload"
    );
    assert_eq!(
        run(&api, &["config", "--get", "remote.共享-remote.proxy"]),
        "custom-proxy"
    );
    assert_eq!(
        run(&api, &["config", "--get", "branch.main.remote"]),
        "共享-remote"
    );
    assert_eq!(
        run(&api, &["config", "--get", "branch.main.pushremote"]),
        "共享-remote"
    );
    assert_eq!(
        run(&api, &["config", "--get", "remote.pushdefault"]),
        "共享-remote"
    );
    let refs = run(
        &api,
        &["for-each-ref", "--format=%(refname)", "refs/remotes/"],
    );
    assert!(refs.contains("refs/remotes/共享-remote/main"), "{refs}");
    assert!(!refs.contains("refs/remotes/shared/"), "{refs}");
    assert_eq!(state(&api), initial_state);
    assert_eq!(local_config(&web), web_config);

    let review = prepare(
        &c,
        api_root,
        Operation::Remove(configured(&api, "共享-remote")),
    );
    assert_eq!(review.title(), "Remove remote");
    assert_eq!(review.destination(), (api_root, None));
    let body = review.body();
    assert!(
        body.contains("local remote-tracking refs")
            && body.contains("Branch remote and upstream")
            && body.contains("server repository is not deleted"),
        "{body}"
    );
    review.execute(&c).unwrap();
    assert!(model::read(&api)
        .unwrap()
        .iter()
        .all(|remote| remote.name != "共享-remote"));
    assert_eq!(
        optional(&api, &["config", "--get", "branch.main.remote"]),
        None
    );
    assert_eq!(
        optional(&api, &["config", "--get", "branch.main.merge"]),
        None
    );
    assert_eq!(
        optional(&api, &["config", "--get", "remote.pushdefault"]),
        None
    );
    let refs = run(
        &api,
        &["for-each-ref", "--format=%(refname)", "refs/remotes/"],
    );
    assert!(!refs.contains("共享-remote"), "{refs}");
    assert_eq!(state(&api), initial_state);
    assert_eq!(configured(&web, "shared").name, "shared");
    assert_eq!(local_config(&web), web_config);
}

#[test]
fn stale_roots_configuration_and_context_are_rejected_before_writing() {
    for change in ["roots", "config", "context"] {
        let (_dir, mut c) = fixture();
        let api_root = root(&c, "api");
        let api = c.repos[api_root].path.clone();
        run(&api, &["remote", "add", "origin", "../original [x].git"]);
        let before = configured(&api, "origin");
        let review = prepare(
            &c,
            api_root,
            Operation::Edit {
                before: before.clone(),
                after: Remote {
                    name: "origin".into(),
                    urls: vec!["../replacement".into()],
                    push_urls: vec![],
                },
            },
        );
        let state_before = state(&api);
        match change {
            "roots" => {
                c.scope.remove(&api_root);
            }
            "config" => {
                run(
                    &api,
                    &["config", "remote.origin.uploadpack", "external-change"],
                );
            }
            "context" => {
                c.repos[api_root].path = api.join("missing");
            }
            _ => unreachable!(),
        }
        let error = review.execute(&c).unwrap_err().to_string();
        match change {
            "roots" => assert!(error.contains("Roots"), "{error}"),
            "config" => assert!(error.contains("configuration"), "{error}"),
            "context" => assert!(error.contains("Repository"), "{error}"),
            _ => unreachable!(),
        }
        assert!(!error.contains("s3cret"), "{error}");
        assert_eq!(state(&api), state_before);
        if change != "config" {
            assert_eq!(configured(&api, "origin"), before);
        } else {
            assert_eq!(
                configured(&api, "origin").urls,
                vec!["../original [x].git".to_owned()]
            );
            assert_eq!(
                run(&api, &["config", "--get", "remote.origin.uploadpack"]),
                "external-change"
            );
        }
    }
}

#[test]
fn validation_accepts_git_paths_and_unicode_but_rejects_options_controls_and_empty_lines() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let api = c.repos[api_root].path.clone();
    let valid_url = "../unused";

    for name in ["", "-origin", "--global", "bad\nname", "bad\u{1b}name"] {
        let error = Review::prepare(
            &c,
            api_root,
            Operation::Add(Remote {
                name: name.into(),
                urls: vec![valid_url.into()],
                push_urls: vec![],
            }),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(!error.is_empty());
    }
    for urls in [
        vec![],
        vec![String::new()],
        vec!["https://example.invalid/a\nb".into()],
        vec!["https://example.invalid/a\tb".into()],
        vec!["https://example.invalid/a\0b".into()],
    ] {
        assert!(Review::prepare(
            &c,
            api_root,
            Operation::Add(Remote {
                name: "invalid-url".into(),
                urls,
                push_urls: vec![],
            }),
        )
        .is_err());
    }
    assert!(Review::prepare(
        &c,
        api_root,
        Operation::Add(Remote {
            name: "invalid-push".into(),
            urls: vec![valid_url.into()],
            push_urls: vec![String::new()],
        }),
    )
    .is_err());

    assert!(validate_name(&api, "url").is_ok());
    assert!(validate_name(&api, "uploadpack").is_ok());
    assert!(validate_name(&api, "remote.origin").is_ok());

    let valid = Remote {
        name: "远程-origin".into(),
        urls: vec![
            "../本地 path/仓库 [literal].git".into(),
            "git@example.invalid:团队/仓库 with space.git".into(),
            "https://example.invalid/a b[c](d)+$.git".into(),
        ],
        push_urls: vec![r"C:\push 目录\仓库.git".into()],
    };
    prepare(&c, api_root, Operation::Add(valid.clone()))
        .execute(&c)
        .unwrap();
    assert_eq!(configured(&api, "远程-origin"), valid);
    let error = Review::prepare(&c, api_root, Operation::Add(valid))
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("already exists"), "{error}");
}

#[test]
fn inherited_remote_and_branch_configuration_is_guarded_with_an_explanation() {
    let (_dir, c) = fixture();
    let api_root = root(&c, "api");
    let api = c.repos[api_root].path.clone();
    let include = api.join(".git/remote-extra.conf");
    fs::write(
        &include,
        "[remote \"inherited\"]\n\turl = ../inherited\n\tfetch = +refs/heads/*:refs/remotes/inherited/*\n",
    )
    .unwrap();
    run(&api, &["config", "include.path", "remote-extra.conf"]);

    let inherited = configured(&api, "inherited");
    let original = fs::read(&include).unwrap();
    let error = Review::prepare(&c, api_root, Operation::Remove(inherited.clone()))
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.to_ascii_lowercase().contains("inherited")
            || error.to_ascii_lowercase().contains("external"),
        "{error}"
    );
    assert_eq!(fs::read(&include).unwrap(), original);
    assert!(Review::prepare(&c, api_root, Operation::Add(inherited)).is_err());

    run(&api, &["remote", "add", "origin", "../origin"]);
    let mut included = fs::read_to_string(&include).unwrap();
    included.push_str("\n[branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n");
    fs::write(&include, &included).unwrap();
    let before = configured(&api, "origin");
    let error = Review::prepare(
        &c,
        api_root,
        Operation::Edit {
            before,
            after: Remote {
                name: "renamed".into(),
                urls: vec!["../renamed".into()],
                push_urls: vec![],
            },
        },
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        error.to_ascii_lowercase().contains("inherited")
            || error.to_ascii_lowercase().contains("external"),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&include).unwrap(), included);
}
