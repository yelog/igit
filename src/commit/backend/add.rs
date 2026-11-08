//! Stage captured untracked leaves without changing other index entries.
use super::working::{read_many as working, Working};
use super::*;

pub struct Plan {
    pub repo: PathBuf,
    pub files: Vec<Change>,
    head: Option<String>,
    branch: String,
    base: String,
    tree: String,
    working: Vec<Working>,
}

impl Plan {
    fn validate(&self) -> Result<()> {
        safe(&self.repo)?;
        if oid(&self.repo)? != self.head || branch_ref(&self.repo)? != self.branch {
            bail!("Branch or HEAD changed. Refresh and retry Add to Git");
        }
        let live: BTreeMap<_, _> = status(&self.repo)?
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect();
        let paths: Vec<_> = self.files.iter().map(|file| file.path.as_path()).collect();
        let current = working(&self.repo, &paths)?;
        for ((file, before), current) in self.files.iter().zip(&self.working).zip(current) {
            if live.get(&file.path) != Some(file) || current != *before {
                bail!("{} changed. Refresh and retry Add to Git", file.label());
            }
        }
        Ok(())
    }
}

pub fn review(repo: &Path, files: Vec<Change>) -> Result<Plan> {
    safe(repo)?;
    if files.is_empty()
        || files
            .iter()
            .any(|file| file.xy != *b"??" || !file.selectable())
    {
        bail!("Add to Git requires untracked files");
    }
    let head = oid(repo)?;
    let branch = branch_ref(repo)?;
    let base = base_tree(repo, head.as_deref())?;
    let paths: Vec<_> = files.iter().map(|file| file.path.as_path()).collect();
    let fingerprints = working(repo, &paths)?;
    for (file, before) in files.iter().zip(&fingerprints) {
        if matches!(before, Working::Missing) {
            bail!("{} disappeared. Refresh and retry Add to Git", file.label());
        }
    }
    let index = git_path(repo, "index")?;
    let temp = tempfile::Builder::new()
        .prefix("igit-add-")
        .tempdir_in(index.parent().context("missing Git directory")?)?;
    let mut plan = Plan {
        repo: repo.into(),
        files,
        head,
        branch,
        base,
        tree: String::new(),
        working: fingerprints,
    };
    plan.validate()?;
    plan.tree = snapshot_untracked(repo, &plan.files, &plan.base, &temp)?;
    plan.validate()?;
    Ok(plan)
}

pub fn execute(plan: &Plan) -> Result<()> {
    let repo = &plan.repo;
    let index = git_path(repo, "index")?;
    let lock = index.with_file_name("index.lock");
    let mut lockfile = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock)
        .context("Cannot lock the Git index; wait for other operations to finish")?;
    let mut guard = Guard(lock.clone(), false);
    plan.validate()?;
    let temp = tempfile::Builder::new()
        .prefix("igit-add-")
        .tempdir_in(index.parent().context("missing Git directory")?)?;
    let tree = snapshot_untracked(repo, &plan.files, &plan.base, &temp)?;
    if tree != plan.tree {
        bail!("Selected content or Git conversion changed. Refresh and retry Add to Git");
    }
    // Start with the current locked index so unrelated staging survives,
    // including changes made after the snapshot was captured.
    let next = temp.path().join("next");
    if index.exists() {
        fs::copy(&index, &next)?;
    } else {
        g(repo, Some(&next), &["read-tree", "--empty"])?;
    }
    reset_index_paths(
        repo,
        &next,
        &tree,
        plan.files.iter().map(|file| file.path.as_path()),
    )?;
    plan.validate()?;
    if index.exists() {
        lockfile.set_permissions(fs::metadata(&index)?.permissions())?;
    }
    lockfile.write_all(&fs::read(next)?)?;
    lockfile.sync_all()?;
    drop(lockfile);
    fs::rename(&lock, &index).context("Cannot publish the updated Git index")?;
    guard.1 = true;
    Ok(())
}

fn snapshot_untracked(repo: &Path, files: &[Change], base: &str, temp: &TempDir) -> Result<String> {
    let index = temp.path().join("selected");
    g(repo, Some(&index), &["read-tree", base])?;
    let paths: Vec<_> = files
        .iter()
        .flat_map(|file| {
            file.path
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .copied()
                .chain(std::iter::once(0))
        })
        .collect();
    checked(run(
        repo,
        Some(&index),
        &[
            OsStr::new("add"),
            OsStr::new("-A"),
            OsStr::new("--pathspec-from-file=-"),
            OsStr::new("--pathspec-file-nul"),
        ],
        Some(&paths),
    )?)?;
    let tree = txt(g(repo, Some(&index), &["write-tree"])?);
    check_tree_scope(repo, base, &tree, files)?;
    Ok(tree)
}

#[cfg(test)]
mod tests;
