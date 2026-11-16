use super::engine::*;
use anyhow::{ensure, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

const LIMIT: u64 = 64 * 1024 * 1024;

struct Lock(PathBuf);

impl Lock {
    fn acquire(path: &Path) -> Result<Self> {
        let path = path.with_extension("lock");
        OpenOptions::new().write(true).create_new(true).open(&path)
            .with_context(|| format!("Cannot lock merge draft {}. Wait for the saving session; if it has exited, remove this stale lock", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Native(Vec<u8>);

impl Native {
    fn new(path: &Path) -> Self {
        #[cfg(unix)]
        {
            Self(path.as_os_str().as_encoded_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self(
                path.as_os_str()
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Identity {
    repo: Native,
    git_dir: Native,
    path: Native,
    stages: [Option<(String, String)>; 3],
    head: Vec<u8>,
    operation: Option<(String, String, String)>,
}

impl Identity {
    fn validate_document(&self, repo: &Path, document: &Document) -> Result<()> {
        ensure!(
            self.repo == Native::new(&fs::canonicalize(repo)?)
                && self.git_dir == Native::new(&fs::canonicalize(git_path(repo, ".")?)?)
                && self.path == Native::new(&document.file.path)
                && self.stages == document.file.stages.0
                && self.operation
                    == document.op.as_ref().map(|op| (
                        op.name.into(),
                        op.head.clone(),
                        op.other.clone(),
                    )),
            "Repository or document identity changed; saved draft was not overwritten"
        );
        Ok(())
    }

    pub(super) fn read(repo: &Path, document: &Document) -> Result<Self> {
        ensure!(
            conflicts(document.file.root, repo)?.contains(&document.file)
                && operation(repo)? == document.op,
            "Conflict stages or operation changed; keep the draft in the editor"
        );
        Ok(Self {
            repo: Native::new(&fs::canonicalize(repo)?),
            git_dir: Native::new(&fs::canonicalize(git_path(repo, ".")?)?),
            path: Native::new(&document.file.path),
            stages: document.file.stages.0.clone(),
            head: checked(repo, &["rev-parse", "--verify", "HEAD"])?,
            operation: document
                .op
                .as_ref()
                .map(|op| (op.name.into(), op.head.clone(), op.other.clone())),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Decision {
    base: String,
    current: String,
    incoming: String,
    result: String,
    pending: [bool; 2],
    continuation: Option<String>,
}

impl Decision {
    fn continuation(chunk: &Chunk) -> Option<String> {
        FileSide::ALL
            .into_iter()
            .find(|&side| chunk.pending(side))
            .map(|side| {
                let mut probe = chunk.clone();
                probe.decide(side, true);
                probe.result
            })
    }

    fn new(chunk: &Chunk) -> Self {
        Self {
            base: chunk.base.clone(),
            current: chunk.left.clone(),
            incoming: chunk.right.clone(),
            result: chunk.result.clone(),
            pending: FileSide::ALL.map(|side| chunk.pending(side)),
            continuation: Self::continuation(chunk),
        }
    }

    fn replay(&self, chunk: &Chunk) -> Result<Chunk> {
        ensure!(
            self.base == chunk.base && self.current == chunk.left && self.incoming == chunk.right,
            "Merge source chunks changed"
        );
        if self.pending == [false, false] {
            let mut restored = chunk.clone();
            restored.resolve(self.result.clone());
            return Ok(restored);
        }
        for accept_current in [false, true] {
            for accept_incoming in [false, true] {
                let mut restored = chunk.clone();
                for (slot, accept) in [accept_current, accept_incoming].into_iter().enumerate() {
                    let side = FileSide::ALL[slot];
                    if !self.pending[slot] && restored.pending(side) {
                        restored.decide(side, accept);
                    }
                }
                restored.result = self.result.clone();
                if FileSide::ALL.map(|side| restored.pending(side)) == self.pending
                    && Self::continuation(&restored) == self.continuation
                {
                    return Ok(restored);
                }
            }
        }
        anyhow::bail!("Saved merge decisions cannot be replayed safely")
    }
}

#[derive(Serialize, Deserialize)]
struct Draft {
    version: u32,
    identity: Identity,
    chunks: Vec<Decision>,
}

fn location(state: &Path, identity: &Identity) -> Result<PathBuf> {
    let hash = serde_json::to_vec(identity)?
        .into_iter()
        .fold(0xcbf29ce484222325u64, |value, byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    Ok(state.join("merge-drafts").join(format!("{hash:016x}.json")))
}

fn read(path: &Path) -> Result<Option<Draft>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        file.metadata()?.len() <= LIMIT,
        "Saved merge draft exceeds 64 MiB"
    );
    let draft: Draft = serde_json::from_reader(file)?;
    ensure!(draft.version == 1, "Unsupported merge draft version");
    Ok(Some(draft))
}

fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn save(
    state: &Path,
    repo: &Path,
    document: &Document,
    identity: &Identity,
) -> Result<()> {
    identity.validate_document(repo, document)?;
    let path = location(state, identity)?;
    let parent = path
        .parent()
        .context("Merge draft has no state directory")?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    ensure!(
        !fs::symlink_metadata(parent)?.is_symlink()
            && !fs::canonicalize(parent)?.starts_with(fs::canonicalize(repo)?),
        "Set IGIT_STATE_DIR to a private directory outside the repository"
    );
    let _lock = Lock::acquire(&path)?;
    if let Some(previous) =
        read(&path).with_context(|| format!("Cannot read saved draft {}", path.display()))?
    {
        ensure!(
            previous.identity == *identity,
            "A stale merge draft is retained at {}; preserve or move it before saving a new draft",
            path.display()
        );
    }
    let draft = Draft {
        version: 1,
        identity: identity.clone(),
        chunks: document.chunks.iter().map(Decision::new).collect(),
    };
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temp.as_file_mut(), &draft)?;
    ensure!(
        temp.as_file().metadata()?.len() <= LIMIT,
        "Merge draft exceeds 64 MiB; editor retained"
    );
    temp.as_file_mut().flush()?;
    temp.as_file().sync_all()?;
    identity.validate_document(repo, document)?;
    temp.persist(&path).map_err(|error| error.error)?;
    sync_dir(parent)
}

pub(super) fn load(state: &Path, repo: &Path, document: &mut Document) -> Result<bool> {
    let identity = Identity::read(repo, document)?;
    let path = location(state, &identity)?;
    let Some(draft) =
        read(&path).with_context(|| format!("Cannot read merge draft {}", path.display()))?
    else {
        return Ok(false);
    };
    ensure!(
        draft.identity == identity,
        "Saved merge draft is stale (repository, path, stages or operation changed). It was retained at {}; no files were changed",
        path.display()
    );
    ensure!(
        draft.chunks.len() == document.chunks.len(),
        "Saved merge chunks changed; draft retained at {}",
        path.display()
    );
    let restored = draft
        .chunks
        .iter()
        .zip(&document.chunks)
        .map(|(decision, chunk)| decision.replay(chunk))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        Identity::read(repo, document)? == identity,
        "Merge sources changed while loading the draft"
    );
    document.chunks = restored;
    Ok(true)
}

pub(super) fn remove(state: &Path, identity: &Identity) -> Result<()> {
    let path = location(state, identity)?;
    if !path.exists() {
        return Ok(());
    }
    let _lock = Lock::acquire(&path)?;
    if let Some(draft) = read(&path)? {
        ensure!(
            draft.identity == *identity,
            "Saved merge draft changed; it was retained"
        );
        fs::remove_file(&path)?;
        sync_dir(path.parent().unwrap())?;
    }
    Ok(())
}
