use super::{PermanentLayout, Rev};
use crate::{
    core::{read_output, Context},
    log::model::all_tips,
};
use anyhow::{bail, Context as _, Result};
use std::{
    collections::{hash_map::DefaultHasher, VecDeque},
    ffi::OsString,
    fs::File,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const TOPOLOGY_LIMIT: usize = 8192;
const CACHE_LIMIT: usize = 8;

struct Cached {
    path: PathBuf,
    identity: Vec<u8>,
    rows: Vec<Rev>,
}

fn cached_metadata(path: &Path, root: usize) -> Result<Vec<Rev>> {
    static CACHE: OnceLock<Mutex<VecDeque<Cached>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let path = std::fs::canonicalize(path)?;
    let mut identity = crate::core::git_bytes(&path, &["rev-parse", "--absolute-git-dir"])?;
    identity.extend(crate::core::git_bytes(
        &path,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
            "refs/replace",
        ],
    )?);
    let special = crate::core::git_bytes(
        &path,
        &[
            "rev-parse",
            "--git-path",
            "shallow",
            "--git-path",
            "info/grafts",
        ],
    )?;
    for value in special
        .split(|byte| *byte == b'\n')
        .filter(|value| !value.is_empty())
    {
        let value = git_path_value(value)?;
        let file = if value.is_absolute() {
            value
        } else {
            path.join(value)
        };
        append_file_identity(&mut identity, &file)?;
    }
    let head = read_output(
        &path,
        "graph HEAD",
        crate::core::command(&path).args(["rev-parse", "--verify", "HEAD"]),
    )?;
    if head.status.success() {
        identity.extend(head.stdout);
    }
    let mut rows = {
        let mut entries = cache.lock().unwrap_or_else(|error| error.into_inner());
        let hit = entries
            .iter()
            .position(|entry| entry.path == path && entry.identity == identity);
        hit.map(|index| {
            let entry = entries.remove(index).unwrap();
            let rows = entry.rows.clone();
            entries.push_front(entry);
            rows
        })
    };
    if rows.is_none() {
        let loaded = metadata(&path, 0)?;
        let mut entries = cache.lock().unwrap_or_else(|error| error.into_inner());
        entries.retain(|entry| entry.path != path);
        entries.push_front(Cached {
            path,
            identity,
            rows: loaded.clone(),
        });
        entries.truncate(CACHE_LIMIT);
        rows = Some(loaded);
    }
    let mut rows = rows.unwrap();
    for row in &mut rows {
        row.root = root;
    }
    Ok(rows)
}

fn git_path_value(value: &[u8]) -> Result<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(value.to_vec()).into())
    }
    #[cfg(windows)]
    {
        Ok(String::from_utf8(value.to_vec())?.into())
    }
}

