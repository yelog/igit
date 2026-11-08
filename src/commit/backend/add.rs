//! Stage reviewed untracked leaves without changing other index entries.
use super::working::{read as working, Working};
use super::*;

pub struct Plan {
    pub root: usize,
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
            bail!("Branch or HEAD changed. Refresh and review Add to Git again");
        }
        let live: BTreeMap<_, _> = status(&self.repo)?
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect();
        for (file, before) in self.files.iter().zip(&self.working) {
            if live.get(&file.path) != Some(file) || working(&self.repo, &file.path)? != *before {
                bail!(
                    "{} changed. Refresh and review Add to Git again",
                    file.label()
                );
            }
        }
        Ok(())
    }
}

pub fn review(root: usize, repo: &Path, files: Vec<Change>) -> Result<Plan> {
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
    let mut fingerprints = Vec::with_capacity(files.len());
    for file in &files {
        let before = working(repo, &file.path)?;
        if matches!(before, Working::Missing) {
            bail!("{} disappeared. Refresh and review again", file.label());
        }
        fingerprints.push(before);
    }
    let index = git_path(repo, "index")?;
    let temp = tempfile::Builder::new()
        .prefix("igit-add-")
        .tempdir_in(index.parent().context("missing Git directory")?)?;
    let mut plan = Plan {
        root,
        repo: repo.into(),
        files,
        head,
        branch,
        base,
        tree: String::new(),
        working: fingerprints,
    };
    plan.validate()?;
    plan.tree = snapshot(repo, &plan.files, &plan.base, &temp)?.1;
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
    let (_, tree) = snapshot(repo, &plan.files, &plan.base, &temp)?;
    if tree != plan.tree {
        bail!("Selected content or Git conversion changed. Review Add to Git again");
    }
    // Start with the current locked index so unrelated staging survives,
    // including changes made after the review was opened.
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

#[cfg(test)]
mod tests;
