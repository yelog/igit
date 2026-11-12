use super::*;
use crate::{commit::stash_backend, core::git_bytes, ui::NoticeLevel};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    process::{Output, Stdio},
};

mod view;

pub(crate) const TOKEN: &str = "branch:checkout";

pub(crate) enum Decision {
    Keep,
    Cancel,
    Run { force: bool },
}

pub(crate) struct Checkout {
    review: Review,
    saved: stash_backend::Plan,
    problem: String,
    scroll: usize,
    selected: usize,
    force: bool,
    hits: Vec<(ratatui::layout::Rect, usize)>,
}

fn detail(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr).trim(),
        String::from_utf8_lossy(&output.stdout).trim()
    )
    .trim()
    .into()
}

fn label(text: &str) -> String {
    text.chars()
        .flat_map(|ch| {
            if ch.is_control() && ch != '\n' {
                ch.escape_default().collect::<Vec<_>>()
            } else {
                vec![ch]
            }
        })
        .collect()
}

fn reflog(repo: &Path) -> Result<Vec<u8>> {
    git_bytes(
        repo,
        &[
            "stash",
            "list",
            "--no-show-signature",
            "--no-notes",
            "--format=%H%x00%gD%x00%gs",
            "-z",
        ],
    )
}

fn clean(repo: &Path) -> Result<bool> {
    Ok(git_bytes(
        repo,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ],
    )?
    .is_empty())
}

fn overlaps(left: &[u8], right: &[u8]) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with(b"/"))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with(b"/"))
}

impl Checkout {
    pub(in crate::branch) fn prepare(
        review: Review,
        c: &Context,
        error: anyhow::Error,
    ) -> Result<Self> {
        let problem = format!("{error:#}");
        if !problem.contains("would be overwritten by checkout:")
            && !problem.contains("would be removed by checkout:")
        {
            return Err(error);
        }
        review.validate(c)?;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let message = format!(
            "IGit Smart Checkout: {} -> {} [{unique}]",
            review.snapshot.head_label(),
            review.target()
        );
        let saved = stash_backend::plan(review.root, &review.path, &message)?
            .context("No stashable changes; move protected files aside before switching")?;
        let checkout = Self {
            review,
            saved,
            problem: label(&problem),
            scroll: 0,
            selected: 2,
            force: false,
            hits: vec![],
        };
        checkout.check_paths()?;
        Ok(checkout)
    }

    pub(crate) fn root(&self) -> usize {
        self.review.root
    }

