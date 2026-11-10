//! Read the same two versions that Commit reviews, without making a snapshot.
use super::{checked, cmd, exact_path, oid, Change};
use crate::core::revision::{self, Source};
use anyhow::{bail, Context as _, Result};
use std::{
    fs,
    io::Read,
    path::{Component, Path},
};

const UNTRACKED_LIMIT: usize = 96 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub revisions: [String; 2],
    pub sources: [Source; 2],
}

pub fn load(repo: &Path, change: &Change) -> Result<Comparison> {
    if !change.selectable() {
        bail!("Split preview is unavailable for ignored or unmerged paths; showing the Git patch");
    }
    if change.directory()
        || change.path.components().next().is_none()
        || change
            .path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("Split preview requires a repository-relative file path; showing the Git patch");
    }

    // Do not use :path revision syntax or a lossy display path. A literal Git
    // pathspec can also match children; exact_path filters those records out.
    let entries = exact_path(repo, None, &["ls-files", "--stage", "-z"], &change.path)?;
    let index = index_entry(&entries)?;
    if change.staged() {
        let head = oid(repo)?;
        let before = match head.as_deref() {
            Some(head) => revision::source(repo, head, &change.path)?,
            None => absent("No HEAD · unborn branch"),
        };
        let after = index_source(repo, index)?;
        return Ok(Comparison {
            revisions: [
                if head.is_some() { "HEAD" } else { "Empty tree" }.into(),
                "Index".into(),
            ],
            sources: [before, after],
        });
    }

    let untracked = change.xy == *b"??";
    let before = if untracked {
        if index.is_some() {
            bail!("The untracked path now has an index entry; refresh before reviewing it");
        }
        absent("Untracked · absent from index")
    } else {
        if index.is_none() {
            bail!("The tracked path no longer has an index entry; refresh before reviewing it");
        }
        // Intent-to-add has a real stage-0 empty blob. Read it as such, rather
        // than looking up HEAD or treating its working contents as staged.
        index_source(repo, index)?
    };
    let after = working(repo, &change.path, untracked, index.map(|(mode, _)| mode))?;
    Ok(Comparison {
        revisions: [
            if untracked { "Empty tree" } else { "Index" }.into(),
            "Working tree".into(),
        ],
        sources: [before, after],
    })
}

fn absent(description: &str) -> Source {
    Source {
        text: String::new(),
        description: description.into(),
    }
}

fn index_entry(entries: &[u8]) -> Result<Option<(&str, &str)>> {
    let mut records = entries.split(|&b| b == 0).filter(|r| !r.is_empty());
    let Some(record) = records.next() else {
        return Ok(None);
    };
    let tab = record
        .iter()
        .position(|&b| b == b'\t')
        .context("Invalid index entry; showing the Git patch")?;
    let metadata = std::str::from_utf8(&record[..tab])?;
    let mut fields = metadata.split(' ');
    let mode = fields.next().context("Missing index mode")?;
    let blob = fields.next().context("Missing index blob")?;
    if fields.next() != Some("0") || records.next().is_some() {
        bail!("Split preview requires a stage-0 index entry; resolve conflicts first");
    }
    Ok(Some((mode, blob)))
}

fn index_source(repo: &Path, entry: Option<(&str, &str)>) -> Result<Source> {
    match entry {
        Some((mode, blob)) => revision::blob(repo, blob, mode),
        None => Ok(absent("Absent from index · staged deletion")),
    }
}

fn limit(untracked: bool) -> usize {
    if untracked {
        UNTRACKED_LIMIT
    } else {
        revision::LIMIT
    }
}

fn check_size(size: u64, untracked: bool) -> Result<()> {
    if size > limit(untracked) as u64 {
        if untracked {
            bail!("Untracked split preview is limited to 96 KiB; showing the bounded Git patch");
        }
        bail!("Split preview is limited to 512 KiB per revision; showing the Git patch");
    }
    Ok(())
}

