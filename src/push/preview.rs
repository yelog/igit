//! Immutable committed objects are read on a worker; UI selection stays on the terminal thread.
use super::*;

#[derive(Clone)]
pub(super) struct Request {
    pub path: PathBuf,
    pub entry: Entry,
    pub commit: Option<Out>,
    pub file: Option<Change>,
    pub icons: IconMode,
    pub failure: Option<String>,
}

pub(super) enum Loaded {
    Files { files: Vec<Change>, summary: String },
    Patch { body: String, is_patch: bool },
}

pub(super) fn read(request: Request) -> Loaded {
    if let Some(file) = &request.file {
        return patch(&request.path, &request.entry, request.commit.as_ref(), file);
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
                "{} {} {}:{}\n\n{} outgoing commits.\n{}\n\n{}",
                e.source,
                request.icons.glyph(Icon::Right),
                e.t.remote,
                e.t.dst,
                e.n,
                e.tracking
                    .as_ref()
                    .map(|h| format!("Last fetched tip: {:.10}", h))
                    .unwrap_or_else(|| "New target (no tracking ref)".into()),
                if e.n > 60 {
                    "List displays newest 60; the complete branch will be pushed."
                } else {
                    "Select a commit to inspect its files."
                },
            ),
        )
    };
    let (files, summary) = match files {
        Ok(files) => (
            files,
            match request.failure {
                Some(error) => format!("FAILED: {error}\n\n{desc}"),
                None => desc,
            },
        ),
        Err(error) => (
            Vec::new(),
            format!("Unable to load changed files: {error}\n\n{desc}"),
        ),
    };
    Loaded::Files { files, summary }
}

fn patch(p: &Path, x: &Entry, commit: Option<&Out>, file: &Change) -> Loaded {
    let path = &file.path;
    let mut cmd = command(p);
    cmd.arg("--literal-pathspecs");
    if let Some(h) = commit {
        cmd.args([
            "show",
            "--diff-merges=first-parent",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            &h.id,
        ]);
    } else if let Some(base) = &x.tracking {
        cmd.args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            base,
            &x.head,
        ]);
    } else {
        let item = cmd
            .args(["ls-tree", "-z", &x.head, "--"])
            .arg(path)
            .output()
            .ok();
        let oid = item
            .as_ref()
            .and_then(|o| o.stdout.split(|v| *v == b'\t').next())
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.split_whitespace().nth(2));
        let body = match oid {
            Some(id)
                if git(p, &["cat-file", "-s", id])
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .is_some_and(|n| n <= 1_048_576) =>
            {
                match git_bytes(p, &["cat-file", "blob", id]) {
                    Ok(b) if !b.contains(&0) => format!(
                        "New target: {}\n\n{}",
                        label(path),
                        String::from_utf8_lossy(&b)
                            .chars()
                            .take(30000)
                            .collect::<String>()
                    ),
                    _ => "Binary or non-blob entry".into(),
                }
            }
            _ => "Preview unavailable: large file or not a blob".into(),
        };
        return Loaded::Patch {
            body,
            is_patch: false,
        };
    }
    cmd.arg("--").arg(path);
    if let Some(previous) = &file.previous {
        cmd.arg(previous);
    }
    let (body, is_patch) = match cmd.output() {
        Ok(o) if o.status.success() => (
            String::from_utf8_lossy(&o.stdout)
                .chars()
                .take(30000)
                .collect(),
            true,
        ),
        Ok(o) => (String::from_utf8_lossy(&o.stderr).into_owned(), false),
        Err(e) => (e.to_string(), false),
    };
    Loaded::Patch { body, is_patch }
}

impl PushPanel {
    fn preview_request(&self, c: &Context, file: Option<Change>) -> Option<Request> {
        let (i, sub) = self.selected()?;
        let entry = self.entries.iter().find(|e| e.idx == i)?.clone();
        Some(Request {
            path: c.repos[i].path.clone(),
            commit: sub.and_then(|k| entry.logs.get(k)).cloned(),
            entry,
            file,
            icons: self.icons,
            failure: self.results.get(&i).and_then(|r| r.as_ref().err()).cloned(),
        })
    }

    pub(super) fn inspect(&mut self, c: &Context) {
        self.preview_load.cancel();
        self.preview_request = None;
        self.detail_selection.reset();
        self.preview = Preview::default();
        self.detail_off = 0;
        let Some(request) = self.preview_request(c, None) else {
            return;
        };
        if !request.entry.error.is_empty() {
            self.preview.body = request.entry.error.clone();
            self.preview.summary = request.entry.error;
        } else if request.commit.is_some() || request.entry.n > 0 {
            self.preview.body = "Loading changed files…".into();
            self.preview_request = Some(request);
            self.preview_load.request();
        }
    }

    pub(super) fn open_file(&mut self, c: &Context) {
        let Some(file) = self
            .preview
            .tree
            .file_index()
            .and_then(|i| self.preview.files.get(i))
            .cloned()
        else {
            return;
        };
        if let Some(request) = self.preview_request(c, Some(file)) {
            self.detail_selection.reset();
            self.detail_off = 0;
            self.preview.body = "Loading diff…".into();
            self.preview.is_patch = false;
            self.preview_request = Some(request);
            self.preview_load.request();
        }
    }
}
