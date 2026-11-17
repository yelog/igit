use super::*;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub(in crate::log) mod editor;

#[cfg(test)]
mod tests;

const LIMIT: usize = 128;
const ATTRIBUTES: [&str; 7] = [
    "filter",
    "working-tree-encoding",
    "ident",
    "text",
    "eol",
    "crlf",
    "merge",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::log) enum Step {
    Pick,
    Squash,
    Fixup,
}

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
        }
    }
}

#[derive(Clone)]
pub(in crate::log) struct Entry {
    pub oid: String,
    pub subject: String,
    pub message: String,
    pub step: Step,
}

#[derive(Clone, PartialEq, Eq)]
struct State {
    repo: PathBuf,
    directory: PathBuf,
    #[cfg(unix)]
    identity: [(u64, u64); 2],
    branch: String,
    head: String,
    base: String,
    commits: Vec<String>,
    refs: Vec<u8>,
    index: Vec<u8>,
    entries: Vec<u8>,
    paths: Vec<u8>,
    touched: Vec<u8>,
    config: Vec<u8>,
    attributes: Vec<Vec<u8>>,
}

fn refs(repo: &Path) -> Result<Vec<u8>> {
    git_bytes(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
        ],
    )
}

fn configured(repo: &Path, key: &str) -> Result<Option<String>> {
    optional(repo, &["config", "--get", key])
}

fn safe_config(repo: &Path) -> Result<()> {
    ensure!(
        configured(repo, "core.autocrlf")?.is_none_or(|value| matches!(
            value.to_ascii_lowercase().as_str(),
            "false" | "no" | "off" | "0"
        )),
        "History cleanup requires core.autocrlf=false; working-tree conversions are unsupported"
    );
    ensure!(
        configured(repo, "core.eol")?.is_none_or(
            |value| value.eq_ignore_ascii_case("lf") || value.eq_ignore_ascii_case("native")
        ),
        "History cleanup does not support CRLF checkout conversions"
    );
    Ok(())
}

fn check_attributes(repo: &Path, paths: &[u8], index: Option<&Path>) -> Result<Vec<u8>> {
    let input = write_temp(paths)?;
    let mut cmd = command(repo);
    cmd.args(["check-attr", "-z", "--stdin"]);
    if let Some(index) = index {
        cmd.arg("--cached").env("GIT_INDEX_FILE", index);
    }
    cmd.args(ATTRIBUTES);
    let bytes = checked(crate::core::process::read_output_with_input(
        repo,
        "check-attr",
        &mut cmd,
        input.reopen()?,
    )?)?;
    let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    ensure!(
        fields.last() == Some(&b"".as_slice())
            && fields.len()
                == paths.iter().filter(|byte| **byte == 0).count() * ATTRIBUTES.len() * 3 + 1,
        "Cannot verify history path attributes"
    );
    for triple in fields[..fields.len() - 1].chunks_exact(3) {
        ensure!(
            matches!(triple[2], b"unspecified" | b"unset"),
            "History cleanup does not support the {} attribute at {}; review conversions with an external Git tool",
            plain(triple[1]),
            plain(triple[0]),
        );
    }
    Ok(bytes)
}

