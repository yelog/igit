use super::{
    base_tree, branch_ref, change_idx, checked, cmd, diff, g, git_path, oid, raw,
    reset_index_paths, safe, status, txt, Change, Guard,
};
use anyhow::{bail, Context as _, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub enum Target {
    Files(BTreeSet<PathBuf>),
    Directory(PathBuf),
}

impl Target {
    pub fn contains(&self, path: &Path) -> bool {
        match self {
            Self::Files(paths) => paths.contains(path),
            Self::Directory(dir) => path.starts_with(dir),
        }
    }
}

#[derive(PartialEq, Eq)]
enum Working {
    Missing,
    File { hash: String, mode: u32 },
    Symlink(PathBuf),
}

pub struct Plan {
    pub root: usize,
    pub repo: PathBuf,
    pub files: Vec<Change>,
    target: Target,
    head: Option<String>,
    branch: String,
    base: String,
    stages: Vec<Vec<u8>>,
    working: Vec<Working>,
    restore: BTreeSet<PathBuf>,
}

fn working(repo: &Path, path: &Path) -> Result<Working> {
    // Never follow a directory symlink or enter another working tree. Leaf
    // symlinks are safe: Git replaces the link, and remove_file unlinks it.
    let mut parent = repo.to_path_buf();
    let parts: Vec<_> = path.components().collect();
    if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
        bail!("Invalid discard path")
    }
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(m) if m.is_dir() && !parent.join(".git").exists() => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => bail!(
                "Unsafe parent of {}; handle file/directory replacements externally",
                path.display()
            ),
        }
    }
    let file = repo.join(path);
    let meta = match fs::symlink_metadata(&file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Working::Missing),
        Err(e) => return Err(e.into()),
    };
    if meta.is_symlink() {
        return Ok(Working::Symlink(fs::read_link(file)?));
    }
    if !meta.is_file() {
        bail!("{} is a directory or special file; handle file/directory replacements and nested repositories externally", path.display())
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = u32::from(meta.permissions().readonly());
    let hash = txt(checked(
        cmd(repo, None)
            .args(["hash-object", "--no-filters", "--"])
            .arg(path)
            .output()?,
    )?);
    Ok(Working::File { hash, mode })
}

type Entries = BTreeMap<PathBuf, Vec<u8>>;

