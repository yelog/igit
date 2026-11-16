use super::*;
use std::{collections::BTreeMap, ffi::OsString, process::Command};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Transport {
    File,
    Http,
    Ssh,
    Git,
}

pub(super) fn validate(
    configuration: &Configuration,
    remote: &str,
    endpoint: &str,
) -> Result<Transport> {
    ensure!(
        !endpoint.is_empty()
            && !endpoint.starts_with('-')
            && !endpoint
                .split_once("::")
                .is_some_and(|(prefix, _)| { !prefix.contains(['/', '\\', '[']) }),
        "Automatic fetch skipped an unsupported transport or remote helper"
    );
    for suffix in ["vcs", "uploadpack"] {
        ensure!(
            configuration
                .last(&format!("remote.{remote}.{suffix}"))
                .is_none_or(|value| value.is_empty()
                    || suffix == "uploadpack" && value == "git-upload-pack"),
            "Automatic fetch skipped a custom remote helper or upload command"
        );
    }
    let transport = if endpoint.starts_with("https://") || endpoint.starts_with("http://") {
        Transport::Http
    } else if endpoint.starts_with("ssh://") || endpoint.starts_with("git+ssh://") {
        Transport::Ssh
    } else if endpoint.starts_with("git://") {
        Transport::Git
    } else if endpoint.starts_with("file://")
        || !endpoint.contains(':')
        || Path::new(endpoint).is_absolute()
    {
        Transport::File
    } else if !endpoint.contains("://")
        && endpoint
            .split_once(':')
            .is_some_and(|(host, _)| !host.contains(['/', '\\']))
    {
        Transport::Ssh
    } else {
        bail!("Automatic fetch skipped an unsupported transport or remote helper");
    };
    if transport == Transport::Ssh {
        for value in [
            configuration.environment("GIT_SSH_COMMAND"),
            configuration.environment("GIT_SSH"),
            configuration
                .last("core.sshcommand")
                .map(std::ffi::OsStr::new),
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                matches!(value.to_str(), Some("ssh" | "/usr/bin/ssh" | "/bin/ssh")),
                "Automatic fetch skipped custom SSH; silent authentication cannot be guaranteed"
            );
        }
    }
    if transport == Transport::Http {
        let mut helpers = BTreeMap::<&str, Vec<&str>>::new();
        let mut credential_endpoints = vec![endpoint];
        let remote_proxy = format!("remote.{remote}.proxy");
        for (key, value) in configuration.entries() {
            if key == "http.proxy"
                || key.starts_with("http.") && key.ends_with(".proxy")
                || key == remote_proxy
            {
                credential_endpoints.extend(value.filter(|value| !value.is_empty()));
            }
        }
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            credential_endpoints.extend(
                configuration
                    .environment(name)
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty()),
            );
        }
        for (key, value) in configuration.entries() {
            if key.starts_with("credential.")
                && key.ends_with(".helper")
                && credential_endpoints
                    .iter()
                    .any(|endpoint| credential_may_match(key, endpoint))
            {
                let helper = value.ok_or_else(|| {
                    anyhow::anyhow!("Automatic fetch skipped an invalid credential helper")
                })?;
                let chain = helpers.entry(key).or_default();
                if helper.is_empty() {
                    chain.clear();
                } else {
                    chain.push(helper);
                }
            }
        }
        ensure!(
            helpers.values().flatten().all(|helper| safe_helper(helper)),
            "Automatic fetch skipped a custom credential helper; silent authentication cannot be guaranteed"
        );
    }
    if transport == Transport::Git {
        ensure!(
            configuration
                .environment("GIT_PROXY_COMMAND")
                .is_none_or(|value| value.is_empty())
                && configuration
                    .values("core.gitproxy")
                    .all(|value| matches!(value, None | Some("" | "none"))),
            "Automatic fetch skipped a custom Git proxy command"
        );
    }
    Ok(transport)
}