impl State {
    fn read(target: &Target, c: &Context) -> Result<Self> {
        let repo = target.repo(c)?;
        ensure_idle(&repo.path)?;
        let head = Head::read(&repo.path)?;
        let branch = head
            .branch
            .filter(|branch| branch.starts_with("refs/heads/"))
            .context("Check out a local branch before cleaning up history")?;
        ensure!(
            git(&repo.path, &["rev-parse", "--is-shallow-repository"])? == "false",
            "History cleanup requires a complete, non-shallow repository"
        );
        let selected = git(
            &repo.path,
            &["rev-list", "--parents", "-1", &target.oid, "--"],
        )?;
        let parents: Vec<_> = selected.split_whitespace().skip(1).collect();
        ensure!(
            parents.len() == 1,
            "Select a non-root, non-merge commit; history cleanup supports a linear suffix only"
        );
        let base = parents[0].to_owned();
        ensure!(
            crate::core::update::is_ancestor(&repo.path, &target.oid, &head.oid)?,
            "Selected commit is outside the current branch"
        );
        let span = format!("{base}..{}", head.oid);
        let history = git(
            &repo.path,
            &[
                "rev-list",
                "--reverse",
                "--topo-order",
                "--parents",
                &span,
                "--",
            ],
        )?;
        let mut commits = Vec::new();
        let mut previous = base.as_str();
        for line in history.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            ensure!(
                fields.len() == 2 && fields[1] == previous,
                "Merge topology is unsupported; choose a linear suffix after the last merge"
            );
            previous = fields[0];
            commits.push(fields[0].to_owned());
        }
        ensure!(
            commits.first() == Some(&target.oid) && commits.last() == Some(&head.oid),
            "Selected history changed; refresh Log and review again"
        );
        ensure!(
            (2..=LIMIT).contains(&commits.len()),
            "Select between 2 and {LIMIT} linear commits, from the selected commit through HEAD"
        );
        ensure_unpublished(&repo.path, &target.oid)?;
        safe_config(&repo.path)?;
        let config = git_bytes(&repo.path, &["config", "--null", "--list"])?;
        let mut paths = BTreeSet::<Vec<u8>>::new();
        let scratch = tempfile::tempdir()?;
        let index = scratch.path().join("index");
        let mut attributes = Vec::new();
        for tree in std::iter::once(&base).chain(commits.iter()) {
            let names = git_bytes(
                &repo.path,
                &["ls-tree", "-r", "--name-only", "-z", tree, "--"],
            )?;
            paths.extend(
                names
                    .split(|byte| *byte == 0)
                    .filter(|path| !path.is_empty())
                    .map(<[u8]>::to_vec),
            );
            checked(crate::core::process::write_output(
                &repo.path,
                "history staging preparation",
                command(&repo.path)
                    .arg("-c")
                    .arg(format!("core.hooksPath={}", scratch.path().display()))
                    .args(["-c", "submodule.recurse=false"])
                    .args(["read-tree", tree])
                    .env("GIT_INDEX_FILE", &index),
            )?)?;
            attributes.push(check_attributes(&repo.path, &names, Some(&index))?);
        }
        let tracked = git_bytes(&repo.path, &["ls-files", "--cached", "-z"])?;
        paths.extend(
            tracked
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(<[u8]>::to_vec),
        );
        let paths: Vec<_> = paths
            .into_iter()
            .flat_map(|mut path| {
                path.push(0);
                path
            })
            .collect();
        attributes.push(check_attributes(&repo.path, &paths, None)?);
        clean(&repo.path)?;
        protect_paths(&repo.path, &span, false)?;
        let touched = git_bytes(
            &repo.path,
            &[
                "log",
                "--format=",
                "--no-show-signature",
                "--no-notes",
                "--no-ext-diff",
                "--no-renames",
                "--ignore-submodules=none",
                "--name-only",
                "-z",
                &span,
                "--",
            ],
        )?;
        let canonical = fs::canonicalize(&repo.path)?;
        let directory = fs::canonicalize(
            revpath(&repo.path, "--absolute-git-dir").context("Git directory unavailable")?,
        )?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            let working = fs::metadata(&canonical)?;
            let git = fs::metadata(&directory)?;
            [(working.dev(), working.ino()), (git.dev(), git.ino())]
        };
        let index = fs::read(directory.join("index"))?;
        let state = Self {
            repo: canonical,
            directory,
            #[cfg(unix)]
            identity,
            branch,
            head: head.oid,
            base,
            commits,
            refs: refs(&repo.path)?,
            index,
            entries: git_bytes(&repo.path, &["ls-files", "--stage", "-v", "-z"])?,
            paths,
            touched,
            config,
            attributes,
        };
        ensure!(
            Head::read(&repo.path)?
                == (Head {
                    branch: Some(state.branch.clone()),
                    oid: state.head.clone(),
                }),
            "Branch or HEAD changed; review again"
        );
        Ok(state)
    }
}

#[derive(Clone)]
pub(in crate::log) struct Draft {
    pub target: Target,
    pub entries: Vec<Entry>,
    state: State,
}

