use super::*;
use notify::event::{AccessKind, AccessMode};
use notify::{Config, Event as FileEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::{Arc, Mutex};

const DEBOUNCE: Duration = Duration::from_millis(300);
const MAX_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct Root {
    worktree: PathBuf,
    git: PathBuf,
    common: PathBuf,
    parent: Option<usize>,
}

struct Paths {
    roots: Vec<Root>,
    state: PathBuf,
}

fn canonical(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn path_text(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes.trim_ascii_end()))
    }
    #[cfg(not(unix))]
    PathBuf::from(String::from_utf8_lossy(bytes).trim_end())
}

impl Paths {
    fn new(c: &Context) -> Self {
        let roots = c
            .repos
            .iter()
            .map(|repo| {
                let marker = repo.path.join(".git");
                let git = fs::read(&marker)
                    .ok()
                    .and_then(|bytes| {
                        bytes
                            .strip_prefix(b"gitdir: ")
                            .map(|path| repo.path.join(path_text(path)))
                    })
                    .unwrap_or(marker);
                let git = canonical(git);
                let common = fs::read(git.join("commondir"))
                    .ok()
                    .map(|bytes| canonical(git.join(path_text(&bytes))))
                    .unwrap_or_else(|| git.clone());
                Root {
                    worktree: canonical(repo.path.clone()),
                    git,
                    common,
                    parent: repo.submodule.then_some(repo.parent).flatten(),
                }
            })
            .collect();
        Self {
            roots,
            state: canonical(c.state_dir.clone()),
        }
    }

    fn all(&self) -> BTreeSet<usize> {
        (0..self.roots.len()).collect()
    }

    fn targets(&self) -> Vec<PathBuf> {
        let mut paths: Vec<_> = self
            .roots
            .iter()
            .flat_map(|r| [&r.worktree, &r.git, &r.common].into_iter().cloned())
            .collect();
        paths.sort_by_key(|path| path.components().count());
        let mut targets = Vec::<PathBuf>::new();
        for path in paths {
            if !targets.iter().any(|parent| path.starts_with(parent)) {
                targets.push(path);
            }
        }
        targets
    }