fn credential_may_match(key: &str, endpoint: &str) -> bool {
    let Some(scope) = key
        .strip_prefix("credential.")
        .and_then(|key| key.strip_suffix(".helper"))
    else {
        return true;
    };
    let Some((scope_scheme, scope_url)) = scope.split_once("://") else {
        return true;
    };
    let Some((endpoint_scheme, endpoint_url)) = endpoint.split_once("://") else {
        return true;
    };
    if !scope_scheme.eq_ignore_ascii_case(endpoint_scheme) {
        return false;
    }
    let host = |url: &str| {
        let authority = url.split('/').next().unwrap_or_default();
        authority
            .rsplit('@')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    let mut scope_host = host(scope_url);
    let mut endpoint_host = host(endpoint_url);
    let default_port = match endpoint_scheme {
        "https" => ":443",
        "http" => ":80",
        _ => return true,
    };
    for host in [&mut scope_host, &mut endpoint_host] {
        if host.ends_with(default_port) {
            host.truncate(host.len() - default_port.len());
        }
    }
    scope_host.is_empty()
        || !scope_host.is_ascii()
        || !endpoint_host.is_ascii()
        || scope_host.contains(['*', '?', '[', ']', '%'])
        || endpoint_host.contains('%')
        || scope_host == endpoint_host
}

fn safe_helper(helper: &str) -> bool {
    if helper.is_empty() {
        return true;
    }
    if helper != helper.trim() {
        return false;
    }
    if !helper
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b" /_.:@=+-".contains(&byte))
    {
        return false;
    }
    let mut arguments = helper.split_whitespace();
    let program = arguments.next().unwrap_or_default();
    if !matches!(
        program,
        "cache" | "store" | "osxkeychain" | "libsecret" | "wincred" | "manager" | "manager-core"
    ) {
        return false;
    }
    match program {
        "cache" => arguments.all(|argument| {
            argument.strip_prefix("--timeout=").is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
            }) || argument.starts_with("--socket=") && argument.len() > "--socket=".len()
        }),
        "store" => arguments
            .all(|argument| argument.starts_with("--file=") && argument.len() > "--file=".len()),
        _ => arguments.next().is_none(),
    }
}

pub(super) fn command(target: &Target, hooks: &Path) -> Command {
    let mut command = super::super::command(&target.path);
    let mut hook_config = OsString::from("core.hooksPath=");
    hook_config.push(hooks);
    command
        .arg("-c")
        .arg(hook_config)
        .args([
            "-c",
            "credential.interactive=false",
            "-c",
            "core.askPass=",
            "-c",
            "http.sslVerify=true",
            "-c",
            "http.proxySSLVerify=true",
            "-c",
            "http.followRedirects=false",
            "-c",
            "fetch.all=false",
            "-c",
            "fetch.bundleURI=",
            "-c",
            "fetch.followRemoteHEAD=never",
            "-c",
            "fetch.writeCommitGraph=false",
            "-c",
            "fetch.recurseSubmodules=false",
            "-c",
            "submodule.recurse=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "transfer.bundleURI=false",
        ])
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env("GCM_INTERACTIVE", "Never")
        .env("GCM_GUI_PROMPT", "false")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", "")
        .env_remove("GIT_SSL_NO_VERIFY")
        .env_remove("GIT_PROXY_SSL_NO_VERIFY")
        .env_remove("GIT_PROXY_COMMAND")
        .env("GIT_ALLOW_PROTOCOL", "file:git:http:https:ssh")
        .env("GIT_PROTOCOL_FROM_USER", "0");
    for (key, _) in target.configuration.entries() {
        if key.starts_with("http.") {
            if key.ends_with(".sslverify") || key.ends_with(".proxysslverify") {
                command.args(["-c", &format!("{key}=true")]);
            } else if key.ends_with(".followredirects") {
                command.args(["-c", &format!("{key}=false")]);
            }
        }
    }
    if target.transport == Transport::Ssh {
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        command
            .env_remove("GIT_SSH")
            .env("GIT_SSH_VARIANT", "ssh")
            .env(
                "GIT_SSH_COMMAND",
                format!(
                    "ssh -F {null} -oBatchMode=yes -oStrictHostKeyChecking=yes -oNumberOfPasswordPrompts=0 -oPermitLocalCommand=no -oProxyCommand=none -oProxyJump=none -oConnectionAttempts=1"
                ),
            );
    }
    command
}

pub(super) fn fetch_command(target: &Target, hooks: &Path) -> Command {
    let mut command = command(target, hooks);
    command.args([
        "fetch",
        "--no-all",
        "--no-tags",
        "--no-prune",
        "--no-prune-tags",
        "--no-recurse-submodules",
        "--no-write-fetch-head",
        "--no-auto-maintenance",
        "--no-write-commit-graph",
        "--atomic",
        "--refmap=",
    ]);
    command
}

pub(super) fn run(
    target: &Target,
    control: &ReadControl,
    operation: &str,
    command: &mut Command,
) -> Result<Vec<u8>> {
    let output = process::controlled_output(&target.path, operation, command, control)?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if authentication_failed(&message) {
            bail!(
                "Automatic fetch authentication failed; authenticate in a foreground Git command"
            );
        }
        let message = message.to_ascii_lowercase();
        if message.contains("host key verification failed")
            || message.contains("remote host identification has changed")
        {
            bail!("Automatic fetch host key verification failed; verify the host in a foreground Git command");
        }
        if message.contains("certificate") || message.contains("ssl") || message.contains("tls") {
            bail!("Automatic fetch TLS verification failed; verify the remote certificate");
        }
        bail!("Git {operation} failed; retry in a foreground Git command for details");
    }
    Ok(output.stdout)
}
