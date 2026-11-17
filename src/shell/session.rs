use super::*;
use crate::core::state::Native;
use crate::ui::panes::SplitId;
use anyhow::{ensure, Context as _};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const MAX_SIZE: u64 = 1024 * 1024;
const MAX_WORKSETS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Identity {
    path: Native,
    storage: Native,
    token: String,
}

impl Identity {
    fn read(repo: &Repo) -> Result<Self> {
        let marker = repo.path.join(".git");
        let storage = if marker.is_dir() {
            marker
        } else {
            ensure!(
                fs::metadata(&marker)?.len() <= 8192,
                "Git marker is too large"
            );
            let marker_text = fs::read(&marker)?;
            let value = marker_text
                .strip_prefix(b"gitdir: ")
                .context("Invalid Git worktree marker")?;
            #[cfg(unix)]
            let target = {
                use std::os::unix::ffi::OsStringExt;
                PathBuf::from(std::ffi::OsString::from_vec(
                    value.strip_suffix(b"\n").unwrap_or(value).to_vec(),
                ))
            };
            #[cfg(windows)]
            let target = PathBuf::from(std::str::from_utf8(value)?.trim_end_matches(['\r', '\n']));
            repo.path.join(target)
        };
        let storage = fs::canonicalize(storage)?;
        let metadata = fs::metadata(&storage)?;
        #[cfg(unix)]
        let token = {
            use std::os::unix::fs::MetadataExt;
            format!("{}:{}", metadata.dev(), metadata.ino())
        };
        #[cfg(windows)]
        let token = format!("{:?}", metadata.created()?);
        Ok(Self {
            path: Native::new(&fs::canonicalize(&repo.path)?),
            storage: Native::new(&storage),
            token,
        })
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Data {
    version: u8,
    revision: u64,
    roots: Option<Vec<Identity>>,
    worksets: BTreeMap<String, Vec<Identity>>,
    tab: Option<Tab>,
    layouts: BTreeMap<String, BTreeMap<SplitId, u16>>,
    log: crate::log::SavedSession,
}

pub(super) struct State {
    data: Data,
    writable: bool,
    pub(super) last_error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            data: Data {
                version: 1,
                ..Data::default()
            },
            writable: true,
            last_error: None,
        }
    }
}

fn read(directory: &Path) -> Result<Option<Data>> {
    let path = directory.join("session.json");
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.len() <= MAX_SIZE,
        "Workspace session exceeds the size limit"
    );
    let data: Data = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(data.version == 1, "Unsupported workspace session version");
    ensure!(
        data.worksets.len() <= MAX_WORKSETS,
        "Too many saved worksets"
    );
    Ok(Some(data))
}

fn selected(context: &Context, roots: &BTreeSet<usize>) -> Result<Vec<Identity>> {
    roots
        .iter()
        .map(|&root| {
            Identity::read(
                context
                    .repos
                    .get(root)
                    .context("Repository no longer exists")?,
            )
        })
        .collect()
}

fn resolve(context: &Context, saved: &[Identity]) -> (BTreeSet<usize>, usize) {
    let identities: Vec<_> = context.repos.iter().map(Identity::read).collect();
    let roots: BTreeSet<_> = identities
        .iter()
        .enumerate()
        .filter_map(|(index, identity)| {
            identity
                .as_ref()
                .ok()
                .filter(|identity| saved.contains(identity))
                .map(|_| index)
        })
        .collect();
    let missing = saved.len().saturating_sub(roots.len());
    (roots, missing)
}

impl State {
    fn save(&mut self, directory: &Path, mut data: Data) -> Result<()> {
        ensure!(
            self.writable,
            "Session is read-only after a load error; existing state was preserved"
        );
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("session.lock"))?;
        lock.try_lock()
            .context("Another IGit instance is saving this workspace")?;
        let current = read(directory)?;
        ensure!(current.as_ref().map_or(0, |data| data.revision) == self.data.revision,
            "Workspace session changed in another IGit instance; reopen the workspace before saving");
        data.revision = self
            .data
            .revision
            .checked_add(1)
            .context("Session revision overflow")?;
        let bytes = serde_json::to_vec(&data)?;
        ensure!(
            bytes.len() as u64 <= MAX_SIZE,
            "Workspace session exceeds the size limit"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(directory.join("session.json"))?;
        self.data = data;
        Ok(())
    }
}

