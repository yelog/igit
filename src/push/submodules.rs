//! Check changed gitlinks and changed sources against the refs they actually advertise.
//! Git's built-in recurse-submodules=check skips uninitialized remotes and uses
//! local tracking refs; those are not sufficient evidence of publication.
use super::published_graph;
use crate::core::git;
use anyhow::{anyhow, bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsString,
    io::{BufRead, BufReader},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

fn local_objects(repo: &Path) -> Command {
    let mut cmd = published_graph(repo);
    cmd.env("GIT_NO_LAZY_FETCH", "1");
    cmd
}

fn object_read(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let result = local_objects(repo).args(args).output()?;
    if !result.status.success() {
        bail!("local Git object is unavailable")
    }
    Ok(result.stdout)
}

#[allow(dead_code)]
pub(super) fn verify(root: &Path, tip: &str, base: Option<&str>) -> Result<()> {
    verify_for_remote(root, tip, base, None)
}

pub(super) fn verify_for_remote(
    root: &Path,
    tip: &str,
    base: Option<&str>,
    push_url: Option<&str>,
) -> Result<()> {
    let range = base
        .map(|b| format!("{b}..{tip}"))
        .unwrap_or_else(|| tip.to_owned());
    // Walk *all* outgoing commits, including non-first-parent sides. For merge
    // commits compare the result to parent 1; other new sides are traversed too.
    // Stream the raw diff so a project without submodules does not allocate a copy
    // of its entire introduction history for its first push.
    let mut child = local_objects(root)
        .args([
            "log",
            "--format=IGIT:%H",
            "-z",
            "--root",
            "--raw",
            "--no-abbrev",
            "--no-renames",
            "--ignore-submodules=none",
            "--no-color",
            "--no-notes",
            "--no-show-signature",
            "--diff-merges=first-parent",
            "-r",
            &range,
            "--",
        ])
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .context("inspect outgoing gitlinks")?;
    let parsed = read_links(BufReader::new(child.stdout.take().unwrap()));
    if parsed.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let changes = parsed?;
    if !status.success() {
        bail!("cannot inspect outgoing gitlinks; fetch complete history before retrying")
    }
    if changes.is_empty() {
        return Ok(());
    }
    let parent_endpoint = push_url.map(|url| normalize(root, url, root)).transpose()?;
    let mut active: HashMap<(PathBuf, String), BTreeSet<String>> = HashMap::new();
    let mut proven = BTreeSet::new();
    let mut configs: HashMap<String, Sources> = HashMap::new();
    let empty = Sources::new();
    for (rev, mut change) in changes {
        // A commit can change where an unchanged gitlink must be fetched. Compare
        // the committed declarations by path and URL, independent of section names.
        let config_changed = change.modules.is_some();
        let (before, after) = match change.modules {
            Some(delta) => delta,
            None => {
                let blob = object_read(
                    root,
                    &["rev-parse", "--verify", &format!("{rev}:.gitmodules")],
                )
                .with_context(|| {
                    format!(
                        "{}: missing .gitmodules for outgoing gitlink at {:.10}",
                        change.links.keys().next().unwrap().display(),
                        rev
                    )
                })?;
                (None, Some(std::str::from_utf8(&blob)?.trim().to_owned()))
            }
        };
        for blob in before.iter().chain(after.iter()) {
            if !configs.contains_key(blob) {
                configs.insert(blob.clone(), module_config(root, blob)?);
            }
        }
        let sources = after.as_ref().map(|id| &configs[id]).unwrap_or(&empty);
        if config_changed {
            let old = before.as_ref().map(|id| &configs[id]).unwrap_or(&empty);
            let paths: BTreeSet<_> = old
                .keys()
                .chain(sources.keys())
                .filter(|p| old.get(*p) != sources.get(*p) && !change.links.contains_key(*p))
                .collect();
            collect_tree_links(root, &rev, paths, &mut change.links)?;
        }
        for (path, oid) in change.links {
            let source = sources
                .get(&path)
                .filter(|url| !url.is_empty())
                .ok_or_else(|| {
                    anyhow!("{}: missing submodule URL at {:.10}", path.display(), rev)
                })?;
            let repo = root.join(&path);
            let target = resolve_url(source, parent_endpoint.as_deref()).with_context(|| {
                format!(
                    "{}: cannot resolve declared submodule source",
                    path.display()
                )
            })?;
            if !proven.insert((path.clone(), target.clone(), oid.clone())) {
                continue;
            }
            let valid = repo.join(".git").exists()
                && std::fs::canonicalize(&repo).is_ok_and(|p| p.starts_with(root) && p != root);
            if !valid {
                bail!(
                    "{}: initialize the submodule before publishing gitlink {:.10}",
                    path.display(),
                    oid
                )
            }
            let key = (repo.clone(), target.clone());
            if !active.contains_key(&key) {
                let norm = normalize(root, &target, root)?;
                let remotes = git(&repo, &["remote"]).unwrap_or_default();
                let found=remotes.lines().filter(|n|!n.starts_with('-')).find(|n|{
                    let Ok(local)=git(&repo,&["remote","get-url",n])else{return false;};
                    normalize(&repo,&local,&repo).is_ok_and(|v|v==norm)
                }).ok_or_else(||anyhow!("{}: no configured child fetch remote matches the declared submodule source; initialize/sync and publish the child first",path.display()))?;
                // Only contact a configured remote whose fetch URL matches that
                // in the reviewed history. Do not execute URLs from .gitmodules.
                let adv =
                    git(&repo, &["ls-remote", "--heads", "--tags", "--", found]).map_err(|_| {
                        anyhow!(
                            "{}: unable to read child remote branches/tags; parent was not pushed",
                            path.display()
                        )
                    })?;
                let tips = adv
                    .lines()
                    .filter_map(|l| l.split_once('\t'))
                    .map(|(id, _)| id.to_owned())
                    .collect();
                active.insert(key.clone(), tips);
            }
            if object_read(&repo, &["cat-file", "-t", &oid])
                .as_deref()
                .ok()
                != Some(b"commit\n")
            {
                bail!(
                    "{}: gitlink {:.10} is not a locally verifiable commit; fetch the child first",
                    path.display(),
                    oid
                )
            }
            let advertised = &active[&key];
            let proven = advertised.contains(&oid)
                || advertised.iter().any(|head| {
                    local_objects(&repo)
                        .args(["merge-base", "--is-ancestor", &oid, head])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|s| s.success())
                });
            if !proven {
                bail!("{}: gitlink {:.10} is not proven reachable from advertised child branches/tags. Fetch the child, publish its commit, then retry the parent.",path.display(),oid)
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Module {
    path: PathBuf,
    url: String,
}
type Sources = BTreeMap<PathBuf, String>;

fn module_config(root: &Path, blob: &str) -> Result<Sources> {
    let bytes = object_read(
        root,
        &[
            "config",
            "--null",
            "--no-includes",
            "--blob",
            blob,
            "--list",
        ],
    )
    .map_err(|_| anyhow!("invalid .gitmodules for an outgoing commit"))?;
    let mut groups = BTreeMap::<Vec<u8>, Module>::new();
    for kv in bytes.split(|&c| c == 0).filter(|v| !v.is_empty()) {
        let mut parts = kv.splitn(2, |&b| b == b'\n');
        let Some(key) = parts.next().unwrap().strip_prefix(b"submodule.") else {
            continue;
        };
        let val = parts.next().unwrap_or_default();
        if let Some(n) = key.strip_suffix(b".path") {
            groups.entry(n.to_vec()).or_default().path = path_from(val)?;
        }
        if let Some(n) = key.strip_suffix(b".url") {
            groups.entry(n.to_vec()).or_default().url = std::str::from_utf8(val)?.into();
        }
    }
    let mut sources = Sources::new();
    for m in groups
        .into_values()
        .filter(|m| !m.path.as_os_str().is_empty())
    {
        if sources.get(&m.path).is_some_and(|url| url != &m.url) {
            bail!("{}: ambiguous submodule source", m.path.display());
        }
        sources.insert(m.path, m.url);
    }
    Ok(sources)
}
fn path_from(raw: &[u8]) -> Result<PathBuf> {
    #[cfg(unix)]
    let p = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(raw.into()))
    };
    #[cfg(not(unix))]
    let p = PathBuf::from(std::str::from_utf8(raw)?);
    if p.as_os_str().is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("invalid submodule path in outgoing tree")
    }
    Ok(p)
}
#[derive(Default)]
struct LinkChanges {
    links: BTreeMap<PathBuf, String>,
    modules: Option<(Option<String>, Option<String>)>,
}
type Changes = BTreeMap<String, LinkChanges>;

// A declaration can disappear together with its gitlink, or refer to an ordinary
// directory. Only query affected paths in this revision; never expand their trees.
fn collect_tree_links(
    root: &Path,
    rev: &str,
    paths: BTreeSet<&PathBuf>,
    links: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    let paths: Vec<_> = paths.into_iter().collect();
    for batch in paths.chunks(32) {
        let output = local_objects(root)
            .arg("--literal-pathspecs")
            .args(["ls-tree", "-z", "--full-tree", rev, "--"])
            .args(batch)
            .output()?;
        if !output.status.success() {
            bail!(
                "cannot inspect gitlinks for changed .gitmodules at {:.10}",
                rev
            );
        }
        for record in output.stdout.split(|b| *b == 0).filter(|v| !v.is_empty()) {
            let pos = record
                .iter()
                .position(|&b| b == b'\t')
                .context("invalid tree entry")?;
            if let Some(oid) = record[..pos].strip_prefix(b"160000 commit ") {
                links.insert(
                    path_from(&record[pos + 1..])?,
                    std::str::from_utf8(oid)?.into(),
                );
            }
        }
    }
    Ok(())
}

fn read_links(mut r: impl BufRead) -> Result<Changes> {
    let mut map = Changes::new();
    let mut rev = String::new();
    let mut token = Vec::new();
    let mut path = Vec::new();
    while {
        token.clear();
        r.read_until(0, &mut token)? != 0
    } {
        if token.last() == Some(&0) {
            token.pop();
        }
        let t = token.strip_prefix(b"\n").unwrap_or(&token);
        if t.is_empty() {
            continue;
        }
        if let Some(id) = t.strip_prefix(b"IGIT:") {
            rev = std::str::from_utf8(id)?.into();
            continue;
        }
        if !t.starts_with(b":") {
            bail!("cannot decode outgoing tree changes")
        }
        let fields: Vec<_> = std::str::from_utf8(t)?.split_ascii_whitespace().collect();
        if fields.len() != 5 {
            bail!("cannot decode outgoing tree record")
        }
        path.clear();
        if r.read_until(0, &mut path)? == 0 {
            bail!("missing outgoing path")
        }
        if path.last() == Some(&0) {
            path.pop();
        }
        if path == b".gitmodules" {
            if rev.len() < 10 {
                bail!("missing revision for changed .gitmodules")
            }
            map.entry(rev.clone()).or_default().modules = Some((
                (fields[0] != ":000000").then(|| fields[2].to_owned()),
                (fields[1] != "000000").then(|| fields[3].to_owned()),
            ));
        }
        if fields[1] == "160000" && (fields[0] != ":160000" || fields[2] != fields[3]) {
            if rev.len() < 10 || fields[3].len() < 10 {
                bail!("invalid gitlink object")
            }
            map.entry(rev.clone())
                .or_default()
                .links
                .insert(path_from(&path)?, fields[3].into());
        }
    }
    Ok(map)
}
fn resolve_url(child: &str, push_url: Option<&str>) -> Result<String> {
    if child.is_empty() || child.contains(['\r', '\n', '\0']) {
        bail!("missing/invalid submodule URL")
    }
    if !(child.starts_with("../") || child.starts_with("./")) {
        return Ok(child.into());
    }
    let mut parent = push_url
        .ok_or_else(|| anyhow!("relative submodule URL requires the actual parent push endpoint"))?
        .trim_end_matches('/')
        .to_owned();
    let mut rest = child;
    while let Some(r) = rest.strip_prefix("../") {
        rest = r;
        let cutoff = parent
            .find("://")
            .map(|n| {
                parent[n + 3..]
                    .find('/')
                    .map(|i| n + 3 + i)
                    .unwrap_or(parent.len())
            })
            .unwrap_or(0);
        if let Some(n) = parent.rfind('/').filter(|i| *i >= cutoff) {
            if n == 0 && parent.starts_with('/') {
                parent.truncate(1)
            } else {
                parent.truncate(n)
            }
        } else if cutoff == 0 && parent.contains(':') {
            let i = parent.find(':').unwrap();
            if parent.len() == i + 1 {
                bail!("submodule URL escapes remote root")
            }
            parent.truncate(i + 1);
        } else if !Path::new(&parent).is_absolute() && !parent.is_empty() {
            parent.clear()
        } else {
            bail!("cannot resolve relative submodule URL")
        }
    }
    while let Some(r) = rest.strip_prefix("./") {
        rest = r;
    }
    if parent.ends_with(':') || parent.ends_with('/') || parent.is_empty() {
        Ok(format!("{parent}{rest}"))
    } else {
        Ok(format!("{parent}/{rest}"))
    }
}
fn normalize(config: &Path, input: &str, cwd: &Path) -> Result<String> {
    // Expands insteadOf without network access. Only match literal remote IDs or
    // equivalent canonical filesystem paths, not lookalike network services.
    let url = git(config, &["ls-remote", "--get-url", "--", input])
        .map_err(|_| anyhow!("cannot normalize submodule location"))?;
    if let Some(p) = url.strip_prefix("file://") {
        if !p.starts_with('/') {
            return Ok(url);
        }
        return Ok(std::fs::canonicalize(p)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(url));
    }
    if url.contains("://") || url.contains(':') {
        return Ok(url.trim_end_matches('/').into());
    }
    let abs = cwd.join(&url);
    Ok(std::fs::canonicalize(&abs)
        .unwrap_or(abs)
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn g(p: &Path, args: &[&str]) -> String {
        git(p, args).unwrap()
    }
    fn repo(p: &Path) {
        std::fs::create_dir_all(p).unwrap();
        g(p, &["init", "-q", "-b", "main"]);
        ident(p);
    }
    fn ident(p: &Path) {
        for (k, v) in [
            ("user.name", "IGit fixture"),
            ("user.email", "igit@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            g(p, &["config", k, v]);
        }
    }
    fn advance(p: &Path, file: &str, body: &str) {
        std::fs::write(p.join(file), body).unwrap();
        g(p, &["add", "--", file]);
        g(p, &["commit", "-qm", body]);
    }
    #[test]
    fn source_only_change_checks_intermediate_commits_before_parent_push() {
        use crate::{
            core::Repo,
            push::{entry, job, push_one},
        };
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().canonicalize().unwrap();
        let root = dir.join("parent");
        let child = dir.join("child");
        let parent_remote = dir.join("parent.git");
        let new_remote = dir.join("new.git");
        repo(&root);
        repo(&child);
        advance(&child, "f", "shared");
        for bare in [&parent_remote, &new_remote] {
            g(
                &dir,
                &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()],
            );
        }
        let path = "deps/sdk [1]";
        g(
            &root,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                "--name",
                "sdk",
                child.to_str().unwrap(),
                path,
            ],
        );
        g(&root, &["commit", "-qm", "shared gitlink"]);
        g(
            &root,
            &["remote", "add", "origin", parent_remote.to_str().unwrap()],
        );
        g(&root, &["push", "-qu", "origin", "main"]);
        let base = g(&root, &["rev-parse", "HEAD"]);
        let sub = root.join(path);
        g(
            &sub,
            &["remote", "add", "mirror", new_remote.to_str().unwrap()],
        );
        let repo = Repo {
            path: root.clone(),
            name: "parent".into(),
            branch: "main".into(),
            submodule: false,
            parent: None,
        };
        let push = || push_one(&job(&repo, &entry(0, &repo, None))?, false, false);
        let declare = |source: &str, message: &str| {
            g(
                &root,
                &["config", "-f", ".gitmodules", "submodule.sdk.url", source],
            );
            g(&root, &["commit", "-qam", message]);
        };

        declare(
            "../new.git",
            "switch the source without changing the gitlink",
        );
        assert!(
            push().is_err(),
            "published a gitlink unavailable at the new source"
        );
        assert_eq!(g(&parent_remote, &["rev-parse", "main"]), base);
        declare(child.to_str().unwrap(), "restore the original source");
        assert!(
            push().is_err(),
            "the intermediate parent commit is also published"
        );
        assert_eq!(g(&parent_remote, &["rev-parse", "main"]), base);
        assert!(g(
            &root,
            &["ls-remote", "--heads", new_remote.to_str().unwrap()]
        )
        .is_empty());

        // The child is published explicitly; the parent must now accept that history.
        g(&sub, &["push", "-q", "mirror", "HEAD:refs/heads/main"]);
        declare("../new.git", "use the now-published source");
        push().unwrap();
        assert_eq!(
            g(&parent_remote, &["rev-parse", "main"]),
            g(&root, &["rev-parse", "HEAD"])
        );
        let fresh = dir.join("fresh");
        g(
            &dir,
            &[
                "-c",
                "protocol.file.allow=always",
                "clone",
                "-q",
                "--recurse-submodules",
                parent_remote.to_str().unwrap(),
                fresh.to_str().unwrap(),
            ],
        );
        assert_eq!(
            g(&fresh.join(path), &["rev-parse", "HEAD"]),
            g(&sub, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn metadata_renames_and_removed_declarations_only_affect_live_gitlinks() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().canonicalize().unwrap();
        repo(&root);
        advance(&root, "README", "seed");
        let oid = g(&root, &["rev-parse", "HEAD"]);
        g(
            &root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{oid},sdk"),
            ],
        );
        advance(
            &root,
            ".gitmodules",
            "[submodule \"old\"]\npath = sdk\nurl = ../sdk.git\n",
        );
        let base = g(&root, &["rev-parse", "HEAD"]);
        // The checkout isn't initialized. A section rename and non-source metadata
        // must not require inspecting/fetching an otherwise unchanged module.
        advance(
            &root,
            ".gitmodules",
            "[submodule \"renamed\"]\npath = sdk\nurl = ../sdk.git\nbranch = main\n",
        );
        let renamed = g(&root, &["rev-parse", "HEAD"]);
        verify(&root, &renamed, Some(&base)).unwrap();
        g(&root, &["rm", "-q", ".gitmodules"]);
        g(
            &root,
            &["commit", "-qm", "leave the gitlink without a source"],
        );
        assert!(verify(&root, &g(&root, &["rev-parse", "HEAD"]), Some(&renamed)).is_err());

        g(&root, &["checkout", "-qb", "remove-together", &renamed]);
        g(&root, &["rm", "-q", "--cached", "sdk"]);
        g(&root, &["rm", "-q", ".gitmodules"]);
        g(&root, &["commit", "-qm", "remove both gitlink and source"]);
        verify(&root, &g(&root, &["rev-parse", "HEAD"]), Some(&renamed)).unwrap();
    }

    #[test]
    fn unpublished_intermediate_child_and_absent_remote_stop_parent() {
        let t = tempfile::tempdir().unwrap();
        let child = t.path().join("child");
        let parent = t.path().join("parent");
        repo(&child);
        repo(&parent);
        advance(&child, "f", "shared");
        let initial = g(&child, &["rev-parse", "HEAD"]);
        g(
            &parent,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                child.to_str().unwrap(),
                "sdk",
            ],
        );
        g(&parent, &["commit", "-qm", "shared gitlink"]);
        let root = std::fs::canonicalize(&parent).unwrap();
        let sub = root.join("sdk");
        ident(&sub);
        let base = g(&root, &["rev-parse", "HEAD"]);
        g(&root, &["config", "diff.ignoreSubmodules", "all"]);
        verify(&root, &base, None).unwrap();
        g(&root, &["checkout", "-qb", "feature"]);
        advance(&sub, "f", "unpublished");
        let hidden = g(&sub, &["rev-parse", "HEAD"]);
        g(&root, &["add", "sdk"]);
        g(&root, &["commit", "-qm", "intermediate unpublished"]);
        g(&sub, &["checkout", "-q", &initial]);
        g(&root, &["add", "sdk"]);
        g(&root, &["commit", "-qm", "restore old gitlink"]);
        g(&root, &["checkout", "-q", "main"]);
        advance(&root, "independent", "another side");
        g(
            &root,
            &["merge", "--no-ff", "-qm", "merge feature", "feature"],
        );
        let tip = g(&root, &["rev-parse", "HEAD"]);
        assert!(verify(&root, &tip, Some(&base))
            .unwrap_err()
            .to_string()
            .contains("not proven"));
        g(&sub, &["remote", "remove", "origin"]);
        assert!(verify(&root, &tip, Some(&base))
            .unwrap_err()
            .to_string()
            .contains("no configured child"));
        g(&sub, &["remote", "add", "origin", child.to_str().unwrap()]);
        g(
            &sub,
            &[
                "push",
                "-q",
                "origin",
                &format!("{hidden}:refs/heads/published"),
            ],
        );
        verify(&root, &tip, Some(&base)).unwrap();
        // Advertise only a *descendant* of the gitlink: ancestry by known objects is valid.
        g(&sub, &["checkout", "-q", "main"]);
        advance(&sub, "f", "published descendant");
        g(&sub, &["push", "-q", "origin", "main:refs/heads/published"]);
        verify(&root, &tip, Some(&base)).unwrap();
        // Merely having a tracking ref in sdk is insufficient after deletion server-side.
        g(&child, &["update-ref", "-d", "refs/heads/published"]);
        assert!(verify(&root, &tip, Some(&base)).is_err());
        advance(&root, "note", "no changed gitlink");
        verify(&root, &g(&root, &["rev-parse", "HEAD"]), Some(&tip)).unwrap();
    }
    #[test]
    fn relative_declared_sources() {
        assert_eq!(
            resolve_url("../sdk.git", Some("git@example:org/parent.git")).unwrap(),
            "git@example:org/sdk.git"
        );
        assert_eq!(
            resolve_url("../sdk.git", Some("https://example/org/parent.git")).unwrap(),
            "https://example/org/sdk.git"
        );
        assert!(resolve_url("../sdk.git", None).is_err());
    }
}