impl Draft {
    pub fn read(c: &Context, target: Target) -> Result<Self> {
        let state = State::read(&target, c)?;
        let entries = state
            .commits
            .iter()
            .map(|oid| {
                let message = message(&state.repo, oid)?;
                Ok(Entry {
                    oid: oid.clone(),
                    subject: message
                        .lines()
                        .next()
                        .unwrap_or("(empty message)")
                        .to_owned(),
                    message,
                    step: Step::Pick,
                })
            })
            .collect::<Result<_>>()?;
        let draft = Self {
            target,
            entries,
            state,
        };
        draft.revalidate(c)?;
        Ok(draft)
    }

    pub fn branch(&self) -> &str {
        self.state
            .branch
            .strip_prefix("refs/heads/")
            .unwrap_or(&self.state.branch)
    }

    pub fn set_step(&mut self, row: usize, step: Step) -> Result<()> {
        ensure!(row < self.entries.len(), "No history row selected");
        ensure!(row > 0 || step == Step::Pick, "The first row must be pick");
        if self.entries[row].step != step {
            self.entries[row].message = message(&self.state.repo, &self.entries[row].oid)?;
            if step == Step::Squash {
                let prior = self.result_message(row - 1);
                self.entries[row].message = format!("{prior}\n\n{}", self.entries[row].message);
            }
            self.entries[row].step = step;
        }
        Ok(())
    }

    pub fn move_by(&mut self, row: usize, delta: isize) -> Result<usize> {
        ensure!(row < self.entries.len(), "No history row selected");
        let next = row.saturating_add_signed(delta).min(self.entries.len() - 1);
        ensure!(
            (next != 0 || self.entries[row].step == Step::Pick)
                && (row != 0 || self.entries[next].step == Step::Pick),
            "The first row must remain pick; change squash/fixup to pick before moving it"
        );
        self.entries.swap(row, next);
        Ok(next)
    }

    pub fn result_message(&self, row: usize) -> &str {
        self.entries[..=row]
            .iter()
            .rev()
            .find(|entry| entry.step != Step::Fixup)
            .map_or("", |entry| &entry.message)
    }

    fn revalidate(&self, c: &Context) -> Result<()> {
        ensure!(
            State::read(&self.target, c).context("Repository changed; review again")? == self.state,
            "Branch, HEAD, publication, refs, index, paths or configuration changed; review again"
        );
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.entries
                .first()
                .is_some_and(|entry| entry.step == Step::Pick),
            "The first row must be pick"
        );
        let mut ids: Vec<_> = self.entries.iter().map(|entry| &entry.oid).collect();
        let mut expected: Vec<_> = self.state.commits.iter().collect();
        ids.sort();
        expected.sort();
        ensure!(
            ids == expected,
            "The plan must contain every reviewed commit exactly once"
        );
        ensure!(
            self.entries.iter().any(|entry| entry.step != Step::Pick)
                || self
                    .entries
                    .iter()
                    .map(|entry| &entry.oid)
                    .ne(self.state.commits.iter()),
            "History plan is unchanged; squash, fixup or reorder a commit first"
        );
        ensure!(
            self.entries.iter().all(|entry| entry.step != Step::Squash
                || !entry.message.trim().is_empty() && !entry.message.contains('\0')),
            "Enter a non-empty squash message without NUL bytes"
        );
        Ok(())
    }
}

pub(in crate::log) struct Reviewed {
    draft: Draft,
    backup: String,
}