impl Shell {
    pub(super) fn restore_session(&mut self, restore_scope: bool) {
        match read(&self.ctx.state_dir) {
            Ok(Some(data)) => self.session.data = data,
            Ok(None) => return,
            Err(error) => {
                self.session.writable = false;
                if restore_scope {
                    self.ctx.scope.clear();
                }
                self.notify(
                    NoticeLevel::Warning,
                    "Session",
                    format!("Could not restore workspace session; saved file retained. Review Roots before writing: {error:#}"),
                );
                return;
            }
        }
        if restore_scope {
            if let Some(saved) = &self.session.data.roots {
                let (roots, missing) = resolve(&self.ctx, saved);
                self.ctx.scope = roots;
                if missing > 0 {
                    self.notify(NoticeLevel::Warning, "Roots",
                        format!("{missing} saved repositories are missing or replaced. Review Roots; new repositories remain excluded."));
                }
            }
        }
        for tab in Tab::DISPLAY_ORDER {
            if let Some(sizes) = self.session.data.layouts.get(tab.name()).cloned() {
                self.layout_mut(tab).restore_sizes(sizes);
            }
        }
        if let Err(error) = self.log.restore_session(&self.session.data.log, &self.ctx) {
            self.notify(
                NoticeLevel::Warning,
                "Session",
                format!("Could not restore Log filters: {error:#}"),
            );
        }
    }

    pub(super) fn restore_last_panel(&mut self) {
        if let Some(tab) = self.session.data.tab {
            self.go(if self.tab_available(tab) { tab } else { Commit });
        }
    }

    fn session_snapshot(&self) -> Result<Data> {
        let mut data = self.session.data.clone();
        data.roots = Some(selected(&self.ctx, &self.ctx.scope)?);
        data.tab = Some(self.tab);
        data.log = if matches!(
            self.log_load,
            startup::LogLoad::Dormant | startup::LogLoad::Pending(_)
        ) {
            self.session.data.log.clone()
        } else {
            self.log.saved_session(&self.ctx)
        };
        data.layouts = Tab::DISPLAY_ORDER
            .into_iter()
            .map(|tab| (tab.name().to_owned(), self.layout(tab).saved_sizes()))
            .collect();
        if self.log_load.pending() {
            if let Some(sizes) = self.session.data.layouts.get(Log.name()).cloned() {
                data.layouts.insert(Log.name().into(), sizes);
            }
        }
        Ok(data)
    }

    pub(super) fn save_session(&mut self) {
        if let Err(error) = self
            .session_snapshot()
            .and_then(|data| self.session.save(&self.ctx.state_dir, data))
        {
            let message = format!("Could not save workspace session: {error:#}");
            self.session.last_error = Some(message.clone());
            self.notify(NoticeLevel::Warning, "Session", message);
        } else {
            self.session.last_error = None;
        }
    }

