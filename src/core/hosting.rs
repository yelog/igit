use super::{command, git};
use anyhow::{bail, ensure, Context as _, Result};
use std::{collections::BTreeSet, path::Path};

const CUSTOM_URL: &str = "Set igit.webCommitUrl to an HTTP(S) URL containing {commit}";

pub(crate) fn commit_url(repo: &Path, oid: &str) -> Result<String> {
    ensure!(
        matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Invalid full commit ID"
    );
    if let Some(template) = setting(repo, "igit.webCommitUrl")? {
        ensure!(template.contains("{commit}"), "{CUSTOM_URL}");
        let url = template.replace("{commit}", oid);
        validate_web_url(&url)?;
        return Ok(url);
    }
    let names = git(repo, &["remote"]).context("Cannot list repository remotes")?;
    let remotes: Vec<_> = names.lines().collect();
    ensure!(!remotes.is_empty(), "No remote configured. {CUSTOM_URL}");
    let remote = if let Some(remote) = setting(repo, "igit.webRemote")? {
        ensure!(
            remotes.contains(&remote.as_str()),
            "igit.webRemote does not name a configured remote"
        );
        remote
    } else {
        choose_remote(repo, oid, &remotes)?
    };
    let url = git(repo, &["remote", "get-url", "--", &remote])
        .map_err(|_| anyhow::anyhow!("Cannot read the selected fetch remote URL"))?;
    remote_commit_url(&url, oid)
}

fn setting(repo: &Path, key: &str) -> Result<Option<String>> {
    let output = command(repo)
        .args(["config", "--get", key])
        .output()
        .context("Cannot read browser configuration")?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8(output.stdout)
                .context("Browser configuration must be UTF-8")?
                .trim()
                .to_owned(),
        )),
        Some(1) => Ok(None),
        _ => bail!("Cannot read browser configuration"),
    }
}

fn choose_remote(repo: &Path, oid: &str, remotes: &[&str]) -> Result<String> {
    let references = git(
        repo,
        &[
            "for-each-ref",
            &format!("--contains={oid}"),
            "--format=%(refname)",
            "refs/remotes",
        ],
    )?;
    let containing: BTreeSet<_> = references
        .lines()
        .filter_map(|reference| {
            let short = reference.strip_prefix("refs/remotes/")?;
            remotes
                .iter()
                .copied()
                .filter(|remote| {
                    short
                        .strip_prefix(remote)
                        .is_some_and(|suffix| suffix.starts_with('/'))
                })
                .max_by_key(|remote| remote.len())
        })
        .collect();
    let candidates: Vec<_> = remotes
        .iter()
        .copied()
        .filter(|remote| containing.is_empty() || containing.contains(remote))
        .collect();
    let upstreams = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(HEAD)%00%(upstream:remotename)",
            "refs/heads",
        ],
    )?;
    let upstream = upstreams
        .lines()
        .filter_map(|line| line.strip_prefix("*\0"))
        .find(|remote| candidates.contains(remote));
    if let Some(remote) = upstream.or_else(|| candidates.iter().copied().find(|r| *r == "origin")) {
        return Ok(remote.to_owned());
    }
    if let [remote] = candidates.as_slice() {
        return Ok((*remote).to_owned());
    }
    bail!("Multiple remotes match; set igit.webRemote to the remote to browse")
}

struct RemoteUrl {
    scheme: String,
    authority: String,
    host: String,
    path: String,
    ssh: bool,
}

impl RemoteUrl {
    fn parse(value: &str) -> Result<Self> {
        ensure!(
            !value.chars().any(char::is_control) && !value.contains('\\'),
            "Invalid remote URL. {CUSTOM_URL}"
        );
        let (scheme, authority, path, ssh) = if let Some((scheme, rest)) = value.split_once("://") {
            let scheme = scheme.to_ascii_lowercase();
            ensure!(
                matches!(
                    scheme.as_str(),
                    "http" | "https" | "ssh" | "git" | "git+ssh" | "ssh+git"
                ),
                "Unsupported remote transport. {CUSTOM_URL}"
            );
            let (authority, path) = rest
                .split_once('/')
                .ok_or_else(|| anyhow::anyhow!("Missing remote repository path. {CUSTOM_URL}"))?;
            let ssh = !matches!(scheme.as_str(), "http" | "https");
            (scheme, authority, path, ssh)
        } else {
            let separator = if let Some(start) = value.find('[') {
                value[start..].find("]:").map(|end| start + end + 1)
            } else {
                value.find(':')
            }
            .ok_or_else(|| anyhow::anyhow!("Local remotes have no web page. {CUSTOM_URL}"))?;
            let (authority, path) = (&value[..separator], &value[separator + 1..]);
            ensure!(
                !authority.contains('/') && authority.len() > 1,
                "Local remotes have no web page. {CUSTOM_URL}"
            );
            ("ssh".into(), authority, path, true)
        };
        let authority = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let (host, port) = if authority.starts_with('[') {
            let end = authority
                .find(']')
                .ok_or_else(|| anyhow::anyhow!("Invalid remote host. {CUSTOM_URL}"))?;
            authority[1..end]
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| anyhow::anyhow!("Invalid remote host. {CUSTOM_URL}"))?;
            (&authority[..=end], &authority[end + 1..])
        } else {
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, ""), |(host, _)| {
                    (host, &authority[host.len()..])
                });
            ensure!(
                !host.is_empty()
                    && host
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte)),
                "Invalid remote host. {CUSTOM_URL}"
            );
            (host, port)
        };
        ensure!(
            port.is_empty()
                || port
                    .strip_prefix(':')
                    .is_some_and(|value| value.parse::<u16>().is_ok()),
            "Invalid remote port. {CUSTOM_URL}"
        );
        let path = path
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        ensure!(
            !path.is_empty() && path.split('/').all(|part| !matches!(part, "" | "." | "..")),
            "Invalid remote repository path. {CUSTOM_URL}"
        );
        Ok(Self {
            scheme: if ssh { "https".into() } else { scheme },
            authority: if ssh { host } else { authority }.to_ascii_lowercase(),
            host: host.to_ascii_lowercase(),
            path: encode_path(path),
            ssh,
        })
    }

    fn named(&self, name: &str) -> bool {
        self.host.split('.').any(|label| label == name)
    }
}