fn working(repo: &Path, path: &Path, untracked: bool, index_mode: Option<&str>) -> Result<Source> {
    // Do not traverse a parent symlink or a nested working tree. A missing
    // parent is an ordinary working deletion; a file parent is a replacement.
    let mut parent = repo.to_path_buf();
    let parts: Vec<_> = path.components().collect();
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(meta) if meta.is_dir() && !parent.join(".git").exists() => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(absent("Absent from working tree"));
            }
            _ => bail!("Split preview cannot traverse symlink parents, nested repositories, or file/directory replacements; showing the Git patch"),
        }
    }
    let path_on_disk = repo.join(path);
    let metadata = match fs::symlink_metadata(&path_on_disk) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(absent("Absent from working tree"));
        }
        Err(error) => {
            return Err(error).context("Cannot inspect working file; showing the Git patch")
        }
    };
    if metadata.is_symlink() {
        let target = fs::read_link(path_on_disk)?;
        let bytes = target
            .to_str()
            .context("Binary or non-UTF-8 symlink text; showing the Git patch")?
            .as_bytes()
            .to_vec();
        check_size(bytes.len() as u64, untracked)?;
        return revision::text(bytes, "120000");
    }
    if !metadata.is_file() || index_mode == Some("120000") {
        bail!("Split preview is unavailable for directories, special files, or emulated symlinks; showing the Git patch");
    }
    check_size(metadata.len(), untracked)?;
    let attributes = attributes(repo, path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path_on_disk)?
        .take((limit(untracked) + 1) as u64)
        .read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64, untracked)?;
    check_line_endings(repo, &attributes, &bytes)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        let output = cmd(repo, None)
            .args(["config", "--type=bool", "--get", "core.filemode"])
            .output()?;
        let filemode = if output.status.code() == Some(1) {
            true
        } else {
            String::from_utf8(checked(output)?)?.trim() == "true"
        };
        if !filemode {
            index_mode.unwrap_or("100644")
        } else if metadata.permissions().mode() & 0o100 != 0 {
            "100755"
        } else {
            "100644"
        }
    };
    #[cfg(not(unix))]
    let mode = index_mode.unwrap_or("100644");
    revision::text(bytes, mode)
}

fn attributes(repo: &Path, path: &Path) -> Result<Vec<u8>> {
    let output = checked(
        cmd(repo, None)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .args([
                "check-attr",
                "-z",
                "text",
                "eol",
                "crlf",
                "working-tree-encoding",
                "filter",
                "ident",
                "diff",
                "--",
            ])
            .arg(path)
            .output()?,
    )?;
    let fields: Vec<_> = output.split(|&b| b == 0).collect();
    if fields.len() != 22 || fields.last().is_none_or(|field| !field.is_empty()) {
        bail!("Cannot verify Git attributes; showing the Git patch");
    }
    for triple in fields[..21].chunks_exact(3) {
        if triple[0] != path.as_os_str().as_encoded_bytes() {
            bail!("Git returned attributes for a different path; showing the Git patch");
        }
        let (name, value) = (triple[1], triple[2]);
        if value == b"unspecified" {
            continue;
        }
        if name == b"diff" && value == b"unset" {
            bail!("Git marks this path as binary; showing the Git patch");
        }
        let supported = match name {
            b"text" => matches!(value, b"set" | b"unset" | b"auto"),
            b"eol" => matches!(value, b"unset" | b"lf" | b"crlf"),
            b"diff" => value == b"set",
            b"working-tree-encoding" | b"filter" | b"ident" => value == b"unset",
            _ => false,
        };
        if !supported {
            bail!(
                "The {} attribute may convert working contents; split preview does not run conversions or textconv, showing the Git patch",
                String::from_utf8_lossy(name)
            );
        }
    }
    Ok(output)
}