    pub(super) fn worksets_event(&mut self, event: &Event) {
        let Some(Popup::Worksets(mut dialog)) = self.pop.take() else {
            return;
        };
        let key = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => Some(key.code),
            _ => None,
        };
        if key == Some(KeyCode::Esc) {
            match dialog.mode {
                Mode::List => {
                    self.pop = Some(Popup::Scope {
                        picked: dialog.picked,
                        row: dialog.origin_row,
                    });
                }
                Mode::Name(_) | Mode::Delete(_) => {
                    dialog.mode = Mode::List;
                    dialog.message.clear();
                    self.pop = Some(Popup::Worksets(dialog));
                }
            }
            return;
        }
        let names: Vec<_> = self.session.data.worksets.keys().cloned().collect();
        match &mut dialog.mode {
            Mode::Name(name) => match event {
                Event::Paste(text) => name.extend(
                    text.chars()
                        .filter(|ch| !ch.is_control())
                        .take(80_usize.saturating_sub(name.chars().count())),
                ),
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Char(ch)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && name.chars().count() < 80 =>
                    {
                        name.push(ch)
                    }
                    KeyCode::Backspace => {
                        name.pop();
                    }
                    KeyCode::Enter => {
                        let name = name.trim().to_owned();
                        let result = (|| -> Result<()> {
                            ensure!(!name.is_empty(), "Enter a workset name");
                            ensure!(
                                !self.session.data.worksets.contains_key(&name),
                                "This name already exists; choose another name"
                            );
                            ensure!(
                                names.len() < MAX_WORKSETS,
                                "At most 32 worksets can be saved"
                            );
                            let mut data = self.session_snapshot()?;
                            data.worksets
                                .insert(name, selected(&self.ctx, &dialog.picked)?);
                            self.session.save(&self.ctx.state_dir, data)
                        })();
                        match result {
                            Ok(()) => {
                                if dialog.return_to_scope {
                                    self.pop = Some(Popup::Scope {
                                        picked: dialog.picked,
                                        row: dialog.origin_row,
                                    });
                                } else {
                                    dialog.mode = Mode::List;
                                    dialog.message.clear();
                                    self.pop = Some(Popup::Worksets(dialog));
                                }
                                return;
                            }
                            Err(error) => dialog.message = format!("{error:#}"),
                        }
                    }
                    _ => (),
                },
                _ => (),
            },
            Mode::Delete(name) => {
                if key == Some(KeyCode::Char('y')) {
                    let mut data = self.session.data.clone();
                    data.worksets.remove(name);
                    match self.session.save(&self.ctx.state_dir, data) {
                        Ok(()) => {
                            dialog.mode = Mode::List;
                            dialog.row = dialog
                                .row
                                .min(self.session.data.worksets.len().saturating_sub(1));
                            dialog.message.clear();
                        }
                        Err(error) => dialog.message = format!("{error:#}"),
                    }
                } else if matches!(key, Some(KeyCode::Char('n') | KeyCode::Enter)) {
                    dialog.mode = Mode::List;
                }
            }
            Mode::List => match key {
                Some(KeyCode::Char('j') | KeyCode::Down) => {
                    dialog.row = (dialog.row + 1).min(names.len().saturating_sub(1))
                }
                Some(KeyCode::Char('k') | KeyCode::Up) => dialog.row = dialog.row.saturating_sub(1),
                Some(KeyCode::Char('s')) => {
                    dialog.return_to_scope = false;
                    dialog.mode = Mode::Name(String::new());
                }
                Some(KeyCode::Char('d')) => {
                    if let Some(name) = names.get(dialog.row) {
                        dialog.mode = Mode::Delete(name.clone());
                    }
                }
                Some(KeyCode::Enter) => {
                    if let Some(roots) = names
                        .get(dialog.row)
                        .and_then(|name| self.session.data.worksets.get(name))
                    {
                        let (picked, missing) = resolve(&self.ctx, roots);
                        self.pop = Some(Popup::Scope { picked, row: 0 });
                        self.notice(format!("Workset loaded for review. Enter applies Roots. {missing} missing or replaced repositories."));
                        return;
                    }
                }
                _ => (),
            },
        }
        self.pop = Some(Popup::Worksets(dialog));
    }

    pub(super) fn draw_worksets(&self, f: &mut Frame, area: Rect, dialog: &Dialog) {
        let width = area.width.saturating_sub(4).min(76);
        let height = area.height.saturating_sub(2).min(18);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let block = pane("Roots worksets", true);
        let inner = block.inner(rect);
        f.render_widget(Clear, rect);
        f.render_widget(block, rect);
        let mut lines = match &dialog.mode {
            Mode::Name(name) => vec![
                Line::from("Save the checked Roots as a named workset"),
                Line::from(""),
                Line::from(format!("Name: {name}")),
                Line::from(""),
                Line::from("Enter Save · Esc Back"),
            ],
            Mode::Delete(name) => vec![
                Line::from(format!("Delete workset {name:?}?")),
                Line::from(""),
                Line::from("y Delete · n Keep · Esc Back"),
            ],
            Mode::List => {
                let mut lines = vec![
                    Line::from(
                        "Enter Load for review · s Save checked Roots · d Delete · Esc Back",
                    ),
                    Line::from(""),
                ];
                let height = height.saturating_sub(6) as usize;
                let start = (dialog.row + 1).saturating_sub(height);
                for (index, (name, roots)) in self
                    .session
                    .data
                    .worksets
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(height)
                {
                    lines.push(Line::styled(
                        format!(
                            "{} {name} · {} roots",
                            if index == dialog.row { ">" } else { " " },
                            roots.len()
                        ),
                        Style::new().fg(if index == dialog.row { ACCENT } else { FG }),
                    ));
                }
                if self.session.data.worksets.is_empty() {
                    lines.push(Line::from("No saved worksets"));
                }
                lines
            }
        };
        if !dialog.message.is_empty() {
            lines.push(Line::from(dialog.message.clone()));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

pub(super) struct Dialog {
    picked: BTreeSet<usize>,
    origin_row: usize,
    row: usize,
    return_to_scope: bool,
    mode: Mode,
    message: String,
}

enum Mode {
    List,
    Name(String),
    Delete(String),
}

impl Dialog {
    pub(super) fn new(picked: BTreeSet<usize>, row: usize, saving: bool) -> Self {
        Self {
            picked,
            origin_row: row,
            row: 0,
            return_to_scope: saving,
            mode: if saving {
                Mode::Name(String::new())
            } else {
                Mode::List
            },
            message: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_scope_excludes_new_missing_and_replaced_repositories() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["one", "two"] {
            fs::create_dir(temp.path().join(name)).unwrap();
            crate::core::git(&temp.path().join(name), &["init", "-q"]).unwrap();
        }
        let mut context = Context::discover(temp.path()).unwrap();
        context.state_dir = temp.path().join("state");
        context.scope = BTreeSet::from([0]);
        let saved = selected(&context, &context.scope).unwrap();
        fs::rename(
            context.repos[0].path.join(".git"),
            temp.path().join("old.git"),
        )
        .unwrap();
        crate::core::git(&context.repos[0].path, &["init", "-q"]).unwrap();
        assert_eq!(resolve(&context, &saved), (BTreeSet::new(), 1));
        assert_eq!(resolve(&context, &[]), (BTreeSet::new(), 0));
    }

    #[test]
    fn session_roundtrip_and_concurrent_writer_preserve_worksets() {
        let temp = tempfile::tempdir().unwrap();
        let mut first = State::default();
        let mut data = first.data.clone();
        data.roots = Some(vec![]);
        data.worksets.insert("empty".into(), vec![]);
        data.tab = Some(Log);
        first.save(temp.path(), data).unwrap();
        let loaded = read(temp.path()).unwrap().unwrap();
        assert_eq!(loaded.tab, Some(Log));
        assert_eq!(loaded.roots, Some(vec![]));
        assert!(loaded.worksets.contains_key("empty"));
        let mut stale = State::default();
        assert!(stale
            .save(temp.path(), Data::default())
            .unwrap_err()
            .to_string()
            .contains("another IGit"));
        assert!(read(temp.path())
            .unwrap()
            .unwrap()
            .worksets
            .contains_key("empty"));
        fs::write(temp.path().join("session.json"), b"broken").unwrap();
        assert!(first.save(temp.path(), first.data.clone()).is_err());
        assert_eq!(
            fs::read(temp.path().join("session.json")).unwrap(),
            b"broken"
        );
    }

    #[test]
    fn restart_restores_roots_panel_and_sizes_without_including_new_repositories() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["one", "two"] {
            fs::create_dir(temp.path().join(name)).unwrap();
            crate::core::git(&temp.path().join(name), &["init", "-q"]).unwrap();
        }
        let mut context = Context::discover(temp.path()).unwrap();
        context.state_dir = temp.path().join("state");
        context.scope = BTreeSet::from([1]);
        let chosen = context.repos[1].path.clone();
        let mut app = Shell::unloaded(context.clone(), AppConfig::default());
        app.tab = Remote;
        app.commit
            .panes
            .restore_sizes(BTreeMap::from([(SplitId::CommitPreview, 42)]));
        app.save_session();
        assert!(context.state_dir.join("session.json").exists());
        fs::create_dir(temp.path().join("aaa")).unwrap();
        crate::core::git(&temp.path().join("aaa"), &["init", "-q"]).unwrap();
        let mut discovered = Context::discover(temp.path()).unwrap();
        discovered.state_dir = context.state_dir;
        let mut restored = Shell::unloaded(discovered, AppConfig::default());
        restored.restore_session(true);
        assert_eq!(restored.ctx.scope.len(), 1);
        assert_eq!(
            restored.ctx.repos[*restored.ctx.scope.first().unwrap()].path,
            chosen
        );
        assert_eq!(restored.session.data.tab, Some(Remote));
        assert_eq!(
            restored
                .commit
                .panes
                .saved_sizes()
                .get(&SplitId::CommitPreview),
            Some(&42)
        );
        restored.ctx.scope.clear();
        restored.save_session();
        let mut again = Shell::unloaded(restored.ctx.clone(), AppConfig::default());
        again.ctx.scope = (0..again.ctx.repos.len()).collect();
        again.restore_session(true);
        assert!(again.ctx.scope.is_empty());
    }

    #[test]
    fn workset_loading_is_reviewed_and_cancel_keeps_scope() {
        let temp = tempfile::tempdir().unwrap();
        let mut context = Context::discover(temp.path()).unwrap();
        context.state_dir = temp.path().join("state");
        let mut app = Shell::unloaded(context, AppConfig::default());
        app.pop = Some(Popup::Worksets(Dialog::new(BTreeSet::new(), 0, true)));
        app.worksets_event(&Event::Paste("Focused 中文".into()));
        app.worksets_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.session.data.worksets.contains_key("Focused 中文"));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
        app.pop = Some(Popup::Worksets(Dialog::new(BTreeSet::new(), 0, false)));
        app.worksets_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(matches!(app.pop, Some(Popup::Scope { picked, .. }) if picked.is_empty()));
        assert!(app.ctx.scope.is_empty());
    }

    #[test]
    fn workset_escape_returns_to_the_list_before_the_roots_draft() {
        let temp = tempfile::tempdir().unwrap();
        let mut context = Context::discover(temp.path()).unwrap();
        context.state_dir = temp.path().join("state");
        let mut app = Shell::unloaded(context, AppConfig::default());
        app.pop = Some(Popup::Worksets(Dialog::new(BTreeSet::new(), 0, false)));
        app.worksets_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::NONE,
        )));
        assert!(matches!(
            app.pop,
            Some(Popup::Worksets(Dialog {
                mode: Mode::Name(_),
                return_to_scope: false,
                ..
            }))
        ));
        app.worksets_event(&Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(matches!(
            app.pop,
            Some(Popup::Worksets(Dialog {
                mode: Mode::List,
                return_to_scope: false,
                ..
            }))
        ));
        app.worksets_event(&Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(matches!(app.pop, Some(Popup::Scope { .. })));
    }

    #[test]
    fn corrupt_session_remains_read_only_and_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        crate::core::git(temp.path(), &["init", "-q"]).unwrap();
        let mut context = Context::discover(temp.path()).unwrap();
        context.state_dir = temp.path().join("state");
        fs::create_dir(&context.state_dir).unwrap();
        fs::write(context.state_dir.join("session.json"), "corrupt").unwrap();
        let mut app = Shell::unloaded(context, AppConfig::default());
        app.restore_session(true);
        app.save_session();
        assert_eq!(
            fs::read(app.ctx.state_dir.join("session.json")).unwrap(),
            b"corrupt"
        );
        assert!(!app.session.writable);
        assert!(app.ctx.scope.is_empty());
    }
}