fn entries(repo: &Path, index: Option<&Path>) -> Result<Entries> {
    g(repo, index, &["ls-files", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .map(|r| {
            let tab = r
                .iter()
                .position(|b| *b == b'\t')
                .context("Invalid index entry")?;
            Ok((PathBuf::from(raw(&r[tab + 1..])), r[..tab].to_vec()))
        })
        .collect()
}

fn validate_layout(file: &Change, entries: &Entries) -> Result<()> {
    let path = &file.path;
    if entries.get(path).is_some_and(|record| {
        ![b"100644", b"100755", b"120000"]
            .iter()
            .any(|mode| record.starts_with(*mode))
    }) {
        bail!(
            "{} is a submodule or unsupported file mode; discard changes in its own repository",
            file.label()
        );
    }
    if path.ancestors().skip(1).any(|p| entries.contains_key(p))
        || entries
            .range(path.clone()..)
            .find(|(p, _)| *p != path)
            .is_some_and(|(p, _)| p.starts_with(path))
    {
        bail!(
            "{} has a file/directory replacement; handle it externally",
            file.label()
        );
    }
    Ok(())
}

fn selected(repo: &Path, target: &Target) -> Result<Vec<Change>> {
    Ok(status(repo)?
        .into_iter()
        .filter(|f| target.contains(&f.path))
        .collect())
}

impl Plan {
    fn validate_head(&self) -> Result<()> {
        safe(&self.repo)?;
        if oid(&self.repo)? != self.head || branch_ref(&self.repo)? != self.branch {
            bail!("Branch or HEAD changed. Press F5 and review discard again")
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        self.validate_head()?;
        if selected(&self.repo, &self.target)? != self.files
            || super::stages(&self.repo, &self.files)? != self.stages
        {
            bail!("Files or staged content changed. Press F5 and review discard again")
        }
        for (file, before) in self.files.iter().zip(&self.working) {
            if working(&self.repo, &file.path)? != *before {
                bail!(
                    "{} changed. Press F5 and review discard again",
                    file.label()
                )
            }
        }
        Ok(())
    }

    pub fn added_copy(&self, i: usize) -> bool {
        let file = &self.files[i];
        file.xy != *b"??"
            && !self.restore.contains(&file.path)
            && !matches!(self.working[i], Working::Missing)
    }

    // Keep the original snapshots for checked leaves. Narrowing a directory
    // review turns it into an exact path set; edits to unchecked files then
    // cannot invalidate an otherwise unchanged selected file.
    pub fn retain(mut self, paths: &BTreeSet<PathBuf>) -> Option<Self> {
        if paths.is_empty() {
            return None;
        }
        if self.files.iter().all(|file| paths.contains(&file.path)) {
            return Some(self);
        }
        let (index, working): (Vec<_>, Vec<_>) = self
            .files
            .into_iter()
            .zip(self.stages)
            .zip(self.working)
            .filter(|((file, _), _)| paths.contains(&file.path))
            .unzip();
        (self.files, self.stages) = index.into_iter().unzip();
        self.working = working;
        self.target = Target::Files(self.files.iter().map(|f| f.path.clone()).collect());
        (!self.files.is_empty()).then_some(self)
    }

    pub fn patch(&self, path: &Path) -> Result<String> {
        let (i, file) = self
            .files
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == path)
            .context("File is outside this rollback review")?;
        let validate = || -> Result<()> {
            self.validate_head()?;
            if !status(&self.repo)?.contains(file)
                || super::stages(&self.repo, std::slice::from_ref(file))?[0] != self.stages[i]
                || working(&self.repo, path)? != self.working[i]
            {
                bail!(
                    "{} changed. Close the dialog, refresh, and review again",
                    file.label()
                );
            }
            Ok(())
        };
        validate()?;
        let mut preview = String::new();
        if file.staged() {
            preview.push_str("Staged changes:\n");
            preview.push_str(&diff(&self.repo, file)?);
            preview.push('\n');
        }
        let mut work = file.clone();
        if file.xy[0] == b'D' && !matches!(self.working[i], Working::Missing) {
            // A staged deletion and its untracked replacement share one row.
            work.xy = *b"??";
        } else if file.xy != *b"??" {
            work.xy[0] = b' ';
        }
        let patch = diff(&self.repo, &work)?;
        if !patch.is_empty() {
            preview.push_str("Working-tree changes:\n");
            preview.push_str(&patch);
        }
        validate()?;
        let mut lines = preview.lines();
        let mut text = lines
            .by_ref()
            .take(500)
            .map(|l| {
                let mut line: String = l.chars().take(1800).collect();
                line.push('\n');
                line
            })
            .collect::<String>();
        if lines.next().is_some() {
            text.push_str("… Diff truncated\n");
        }
        Ok(text)
    }
}

pub fn review(
    root: usize,
    repo: &Path,
    target: Target,
) -> Result<(Plan, BTreeMap<PathBuf, String>)> {
    safe(repo)?;
    let files = selected(repo, &target)?;
    if files.is_empty() {
        bail!("No discardable changes. Select a changed file or directory and press F5")
    }
    let head = oid(repo)?;
    let branch = branch_ref(repo)?;
    let base = base_tree(repo, head.as_deref())?;
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("base");
    g(repo, Some(&index), &["read-tree", &base])?;
    let base_entries = entries(repo, Some(&index))?;
    let index_entries = entries(repo, None)?;
    let mut p = Plan {
        root,
        repo: repo.into(),
        restore: BTreeSet::new(),
        files: vec![],
        target,
        head,
        branch,
        base,
        stages: vec![],
        working: vec![],
    };
    let mut unavailable = BTreeMap::new();
    for file in files {
        let snapshot = (|| {
            if !file.selectable() {
                bail!("Resolve the conflict before rolling back this file");
            }
            validate_layout(&file, &base_entries)?;
            validate_layout(&file, &index_entries)?;
            working(repo, &file.path)
        })();
        let before = match snapshot {
            Ok(before) => before,
            Err(error) => {
                unavailable.insert(file.path, format!("{error:#}"));
                continue;
            }
        };
        if base_entries.contains_key(&file.path) {
            p.restore.insert(file.path.clone());
        }
        // Capture the same exact stage record as `stages` without launching Git
        // once per row. Revalidation still reads the checked leaves from Git.
        let mut stage = index_entries.get(&file.path).cloned().unwrap_or_default();
        if !stage.is_empty() {
            stage.push(b'\t');
            stage.extend(file.path.as_os_str().as_encoded_bytes());
            stage.push(0);
        }
        p.stages.push(stage);
        p.working.push(before);
        p.files.push(file);
    }
    if !unavailable.is_empty() || matches!(p.target, Target::Files(_)) {
        p.target = Target::Files(p.files.iter().map(|f| f.path.clone()).collect());
    }
    Ok((p, unavailable))
}

fn copy_index(repo: &Path, real: &Path, next: &Path) -> Result<()> {
    if real.exists() {
        fs::copy(real, next)?;
    } else {
        g(repo, Some(next), &["read-tree", "--empty"])?;
    }
    Ok(())
}

pub fn execute(p: &Plan, delete_added: bool) -> Result<()> {
    let repo = &p.repo;
    let index = git_path(repo, "index")?;
    let lock = index.with_file_name("index.lock");
    let mut lockfile = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock)
        .context("Cannot lock the Git index; wait for other operations to finish")?;
    let mut guard = Guard(lock.clone(), false);
    p.validate()?;
    let temp = tempfile::tempdir()?;
    let base = temp.path().join("base");
    g(repo, Some(&base), &["read-tree", &p.base])?;
    let next = temp.path().join("next");
    copy_index(repo, &index, &next)?;
    reset_index_paths(
        repo,
        &next,
        &p.base,
        p.files.iter().map(|f| f.path.as_path()),
    )?;
    // Finish all potentially scope-expanding preparation before touching files.
    p.validate()?;
    let mut completed = vec![];
    let result = p
        .files
        .iter()
        .zip(&p.working)
        .try_for_each(|(f, before)| -> Result<()> {
            if working(repo, &f.path)? != *before {
                bail!("{} changed during discard; review it again", f.label())
            }
            if p.restore.contains(&f.path) {
                change_idx(repo, &base, &["checkout-index", "--force"], &f.path)?;
            } else if (delete_added || f.xy == *b"??") && !matches!(before, Working::Missing) {
                fs::remove_file(repo.join(&f.path))
                    .with_context(|| format!("Cannot remove {}", f.label()))?;
            }
            completed.push(f.path.as_path());
            Ok(())
        });
    if completed.is_empty() {
        return result;
    }
    if result.is_err() {
        // Publish only completed paths after a partial filesystem failure.
        // Other selected files retain their real index versions for re-review.
        copy_index(repo, &index, &next)?;
        reset_index_paths(repo, &next, &p.base, completed.iter().copied())?;
    }
    if index.exists() {
        lockfile.set_permissions(fs::metadata(&index)?.permissions())?;
    }
    lockfile.write_all(&fs::read(&next)?)
        .context("Working-tree changes discarded, but writing the index failed; inspect files and index before retrying")?;
    lockfile.sync_all()?;
    drop(lockfile);
    guard.1 = true;
    fs::rename(&lock, &index)
        .context("Working-tree changes discarded, but publishing the index failed. Keep index.lock and inspect before retrying")?;
    result.with_context(|| {
        format!(
            "Discarded {} of {} files; refresh and review the remaining changes",
            completed.len(),
            p.files.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::backend::tests::{commit_all, get, put, setup};

    fn plan(root: usize, repo: &Path, target: Target) -> Result<Plan> {
        let (plan, unavailable) = super::review(root, repo, target)?;
        if let Some(reason) = unavailable.into_values().next() {
            bail!("{reason}");
        }
        plan.validate()?;
        Ok(plan)
    }

    fn review(repo: &Path, names: &[&str]) -> Plan {
        plan(
            0,
            repo,
            Target::Files(names.iter().map(PathBuf::from).collect()),
        )
        .unwrap()
    }

    #[test]
    fn discard_restores_both_versions_and_preserves_unrelated_staging() {
        let dir = setup();
        let repo = dir.path();
        put(repo, "mixed", "base\n");
        put(repo, "other", "base\n");
        commit_all(repo);
        let head = get(repo, &["rev-parse", "HEAD"]);
        put(repo, "mixed", "staged\n");
        put(repo, "other", "other staged\n");
        g(repo, None, &["add", "."]).unwrap();
        put(repo, "mixed", "working\n");
        let index = get(repo, &["ls-files", "--stage"]);
        let p = review(repo, &["mixed"]);
        let patch = p.patch(Path::new("mixed")).unwrap();
        assert!(patch.contains("+staged") && patch.contains("+working"));
        assert_eq!(get(repo, &["ls-files", "--stage"]), index);
        assert_eq!(fs::read(repo.join("mixed")).unwrap(), b"working\n");
        execute(&p, true).unwrap();
        assert_eq!(get(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(get(repo, &["show", ":mixed"]), "base");
        assert_eq!(fs::read(repo.join("mixed")).unwrap(), b"base\n");
        assert_eq!(get(repo, &["show", ":other"]), "other staged");
        assert_eq!(get(repo, &["status", "--porcelain"]), "M  other");
    }

    #[test]
    fn discard_directory_includes_new_deleted_and_untracked_but_keeps_ignored_files() {
        let dir = setup();
        let repo = dir.path();
        fs::create_dir(repo.join("src")).unwrap();
        put(repo, "src/deleted", "base");
        put(repo, "src/modified", "base");
        put(repo, ".gitignore", "src/cache/\n");
        commit_all(repo);
        fs::remove_file(repo.join("src/deleted")).unwrap();
        put(repo, "src/modified", "changed");
        put(repo, "src/added", "added staged");
        g(repo, None, &["add", "-A"]).unwrap();
        put(repo, "src/added", "added working");
        put(repo, "src/untracked", "new");
        fs::create_dir(repo.join("src/cache")).unwrap();
        put(repo, "src/cache/keep", "ignored");
        let p = plan(0, repo, Target::Directory("src".into())).unwrap();
        assert_eq!(p.files.len(), 4);
        execute(&p, true).unwrap();
        assert!(status(repo).unwrap().is_empty());
        assert_eq!(fs::read(repo.join("src/deleted")).unwrap(), b"base");
        assert_eq!(fs::read(repo.join("src/modified")).unwrap(), b"base");
        assert!(!repo.join("src/added").exists());
        assert!(!repo.join("src/untracked").exists());
        assert_eq!(fs::read(repo.join("src/cache/keep")).unwrap(), b"ignored");
    }

    #[test]
    fn discard_initial_additions_does_not_create_a_commit() {
        let dir = setup();
        let repo = dir.path();
        put(repo, "added", "staged");
        g(repo, None, &["add", "added"]).unwrap();
        put(repo, "added", "working");
        put(repo, "new", "untracked");
        execute(&review(repo, &["added", "new"]), true).unwrap();
        assert!(oid(repo).unwrap().is_none());
        assert!(status(repo).unwrap().is_empty());
        assert!(!repo.join("added").exists() && !repo.join("new").exists());
    }

    #[test]
    fn discard_staged_deletion_removes_its_untracked_replacement() {
        let dir = setup();
        let repo = dir.path();
        put(repo, "file", "base");
        commit_all(repo);
        g(repo, None, &["rm", "file"]).unwrap();
        put(repo, "file", "replacement");
        let p = review(repo, &["file"]);
        assert!(p.patch(Path::new("file")).unwrap().contains("replacement"));
        execute(&p, true).unwrap();
        assert_eq!(fs::read(repo.join("file")).unwrap(), b"base");
        assert!(status(repo).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn discard_uses_native_literal_paths_and_unlinks_symlinks_without_following_them() {
        use std::os::unix::fs::symlink;
        let dir = setup();
        let repo = dir.path();
        let name = PathBuf::from("src/[literal]*?/ｕｉ/file\nname");
        fs::create_dir_all(repo.join(&name).parent().unwrap()).unwrap();
        fs::write(repo.join(&name), "base").unwrap();
        symlink("target", repo.join("link")).unwrap();
        commit_all(repo);
        fs::write(repo.join(&name), "changed").unwrap();
        fs::write(repo.join("target"), "keep target").unwrap();
        fs::remove_file(repo.join("link")).unwrap();
        symlink("elsewhere", repo.join("link")).unwrap();
        symlink("target", repo.join("untracked-link")).unwrap();
        let mut p = review(repo, &["link", "untracked-link"]);
        execute(&p, true).unwrap();
        assert_eq!(
            fs::read_link(repo.join("link")).unwrap(),
            Path::new("target")
        );
        assert!(fs::symlink_metadata(repo.join("untracked-link")).is_err());
        assert_eq!(fs::read(repo.join("target")).unwrap(), b"keep target");
        p = plan(0, repo, Target::Files([name.clone()].into())).unwrap();
        execute(&p, true).unwrap();
        assert_eq!(fs::read(repo.join(name)).unwrap(), b"base");
    }

    #[test]
    fn discard_refuses_ignored_conflicted_submodule_and_directory_replacement_paths() {
        let dir = setup();
        let repo = dir.path();
        put(repo, "file", "base");
        put(repo, ".gitignore", "cache/\n");
        commit_all(repo);
        let head = get(repo, &["rev-parse", "HEAD"]);
        g(
            repo,
            None,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{head},sdk"),
            ],
        )
        .unwrap();
        fs::create_dir(repo.join("cache")).unwrap();
        put(repo, "cache/private", "ignored");
        fs::remove_file(repo.join("file")).unwrap();
        fs::create_dir(repo.join("file")).unwrap();
        put(repo, "file/child", "keep child");
        for name in ["file", "cache/private", "sdk"] {
            assert!(
                plan(0, repo, Target::Files([name.into()].into())).is_err(),
                "{name}"
            );
        }
        assert_eq!(fs::read(repo.join("file/child")).unwrap(), b"keep child");
        assert_eq!(fs::read(repo.join("cache/private")).unwrap(), b"ignored");
        // A selected file must not silently bring back HEAD descendants.
        fs::remove_file(repo.join("file/child")).unwrap();
        fs::remove_dir(repo.join("file")).unwrap();
        put(repo, "file", "base");
        g(repo, None, &["reset", "-q", "HEAD"]).unwrap();
        let blob = get(repo, &["rev-parse", "HEAD:file"]);
        let conflict =
            format!("100644 {blob} 1\tfile\n100644 {blob} 2\tfile\n100644 {blob} 3\tfile\n");
        checked(
            super::super::run(
                repo,
                None,
                &["update-index".as_ref(), "--index-info".as_ref()],
                Some(conflict.as_bytes()),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(plan(0, repo, Target::Files(["file".into()].into())).is_err());
    }

    #[test]
    fn discard_refuses_file_over_head_directory_and_new_nested_repositories() {
        let dir = setup();
        let repo = dir.path();
        fs::create_dir(repo.join("folder")).unwrap();
        put(repo, "folder/child", "base");
        commit_all(repo);
        g(repo, None, &["rm", "folder/child"]).unwrap();
        put(repo, "folder", "replacement");
        g(repo, None, &["add", "folder"]).unwrap();
        let before = get(repo, &["ls-files", "--stage"]);
        assert!(plan(0, repo, Target::Files(["folder".into()].into())).is_err());
        assert_eq!(get(repo, &["ls-files", "--stage"]), before);
        assert_eq!(fs::read(repo.join("folder")).unwrap(), b"replacement");
        fs::create_dir(repo.join("nested")).unwrap();
        g(&repo.join("nested"), None, &["init", "-q"]).unwrap();
        put(repo, "nested/keep", "keep nested");
        assert!(plan(0, repo, Target::Directory("nested".into())).is_err());
        assert_eq!(fs::read(repo.join("nested/keep")).unwrap(), b"keep nested");
    }

    #[cfg(unix)]
    #[test]
    fn discard_refuses_a_parent_replaced_with_a_symlink_after_review() {
        use std::os::unix::fs::symlink;
        let dir = setup();
        let repo = dir.path();
        fs::create_dir(repo.join("src")).unwrap();
        put(repo, "src/file", "base");
        commit_all(repo);
        put(repo, "src/file", "reviewed");
        let p = review(repo, &["src/file"]);
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("file"), "outside").unwrap();
        fs::rename(repo.join("src"), repo.join("old-src")).unwrap();
        symlink(outside.path(), repo.join("src")).unwrap();
        assert!(execute(&p, true).is_err());
        assert_eq!(fs::read(outside.path().join("file")).unwrap(), b"outside");
        assert_eq!(fs::read(repo.join("old-src/file")).unwrap(), b"reviewed");
    }

    #[cfg(unix)]
    #[test]
    fn discard_partial_failure_publishes_only_completed_index_entries() {
        let dir = setup();
        let repo = dir.path();
        put(repo, ".gitattributes", "a filter=mutate\n");
        put(repo, "a", "base");
        put(repo, "b", "base");
        commit_all(repo);
        g(repo, None, &["config", "filter.mutate.clean", "cat"]).unwrap();
        g(
            repo,
            None,
            &["config", "filter.mutate.smudge", "printf external > b; cat"],
        )
        .unwrap();
        put(repo, "a", "a staged");
        put(repo, "b", "b staged");
        g(repo, None, &["add", "a", "b"]).unwrap();
        let p = review(repo, &["a", "b"]);
        let error = execute(&p, true).unwrap_err();
        assert!(
            format!("{error:#}").contains("Discarded 1 of 2 files"),
            "{error:#}"
        );
        assert_eq!(get(repo, &["show", ":a"]), "base");
        assert_eq!(get(repo, &["show", ":b"]), "b staged");
        assert_eq!(fs::read(repo.join("a")).unwrap(), b"base");
        assert_eq!(fs::read(repo.join("b")).unwrap(), b"external");
        assert!(!repo.join(".git/index.lock").exists());
    }

    #[test]
    fn discard_refuses_stale_content_staging_head_branch_and_directory_membership() {
        for mutation in [
            "worktree",
            "index",
            "staged blob",
            "head",
            "branch",
            "new file",
            "lock",
        ] {
            let dir = setup();
            let repo = dir.path();
            fs::create_dir(repo.join("src")).unwrap();
            put(repo, "src/file", "base");
            commit_all(repo);
            put(repo, "src/file", "reviewed");
            if mutation == "staged blob" {
                g(repo, None, &["add", "src/file"]).unwrap();
                put(repo, "src/file", "working");
            }
            let p = plan(0, repo, Target::Directory("src".into())).unwrap();
            match mutation {
                "worktree" => put(repo, "src/file", "external"),
                "index" => {
                    g(repo, None, &["add", "src/file"]).unwrap();
                }
                "staged blob" => {
                    put(repo, "src/file", "other staged version");
                    g(repo, None, &["add", "src/file"]).unwrap();
                    put(repo, "src/file", "working");
                }
                "head" => {
                    g(repo, None, &["commit", "--allow-empty", "-qm", "external"]).unwrap();
                }
                "branch" => {
                    g(repo, None, &["switch", "-qc", "external"]).unwrap();
                }
                "new file" => put(repo, "src/new", "external"),
                "lock" => put(repo, ".git/index.lock", "external lock"),
                _ => unreachable!(),
            }
            let index = get(repo, &["ls-files", "--stage"]);
            let bytes = fs::read(repo.join("src/file")).unwrap();
            assert!(execute(&p, true).is_err(), "{mutation}");
            assert_eq!(get(repo, &["ls-files", "--stage"]), index, "{mutation}");
            assert_eq!(
                fs::read(repo.join("src/file")).unwrap(),
                bytes,
                "{mutation}"
            );
            if mutation == "lock" {
                assert_eq!(
                    fs::read(repo.join(".git/index.lock")).unwrap(),
                    b"external lock"
                );
            } else {
                assert!(!repo.join(".git/index.lock").exists());
            }
        }
    }
}
