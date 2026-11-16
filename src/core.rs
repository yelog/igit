pub(crate) mod fetch;
pub(crate) mod hosting;
pub(crate) mod loading;
pub(crate) mod paths;
pub(crate) mod process;
pub(crate) mod revision;
pub(crate) mod schedule;
mod state;
pub(crate) mod strategy;
pub(crate) mod update;

use anyhow::{bail, Context as _, Result};
use std::{
    collections::{BTreeSet, HashMap},
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

pub(crate) use process::read_output;

#[derive(Clone, Debug)]
pub struct Repo {
    pub path: PathBuf,
    pub name: String,
    pub branch: String,
    pub submodule: bool,
    pub parent: Option<usize>,
}
impl Repo {
    pub(crate) fn refresh_branch(&mut self) {
        self.branch = branch(&self.path);
    }
}
#[derive(Clone, Debug)]
pub struct Context {
    pub repos: Vec<Repo>,
    pub scope: BTreeSet<usize>,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
}

pub fn command(path: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(git_path(path))
        .args(["--no-pager", "-c", "color.ui=false"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_PREFIX")
        .env_remove("GIT_NAMESPACE")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "false")
        .env("GCM_INTERACTIVE", "Never")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    c
}

pub(crate) fn git_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            match prefix.kind() {
                Prefix::VerbatimDisk(drive) => {
                    let mut normalized = PathBuf::from(format!("{}:\\", char::from(drive)));
                    extend_without_root(&mut normalized, components);
                    return normalized;
                }
                Prefix::VerbatimUNC(server, share) => {
                    let mut normalized = PathBuf::from(r"\\");
                    normalized.push(server);
                    normalized.push(share);
                    extend_without_root(&mut normalized, components);
                    return normalized;
                }
                _ => {}
            }
        }
    }
    path.to_path_buf()
}

#[cfg(windows)]
fn extend_without_root<'a>(
    path: &mut PathBuf,
    components: impl Iterator<Item = std::path::Component<'a>>,
) {
    for component in components {
        if !matches!(component, std::path::Component::RootDir) {
            path.push(component.as_os_str());
        }
    }
}
pub fn git_bytes(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = command(path);
    cmd.args(args);
    let o = if let Some(operation) = read_operation(args) {
        let started = Instant::now();
        let output = read_output(path, operation, &mut cmd)?;
        if !output.status.success() {
            bail!(
                "Git {operation} failed in {} after {:.3}s ({}); refresh to retry",
                path.display(),
                started.elapsed().as_secs_f64(),
                output.status
            );
        }
        output
    } else {
        cmd.output()
            .with_context(|| format!("Git launch: {}", path.display()))?
    };
    if !o.status.success() {
        bail!(
            "{}",
            String::from_utf8_lossy(&o.stderr)
                .trim()
                .to_owned()
                .if_empty("Git command failed")
        )
    }
    Ok(o.stdout)
}

pub(crate) fn read_operation<'a>(mut args: &[&'a str]) -> Option<&'a str> {
    loop {
        let (option, rest) = args.split_first()?;
        match *option {
            "-c" | "--config-env" => {
                rest.first()?;
                args = &rest[1..];
            }
            "--no-pager"
            | "--no-optional-locks"
            | "--no-replace-objects"
            | "--literal-pathspecs"
            | "--no-literal-pathspecs"
            | "--glob-pathspecs"
            | "--noglob-pathspecs"
            | "--icase-pathspecs"
            | "--no-lazy-fetch" => args = rest,
            value
                if value.starts_with("-c") && value.len() > 2
                    || value.starts_with("--config-env=") =>
            {
                args = rest;
            }
            _ => break,
        }
    }
    let (&operation, options) = args.split_first()?;
    if options
        .iter()
        .any(|option| *option == "--output" || option.starts_with("--output="))
    {
        return None;
    }
    let read = match operation {
        "cat-file" | "check-attr" | "check-ignore" | "check-ref-format" | "cherry" | "describe"
        | "diff" | "diff-files" | "diff-index" | "diff-tree" | "for-each-ref" | "log"
        | "ls-files" | "ls-remote" | "ls-tree" | "merge-base" | "name-rev" | "rev-list"
        | "rev-parse" | "show" | "show-ref" | "status" | "var" | "version" => true,
        "--version" | "--exec-path" => options.is_empty(),
        "config" => config_read(options),
        "remote" => {
            options.is_empty()
                || matches!(options, ["-v"] | ["--verbose"])
                || options.first() == Some(&"get-url")
        }
        "symbolic-ref" => {
            let mut names = 0;
            let mut literal = false;
            for option in options {
                if literal || !option.starts_with('-') {
                    names += 1;
                } else if *option == "--" {
                    literal = true;
                } else if !matches!(
                    *option,
                    "-q" | "--quiet" | "--short" | "--recurse" | "--no-recurse"
                ) {
                    return None;
                }
            }
            names == 1
        }
        "hash-object" => !options.iter().any(|option| {
            option.starts_with('-') && !option.starts_with("--") && option.contains('w')
        }),
        "branch" => options.is_empty() || options == ["--show-current"],
        "tag" => options.is_empty(),
        "stash" => matches!(options.first().copied(), Some("list" | "show")),
        "reflog" => {
            options.is_empty()
                || matches!(options.first().copied(), Some("show" | "list" | "exists"))
        }
        _ => false,
    };
    read.then_some(operation)
}

