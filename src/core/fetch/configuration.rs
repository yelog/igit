use super::*;
use std::{
    collections::hash_map::DefaultHasher,
    ffi::OsString,
    fs,
    hash::{Hash, Hasher},
    io::Read,
};

#[derive(Clone, Eq, PartialEq, Hash)]
pub(super) struct Configuration {
    entries: Vec<(String, Option<String>)>,
    environment: Vec<(String, Option<OsString>)>,
}

impl Configuration {
    fn read(path: &Path) -> Result<Self> {
        let mut query = command(path);
        query
            .env_remove("GIT_CONFIG")
            .args(["config", "--includes", "--null", "--list"]);
        let output = read_output(path, "automatic fetch configuration", &mut query)?;
        ensure!(
            output.status.success(),
            "Cannot read automatic fetch configuration"
        );
        let mut entries = Vec::new();
        for item in output.stdout.split(|byte| *byte == 0) {
            if item.is_empty() {
                continue;
            }
            let item = std::str::from_utf8(item).map_err(|_| {
                anyhow::anyhow!("Unsupported automatic fetch configuration encoding")
            })?;
            let (name, value) = match item.split_once('\n') {
                Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
                None => (item.to_owned(), None),
            };
            entries.push((name, value));
        }
        let environment = [
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_SSH_VARIANT",
            "GIT_PROXY_COMMAND",
            "GIT_SSL_NO_VERIFY",
            "GIT_PROXY_SSL_NO_VERIFY",
            "GIT_SSL_CAINFO",
            "GIT_SSL_CAPATH",
            "GIT_SSL_CERT",
            "GIT_SSL_KEY",
            "GIT_SSL_CERT_PASSWORD_PROTECTED",
            "GIT_HTTP_PROXY_AUTHMETHOD",
            "GIT_EXEC_PATH",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "SSH_AUTH_SOCK",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "PATH",
            "HOME",
            "USERPROFILE",
            "XDG_CONFIG_HOME",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), std::env::var_os(name)))
        .collect();
        Ok(Self {
            entries,
            environment,
        })
    }

    pub(super) fn values<'a>(
        &'a self,
        key: &'a str,
    ) -> impl DoubleEndedIterator<Item = Option<&'a str>> {
        self.entries
            .iter()
            .filter(move |(name, _)| name == key)
            .map(|(_, value)| value.as_deref())
    }

    pub(super) fn last(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| value.as_deref())
    }

    pub(super) fn entries(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.entries
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_deref()))
    }

    pub(super) fn skip(&self, remote: &str) -> Result<bool> {
        let skip_fetch = format!("remote.{remote}.skipfetchall");
        let skip_default = format!("remote.{remote}.skipdefaultupdate");
        for (key, value) in self.entries.iter().rev() {
            if key == &skip_fetch || key == &skip_default {
                return match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    None | Some("true" | "yes" | "on" | "1") => Ok(true),
                    Some("" | "false" | "no" | "off" | "0") => Ok(false),
                    Some(value) => value.parse::<i64>().map(|number| number != 0).map_err(|_| {
                        anyhow::anyhow!(
                            "Automatic fetch skipped a remote with invalid skipFetchAll"
                        )
                    }),
                };
            }
        }
        Ok(false)
    }

    pub(super) fn environment(&self, name: &str) -> Option<&std::ffi::OsStr> {
        self.environment
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| value.as_deref())
    }
}

pub(super) struct Repository {
    pub path: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub configuration: Configuration,
}