fn remote_commit_url(value: &str, oid: &str) -> Result<String> {
    let mut remote = RemoteUrl::parse(value)?;
    if remote.host == "ssh.github.com" {
        remote.authority = "github.com".into();
    } else if remote.host == "altssh.gitlab.com" {
        remote.authority = "gitlab.com".into();
    }
    let suffix = if matches!(
        remote.host.as_str(),
        "ssh.dev.azure.com" | "vs-ssh.visualstudio.com"
    ) {
        let parts: Vec<_> = remote.path.split('/').collect();
        let ["v3", organization, project, repo] = parts.as_slice() else {
            bail!("Invalid Azure DevOps SSH path. {CUSTOM_URL}");
        };
        remote.authority = "dev.azure.com".into();
        remote.path = format!("{organization}/{project}/_git/{repo}");
        "commit"
    } else if remote.host.ends_with(".visualstudio.com") || remote.host == "dev.azure.com" {
        if remote.ssh {
            remote.path = remote.path.replace("/_ssh/", "/_git/");
            if let Some(path) = remote.path.strip_prefix("_ssh/") {
                remote.path = format!("_git/{path}");
            }
            remote.authority = remote.authority.replace(".vs-ssh.", ".");
        }
        ensure!(
            remote.path.starts_with("_git/") || remote.path.contains("/_git/"),
            "Invalid Azure DevOps repository path. {CUSTOM_URL}"
        );
        "commit"
    } else if remote.host == "bitbucket.org" {
        "commits"
    } else if remote.named("bitbucket") || remote.named("stash") {
        if let Some((base, repo)) = remote.path.rsplit_once("/scm/") {
            remote.path = bitbucket_path(base, repo)?;
        } else if let Some(repo) = remote.path.strip_prefix("scm/") {
            remote.path = bitbucket_path("", repo)?;
        } else if remote.ssh {
            remote.path = bitbucket_path("", &remote.path)?;
        } else {
            ensure!(
                remote.path.contains("/repos/"),
                "Invalid Bitbucket Data Center path. {CUSTOM_URL}"
            );
        }
        "commits"
    } else if remote.named("gitlab") {
        "-/commit"
    } else if remote.named("github")
        || remote.named("gitee")
        || remote.named("gitea")
        || remote.named("forgejo")
        || remote.host == "codeberg.org"
    {
        "commit"
    } else {
        bail!("Unrecognized Git hosting service. {CUSTOM_URL}");
    };
    Ok(format!(
        "{}://{}/{}/{suffix}/{oid}",
        remote.scheme, remote.authority, remote.path
    ))
}

fn bitbucket_path(base: &str, path: &str) -> Result<String> {
    let parts: Vec<_> = path.split('/').collect();
    let [project, repo] = parts.as_slice() else {
        bail!("Invalid Bitbucket Data Center path. {CUSTOM_URL}");
    };
    let prefix = if base.is_empty() {
        String::new()
    } else {
        format!("{base}/")
    };
    let project = project.strip_prefix('~').map_or_else(
        || format!("projects/{project}"),
        |user| format!("users/{user}"),
    );
    Ok(format!("{prefix}{project}/repos/{repo}"))
}

fn encode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut encoded = String::new();
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%'
            && bytes
                .get(index + 1..index + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            encoded.push_str(&path[index..index + 3]);
            index += 3;
        } else {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
                encoded.push(byte as char);
            } else {
                use std::fmt::Write;
                let _ = write!(encoded, "%{byte:02X}");
            }
            index += 1;
        }
    }
    encoded
}

pub(crate) fn validate_web_url(url: &str) -> Result<()> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("{CUSTOM_URL}"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    ensure!(
        matches!(scheme, "https" | "http") && !url.contains(['{', '}']) && !authority.contains('@'),
        "Browser URLs must use HTTP(S) without credentials. {CUSTOM_URL}"
    );
    ensure!(
        !url.chars()
            .any(|character| character.is_whitespace() || character.is_control())
            && !url.contains('\\'),
        "Browser URLs must encode spaces. {CUSTOM_URL}"
    );
    RemoteUrl::parse(&format!("{scheme}://{authority}/validate"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