fn config_read(args: &[&str]) -> bool {
    if matches!(args.first().copied(), Some("get" | "list")) {
        return true;
    }
    let mut args = args.iter();
    let mut action = false;
    let mut names = 0;
    while let Some(&option) = args.next() {
        match option {
            "--get" | "--get-all" | "--get-regexp" | "--get-urlmatch" | "--list" | "-l" => {
                action = true;
            }
            "--local" | "--global" | "--system" | "--worktree" | "--includes" | "--no-includes"
            | "--null" | "-z" | "--name-only" | "--show-origin" | "--show-scope"
            | "--fixed-value" | "--bool" | "--int" | "--bool-or-int" | "--bool-or-str"
            | "--path" | "--expiry-date" => (),
            "--type" | "--default" | "--file" | "-f" | "--blob" => {
                if args.next().is_none() {
                    return false;
                }
            }
            "--" => {
                names += args.count();
                break;
            }
            value
                if value.starts_with("--type=")
                    || value.starts_with("--default=")
                    || value.starts_with("--file=")
                    || value.starts_with("--blob=") => {}
            value if value.starts_with('-') => return false,
            _ => names += 1,
        }
    }
    action || names == 1
}
trait Empty {
    fn if_empty(self, s: &str) -> String;
}
impl Empty for String {
    fn if_empty(self, s: &str) -> String {
        if self.is_empty() {
            s.into()
        } else {
            self
        }
    }
}
pub fn git(path: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git_bytes(path, args)?)
        .trim_end_matches(['\n', '\r'])
        .into())
}
pub(crate) fn redact_remote_error(message: &str, url: &str) -> String {
    let mut text = if url.is_empty() {
        message.to_owned()
    } else {
        message.replace(url, "<remote>")
    };
    for word in message.split_whitespace() {
        if word.contains("://") && word.contains('@') {
            text = text.replace(word, "<remote>");
        }
    }
    text
}
#[cfg(unix)]
fn os_path(v: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(v))
}
#[cfg(not(unix))]
fn os_path(v: Vec<u8>) -> PathBuf {
    String::from_utf8_lossy(&v).into_owned().into()
}
pub(crate) fn revpath(p: &Path, what: &str) -> Option<PathBuf> {
    read_revpath(p, what).ok().flatten()
}