impl Repository {
    pub(super) fn read(path: &Path) -> Result<Self> {
        process::check_read_control()?;
        let path = fs::canonicalize(path)
            .map_err(|_| anyhow::anyhow!("Cannot locate automatic fetch repository"))?;
        let git_dir = directory(&path, &["rev-parse", "--absolute-git-dir"])?;
        let common_dir = directory(
            &path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let configuration = Configuration::read(&path)?;
        Ok(Self {
            path,
            git_dir,
            common_dir,
            configuration,
        })
    }

    pub(super) fn remotes(&self) -> Result<Vec<String>> {
        process::check_read_control()?;
        Ok(self
            .configuration
            .entries()
            .filter_map(|(key, _)| {
                key.strip_prefix("remote.")
                    .and_then(|key| key.rsplit_once('.'))
                    .map(|(name, _)| name.to_owned())
                    .filter(|name| !name.is_empty())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    pub(super) fn default_remote(&self, remotes: &[String]) -> Result<Option<String>> {
        let output = read_output(
            &self.path,
            "automatic fetch branch",
            command(&self.path).args(["symbolic-ref", "-q", "HEAD"]),
        )?;
        if output.status.success() {
            let branch = std::str::from_utf8(&output.stdout)
                .ok()
                .map(|value| value.trim_end_matches(['\r', '\n']))
                .and_then(|value| value.strip_prefix("refs/heads/"));
            if let Some(remote) = branch
                .and_then(|branch| self.configuration.last(&format!("branch.{branch}.remote")))
            {
                if remotes.iter().any(|item| item == remote) {
                    return Ok(Some(remote.to_owned()));
                }
            }
        }
        Ok(if remotes.iter().any(|remote| remote == "origin") {
            Some("origin".into())
        } else if remotes.len() == 1 {
            remotes.first().cloned()
        } else {
            None
        })
    }

    pub(super) fn target(&self, remote: &str, branch: Option<&str>) -> Result<Target> {
        let branch = branch
            .map(|branch| {
                let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
                check_ref(&self.path, &format!("refs/heads/{branch}"), false)?;
                Ok::<_, anyhow::Error>(branch.to_owned())
            })
            .transpose()?;
        let output = self.endpoints(remote, branch.is_some())?;
        let text = std::str::from_utf8(&output)
            .map_err(|_| anyhow::anyhow!("Unsupported automatic fetch endpoint encoding"))?;
        let urls: Vec<_> = text.trim_end_matches('\n').split('\n').collect();
        ensure!(
            !urls[0].is_empty(),
            "Automatic fetch skipped a remote without a usable endpoint"
        );
        ensure!(
            branch.is_none() || urls.len() == 1,
            "Automatic fetch skipped a remote with multiple push URLs"
        );
        ensure!(
            !self.configuration.entries().any(|(key, value)| {
                (key == format!("remote.{remote}.url")
                    || branch.is_some() && key == format!("remote.{remote}.pushurl")
                    || key.starts_with("url."))
                    && (key.chars().any(char::is_control)
                        || value.is_some_and(|value| value.chars().any(char::is_control)))
            }),
            "Automatic fetch skipped an invalid endpoint or URL rewrite"
        );
        let mut endpoint = urls[0].to_owned();
        ensure!(
            !endpoint.chars().any(char::is_control),
            "Automatic fetch skipped an invalid endpoint"
        );
        let transport = transport::validate(&self.configuration, remote, &endpoint)?;
        if transport == transport::Transport::File && !endpoint.starts_with("file://") {
            let path = Path::new(&endpoint);
            endpoint = if path.is_absolute() {
                path.to_path_buf()
            } else {
                let output = read_output(
                    &self.path,
                    "automatic fetch relative endpoint",
                    command(&self.path).args(["rev-parse", "--show-toplevel"]),
                )?;
                let base = if output.status.success() {
                    let mut bytes = output.stdout;
                    if bytes.last() == Some(&b'\n') {
                        bytes.pop();
                    }
                    #[cfg(windows)]
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    super::super::os_path(bytes)
                } else {
                    self.git_dir.clone()
                };
                base.join(path)
            }
            .into_os_string()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Unsupported automatic fetch endpoint encoding"))?;
        }
        ensure!(
            !self.configuration.entries().any(|(key, value)| {
                key.starts_with("url.")
                    && key.ends_with(".insteadof")
                    && value.is_some_and(|prefix| endpoint.starts_with(prefix))
            }),
            "Automatic fetch skipped a chained URL rewrite; the endpoint is ambiguous"
        );
        let refspecs = if branch.is_none() {
            let key = format!("remote.{remote}.fetch");
            let specs = self
                .configuration
                .values(&key)
                .map(|value| value.unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            validate_refspecs(&self.path, &specs)?;
            specs
        } else {
            Vec::new()
        };
        let mut fingerprint = DefaultHasher::new();
        self.configuration.hash(&mut fingerprint);
        endpoint.hash(&mut fingerprint);
        refspecs.hash(&mut fingerprint);
        Ok(Target {
            key: Key {
                common_dir: self.common_dir.clone(),
                remote: remote.to_owned(),
                fingerprint: fingerprint.finish(),
                branch,
            },
            path: self.path.clone(),
            endpoint,
            git_dir: self.git_dir.clone(),
            configuration: self.configuration.clone(),
            refspecs,
            transport,
            automatic: true,
        })
    }

    fn endpoints(&self, remote: &str, push: bool) -> Result<Vec<u8>> {
        process::check_read_control()?;
        let directory = tempfile::tempdir()
            .map_err(|_| anyhow::anyhow!("Cannot isolate automatic fetch URL resolution"))?;
        for name in ["objects", "refs"] {
            fs::create_dir(directory.path().join(name))
                .map_err(|_| anyhow::anyhow!("Cannot isolate automatic fetch URL resolution"))?;
        }
        fs::write(
            directory.path().join("HEAD"),
            b"ref: refs/heads/igit-fetch-endpoint\n",
        )
        .map_err(|_| anyhow::anyhow!("Cannot isolate automatic fetch URL resolution"))?;
        let url_key = format!("remote.{remote}.url");
        let push_key = format!("remote.{remote}.pushurl");
        let mut config = String::from("[core]\n\tbare = true\n");
        for (key, value) in self.configuration.entries() {
            let setting = if key == url_key {
                Some(("remote", remote, "url"))
            } else if key == push_key {
                Some(("remote", remote, "pushurl"))
            } else {
                key.strip_prefix("url.").and_then(|key| {
                    key.strip_suffix(".insteadof")
                        .map(|base| ("url", base, "insteadOf"))
                        .or_else(|| {
                            key.strip_suffix(".pushinsteadof")
                                .map(|base| ("url", base, "pushInsteadOf"))
                        })
                })
            };
            let Some((section, subsection, variable)) = setting else {
                continue;
            };
            config.push_str(&format!(
                "[{section} {}]\n\t{variable}",
                quote_config(subsection)?
            ));
            if let Some(value) = value {
                config.push_str(" = ");
                config.push_str(&quote_config(value)?);
            }
            config.push('\n');
        }
        fs::write(directory.path().join("config"), config)
            .map_err(|_| anyhow::anyhow!("Cannot isolate automatic fetch URL resolution"))?;
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let mut git_dir = OsString::from("--git-dir=");
        git_dir.push(super::super::git_path(directory.path()));
        let mut query = command(directory.path());
        query
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null)
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_CONFIG")
            .arg(git_dir);
        query.args(["remote", "get-url", "--all"]);
        if push {
            query.arg("--push");
        }
        query.args(["--", remote]);
        let output = read_output(&self.path, "automatic fetch endpoint", &mut query)?;
        ensure!(
            output.status.success(),
            "Automatic fetch skipped a remote without a usable endpoint"
        );
        Ok(output.stdout)
    }
}

fn quote_config(value: &str) -> Result<String> {
    ensure!(
        !value.chars().any(char::is_control),
        "Automatic fetch skipped an invalid endpoint or URL rewrite"
    );
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn directory(path: &Path, arguments: &[&str]) -> Result<PathBuf> {
    let output = read_output(
        path,
        "automatic fetch Git directory",
        command(path).args(arguments),
    )?;
    ensure!(
        output.status.success(),
        "Cannot locate automatic fetch Git directory"
    );
    let mut bytes = output.stdout;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    #[cfg(windows)]
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    ensure!(!bytes.is_empty(), "Missing automatic fetch Git directory");
    fs::canonicalize(super::super::os_path(bytes))
        .map_err(|_| anyhow::anyhow!("Cannot resolve automatic fetch Git directory"))
}

fn check_ref(path: &Path, reference: &str, pattern: bool) -> Result<()> {
    let mut query = command(path);
    query.arg("check-ref-format");
    if pattern {
        query.arg("--refspec-pattern");
    }
    query.arg(reference);
    let output = read_output(path, "automatic fetch refspec", &mut query)?;
    ensure!(
        output.status.success(),
        "Automatic fetch skipped an invalid refspec or branch"
    );
    Ok(())
}

fn validate_refspecs(path: &Path, specs: &[String]) -> Result<()> {
    let mut positive = false;
    for spec in specs {
        process::check_read_control()?;
        if let Some(source) = spec.strip_prefix('^') {
            ensure!(
                source.starts_with("refs/") && !source.contains(':'),
                "Automatic fetch skipped an unsupported negative refspec"
            );
            check_ref(path, source, true)?;
            continue;
        }
        let spec = spec.strip_prefix('+').unwrap_or(spec);
        let (source, destination) = spec.split_once(':').ok_or_else(|| {
            anyhow::anyhow!("Automatic fetch requires an explicit remote-tracking refspec")
        })?;
        ensure!(
            destination.starts_with("refs/remotes/"),
            "Automatic fetch skipped an unsafe refspec; only refs/remotes/ destinations are supported"
        );
        ensure!(
            source.starts_with("refs/") || source == "HEAD" || valid_oid(source),
            "Automatic fetch skipped an unsupported refspec source"
        );
        if source != "HEAD" && !valid_oid(source) {
            check_ref(path, source, true)?;
        }
        check_ref(path, destination, true)?;
        ensure!(
            source.matches('*').count() == destination.matches('*').count(),
            "Automatic fetch skipped an invalid wildcard refspec"
        );
        positive = true;
    }
    ensure!(
        positive,
        "Automatic fetch requires a safe remote-tracking refspec"
    );
    Ok(())
}

pub(super) fn protect_refs(target: &Target) -> Result<()> {
    ensure!(
        target.configuration.last("extensions.refstorage") != Some("reftable"),
        "Automatic fetch skipped unsupported symbolic-ref storage"
    );
    let output = read_output(
        &target.path,
        "automatic fetch HEAD safety",
        command(&target.path).args(["symbolic-ref", "-q", "HEAD"]),
    )?;
    ensure!(
        !output.status.success() || !output.stdout.starts_with(b"refs/remotes/"),
        "Automatic fetch skipped a checked-out remote-tracking ref"
    );
    let root = target.key.common_dir.join("refs/remotes");
    let mut pending = vec![root];
    let mut visited = 0usize;
    while let Some(path) = pending.pop() {
        process::check_read_control()?;
        visited += 1;
        ensure!(
            visited <= 100_000,
            "Automatic fetch reference safety scan exceeded its limit"
        );
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => bail!("Cannot inspect automatic fetch reference safety"),
        };
        ensure!(
            !metadata.file_type().is_symlink(),
            "Automatic fetch skipped a filesystem-linked remote-tracking ref"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)
                .map_err(|_| anyhow::anyhow!("Cannot inspect automatic fetch reference safety"))?
            {
                let entry = entry.map_err(|_| {
                    anyhow::anyhow!("Cannot inspect automatic fetch reference safety")
                })?;
                pending.push(entry.path());
            }
        } else if metadata.is_file() {
            let mut bytes = Vec::new();
            fs::File::open(&path)
                .and_then(|file| file.take(8193).read_to_end(&mut bytes))
                .map_err(|_| anyhow::anyhow!("Cannot inspect automatic fetch symbolic refs"))?;
            ensure!(
                bytes.len() <= 8192,
                "Automatic fetch skipped an oversized remote-tracking ref"
            );
            if let Some(reference) = bytes.strip_prefix(b"ref: ") {
                let reference = std::str::from_utf8(reference)
                    .map_err(|_| anyhow::anyhow!("Invalid automatic fetch symbolic ref"))?
                    .trim_end_matches(['\r', '\n']);
                ensure!(
                    reference.starts_with("refs/remotes/"),
                    "Automatic fetch skipped a symbolic ref outside refs/remotes/"
                );
                check_ref(&target.path, reference, false)?;
            }
        } else {
            bail!("Automatic fetch skipped unsupported remote-tracking ref storage");
        }
    }
    Ok(())
}