    fn check_paths(&self) -> Result<()> {
        let repo = &self.review.path;
        let indexed = git_bytes(repo, &["ls-files", "-v", "-z"])?;
        ensure!(
            !indexed.split(|byte| *byte == 0).any(|entry| matches!(
                entry,
                [b'S' | b'a'..=b'z', b' ', ..]
            )),
            "Smart/Force Checkout requires ordinary index entries; review skip-worktree and assume-unchanged flags first"
        );
        let ignore_case = cfg!(any(target_os = "macos", windows))
            || git(repo, &["config", "--bool", "--get", "core.ignoreCase"])
                .is_ok_and(|value| value == "true");
        let key = |path: &[u8]| {
            if ignore_case {
                String::from_utf8_lossy(path).to_lowercase().into_bytes()
            } else {
                path.to_vec()
            }
        };
        let tree = git_bytes(repo, &["ls-tree", "-r", "-z", &self.review.source.oid])?;
        for entry in tree
            .split(|byte| *byte == 0)
            .filter(|entry| entry.starts_with(b"160000 ") || entry.starts_with(b"120000 "))
        {
            let tab = entry
                .iter()
                .position(|byte| *byte == b'\t')
                .context("Invalid Git tree")?;
            let path = key(&entry[tab + 1..]);
            ensure!(
                !self.saved.files().iter().any(|file| {
                    let saved = key(file.path.as_os_str().as_encoded_bytes());
                    if entry.starts_with(b"160000 ") {
                        overlaps(&saved, &path)
                    } else {
                        saved.strip_prefix(path.as_slice()).is_some_and(|suffix| suffix.starts_with(b"/"))
                    }
                }),
                "Local changes overlap a submodule or directory symlink in the target branch; handle this switch separately"
            );
        }
        let ignored = git_bytes(
            repo,
            &[
                "ls-files",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
                "-z",
            ],
        )?;
        let head = self
            .review
            .snapshot
            .oid
            .as_deref()
            .context("Current commit unavailable")?;
        let changed = git_bytes(
            repo,
            &[
                "diff-tree",
                "--no-commit-id",
                "--no-renames",
                "-r",
                "--name-only",
                "-z",
                head,
                &self.review.source.oid,
                "--",
            ],
        )?;
        for path in ignored
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let path = path.strip_suffix(b"/").unwrap_or(path);
            ensure!(
                !changed
                    .split(|byte| *byte == 0)
                    .filter(|name| !name.is_empty())
                    .any(|name| overlaps(&key(name), &key(path))),
                "Ignored files may be overwritten at {}; move them aside before switching",
                label(&String::from_utf8_lossy(path))
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn execute(self, c: &Context, force: bool) -> (NoticeLevel, String) {
        let (level, message, _) = self.execute_outcome(c, force);
        (level, message)
    }

    pub(crate) fn execute_outcome(
        self,
        c: &Context,
        force: bool,
    ) -> (NoticeLevel, String, Option<crate::branch::CheckoutDone>) {
        let (level, message, checkout) = match self.apply(c, force) {
            Ok(outcome) => outcome,
            Err(error) => return (NoticeLevel::Error, format!("{error:#}"), None),
        };
        (level, message, Some(checkout))
    }

    fn apply(
        self,
        c: &Context,
        force: bool,
    ) -> Result<(NoticeLevel, String, crate::branch::CheckoutDone)> {
        self.review.validate(c)?;
        self.saved.validate()?;
        self.check_paths()?;
        if force {
            let message = self.review.run(true)?;
            return Ok((
                NoticeLevel::Success,
                message,
                self.review.checkout_outcome()?,
            ));
        }
        let saved = stash_backend::execute(&self.saved)?;
        let oid = &saved.hash;
        let recovery = format!(
            "Saved changes kept in Stash: {}. Inspect Commit and Stash before retrying.",
            &oid[..oid.len().min(12)]
        );
        let run = || -> Result<(NoticeLevel, String, crate::branch::CheckoutDone)> {
            ensure!(
                saved.warning.is_none(),
                "{}",
                saved.warning.as_deref().unwrap_or("")
            );
            let entries = reflog(&self.review.path)?;
            let fields: Vec<_> = entries.split(|byte| *byte == 0).take(3).collect();
            ensure!(
                fields.first().copied() == Some(oid.as_bytes())
                    && fields
                        .get(2)
                        .is_some_and(|subject| subject.ends_with(self.saved.message.as_bytes())),
                "Stash list changed during save"
            );
            self.review.validate(c)?;
            ensure!(clean(&self.review.path)?, "Files changed after saving");
            if let Err(error) = self.review.run(false) {
                if self.review.validate(c).is_ok() && clean(&self.review.path)? {
                    let restored = self.restore(oid)?;
                    if restored.complete {
                        self.review.validate(c)?;
                        let dropped = self.drop_saved(&entries, oid);
                        bail!("{error:#}\nOriginal branch and local changes restored. {dropped}");
                    }
                    bail!("{error:#}\n{}", restored.message);
                }
                bail!("{error:#}\nCheckout state changed");
            }
            let expected = if self.review.operation == Operation::Track {
                format!("refs/heads/{}", self.review.name)
            } else {
                self.review.source.reference.clone()
            };
            ensure!(
                git(&self.review.path, &["symbolic-ref", "-q", "HEAD"])? == expected
                    && git(&self.review.path, &["rev-parse", "HEAD"])? == self.review.source.oid
                    && clean(&self.review.path)?,
                "Branch, HEAD, or files changed during checkout"
            );
            let restored = self.restore(oid)?;
            ensure!(
                git(&self.review.path, &["symbolic-ref", "-q", "HEAD"])? == expected
                    && git(&self.review.path, &["rev-parse", "HEAD"])? == self.review.source.oid,
                "Branch or HEAD changed during restoration"
            );
            if !restored.complete {
                let done = self.review.checkout_outcome()?;
                return Ok((
                    NoticeLevel::Warning,
                    format!(
                        "Switched to {} in {}. {}. {recovery}",
                        label(self.review.target()),
                        label(&self.review.repository),
                        restored.message
                    ),
                    done,
                ));
            }
            let dropped = self.drop_saved(&entries, oid);
            let done = self.review.checkout_outcome()?;
            Ok((
                if dropped.is_empty() { NoticeLevel::Success } else { NoticeLevel::Warning },
                format!(
                    "Smart Checkout completed in {}: {}. Local changes and staging restored. {dropped}",
                    label(&self.review.repository),
                    label(self.review.target())
                ),
                done,
            ))
        };
        run().with_context(|| recovery)
    }

    fn drop_saved(&self, entries: &[u8], oid: &str) -> String {
        let repo = &self.review.path;
        let cleanup = || -> Result<()> {
            ensure!(
                reflog(repo)? == entries
                    && git(repo, &["rev-parse", "--verify", "refs/stash"])? == oid,
                "Stash list changed"
            );
            let output = command(repo)
                .args(["stash", "drop", "stash@{0}"])
                .output()?;
            ensure!(output.status.success(), "{}", detail(&output));
            Ok(())
        };
        match cleanup() {
            Ok(()) => String::new(),
            Err(error) => format!(
                "Changes restored; backup cleanup skipped. Review Stash; do not apply again. {error:#}"
            ),
        }
    }

    fn restore(&self, oid: &str) -> Result<Restored> {
        let repo = &self.review.path;
        let apply = |index: bool| -> Result<Output> {
            let mut cmd = command(repo);
            cmd.args([
                "-c",
                "submodule.recurse=false",
                "-c",
                "stash.index=false",
                "-c",
                "rerere.enabled=false",
                "-c",
                "merge.directoryRenames=false",
                "stash",
                "apply",
            ]);
            if index {
                cmd.arg("--index");
            }
            Ok(cmd.arg(oid).output()?)
        };
        let mut output = apply(true)?;
        if output.status.success() {
            return Ok(Restored {
                complete: true,
                message: String::new(),
            });
        }
        if !crate::core::update::conflicts(repo) && clean(repo)? {
            output = apply(false)?;
        }
        let staging = restore_staging(repo, oid)?;
        let conflicts = crate::core::update::conflicts(repo);
        Ok(Restored {
            complete: output.status.success() && staging,
            message: if conflicts {
                "Resolve restored changes in Conflicts; the backup retains the original staging"
                    .into()
            } else if output.status.success() {
                "Changes restored, but some staging could not be restored; review Commit and the backup".into()
            } else {
                format!("Restore incomplete; {}", label(&detail(&output)))
            },
        })
    }
}

struct Restored {
    complete: bool,
    message: String,
}

impl Review {
    fn target(&self) -> &str {
        if self.operation == Operation::Track {
            &self.name
        } else {
            &self.source.name
        }
    }
}

fn index_entries(bytes: &[u8]) -> BTreeMap<Vec<u8>, Vec<Vec<u8>>> {
    let mut entries = BTreeMap::<_, Vec<_>>::new();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        if let Some(tab) = record.iter().position(|byte| *byte == b'\t') {
            entries
                .entry(record[tab + 1..].to_vec())
                .or_default()
                .push(record.to_vec());
        }
    }
    entries
}

fn restore_staging(repo: &Path, oid: &str) -> Result<bool> {
    let base = format!("{oid}^1");
    let staged = format!("{oid}^2");
    let patch = git_bytes(
        repo,
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            &base,
            &staged,
            "--",
        ],
    )?;
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let mut read = command(repo);
    let output = read
        .env("GIT_INDEX_FILE", &index)
        .args(["read-tree", "HEAD"])
        .output()?;
    ensure!(
        output.status.success(),
        "Cannot prepare staging recovery: {}",
        detail(&output)
    );
    let mut staged_merged = true;
    if !patch.is_empty() {
        let mut child = command(repo)
            .env("GIT_INDEX_FILE", &index)
            .args(["apply", "--cached", "--3way", "--whitespace=nowarn"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("Cannot restore staging")?
            .write_all(&patch)?;
        let output = child.wait_with_output()?;
        staged_merged = output.status.success();
        ensure!(
            output.status.success() || output.status.code() == Some(1),
            "Cannot merge saved staging: {}",
            detail(&output)
        );
    }
    let output = command(repo)
        .env("GIT_INDEX_FILE", &index)
        .args(["ls-files", "--stage", "-z"])
        .output()?;
    ensure!(output.status.success(), "Cannot read recovered staging");
    let desired = index_entries(&output.stdout);
    let current = index_entries(&git_bytes(repo, &["ls-files", "--stage", "-z"])?);
    let mut affected = BTreeSet::new();
    for tree in [oid, &staged] {
        let paths = git_bytes(
            repo,
            &[
                "diff-tree",
                "--no-commit-id",
                "-r",
                "--no-renames",
                "--name-only",
                "-z",
                &base,
                tree,
                "--",
            ],
        )?;
        affected.extend(
            paths
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(<[u8]>::to_vec),
        );
    }
    let mut input = Vec::new();
    let mut complete = staged_merged;
    for path in affected {
        let unmerged = |entries: Option<&Vec<Vec<u8>>>| {
            entries.is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry
                        .iter()
                        .position(|byte| *byte == b'\t')
                        .is_some_and(|tab| entry[tab - 1] != b'0')
                })
            })
        };
        if unmerged(current.get(&path)) {
            continue;
        }
        if unmerged(desired.get(&path)) {
            complete = false;
            continue;
        }
        input.extend_from_slice(format!("0 {}\t", "0".repeat(oid.len())).as_bytes());
        input.extend_from_slice(&path);
        input.push(0);
        if let Some(entries) = desired.get(&path) {
            for entry in entries {
                input.extend_from_slice(entry);
                input.push(0);
            }
        }
    }
    if !input.is_empty() {
        let mut child = command(repo)
            .args(["update-index", "-z", "--index-info"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("Cannot restore staging")?
            .write_all(&input)?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "Cannot restore staging: {}",
            detail(&output)
        );
    }
    Ok(complete)
}