fn read_revpath(p: &Path, what: &str) -> Result<Option<PathBuf>> {
    let output = read_output(p, "repository path", command(p).args(["rev-parse", what]))?;
    if !output.status.success() {
        return Ok(None);
    }
    let mut b = output.stdout;
    if b.last() == Some(&b'\n') {
        b.pop();
    }
    if b.is_empty() {
        return Ok(None);
    }
    Ok(fs::canonicalize(os_path(b)).ok())
}
fn skip(n: &OsStr) -> bool {
    matches!(
        n.to_str(),
        Some(
            ".git"
                | ".hg"
                | ".svn"
                | "node_modules"
                | ".venv"
                | "venv"
                | "target"
                | "dist"
                | "build"
                | ".cache"
                | ".gradle"
                | ".next"
                | ".idea"
                | "Library"
                | ".Trash"
                | ".cargo"
                | ".rustup"
        )
    )
}
pub(crate) fn branch_label(reference: &str, head: Option<&str>) -> String {
    if reference == "HEAD" {
        let oid = head.unwrap_or("?");
        format!("detached@{}", oid.get(..8).unwrap_or(oid))
    } else {
        reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .into()
    }
}
pub fn branch(p: &Path) -> String {
    match git(p, &["symbolic-ref", "-q", "HEAD"]) {
        Ok(reference) => branch_label(&reference, None),
        Err(_) => branch_label(
            "HEAD",
            git(p, &["rev-parse", "--verify", "HEAD"]).ok().as_deref(),
        ),
    }
}
impl Context {
    pub fn discover(path: &Path) -> Result<Self> {
        process::check_read_control()?;
        let workspace =
            fs::canonicalize(path).with_context(|| format!("Workspace: {}", path.display()))?;
        if !workspace.is_dir() {
            bail!("Not a directory: {}", workspace.display())
        }
        let mut roots = BTreeSet::new();
        if let Some(r) = read_revpath(&workspace, "--show-toplevel")? {
            roots.insert(r);
        }
        let mut todo = vec![workspace.clone()];
        while let Some(dir) = todo.pop() {
            process::check_read_control()?;
            if dir.join(".git").exists() {
                if let Some(r) = read_revpath(&dir, "--show-toplevel")? {
                    roots.insert(r);
                }
            }
            // Bare repositories are push targets, never editable working trees.
            if dir.join("HEAD").is_file()
                && dir.join("objects").is_dir()
                && dir.join("refs").is_dir()
            {
                continue;
            }
            let Ok(items) = fs::read_dir(dir) else {
                continue;
            };
            for e in items.flatten() {
                process::check_read_control()?;
                if !skip(&e.file_name()) && e.file_type().is_ok_and(|t| t.is_dir()) {
                    todo.push(e.path())
                }
            }
        }
        let paths: Vec<_> = roots.into_iter().collect();
        let idx: HashMap<_, _> = paths
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, p)| (p, i))
            .collect();
        let mut count = HashMap::new();
        for p in &paths {
            *count
                .entry(p.file_name().unwrap_or_default().to_os_string())
                .or_insert(0) += 1;
        }
        let repos = paths
            .iter()
            .map(|p| {
                process::check_read_control()?;
                let base = p.file_name().unwrap_or_default();
                let sup = read_revpath(p, "--show-superproject-working-tree")?;
                let reference = read_output(
                    p,
                    "current branch",
                    command(p).args(["symbolic-ref", "-q", "HEAD"]),
                )?;
                let branch = if reference.status.success() {
                    branch_label(
                        String::from_utf8_lossy(&reference.stdout).trim_end_matches(['\n', '\r']),
                        None,
                    )
                } else {
                    let head = read_output(
                        p,
                        "current commit",
                        command(p).args(["rev-parse", "--verify", "HEAD"]),
                    )?;
                    branch_label(
                        "HEAD",
                        head.status
                            .success()
                            .then(|| {
                                String::from_utf8_lossy(&head.stdout)
                                    .trim_end_matches(['\n', '\r'])
                                    .to_owned()
                            })
                            .as_deref(),
                    )
                };
                Ok(Repo {
                    path: p.clone(),
                    name: if count[base] > 1 {
                        p.strip_prefix(&workspace)
                            .unwrap_or(p)
                            .display()
                            .to_string()
                    } else {
                        base.to_string_lossy().into()
                    },
                    branch,
                    submodule: sup.is_some(),
                    parent: sup.and_then(|q| idx.get(&q).copied()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        process::check_read_control()?;
        let state_base = state::directory()?;
        // Hash canonical workspace path using deterministic FNV-1a for file-system-safe names.
        let id = workspace
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .fold(0xcbf29ce484222325u64, |h, x| {
                (h ^ u64::from(*x)).wrapping_mul(0x100000001b3)
            });
        let state_dir = state_base.join(format!("{id:016x}"));
        Ok(Self {
            scope: (0..repos.len()).collect(),
            repos,
            workspace,
            state_dir,
        })
    }
}
