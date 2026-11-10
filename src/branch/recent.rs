use super::model::Branch;
use crate::core::{git, Repo};
use std::{cmp::Reverse, collections::BTreeMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LastUsed {
    pub timestamp: i64,
    pub sequence: Reverse<usize>,
}

pub(super) fn read(repo: &Repo, branches: &BTreeMap<String, Branch>) -> BTreeMap<String, LastUsed> {
    git(
        &repo.path,
        &[
            "reflog",
            "show",
            "--date=unix",
            "--format=%gD%x00%gs",
            "HEAD",
            "--",
        ],
    )
    .map(|data| parse(&data, branches))
    .unwrap_or_default()
}

fn parse(data: &str, branches: &BTreeMap<String, Branch>) -> BTreeMap<String, LastUsed> {
    let mut names: BTreeMap<_, _> = branches
        .values()
        .filter(|branch| !branch.remote)
        .map(|branch| (branch.reference.clone(), branch.reference.clone()))
        .collect();
    let mut recent = BTreeMap::new();
    for (sequence, line) in data.lines().enumerate() {
        let Some((selector, subject)) = line.split_once('\0') else {
            continue;
        };
        if let Some((old, new)) = subject
            .strip_prefix("Branch: renamed ")
            .and_then(|names| names.split_once(" to "))
        {
            if let Some(reference) = names.remove(new) {
                names.insert(old.into(), reference);
            } else {
                names.remove(old);
            }
            continue;
        }
        let Some((_, destination)) = subject
            .strip_prefix("checkout: moving from ")
            .and_then(|names| names.split_once(" to "))
        else {
            continue;
        };
        let Some(reference) = names.get(&format!("refs/heads/{destination}")) else {
            continue;
        };
        let Some(timestamp) = selector
            .strip_prefix("HEAD@{")
            .and_then(|date| date.strip_suffix('}'))
            .and_then(|date| date.parse().ok())
        else {
            continue;
        };
        recent.entry(reference.clone()).or_insert(LastUsed {
            timestamp,
            sequence: Reverse(sequence),
        });
    }
    recent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_ignores_unrelated_events_and_follows_rename_chains() {
        let branches = ["main", "feature/新名", "unused"]
            .into_iter()
            .map(|name| {
                let reference = format!("refs/heads/{name}");
                (
                    reference.clone(),
                    Branch {
                        reference,
                        name: name.into(),
                        oid: String::new(),
                        upstream: String::new(),
                        worktree: String::new(),
                        current: name == "main",
                        remote: false,
                        gone: false,
                    },
                )
            })
            .collect();
        let data = concat!(
            "HEAD@{900}\0commit: checkout: moving from main to unused\n",
            "HEAD@{bad}\0checkout: moving from main to unused\n",
            "HEAD@{800}\0checkout: moving from main to deadbeef\n",
            "HEAD@{700}\0checkout: moving from feature/新名 to main\n",
            "HEAD@{600}\0Branch: renamed refs/heads/middle to refs/heads/feature/新名\n",
            "HEAD@{500}\0Branch: renamed refs/heads/old to refs/heads/middle\n",
            "HEAD@{400}\0checkout: moving from main to old\n",
            "HEAD@{300}\0checkout: moving from old to main\n",
            "HEAD@{200}\0checkout: moving from main to removed\n",
            "malformed\n",
        );
        let recent = parse(data, &branches);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent["refs/heads/main"].timestamp, 700);
        assert_eq!(recent["refs/heads/feature/新名"].timestamp, 400);
        assert_eq!(recent["refs/heads/feature/新名"].sequence, Reverse(6));
    }
}
