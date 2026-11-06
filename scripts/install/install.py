#!/usr/bin/env python3
"""IGit native installer/updater. Python 3.9+, curl, and no third-party modules."""

import argparse
import contextlib
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import uuid
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse

TARGETS = (
    "x86_64-apple-darwin", "aarch64-apple-darwin",
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
)
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-beta\.([1-9][0-9]*))?")
BASE_URL = "https://igit.yelog.org/channels"
MAX_ARCHIVE = 128 * 1024 * 1024


def version_key(version):
    match = VERSION.fullmatch(version)
    if not match:
        raise ValueError("invalid IGit version: " + version)
    return tuple(int(part) for part in match.groups()[:3]) + (
        match[4] is None, int(match[4] or 0),
    )


def channel_for(version):
    version_key(version)
    return "beta" if "-beta." in version else "stable"


def host_target():
    system, machine = platform.system(), platform.machine().lower()
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64", "amd64": "x86_64"}.get(machine)
    suffix = {"Darwin": "apple-darwin", "Linux": "unknown-linux-musl"}.get(system)
    if not arch or not suffix:
        raise ValueError("unsupported platform: %s/%s" % (system, machine))
    return arch + "-" + suffix


def allowed_url(url):
    parsed = urlparse(url)
    return (
        bool(parsed.hostname) and not parsed.username and not parsed.password and not parsed.fragment
        and (parsed.scheme == "https" or (
            parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "localhost", "::1"}
        ))
    )


def validate_manifest(data, channel, target, base_url):
    if not isinstance(data, dict) or data.get("schema") != 1 or data.get("product") != "igit" or data.get("channel") != channel:
        raise ValueError("channel manifest identity mismatch")
    version = data.get("version", "")
    if channel_for(version) != channel or data.get("tag") != "v" + version or data.get("prerelease") is not (channel == "beta"):
        raise ValueError("channel manifest version mismatch")
    if not isinstance(data.get("assets"), dict) or set(data["assets"]) != set(TARGETS) or target not in TARGETS:
        raise ValueError("channel manifest target set mismatch")
    base = urlparse(base_url)
    # A loopback mirror is useful for isolated installer tests. Remote manifests
    # may only point at this repository's exact public release asset URLs.
    origin = (base.scheme + "://" + base.netloc) if base.scheme == "http" and allowed_url(base_url) else "https://github.com"
    for name, asset in data["assets"].items():
        extension = ".zip" if "windows" in name else ".tar.gz"
        filename = "igit_%s_%s%s" % (version, name, extension)
        expected = "%s/yelog/igit/releases/download/v%s/%s" % (origin, version, filename)
        if not isinstance(asset, dict) or asset.get("url") != expected or not re.fullmatch(r"[0-9a-f]{64}", asset.get("sha256", "")):
            raise ValueError("invalid release asset: " + name)
    return version, data["assets"][target]


def download(url, destination, limit=MAX_ARCHIVE):
    if not allowed_url(url):
        raise ValueError("downloads require HTTPS (HTTP is allowed only on loopback)")
    protocols = "=https,http" if urlparse(url).scheme == "http" else "=https"
    command = [
        "curl", "--fail", "--silent", "--show-error", "--location",
        "--proto", protocols, "--proto-redir", "=https", "--tlsv1.2",
        "--connect-timeout", "15", "--max-time", "120", "--retry", "2",
        "--max-filesize", str(limit), "--output", str(destination), url,
    ]
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=400)
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValueError("download failed: " + detail)
    if not destination.is_file() or not 0 < destination.stat().st_size <= limit:
        raise ValueError("download is empty or exceeds its size limit")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def relative_file(name):
    path = PurePosixPath(name)
    if not name or "\\" in name or "\0" in name or path.is_absolute() or any(part in {"", ".", ".."} for part in name.split("/")):
        raise ValueError("unsafe archive entry: " + name)
    return path


