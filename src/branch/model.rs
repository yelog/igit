use crate::core::{git, Repo};
use anyhow::Result;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Branch {
    pub reference: String,
    pub name: String,
    pub oid: String,
    pub upstream: String,
    pub worktree: String,
    pub current: bool,
    pub remote: bool,
    pub gone: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub head: Option<String>,
    pub oid: Option<String>,
    pub branches: BTreeMap<String, Branch>,
    pub remotes: Vec<String>,
}

impl Snapshot {
    pub fn read(repo: &Repo) -> Result<Self> {
        let data = git(
            &repo.path,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)%00%(worktreepath)%00",
                "refs/heads/",
                "refs/remotes/",
            ],
        )?;
        let head = git(&repo.path, &["symbolic-ref", "-q", "HEAD"]).ok();
        let oid = git(&repo.path, &["rev-parse", "--verify", "HEAD"]).ok();
        let mut branches = BTreeMap::new();
        // NUL fields preserve Unicode and even newlines in worktree paths.
        let fields: Vec<_> = data.split('\0').collect();
        for fields in fields.chunks_exact(5) {
            let reference = fields[0].trim_start_matches('\n');
            if !fields[3].is_empty() {
                continue; // origin/HEAD is an alias, not a branch.
            }
            let remote = reference.starts_with("refs/remotes/");
            let name = short_ref(reference);
            branches.insert(
                reference.into(),
                Branch {
                    reference: reference.into(),
                    name: name.into(),
                    oid: fields[1].into(),
                    upstream: fields[2].into(),
                    worktree: fields[4].into(),
                    current: head.as_deref() == Some(reference),
                    remote,
                    gone: false,
                },
            );
        }
        let refs: std::collections::BTreeSet<_> = branches.keys().cloned().collect();
        for branch in branches.values_mut() {
            branch.gone = !branch.upstream.is_empty() && !refs.contains(&branch.upstream);
        }
        // An unborn branch has a symbolic HEAD but no ref yet.
        if let Some(reference) = head.as_ref().filter(|r| !branches.contains_key(*r)) {
            branches.insert(
                reference.clone(),
                Branch {
                    reference: reference.clone(),
                    name: short_ref(reference).into(),
                    oid: String::new(),
                    upstream: String::new(),
                    worktree: repo.path.to_string_lossy().into_owned(),
                    current: true,
                    remote: false,
                    gone: false,
                },
            );
        }
        let remotes = git(&repo.path, &["remote"])?
            .lines()
            .map(str::to_owned)
            .collect();
        Ok(Self {
            head,
            oid,
            branches,
            remotes,
        })
    }

    pub fn local_name<'a>(&self, branch: &'a Branch) -> Option<&'a str> {
        self.remotes
            .iter()
            .filter_map(|remote| branch.name.strip_prefix(&format!("{remote}/")))
            .min_by_key(|name| name.len())
    }

    pub fn head_label(&self) -> String {
        self.head.as_deref().map_or_else(
            || {
                format!(
                    "detached@{}",
                    self.oid
                        .as_deref()
                        .unwrap_or("?")
                        .chars()
                        .take(8)
                        .collect::<String>()
                )
            },
            |head| short_ref(head).into(),
        )
    }
}

pub(super) fn short_ref(reference: &str) -> &str {
    reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/remotes/"))
        .unwrap_or(reference)
}

pub(super) struct Repository {
    pub root: usize,
    pub name: String,
    pub path: PathBuf,
    pub state: Result<Snapshot, String>,
}
