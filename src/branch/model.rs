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

#[derive(Default)]
pub(super) struct Metadata {
    pub default_branch: Option<String>,
    pub tracking: BTreeMap<String, (usize, usize)>,
}

impl Snapshot {
    pub fn read(repo: &Repo) -> Result<Self> {
        Self::read_refs(repo, false).map(|(snapshot, _)| snapshot)
    }

    pub fn read_for_display(repo: &Repo) -> Result<(Self, Metadata)> {
        Self::read_refs(repo, true)
    }

    fn read_refs(repo: &Repo, include_tracking: bool) -> Result<(Self, Metadata)> {
        // Only background display reads walk history for ahead/behind counts.
        let tracking = if include_tracking {
            "%(upstream:track,nobracket)"
        } else {
            ""
        };
        let format = format!(
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)%00%(worktreepath)%00{tracking}%00"
        );
        let data = git(
            &repo.path,
            &["for-each-ref", &format, "refs/heads/", "refs/remotes/"],
        )?;
        let head = git(&repo.path, &["symbolic-ref", "-q", "HEAD"]).ok();
        let oid = git(&repo.path, &["rev-parse", "--verify", "HEAD"]).ok();
        let mut branches = BTreeMap::new();
        let mut remote_heads = BTreeMap::new();
        let mut metadata = Metadata::default();
        // NUL fields preserve Unicode and even newlines in worktree paths.
        let fields: Vec<_> = data.split('\0').collect();
        for fields in fields.chunks_exact(6) {
            let reference = fields[0].trim_start_matches('\n');
            if !fields[3].is_empty() {
                if let Some(remote) = reference
                    .strip_prefix("refs/remotes/")
                    .and_then(|name| name.strip_suffix("/HEAD"))
                {
                    remote_heads.insert(remote, fields[3]);
                }
                continue; // Keep default-branch metadata, but omit symbolic aliases from the list.
            }
            let remote = reference.starts_with("refs/remotes/");
            let name = short_ref(reference);
            if include_tracking && !remote && !fields[2].is_empty() && fields[5] != "gone" {
                let mut counts = (0, 0);
                for part in fields[5].split(", ") {
                    if let Some(n) = part.strip_prefix("ahead ") {
                        counts.0 = n.parse()?;
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        counts.1 = n.parse()?;
                    }
                }
                metadata.tracking.insert(reference.into(), counts);
            }
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
        let remotes: Vec<_> = git(&repo.path, &["remote"])?
            .lines()
            .map(str::to_owned)
            .collect();
        metadata.default_branch = std::iter::once("origin")
            .chain(remotes.iter().map(String::as_str))
            .find_map(|remote| {
                let target = *remote_heads.get(remote)?;
                let name = target.strip_prefix(&format!("refs/remotes/{remote}/"))?;
                let local = format!("refs/heads/{name}");
                branches.contains_key(target).then(|| {
                    if branches.contains_key(&local) {
                        local
                    } else {
                        target.into()
                    }
                })
            });
        Ok((
            Self {
                head,
                oid,
                branches,
                remotes,
            },
            metadata,
        ))
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
    pub submodule: bool,
    pub state: Result<Snapshot, String>,
    pub metadata: Metadata,
    pub upstream_widths: Option<(usize, usize)>,
}