fn check_line_endings(repo: &Path, attributes: &[u8], bytes: &[u8]) -> Result<()> {
    let fields: Vec<_> = attributes.split(|&b| b == 0).collect();
    let attr = |name: &[u8]| {
        fields
            .chunks_exact(3)
            .find(|triple| triple[1] == name)
            .map(|triple| triple[2])
            .unwrap_or(b"unspecified")
    };
    if attr(b"text") == b"unset" {
        return Ok(());
    }
    let output = cmd(repo, None)
        .args(["config", "--type=bool-or-str", "--get", "core.autocrlf"])
        .output()?;
    let autocrlf = if output.status.code() == Some(1) {
        "false".into()
    } else {
        String::from_utf8(checked(output)?)?
            .trim()
            .to_ascii_lowercase()
    };
    if !matches!(autocrlf.as_str(), "false" | "true" | "input") {
        bail!("Cannot verify core.autocrlf conversions; showing the Git patch");
    }
    let normalize = matches!(attr(b"text"), b"set" | b"auto")
        || matches!(attr(b"eol"), b"lf" | b"crlf")
        || autocrlf != "false";
    if normalize && bytes.windows(2).any(|pair| pair == b"\r\n") {
        bail!("Git normalizes working-tree line endings; split preview does not run conversions, showing the Git patch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::backend::{
        g, status,
        tests::{commit_all, get, put, setup},
    };
    use std::{collections::BTreeMap, path::PathBuf};
    use tempfile::TempDir;

    fn fixture() -> TempDir {
        let dir = setup();
        for (key, value) in [
            ("core.autocrlf", "false"),
            ("core.filemode", "true"),
            ("core.fsmonitor", "false"),
            ("maintenance.auto", "false"),
            ("gc.auto", "0"),
            ("core.attributesFile", ".git/info/attributes"),
        ] {
            g(dir.path(), None, &["config", key, value]).unwrap();
        }
        dir
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Contents {
        File(Vec<u8>),
        Link(PathBuf),
        Directory,
        Special,
    }

    // Include .git itself: HEAD, the real index bytes, refs, and objects must
    // all survive both successful reads and fallback errors unchanged.
    fn snapshot(repo: &Path) -> BTreeMap<PathBuf, Contents> {
        fn walk(repo: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Contents>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let kind = fs::symlink_metadata(&path)
                    .unwrap_or_else(|error| {
                        panic!("snapshot metadata for {}: {error}", path.display())
                    })
                    .file_type();
                let contents = if kind.is_symlink() {
                    Contents::Link(fs::read_link(&path).unwrap())
                } else if kind.is_dir() {
                    walk(repo, &path, files);
                    Contents::Directory
                } else if kind.is_file() {
                    Contents::File(fs::read(&path).unwrap())
                } else {
                    Contents::Special
                };
                files.insert(path.strip_prefix(repo).unwrap().into(), contents);
            }
        }
        let mut files = BTreeMap::new();
        walk(repo, repo, &mut files);
        files
    }

    fn read_only(repo: &Path, change: &Change) -> Result<Comparison> {
        let before = snapshot(repo);
        let result = load(repo, change);
        assert_eq!(
            snapshot(repo),
            before,
            "preview changed Git metadata or working contents"
        );
        result
    }

    fn change(repo: &Path, path: impl AsRef<Path>) -> Change {
        status(repo)
            .unwrap()
            .into_iter()
            .find(|change| change.path == path.as_ref())
            .unwrap_or_else(|| panic!("missing change for {}", path.as_ref().display()))
    }

    fn compare(repo: &Path, path: impl AsRef<Path>) -> Comparison {
        read_only(repo, &change(repo, path)).unwrap()
    }

    fn texts(comparison: &Comparison) -> [&str; 2] {
        comparison.sources.each_ref().map(|s| s.text.as_str())
    }

    fn fallback(repo: &Path, change: &Change, explanation: &str) {
        let error = read_only(repo, change).unwrap_err().to_string();
        assert!(error.contains(explanation), "{error}");
    }

    #[test]
    fn mixed_staging_uses_head_and_real_index_and_preserves_unchecked_sibling() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "selected", "committed\n");
        put(repo, "unchecked", "sibling committed\n");
        commit_all(repo);
        put(repo, "selected", "selected staged\n");
        put(repo, "unchecked", "unchecked staged\n");
        g(repo, None, &["add", "selected", "unchecked"]).unwrap();
        put(repo, "selected", "later working bytes\r\n");
        let selected = change(repo, "selected");
        assert_eq!(selected.xy, *b"MM");
        let comparison = read_only(repo, &selected).unwrap();
        assert_eq!(comparison.revisions, ["HEAD", "Index"]);
        assert_eq!(texts(&comparison), ["committed\n", "selected staged\n"]);
        assert_eq!(get(repo, &["show", ":unchecked"]), "unchecked staged");
        assert_eq!(get(repo, &["show", "HEAD:unchecked"]), "sibling committed");

        fs::remove_file(repo.join("selected")).unwrap();
        let selected = change(repo, "selected");
        assert_eq!(selected.xy, *b"MD");
        assert_eq!(read_only(repo, &selected).unwrap(), comparison);
    }

    #[test]
    fn unstaged_additions_and_deletions_use_the_correct_absent_side() {
        let dir = fixture();
        let repo = dir.path();
        for path in ["modified", "deleted", "staged-deleted"] {
            put(repo, path, "before\n");
        }
        commit_all(repo);
        put(repo, "modified", "working\r\n");
        fs::remove_file(repo.join("deleted")).unwrap();
        g(repo, None, &["rm", "-q", "staged-deleted"]).unwrap();
        put(repo, "staged-deleted", "untracked replacement\n");
        put(repo, "untracked", "new text\n");
        put(repo, "added", "staged addition\n");
        g(repo, None, &["add", "added"]).unwrap();
        put(repo, "added", "later addition edit\n");

        let modified = compare(repo, "modified");
        assert_eq!(modified.revisions, ["Index", "Working tree"]);
        assert_eq!(texts(&modified), ["before\n", "working\r\n"]);
        assert!(modified.sources[1].description.contains("CRLF"));
        let deleted = compare(repo, "deleted");
        assert_eq!(texts(&deleted), ["before\n", ""]);
        assert!(deleted.sources[1].description.contains("Absent"));
        let deletion = change(repo, "staged-deleted");
        assert_eq!(deletion.xy, *b"D ");
        let deletion = read_only(repo, &deletion).unwrap();
        assert_eq!(deletion.revisions, ["HEAD", "Index"]);
        assert_eq!(texts(&deletion), ["before\n", ""]);
        assert!(deletion.sources[1].description.contains("staged deletion"));
        let untracked = compare(repo, "untracked");
        assert_eq!(untracked.revisions, ["Empty tree", "Working tree"]);
        assert_eq!(texts(&untracked), ["", "new text\n"]);
        assert!(untracked.sources[0].description.contains("Untracked"));
        let added = compare(repo, "added");
        assert_eq!(added.revisions, ["HEAD", "Index"]);
        assert_eq!(texts(&added), ["", "staged addition\n"]);
        assert!(added.sources[0].description.contains("Absent"));
    }

    #[test]
    fn unborn_sources_do_not_create_an_index_or_empty_tree_object() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "initial", "initial working\n");
        assert!(!repo.join(".git/index").exists());
        let untracked = compare(repo, "initial");
        assert_eq!(texts(&untracked), ["", "initial working\n"]);
        assert!(!repo.join(".git/index").exists());
        g(repo, None, &["add", "initial"]).unwrap();
        put(repo, "initial", "later working\n");
        let staged = compare(repo, "initial");
        assert_eq!(staged.revisions, ["Empty tree", "Index"]);
        assert_eq!(texts(&staged), ["", "initial working\n"]);
        assert!(staged.sources[0].description.contains("unborn"));
        assert!(oid(repo).unwrap().is_none());
    }

    #[test]
    fn intent_to_add_compares_the_empty_index_blob_to_working_text() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "intent", "not staged\n");
        g(repo, None, &["add", "-N", "intent"]).unwrap();
        let selected = change(repo, "intent");
        assert_eq!(selected.xy, *b" A");
        assert!(!selected.staged());
        let comparison = read_only(repo, &selected).unwrap();
        assert_eq!(comparison.revisions, ["Index", "Working tree"]);
        assert_eq!(texts(&comparison), ["", "not staged\n"]);
        assert!(comparison.sources[0].description.contains("Empty file"));
        assert!(oid(repo).unwrap().is_none());
    }

    #[test]
    fn existing_index_lock_does_not_block_read_only_sources() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "file", "head\n");
        commit_all(repo);
        put(repo, "file", "working\n");
        let selected = change(repo, "file");
        put(repo, ".git/index.lock", "another operation\n");
        assert_eq!(
            texts(&read_only(repo, &selected).unwrap()),
            ["head\n", "working\n"]
        );
    }

    #[test]
    fn literal_unicode_paths_keep_native_identity_and_original_text_bytes() {
        let dir = fixture();
        let repo = dir.path();
        let paths = [
            PathBuf::from("src/[literal] 中文/名.txt"),
            PathBuf::from("src/[literal] 中文/名1.txt"),
            #[cfg(unix)]
            PathBuf::from("src/*?/[literal]\t名\n.txt"),
            #[cfg(unix)]
            PathBuf::from(r"src/back\slash/名.txt"),
            #[cfg(unix)]
            PathBuf::from(":(glob)名[1].txt"),
            #[cfg(all(unix, not(target_os = "macos")))]
            PathBuf::from(super::super::raw(b"native-\xff.txt")),
        ];
        for (i, path) in paths.iter().enumerate() {
            fs::create_dir_all(repo.join(path).parent().unwrap()).unwrap();
            fs::write(repo.join(path), format!("head {i}\n")).unwrap();
        }
        commit_all(repo);
        for (i, path) in paths.iter().enumerate() {
            fs::write(repo.join(path), format!("\t索引 {i}\r\nlast")).unwrap();
        }
        g(repo, None, &["add", "-A"]).unwrap();
        for (i, path) in paths.iter().enumerate() {
            fs::write(repo.join(path), format!("working {i}\n")).unwrap();
            let comparison = compare(repo, path);
            assert_eq!(comparison.sources[0].text, format!("head {i}\n"));
            assert_eq!(comparison.sources[1].text, format!("\t索引 {i}\r\nlast"));
            assert!(comparison.sources[1].description.contains("No newline"));
        }
    }

    #[test]
    fn staged_deletion_does_not_read_a_same_named_index_descendant() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "node", "old leaf\n");
        commit_all(repo);
        fs::remove_file(repo.join("node")).unwrap();
        fs::create_dir(repo.join("node")).unwrap();
        put(repo, "node/child", "different staged leaf\n");
        g(repo, None, &["add", "-A"]).unwrap();
        let deleted = compare(repo, "node");
        assert_eq!(texts(&deleted), ["old leaf\n", ""]);
        assert!(deleted.sources[1].description.contains("Absent"));
        assert_eq!(
            texts(&compare(repo, "node/child")),
            ["", "different staged leaf\n"]
        );
    }

    #[test]
    fn untracked_limit_falls_back_without_truncating_the_split_sources() {
        let dir = fixture();
        let repo = dir.path();
        for length in [UNTRACKED_LIMIT - 1, UNTRACKED_LIMIT, UNTRACKED_LIMIT + 1] {
            fs::write(repo.join("new"), vec![b'x'; length]).unwrap();
            let selected = change(repo, "new");
            if length <= UNTRACKED_LIMIT {
                assert_eq!(
                    read_only(repo, &selected).unwrap().sources[1].text.len(),
                    length
                );
            } else {
                fallback(repo, &selected, "96 KiB");
                assert!(super::super::diff(repo, &selected)
                    .unwrap()
                    .contains("Preview truncated at 96 KiB"));
            }
            assert!(!repo.join(".git/index").exists());
        }
    }

    #[test]
    fn head_index_and_working_sources_each_have_a_512_kib_bound() {
        for side in ["head", "index", "working"] {
            let dir = fixture();
            let repo = dir.path();
            put(repo, "file", "base\n");
            commit_all(repo);
            fs::write(repo.join("file"), vec![b'x'; revision::LIMIT]).unwrap();
            if side != "working" {
                g(repo, None, &["add", "file"]).unwrap();
            }
            assert_eq!(compare(repo, "file").sources[1].text.len(), revision::LIMIT);
            fs::write(repo.join("file"), vec![b'x'; revision::LIMIT + 1]).unwrap();
            match side {
                "head" => {
                    commit_all(repo);
                    put(repo, "file", "small index\n");
                    g(repo, None, &["add", "file"]).unwrap();
                }
                "index" => {
                    g(repo, None, &["add", "file"]).unwrap();
                    put(repo, "file", "small working\n");
                }
                _ => (),
            }
            fallback(repo, &change(repo, "file"), "512 KiB");
        }
    }

    #[test]
    fn binary_and_non_utf8_sources_fall_back_on_every_side() {
        for bytes in [&b"binary\0text"[..], &b"invalid \xff text"[..]] {
            for side in ["head", "index", "working", "untracked"] {
                let dir = fixture();
                let repo = dir.path();
                if side != "untracked" {
                    put(repo, "file", "base\n");
                    commit_all(repo);
                }
                fs::write(repo.join("file"), bytes).unwrap();
                match side {
                    "head" => {
                        commit_all(repo);
                        put(repo, "file", "now text\n");
                        g(repo, None, &["add", "file"]).unwrap();
                    }
                    "index" => {
                        g(repo, None, &["add", "file"]).unwrap();
                        put(repo, "file", "working text\n");
                    }
                    _ => (),
                }
                fallback(repo, &change(repo, "file"), "Binary or non-UTF-8");
            }
        }
    }

    #[test]
    fn conversions_and_textconv_attributes_fall_back_without_running_commands() {
        for attribute in [
            "filter=unsafe",
            "working-tree-encoding=UTF-16",
            "ident",
            "crlf",
            "-diff",
            "diff=custom",
        ] {
            let dir = fixture();
            let repo = dir.path();
            put(repo, "file", "head\n");
            commit_all(repo);
            for (key, value) in [
                ("filter.unsafe.clean", "touch conversion-ran; cat"),
                ("filter.unsafe.smudge", "touch conversion-ran; cat"),
                ("filter.unsafe.process", "touch conversion-ran"),
                ("diff.custom.textconv", "touch conversion-ran; cat"),
                ("diff.custom.command", "touch conversion-ran"),
            ] {
                g(repo, None, &["config", key, value]).unwrap();
            }
            put(repo, ".gitattributes", &format!("file {attribute}\n"));
            put(repo, "file", "working text\n");
            let selected = Change {
                path: "file".into(),
                xy: *b" M",
                conflict: false,
            };
            fallback(
                repo,
                &selected,
                if attribute == "-diff" {
                    "binary"
                } else {
                    "attribute"
                },
            );
            assert!(!repo.join("conversion-ran").exists());
        }
    }

    #[test]
    fn staged_sources_ignore_working_conversions_and_do_not_execute_filters() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "file", "head\n");
        commit_all(repo);
        put(repo, "file", "index\n");
        g(repo, None, &["add", "file"]).unwrap();
        put(
            repo,
            ".gitattributes",
            "file filter=unsafe working-tree-encoding=UTF-16 ident diff=custom\n",
        );
        for key in ["filter.unsafe.clean", "diff.custom.textconv"] {
            g(repo, None, &["config", key, "touch conversion-ran; cat"]).unwrap();
        }
        fs::write(repo.join("file"), b"working\0\xff").unwrap();
        // Avoid status here: Git may run a clean filter to refresh its view of
        // an unstaged working change. The loader itself must never do so.
        let selected = Change {
            path: "file".into(),
            xy: *b"MM",
            conflict: false,
        };
        assert_eq!(
            texts(&read_only(repo, &selected).unwrap()),
            ["head\n", "index\n"]
        );
        assert!(!repo.join("conversion-ran").exists());
    }

    #[test]
    fn line_ending_rules_reject_normalized_crlf_and_allow_byte_preserving_text() {
        for (rule, autocrlf, content, supported) in [
            ("", "false", "working\r\n", true),
            ("-text", "true", "working\r\n", true),
            ("text eol=lf", "false", "working\n", true),
            ("text eol=crlf", "false", "working\n", true),
            ("text=auto", "input", "working\n", true),
            ("text eol=lf", "false", "working\r\n", false),
            ("eol=crlf", "false", "working\r\n", false),
            ("", "true", "working\r\n", false),
            ("", "input", "working\r\n", false),
        ] {
            let dir = fixture();
            let repo = dir.path();
            put(repo, "file", "head\n");
            commit_all(repo);
            put(repo, ".gitattributes", &format!("file {rule}\n"));
            g(repo, None, &["config", "core.autocrlf", autocrlf]).unwrap();
            put(repo, "file", content);
            // A normalized CRLF-only edit can disappear from porcelain; use
            // the selected row to verify the loader's conversion guard itself.
            let selected = Change {
                path: "file".into(),
                xy: *b" M",
                conflict: false,
            };
            if supported {
                assert_eq!(
                    texts(&read_only(repo, &selected).unwrap()),
                    ["head\n", content],
                    "{rule}, {autocrlf}"
                );
            } else {
                fallback(repo, &selected, "line endings");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_link_text_and_symlink_parents_are_never_traversed() {
        use std::os::unix::fs::symlink;
        let dir = fixture();
        let repo = dir.path();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("target");
        fs::write(&target, b"target bytes are not UTF-8 \0\xff").unwrap();
        symlink(&target, repo.join("link")).unwrap();
        commit_all(repo);
        fs::remove_file(repo.join("link")).unwrap();
        symlink("missing target", repo.join("link")).unwrap();
        let working = compare(repo, "link");
        assert_eq!(
            texts(&working),
            [target.to_str().unwrap(), "missing target"]
        );
        assert!(working
            .sources
            .iter()
            .all(|s| s.description.contains("link text")));
        g(repo, None, &["add", "link"]).unwrap();
        fs::remove_file(repo.join("link")).unwrap();
        symlink(&target, repo.join("link")).unwrap();
        assert_eq!(
            texts(&compare(repo, "link")),
            [target.to_str().unwrap(), "missing target"]
        );
        symlink(&target, repo.join("new-link")).unwrap();
        assert_eq!(
            texts(&compare(repo, "new-link")),
            ["", target.to_str().unwrap()]
        );
        fs::create_dir(repo.join("dir")).unwrap();
        put(repo, "dir/file", "tracked\n");
        commit_all(repo);
        fs::write(outside.path().join("file"), "outside\n").unwrap();
        fs::remove_file(repo.join("dir/file")).unwrap();
        fs::remove_dir(repo.join("dir")).unwrap();
        symlink(outside.path(), repo.join("dir")).unwrap();
        let selected = Change {
            path: "dir/file".into(),
            xy: *b" D",
            conflict: false,
        };
        fallback(repo, &selected, "symlink parents");
        assert_eq!(
            fs::read(&target).unwrap(),
            b"target bytes are not UTF-8 \0\xff"
        );
        assert_eq!(fs::read(outside.path().join("file")).unwrap(), b"outside\n");
    }

    #[cfg(unix)]
    #[test]
    fn working_mode_annotations_respect_core_filemode_and_preserve_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = fixture();
        let repo = dir.path();
        put(repo, "file", "head\n");
        fs::set_permissions(repo.join("file"), fs::Permissions::from_mode(0o644)).unwrap();
        commit_all(repo);
        put(repo, "file", "working\n");
        fs::set_permissions(repo.join("file"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(compare(repo, "file").sources[1]
            .description
            .starts_with("100755"));
        g(repo, None, &["config", "core.filemode", "false"]).unwrap();
        assert!(compare(repo, "file").sources[1]
            .description
            .starts_with("100644"));
        put(repo, "new", "untracked executable\n");
        fs::set_permissions(repo.join("new"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(compare(repo, "new").sources[1]
            .description
            .starts_with("100644"));
        assert_eq!(
            fs::metadata(repo.join("file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[cfg(unix)]
    #[test]
    fn special_files_fail_before_opening_the_working_entry() {
        use std::os::unix::net::UnixListener;
        let dir = fixture();
        let repo = dir.path();
        put(repo, "file", "head\n");
        commit_all(repo);
        fs::remove_file(repo.join("file")).unwrap();
        let _socket = UnixListener::bind(repo.join("file")).unwrap();
        let selected = Change {
            path: "file".into(),
            xy: *b" T",
            conflict: false,
        };
        fallback(repo, &selected, "special files");
    }

    #[test]
    fn directories_submodules_ignored_and_unmerged_paths_explain_the_fallback() {
        let dir = fixture();
        let repo = dir.path();
        put(repo, "node", "head\n");
        commit_all(repo);
        fs::remove_file(repo.join("node")).unwrap();
        fs::create_dir(repo.join("node")).unwrap();
        put(repo, "node/child", "working child\n");
        fallback(repo, &change(repo, "node"), "directories");

        let head = oid(repo).unwrap().unwrap();
        g(
            repo,
            None,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{head},sdk"),
            ],
        )
        .unwrap();
        fallback(repo, &change(repo, "sdk"), "submodule");
        for (xy, conflict, explanation) in [(*b"!!", false, "ignored"), (*b"UU", true, "unmerged")]
        {
            let selected = Change {
                path: "node".into(),
                xy,
                conflict,
            };
            fallback(repo, &selected, explanation);
        }
    }
}