impl Reviewed {
    pub fn prepare(c: &Context, draft: &Draft) -> Result<(Self, String)> {
        draft.validate()?;
        draft.revalidate(c)?;
        let backup = format!(
            "refs/igit/backups/history-{}-{}-{}",
            &draft.state.head[..8],
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        );
        let mut body = format!(
            "Repository: {}\nPath: {}\nBranch: {}\nReviewed HEAD: {}\nUnchanged base: {}\nBackup ref: {backup}\n\nExecution order (old IDs):\n",
            plain(c.repos[draft.target.root].name.as_bytes()),
            plain(draft.state.repo.as_os_str().as_encoded_bytes()),
            plain(draft.state.branch.as_bytes()),
            draft.state.head,
            draft.state.base,
        );
        let mut destination = "";
        for (row, entry) in draft.entries.iter().enumerate() {
            use std::fmt::Write as _;
            if entry.step == Step::Pick {
                destination = &entry.oid;
            }
            writeln!(
                body,
                "{}. {} {}  {}{}",
                row + 1,
                entry.step.label(),
                entry.oid,
                plain(entry.subject.as_bytes()),
                if entry.step == Step::Pick {
                    String::new()
                } else {
                    format!("\n   Combine into {destination}")
                },
            )?;
            if draft
                .entries
                .get(row + 1)
                .is_none_or(|next| next.step == Step::Pick)
            {
                writeln!(
                    body,
                    "   Result message:\n{}\n",
                    draft
                        .result_message(row)
                        .split('\n')
                        .map(|line| plain(line.as_bytes()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                )?;
            }
        }
        body.push_str("Affected paths across the entire plan:\n");
        let paths: BTreeSet<_> = draft
            .state
            .touched
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .collect();
        if paths.is_empty() {
            body.push_str("  (empty commits; metadata only)\n");
        }
        for path in paths {
            body.push_str(&format!("  {}\n", plain(path)));
        }
        body.push('\n');
        body.push_str(
            "Replayed commits may receive new IDs; squash/fixup remove their separate IDs. Rewritten signatures are removed.\nOnly this branch moves. Other branches/tags remain at their original IDs.\nUnpublished according to the reviewed local remote-tracking refs; this does not fetch.\nTracked working tree and index must remain clean. All paths in these commits are in scope; Log path filters do not narrow a history rewrite.\nNo autostash, update-refs, fork-point or automatic autosquash.\nConflicts keep native rebase state: review Continue or Abort separately in Conflicts.\n",
        );
        Ok((
            Self {
                draft: draft.clone(),
                backup,
            },
            body,
        ))
    }

    fn todo(&self) -> Result<String> {
        let mut todo = String::new();
        let mut destination = "";
        for (row, entry) in self.draft.entries.iter().enumerate() {
            use std::fmt::Write as _;
            match entry.step {
                Step::Pick => {
                    destination = &entry.oid;
                    writeln!(todo, "pick {}", entry.oid)?;
                }
                Step::Fixup | Step::Squash => writeln!(todo, "fixup {}", entry.oid)?,
            }
            if self
                .draft
                .entries
                .get(row + 1)
                .is_none_or(|next| next.step == Step::Pick)
            {
                let raw = git_bytes(&self.draft.state.repo, &["cat-file", "commit", destination])?;
                let raw = super::reword::replace(
                    &raw,
                    &HashMap::new(),
                    Some(self.draft.result_message(row)),
                )?;
                let oid = git_input(
                    &self.draft.state.repo,
                    &["hash-object", "-t", "commit", "-w", "--stdin"],
                    &raw,
                )?;
                writeln!(
                    todo,
                    "exec GIT_NO_REPLACE_OBJECTS=1 GIT_GRAFT_FILE= git --no-replace-objects -c core.hooksPath=/dev/null -c submodule.recurse=false commit --amend --only --allow-empty --allow-empty-message --no-verify --no-post-rewrite --no-gpg-sign --cleanup=verbatim -C {oid}",
                )?;
            }
        }
        Ok(todo)
    }

    pub fn execute(self, c: &Context) -> Result<(NoticeLevel, String)> {
        self.draft.validate()?;
        self.draft.revalidate(c)?;
        let state = &self.draft.state;
        ensure!(
            optional(
                &state.repo,
                &["rev-parse", "--verify", "--quiet", &self.backup]
            )?
            .is_none(),
            "Recovery reference already exists; review again"
        );
        let todo = write_temp(self.todo()?.as_bytes())?;
        let hooks = tempfile::tempdir()?;
        self.draft.revalidate(c)?;
        git_input(
            &state.repo,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "update-ref",
                "-m",
                "igit: backup reviewed linear history",
                "--stdin",
            ],
            format!(
                "start\nverify {} {}\ncreate {} {}\nprepare\ncommit\n",
                state.branch, state.head, self.backup, state.head,
            )
            .as_bytes(),
        )?;
        ensure!(
            Head::read(&state.repo)?
                == (Head {
                    branch: Some(state.branch.clone()),
                    oid: state.head.clone(),
                }),
            "Branch or HEAD changed; inspect the repository. Original tip: {}",
            self.backup,
        );
        let output = crate::core::process::write_output(
            &state.repo,
            "reviewed history rebase",
            rebase_command(&state.repo, hooks.path())
                .env("IGIT_HISTORY_TODO", todo.path())
                .env(
                    "GIT_SEQUENCE_EDITOR",
                    "sh -c 'cat \"$IGIT_HISTORY_TODO\" > \"$1\"' igit-history",
                )
                .args([
                    "rebase",
                    "--interactive",
                    "--merge",
                    "--force-rebase",
                    "--no-rebase-merges",
                    "--no-autostash",
                    "--no-update-refs",
                    "--no-autosquash",
                    "--no-fork-point",
                    "--no-rerere-autoupdate",
                    "--no-gpg-sign",
                    "--no-verify",
                    "--keep-empty",
                    "--empty=stop",
                    "--reapply-cherry-picks",
                    "--strategy=ort",
                    "--strategy-option=no-renames",
                    "--strategy-option=no-renormalize",
                    "--onto",
                    &state.base,
                    &state.base,
                ]),
        )
        .context("Cannot start reviewed rebase")?;
        if !output.status.success() {
            let stopped = state.directory.join("rebase-merge").is_dir();
            return Ok((
                if stopped {
                    NoticeLevel::Warning
                } else {
                    NoticeLevel::Error
                },
                format!(
                    "History cleanup {} · Backup: {}{} · {}",
                    if stopped { "stopped" } else { "failed" },
                    self.backup,
                    if stopped {
                        " · Conflicts: review Continue or Abort"
                    } else {
                        ""
                    },
                    plain(&output.stderr),
                ),
            ));
        }
        ensure!(
            Head::read(&state.repo)?.branch.as_deref() == Some(state.branch.as_str()),
            "Branch changed during history cleanup; inspect the repository. Backup: {}",
            self.backup
        );
        let remaining = |bytes: &[u8]| {
            bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| {
                    let reference = line.split(|byte| *byte == 0).next().unwrap_or_default();
                    reference != state.branch.as_bytes() && reference != self.backup.as_bytes()
                })
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>()
        };
        ensure!(
            remaining(&refs(&state.repo)?) == remaining(&state.refs),
            "Other refs changed during cleanup; inspect the repository. Backup: {}",
            self.backup
        );
        Ok((
            NoticeLevel::Success,
            format!(
                "Cleaned up {} on {} · Backup: {}",
                c.repos[self.draft.target.root].name,
                self.draft.branch(),
                self.backup,
            ),
        ))
    }
}

fn rebase_command(repo: &Path, hooks: &Path) -> Command {
    let mut cmd = command(repo);
    cmd.args([
        "-c",
        "submodule.recurse=false",
        "-c",
        "merge.directoryRenames=false",
        "-c",
        "merge.renormalize=false",
        "-c",
        "merge.autoStash=false",
        "-c",
        "rebase.autoStash=false",
        "-c",
        "rebase.updateRefs=false",
        "-c",
        "rebase.autoSquash=false",
        "-c",
        "rebase.abbreviateCommands=false",
        "-c",
        "rebase.instructionFormat=%s",
        "-c",
        "rebase.missingCommitsCheck=error",
        "-c",
        "rebase.rescheduleFailedExec=false",
        "-c",
        "rerere.enabled=false",
        "-c",
        "rerere.autoupdate=false",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "commit.cleanup=verbatim",
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.eol=lf",
        "-c",
        "core.logAllRefUpdates=true",
        "-c",
    ])
    .arg(format!("core.hooksPath={}", hooks.display()))
    .stdin(Stdio::null());
    cmd
}