    fn changed(&self, event: &FileEvent) -> BTreeSet<usize> {
        if event.need_rescan() {
            return self.all();
        }
        // Reads (including Git status/diff) must not schedule another scan.
        if matches!(event.kind, EventKind::Access(kind)
            if kind != AccessKind::Close(AccessMode::Write))
        {
            return BTreeSet::new();
        }
        if event.paths.is_empty() {
            return self.all();
        }
        let mut changed = BTreeSet::new();
        for path in &event.paths {
            if path.starts_with(&self.state) {
                continue;
            }
            // Prefer the deepest Git directory: submodule metadata often lives
            // below its parent's .git/modules; linked worktrees share common refs.
            let depth = self
                .roots
                .iter()
                .flat_map(|r| [&r.git, &r.common])
                .filter(|base| path.starts_with(base))
                .map(|base| base.components().count())
                .max();
            if let Some(depth) = depth {
                for (root, r) in self.roots.iter().enumerate() {
                    for base in [&r.git, &r.common] {
                        if base.components().count() == depth {
                            if let Ok(relative) = path.strip_prefix(base) {
                                if metadata_changed(relative)
                                    && (base == &r.git || shared_metadata(relative))
                                {
                                    changed.insert(root);
                                    // A child's HEAD/ref change can change the parent's gitlink.
                                    // Ordinary child edits and index changes do not.
                                    if head_changed(relative) {
                                        let mut parent = r.parent;
                                        while let Some(index) = parent {
                                            changed.insert(index);
                                            parent = self.roots[index].parent;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let owner = self
                .roots
                .iter()
                .enumerate()
                .filter(|(_, r)| path.starts_with(&r.worktree))
                .max_by_key(|(_, r)| r.worktree.components().count());
            if let Some((root, r)) = owner {
                let relative = path.strip_prefix(&r.worktree).unwrap();
                // Do not treat an undiscovered nested repository's object database
                // as working files, but notice replacement of a .git marker itself.
                if !relative.components().any(|part| part.as_os_str() == ".git")
                    || relative == Path::new(".git")
                {
                    changed.insert(root);
                }
            }
            // Removing/renaming a directory containing a root affects that root too.
            for (root, r) in self.roots.iter().enumerate() {
                if r.worktree.starts_with(path) {
                    changed.insert(root);
                }
            }
        }
        changed
    }
}

fn head_changed(path: &Path) -> bool {
    path.as_os_str().is_empty()
        || path == Path::new("HEAD")
        || path == Path::new("packed-refs")
        || path.starts_with("refs")
        || path.starts_with("reftable")
}

fn metadata_changed(path: &Path) -> bool {
    if path
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().ends_with(b".lock"))
    {
        return false;
    }
    head_changed(path)
        || [
            "index",
            "config",
            "config.worktree",
            "commondir",
            "gitdir",
            "shallow",
            "info/exclude",
            "info/attributes",
            "info/sparse-checkout",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
        ]
        .iter()
        .any(|known| path == Path::new(known))
}

fn shared_metadata(path: &Path) -> bool {
    // A linked worktree uses the common object/ref store, but has its own
    // HEAD, index, operation files, and worktree-specific configuration.
    head_changed(path) && path != Path::new("HEAD")
        || ["config", "shallow", "info/exclude", "info/attributes"]
            .iter()
            .any(|known| path == Path::new(known))
}

#[derive(Clone, Copy)]
struct Burst {
    first: Instant,
    last: Instant,
}

#[derive(Default)]
struct Events {
    dirty: BTreeMap<usize, Burst>,
    error: String,
}

impl Events {
    fn mark(&mut self, roots: BTreeSet<usize>, now: Instant) {
        for root in roots {
            self.dirty
                .entry(root)
                .and_modify(|burst| burst.last = now)
                .or_insert(Burst {
                    first: now,
                    last: now,
                });
        }
    }

    fn take(&mut self, scope: &BTreeSet<usize>, now: Instant, immediate: bool) -> BTreeSet<usize> {
        let due = self
            .dirty
            .iter()
            .filter(|(root, burst)| {
                scope.contains(root)
                    && (immediate
                        || now.saturating_duration_since(burst.last) >= DEBOUNCE
                        || now.saturating_duration_since(burst.first) >= MAX_DELAY)
            })
            .map(|(&root, _)| root)
            .collect::<BTreeSet<_>>();
        self.dirty.retain(|root, _| !due.contains(root));
        due
    }
}

pub(super) struct Watch {
    // Native handles are stopped by Drop when the workspace/panel is replaced.
    _watcher: Option<RecommendedWatcher>,
    events: Arc<Mutex<Events>>,
    roots: Vec<PathBuf>,
}

impl Watch {
    pub(super) fn new(c: &Context) -> Self {
        let paths = Paths::new(c);
        let targets = paths.targets();
        let events = Arc::new(Mutex::new(Events::default()));
        let received = events.clone();
        let watcher = RecommendedWatcher::new(
            move |event: notify::Result<FileEvent>| {
                let mut state = received.lock().unwrap_or_else(|e| e.into_inner());
                let roots = match event {
                    Ok(event) => paths.changed(&event),
                    Err(error) => {
                        state.error = format!(
                            "File watching: {error}. A 30-second scan will keep changes updated"
                        );
                        paths.all()
                    }
                };
                state.mark(roots, Instant::now());
            },
            Config::default().with_follow_symlinks(false),
        );
        let mut watcher = match watcher {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                events.lock().unwrap().error =
                    format!("File watching unavailable: {error}. Refreshing every 30 seconds");
                None
            }
        };
        if let Some(watcher) = &mut watcher {
            for path in targets {
                if let Err(error) = watcher.watch(&path, RecursiveMode::Recursive) {
                    events.lock().unwrap().error = format!(
                        "File watching: {error}. A 30-second scan will keep changes updated"
                    );
                }
            }
        }
        Self {
            _watcher: watcher,
            events,
            roots: c.repos.iter().map(|r| r.path.clone()).collect(),
        }
    }

    pub(super) fn matches(&self, c: &Context) -> bool {
        self.roots.iter().eq(c.repos.iter().map(|r| &r.path))
    }

    pub(super) fn take(
        &self,
        scope: &BTreeSet<usize>,
        now: Instant,
        immediate: bool,
    ) -> BTreeSet<usize> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take(scope, now, immediate)
    }

    pub(super) fn error(&self) -> String {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }
}

#[cfg(test)]
mod tests;