def extract_archive(archive, destination, version, target):
    prefix = "igit_%s_%s/" % (version, target)
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if not 2 <= len(members) <= 256 or sum(member.size for member in members) > MAX_ARCHIVE:
            raise ValueError("archive exceeds its extraction limits")
        names = set()
        for member in members:
            relative_file(member.name)
            if not member.isfile() or not member.name.startswith(prefix) or member.name in names:
                raise ValueError("archive contains an unexpected or nonregular file")
            names.add(member.name)
        if not {prefix + "igit", prefix + "ig"} <= names:
            raise ValueError("archive must contain both igit and ig")
        for member in members:
            name = relative_file(member.name[len(prefix):])
            path = destination.joinpath(*name.parts)
            path.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(member) as source, path.open("xb") as output:
                shutil.copyfileobj(source, output)
            path.chmod(0o755 if str(name) in {"igit", "ig"} else 0o644)


def verify_binaries(directory, version):
    expected = "IGit %s — terminal Git workbench" % version
    for name in ("igit", "ig"):
        result = subprocess.run([str(directory / name), "--version"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
        if result.returncode or result.stdout.decode("utf-8", errors="replace").strip() != expected:
            raise ValueError("staged %s does not report version %s" % (name, version))


def read_json(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 1024 * 1024:
        raise ValueError("missing or invalid installation metadata: " + str(path))
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("installation metadata must be a JSON object: " + str(path))
    return data


def atomic_write(path, text):
    descriptor, temporary = tempfile.mkstemp(prefix="." + path.name + ".", dir=str(path.parent))
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def write_json(path, data):
    atomic_write(path, json.dumps(data, indent=2, sort_keys=True) + "\n")


def release_record(directory, version, target, digest):
    return {
        "product": "igit", "version": version, "target": target, "archive_sha256": digest,
        "files": {str(path.relative_to(directory)): sha256(path) for path in directory.rglob("*") if path.is_file()},
    }


def verify_release(directory, target):
    record = read_json(directory / ".igit-release.json")
    version = record.get("version", "")
    version_key(version)
    if record.get("product") != "igit" or record.get("target") != target or directory.name != version + "_" + target:
        raise ValueError("installed release identity mismatch")
    files = record.get("files", {})
    if not isinstance(files, dict) or not {"igit", "ig"} <= files.keys() or not re.fullmatch(r"[0-9a-f]{64}", record.get("archive_sha256", "")):
        raise ValueError("installed release ownership is incomplete")
    for name, expected in files.items():
        path = directory.joinpath(*relative_file(name).parts)
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(directory.resolve()) or sha256(path) != expected:
            raise ValueError("installed file has changed: " + str(path))
    return record


def inspect_installation(root, install_dir=None, executable=None):
    state_path = root / "install.json"
    if not state_path.exists() and not state_path.is_symlink():
        return None
    state = read_json(state_path)
    if state.get("schema") != 1 or state.get("product") != "igit" or state.get("manager") != "native" or state.get("root") != str(root):
        raise ValueError("installation ownership mismatch")
    target = state.get("target")
    if target not in TARGETS or "windows" in target:
        raise ValueError("installation target is not supported by this installer")
    recorded_bin = Path(state.get("bin_dir", ""))
    if not recorded_bin.is_absolute() or install_dir is not None and recorded_bin != install_dir:
        raise ValueError("installation directory differs from the recorded owner")
    current = root / "current"
    if not current.is_symlink():
        raise ValueError("native current release link is missing or replaced")
    release = current.resolve(strict=True)
    if release.parent != root / "releases" or release.is_symlink():
        raise ValueError("native current release escaped its installation")
    for name in ("igit", "ig"):
        launcher = recorded_bin / name
        if not launcher.is_symlink() or os.readlink(launcher) != str(current / name):
            raise ValueError("launcher is not owned by this installation: " + str(launcher))
    record = verify_release(release, target)
    if executable is not None and executable.resolve(strict=True) not in {release / "igit", release / "ig"}:
        raise ValueError("the running executable is not the active native release; restart IGit")
    return dict(state, version=record["version"], channel=channel_for(record["version"]), release=release)


@contextlib.contextmanager
def installation_lock(root):
    lock = root / ".install.lock"
    try:
        descriptor = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError:
        raise ValueError("another installer may be running; inspect " + str(lock))
    with os.fdopen(descriptor, "w") as stream:
        stream.write(str(os.getpid()) + "\n")
    identity = lock.stat()
    try:
        yield
    finally:
        if lock.exists() and not lock.is_symlink() and lock.stat().st_ino == identity.st_ino:
            lock.unlink()


def activate(root, release):
    temporary = root / (".current-" + uuid.uuid4().hex)
    try:
        temporary.symlink_to(Path("releases") / release.name)
        os.replace(temporary, root / "current")
    finally:
        if temporary.is_symlink():
            temporary.unlink()


def install_release(root, install_dir, archive, version, target, digest):
    existing = inspect_installation(root, install_dir)
    if existing is None:
        if root.exists() and any(root.iterdir()):
            raise ValueError("refusing an unowned nonempty installation root: " + str(root))
        for name in ("igit", "ig"):
            path = install_dir / name
            if path.exists() or path.is_symlink():
                raise ValueError("refusing to overwrite an existing command: " + str(path))
    root.mkdir(parents=True, exist_ok=True)
    with installation_lock(root):
        releases = root / "releases"
        if releases.is_symlink():
            raise ValueError("release directory is a symlink")
        releases.mkdir(exist_ok=True)
        release = releases / (version + "_" + target)
        created_release, created_launchers, created_state, activated = False, [], False, False
        created_bin = not install_dir.exists()
        try:
            if release.exists() or release.is_symlink():
                if release.is_symlink() or verify_release(release, target)["archive_sha256"] != digest:
                    raise ValueError("existing release differs; refusing to replace it")
            else:
                with tempfile.TemporaryDirectory(prefix=".stage-", dir=str(releases)) as temporary:
                    staged = Path(temporary) / "release"
                    staged.mkdir()
                    extract_archive(archive, staged, version, target)
                    verify_binaries(staged, version)
                    write_json(staged / ".igit-release.json", release_record(staged, version, target, digest))
                    os.rename(staged, release)
                    created_release = True
            if existing:
                current = inspect_installation(root, install_dir)
                if current["release"] != existing["release"]:
                    raise ValueError("active release changed during installation")
            else:
                install_dir.mkdir(parents=True, exist_ok=True)
                for name in ("igit", "ig"):
                    path = install_dir / name
                    path.symlink_to(root / "current" / name)
                    created_launchers.append(path)
                write_json(root / "install.json", {
                    "schema": 1, "product": "igit", "manager": "native", "root": str(root),
                    "bin_dir": str(install_dir), "target": target,
                })
                created_state = True
            activate(root, release)
            activated = True
        finally:
            if not activated:
                for launcher in created_launchers:
                    launcher.unlink()
                if created_state:
                    (root / "install.json").unlink()
                if created_release:
                    shutil.rmtree(release)
                if created_bin and install_dir.is_dir() and not any(install_dir.iterdir()):
                    install_dir.rmdir()
                if not existing and releases.is_dir() and not any(releases.iterdir()):
                    releases.rmdir()


def configure_path(install_dir, modify):
    shell = Path(os.environ.get("SHELL", "sh")).name
    command = 'export PATH=%s:"$PATH"' % shlex.quote(str(install_dir))
    profile = None
    if shell == "zsh":
        profile = Path(os.environ.get("ZDOTDIR", str(Path.home()))) / ".zshrc"
    elif shell == "bash":
        profile = Path.home() / (".bash_profile" if platform.system() == "Darwin" else ".bashrc")
    elif shell in {"sh", "dash", "ash"}:
        profile = Path.home() / ".profile"
    elif shell == "fish":
        command = "fish_add_path --path -- '%s'" % str(install_dir).replace("\\", "\\\\").replace("'", "\\'")
        profile = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "fish/config.fish"
    block = "# >>> IGit PATH >>>\n" + command + "\n# <<< IGit PATH <<<\n"
    if modify and profile:
        try:
            if profile.is_symlink():
                raise ValueError("shell profile is a symlink")
            previous = profile.read_text(encoding="utf-8") if profile.exists() else ""
            if block not in previous:
                if "# >>> IGit PATH >>>" in previous:
                    raise ValueError("shell profile contains another IGit PATH block")
                profile.parent.mkdir(parents=True, exist_ok=True)
                with profile.open("a", encoding="utf-8") as stream:
                    stream.write(("\n" if previous and not previous.endswith("\n") else "") + "\n" + block)
                print("Added IGit to " + str(profile), file=sys.stderr)
        except (OSError, UnicodeError, ValueError) as error:
            print("PATH setup skipped: " + str(error), file=sys.stderr)
    print("For this terminal, run:\n  " + command + "\n  igit", file=sys.stderr)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--channel", choices=("stable", "beta"), default=os.environ.get("IGIT_CHANNEL"))
    result.add_argument("--version", default=os.environ.get("IGIT_VERSION"))
    result.add_argument("--install-dir", default=os.environ.get("IGIT_INSTALL_DIR"))
    result.add_argument("--install-root", default=os.environ.get("IGIT_INSTALL_ROOT"))
    result.add_argument("--no-modify-path", action="store_true")
    result.add_argument("--check", action="store_true")
    result.add_argument("--json", action="store_true")
    result.add_argument("--allow-downgrade", action="store_true")
    result.add_argument("--from-executable", help=argparse.SUPPRESS)
    result.add_argument("--running-version", help=argparse.SUPPRESS)
    return result


def run(args):
    if sys.version_info < (3, 9):
        raise ValueError("Python 3.9 or newer is required")
    executable = Path(args.from_executable).resolve(strict=True) if args.from_executable else None
    if executable:
        root = executable.parent.parent.parent
        state = inspect_installation(root, executable=executable)
        if state is None or state["version"] != args.running_version:
            raise ValueError("executable is not owned by the active native installation")
        install_dir, target = Path(state["bin_dir"]), state["target"]
    else:
        root = Path(args.install_root or Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "igit").expanduser().resolve()
        install_dir = Path(args.install_dir or Path.home() / ".local/bin").expanduser().resolve()
        state = inspect_installation(root, install_dir)
        target = state["target"] if state else host_target()
    channel = args.channel or (state["channel"] if state else "stable")
    if channel not in {"stable", "beta"}:
        raise ValueError("invalid release channel")
    locked = os.environ.get("IGIT_CHANNEL_LOCKED")
    if locked and locked != channel:
        raise ValueError("this installer is fixed to the %s channel" % locked)
    base = os.environ.get("IGIT_CHANNEL_BASE_URL", BASE_URL).rstrip("/")
    if not allowed_url(base):
        raise ValueError("invalid channel base URL")
    current = state["version"] if state else None
    report = {"schema": 1, "product": "igit", "channel": channel, "current_version": current, "target_version": None}
    with tempfile.TemporaryDirectory(prefix="igit-download-") as temporary:
        temporary = Path(temporary)
        manifest = temporary / "manifest.json"
        try:
            download(base + "/" + channel + ".json", manifest, 1024 * 1024)
        except ValueError as error:
            raise ValueError("cannot read the %s release channel; it may not have a published release yet. %s" % (channel, error)) from error
        version, asset = validate_manifest(json.loads(manifest.read_text(encoding="utf-8")), channel, target, base)
        if args.version and args.version != version:
            raise ValueError("requested version is not the published channel version")
        report["target_version"] = version
        comparison = (version_key(version) > version_key(current)) - (version_key(version) < version_key(current)) if current else 1
        if comparison < 0 and not args.allow_downgrade:
            if channel != state["channel"]:
                raise ValueError("channel switch would downgrade IGit; use --allow-downgrade explicitly")
            report["status"] = "up_to_date"
        elif comparison == 0:
            report["status"] = "up_to_date"
        elif args.check:
            report["status"] = "update_available"
        else:
            print("Downloading IGit %s (%s)..." % (version, target), file=sys.stderr)
            archive = temporary / "release.tar.gz"
            download(asset["url"], archive)
            if sha256(archive) != asset["sha256"]:
                raise ValueError("release archive checksum mismatch; installation unchanged")
            install_release(root, install_dir, archive, version, target, asset["sha256"])
            report["status"] = "updated" if state else "installed"
            if not executable:
                configure_path(install_dir, not args.no_modify_path)
    return report


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        report = run(args)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, tarfile.TarError) as error:
        if args.json:
            print(json.dumps({"schema": 1, "product": "igit", "status": "error", "error": str(error)}))
        else:
            print("IGit installer: " + str(error), file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(report))
    else:
        print("IGit %s: %s (%s)" % (report["target_version"], report["status"].replace("_", " "), report["channel"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