fn append_file_identity(identity: &mut Vec<u8>, path: &Path) -> Result<()> {
    identity.extend((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
    identity.extend(path.as_os_str().as_encoded_bytes());
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            identity.push(0);
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    identity.push(1);
    let mut hasher = DefaultHasher::new();
    let mut length = 0_u64;
    let mut buffer = [0; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length = length.saturating_add(count as u64);
        buffer[..count].hash(&mut hasher);
    }
    identity.extend(length.to_le_bytes());
    identity.extend(hasher.finish().to_le_bytes());
    Ok(())
}

impl PermanentLayout {
    pub(in crate::log) fn load(
        context: &Context,
        cancelled: &impl Fn() -> bool,
    ) -> (Self, Vec<(usize, String)>) {
        let mut commits = Vec::new();
        let mut errors = Vec::new();
        for &root in &context.scope {
            if cancelled() {
                return (Self::default(), errors);
            }
            let Some(repo) = context.repos.get(root) else {
                continue;
            };
            match cached_metadata(&repo.path, root) {
                Ok(rows) => commits.extend(rows),
                Err(error) => errors.push((root, format!("{}: graph layout: {error}", repo.name))),
            }
        }
        if cancelled() {
            return (Self::default(), errors);
        }
        (Self::new(&commits), errors)
    }
}

// IDEA's DFS fragment colors depend on older ancestors too. Read only object
// IDs, parents and refs for layout; paged history still owns messages/authors.
fn metadata(path: &Path, root: usize) -> Result<Vec<Rev>> {
    let output = read_output(
        path,
        "log",
        crate::core::command(path)
            .args([
                "log",
                "--date-order",
                "--no-show-signature",
                "--no-notes",
                "--no-color",
                "--decorate=full",
                "--decorate-refs=HEAD",
                "--decorate-refs=refs/heads/*",
                "--decorate-refs=refs/remotes/*",
                "--decorate-refs=refs/tags/*",
                "--decorate-refs-exclude=refs/remotes/*/HEAD",
                "--ignore-missing",
                "-z",
                "--format=%H%x00%P%x00%D",
            ])
            .arg(format!("--max-count={TOPOLOGY_LIMIT}"))
            .args(all_tips())
            .arg("--"),
    )?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let data = String::from_utf8(output.stdout).context("Git graph metadata is not UTF-8")?;
    Ok(data
        .split_terminator('\0')
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .map(|fields| Rev {
            root,
            oid: fields[0].into(),
            parents: fields[1].split_whitespace().map(str::to_owned).collect(),
            refs: fields[2].into(),
            ..Default::default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::process::{with_read_control, ReadControl};
    use crate::log::tests::{commit, git, init};

    #[test]
    fn cached_graph_revalidates_refs_and_remaps_repository_identity() {
        let directory = init();
        commit(directory.path(), "file", "first");
        commit(directory.path(), "file", "second");
        let replaced = git(directory.path(), &["rev-parse", "HEAD"]);
        let first = cached_metadata(directory.path(), 4).unwrap();
        let reused = cached_metadata(directory.path(), 9).unwrap();
        assert_eq!(first.len(), reused.len());
        assert!(reused.iter().all(|row| row.root == 9));
        let tree = git(directory.path(), &["rev-parse", "HEAD^{tree}"]);
        let replacement = git(
            directory.path(),
            &["commit-tree", &tree, "-m", "replacement"],
        );
        git(directory.path(), &["replace", &replaced, &replacement]);
        let replaced_graph = cached_metadata(directory.path(), 4).unwrap();
        assert_ne!(
            replaced_graph
                .iter()
                .find(|row| row.oid == replaced)
                .unwrap()
                .parents,
            first
                .iter()
                .find(|row| row.oid == replaced)
                .unwrap()
                .parents
        );
        commit(directory.path(), "file", "third");
        let changed = cached_metadata(directory.path(), 4).unwrap();
        assert_eq!(changed.len(), replaced_graph.len() + 1);
        assert_ne!(changed[0].oid, replaced_graph[0].oid);
    }

    #[test]
    fn metadata_propagates_cancellation_instead_of_empty_layout() {
        let directory = init();
        let control = ReadControl::default();
        control.cancel();
        let error = with_read_control(&control, || metadata(directory.path(), 0)).unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error:#}");
    }

    #[test]
    fn metadata_includes_older_parents_without_messages_or_worktree_writes() {
        let dir = init();
        let path = dir.path();
        commit(path, "file", "base");
        let base = git(path, &["rev-parse", "HEAD"]);
        commit(path, "file", "middle");
        commit(path, "file", "tip");
        git(path, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        std::fs::write(path.join("file"), "unstaged").unwrap();
        let before = git(path, &["status", "--porcelain"]);
        let rows = metadata(path, 7).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.last().unwrap().oid, base);
        assert!(rows
            .iter()
            .all(|r| r.root == 7 && r.subject.is_empty() && r.author.is_empty()));
        assert!(rows[0].refs.contains("refs/remotes/origin/main"));
        assert_eq!(rows[0].parents, [rows[1].oid.clone()]);
        assert_eq!(rows[1].parents, [base]);
        assert_eq!(git(path, &["status", "--porcelain"]), before);
        assert_eq!(
            std::fs::read_to_string(path.join("file")).unwrap(),
            "unstaged"
        );
    }
}
