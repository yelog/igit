use super::{PermanentLayout, Rev};
use crate::{core::Context, log::model::all_tips};
use anyhow::{bail, Context as _, Result};
use std::path::Path;

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
            match metadata(&repo.path, root) {
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
    let output = crate::core::command(path)
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
        .args(all_tips())
        .arg("--")
        .output()
        .context("git log")?;
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
    use crate::log::tests::{commit, git, init};

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
