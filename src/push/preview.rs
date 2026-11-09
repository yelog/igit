//! Immutable committed objects are read on a worker; UI selection stays on the terminal thread.
use super::*;
use crate::{
    core::revision::{self, Source},
    merge::RevisionDiff,
};

#[derive(Clone)]
pub(super) struct Request {
    pub path: PathBuf,
    pub entry: Entry,
    pub commit: Option<Out>,
    pub file: Option<Change>,
    pub icons: IconMode,
    pub result: Option<Result<PushOutcome, String>>,
    pub set_upstream: bool,
}

pub(super) enum Loaded {
    Files { files: Vec<Change>, summary: String },
    Diff(FileDiff),
}

pub(super) enum FileDiff {
    Split(Box<RevisionDiff>),
    Patch(String),
}

pub(super) fn read(request: Request) -> Loaded {
    if let Some(file) = &request.file {
        return Loaded::Diff(
            file_diff(&request.path, &request.entry, request.commit.as_ref(), file)
                .unwrap_or_else(|e| FileDiff::Patch(format!("Unable to load diff: {e:#}"))),
        );
    }
    let p = &request.path;
    let e = &request.entry;
    let (files, desc) = if let Some(h) = &request.commit {
        (
            git_bytes(
                p,
                &[
                    "diff-tree",
                    "--root",
                    "--diff-merges=first-parent",
                    "--no-commit-id",
                    "--name-status",
                    "-z",
                    "-r",
                    "--find-renames",
                    &h.id,
                    "--",
                ],
            )
            .map(files::name_status),
            git(
                p,
                &[
                    "show",
                    "-s",
                    "--format=%s%n%n%b%n%h   %an <%ae>%n%ai",
                    &h.id,
                ],
            )
            .unwrap_or_default(),
        )
    } else {
        let paths = if let Some(base) = &e.tracking {
            git_bytes(
                p,
                &[
                    "diff",
                    "--name-status",
                    "-z",
                    "--find-renames",
                    base,
                    &e.head,
                    "--",
                ],
            )
            .map(files::name_status)
        } else {
            git_bytes(p, &["ls-tree", "-r", "--name-only", "-z", &e.head]).map(files::added_paths)
        };
        (
            paths,
            format!(
                "{} {} {}:{}\n\n{} outgoing commits.\n{}\n\nUpstream: {}\n{}\n\n{}",
                e.source,
                request.icons.glyph(Icon::Right),
                e.t.remote,
                e.t.dst,
                e.n,
                e.tracking
                    .as_ref()
                    .map(|h| format!("Last fetched tip: {:.10}", h))
                    .unwrap_or_else(|| "New target (no tracking ref)".into()),
                e.config.description(&e.source),
                if request.set_upstream {
                    format!(
                        "Set upstream to {}:{} after push succeeds.",
                        e.t.remote, e.t.dst
                    )
                } else {
                    "Keep upstream unchanged. Target (t) edits tracking choice.".into()
                },
                if e.n > 60 {
                    "List displays newest 60; the complete branch will be pushed."
                } else {
                    "Select a commit to inspect its files."
                },
            ),
        )
    };
    let desc = with_result(&desc, request.result.as_ref());
    let (files, summary) = match files {
        Ok(files) => (files, desc),
        Err(error) => (
            Vec::new(),
            format!("Unable to load changed files: {error}\n\n{desc}"),
        ),
    };
    Loaded::Files { files, summary }
}

fn with_result(desc: &str, result: Option<&Result<PushOutcome, String>>) -> String {
    match result {
        Some(Err(error)) => format!("FAILED: {error}\n\n{desc}"),
        Some(Ok(PushOutcome::UpstreamNotSet(warning))) => {
            format!("PUSH SUCCEEDED; UPSTREAM NOT SET\n{warning}\n\n{desc}")
        }
        Some(Ok(PushOutcome::Complete)) => format!("Operation completed successfully.\n\n{desc}"),
        None => desc.into(),
    }
}

