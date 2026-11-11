use super::*;

pub(super) fn execute(repo: &Path, head: &Head, parent: Option<&str>, kind: Kind) -> Result<()> {
    let tree = if kind == Kind::Drop {
        Some(match parent {
            Some(parent) => parent.to_owned(),
            None => git_input(repo, &["hash-object", "-t", "tree", "-w", "--stdin"], b"")?,
        })
    } else {
        None
    };
    let transaction = transaction::Transaction::prepare(
        repo,
        head,
        parent,
        if kind == Kind::Drop {
            "igit: drop commit"
        } else {
            "igit: undo commit"
        },
    )?;
    ensure_idle(repo)?;
    ensure_unpublished(repo, &head.oid)?;
    if let Some(tree) = &tree {
        clean(repo)?;
        protect_paths(repo, &head.oid, true)?;
        read_tree(repo, &head.oid, tree)?;
    }
    let result = transaction.commit();
    if let (Err(error), Some(tree)) = (&result, tree) {
        ensure!(
            Head::read(repo)? == *head,
            "Ref update failed and HEAD changed; inspect the branch, index and working tree before retrying: {error:#}"
        );
        read_tree(repo, &tree, &head.oid).with_context(|| {
            format!("Could not remove the commit: {error:#}. Restoring its files also failed; inspect the index and working tree before retrying")
        })?;
    }
    result
}

fn read_tree(repo: &Path, old: &str, new: &str) -> Result<()> {
    git(
        repo,
        &[
            "-c",
            "submodule.recurse=false",
            "read-tree",
            "-m",
            "-u",
            old,
            new,
        ],
    )?;
    Ok(())
}