fn file_diff(p: &Path, x: &Entry, commit: Option<&Out>, file: &Change) -> Result<FileDiff> {
    let (before, after, empty_note) = if let Some(h) = commit {
        let parents = git(
            p,
            &[
                "show",
                "-s",
                "--no-show-signature",
                "--no-notes",
                "--format=%P",
                &h.id,
                "--",
            ],
        )?;
        (
            parents.split_whitespace().next().map(str::to_owned),
            &h.id,
            "No parent · initial commit",
        )
    } else {
        (x.tracking.clone(), &x.head, "New target · no tracking ref")
    };
    let sources = (|| -> Result<[Source; 2]> {
        let mut old = if let Some(oid) = &before {
            revision::source(p, oid, file.previous.as_deref().unwrap_or(&file.path))?
        } else {
            Source {
                text: String::new(),
                description: empty_note.into(),
            }
        };
        if let Some(previous) = &file.previous {
            old.description.push_str(&format!(" · {}", label(previous)));
        }
        Ok([old, revision::source(p, after, &file.path)?])
    })();
    match sources {
        Ok(sources) => {
            let [old, new] = sources;
            Ok(FileDiff::Split(Box::new(RevisionDiff::two_way(
                label(&file.path),
                [before.unwrap_or_else(|| "Empty tree".into()), after.clone()],
                [old.text, new.text],
                [old.description, new.description],
            ))))
        }
        Err(error) => {
            let body = match patch(p, before.as_deref(), after, file) {
                Ok(body) => format!("{error}\n\n{body}"),
                Err(patch_error) => format!("{error}\n\nUnable to load Git patch: {patch_error}"),
            };
            Ok(FileDiff::Patch(body))
        }
    }
}

fn patch(p: &Path, before: Option<&str>, after: &str, file: &Change) -> Result<String> {
    let empty;
    let before = match before {
        Some(oid) => oid,
        None => {
            // Git recognizes its empty tree ID without writing an object.
            empty = git(p, &["hash-object", "-t", "tree", "--stdin"])?;
            &empty
        }
    };
    let mut cmd = command(p);
    cmd.args([
        "--literal-pathspecs",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--find-renames",
        before,
        after,
        "--",
    ])
    .arg(&file.path);
    if let Some(previous) = &file.previous {
        cmd.arg(previous);
    }
    let (raw, cut) = revision::read_patch(cmd)?;
    let mut body = String::from_utf8_lossy(&raw).into_owned();
    if cut {
        body.push_str("\n… Diff limited to 512 KiB · Use Git for the full patch");
    }
    Ok(body)
}

impl PushPanel {
    pub(super) fn preview_request(&self, c: &Context, file: Option<Change>) -> Option<Request> {
        let (i, sub) = self.selected()?;
        let entry = self.entries.iter().find(|e| e.idx == i)?.clone();
        Some(Request {
            path: c.repos[i].path.clone(),
            commit: sub.and_then(|k| entry.logs.get(k)).cloned(),
            set_upstream: self.sets_upstream(&entry),
            entry,
            file,
            icons: self.icons,
            result: self.results.get(&i).cloned(),
        })
    }

    pub(super) fn inspect(&mut self, c: &Context) {
        self.preview_load.cancel();
        self.preview_request = None;
        self.diff = None;
        self.detail_selection.reset();
        self.preview = Preview::default();
        self.file_search.invalidate();
        self.detail_off = 0;
        let Some(request) = self.preview_request(c, None) else {
            return;
        };
        if !request.entry.error.is_empty() {
            self.preview.summary = with_result(&request.entry.error, request.result.as_ref());
            self.preview.body = self.preview.summary.clone();
        } else if request.commit.is_some() || request.entry.n > 0 || request.result.is_some() {
            self.preview.body = "Loading changed files…".into();
            self.preview_request = Some(request);
            self.preview_load.request();
        }
    }
}
