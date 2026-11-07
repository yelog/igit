#!/usr/bin/env python3
"""Package, inspect, and smoke-test IGit release archives without installing them."""

import argparse
import gzip
import hashlib
import io
import os
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path

from metadata import cargo_version, version_info

TARGETS = (
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
)
DOCUMENTS = (
    "README.md",
    "CHANGELOG.md",
    "NOTICE.txt",
    "licenses/Apache-2.0.txt",
    "config/default.toml",
    "docs/configuration.md",
    "docs/releasing.md",
    "contrib/kitty/igit_resize.py",
)


def archive_name(version, target):
    version = version_info(version)["version"]
    if target not in TARGETS:
        raise ValueError(f"unsupported release target: {target}")
    return f"igit_{version}_{target}" + (".zip" if "windows" in target else ".tar.gz")


def binary_names(target):
    return tuple(
        name + (".exe" if "windows" in target else "") for name in ("igit", "ig")
    )


def package(root, bin_dir, output, target, version):
    if cargo_version(root) != version:
        raise ValueError("archive version does not match Cargo metadata")
    archive_filename = archive_name(version, target)
    prefix = f"igit_{version}_{target}"
    members = [(name, bin_dir / name, 0o755) for name in binary_names(target)]
    members += [(name, root / name, 0o644) for name in DOCUMENTS]
    for name, path, _ in members:
        if not path.is_file() or path.is_symlink() or not path.stat().st_size:
            raise ValueError(f"missing, empty, or symlinked package file: {name}")
    output.mkdir(parents=True, exist_ok=True)
    archive = output / archive_filename
    if archive.exists():
        raise ValueError(f"archive already exists: {archive}")
    if "windows" in target:
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
            for member, path, mode in members:
                info = zipfile.ZipInfo(f"{prefix}/{member}")
                info.create_system = 3
                info.external_attr = (stat.S_IFREG | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                bundle.writestr(info, path.read_bytes())
    else:
        with (
            archive.open("xb") as output_file,
            gzip.GzipFile(
                filename="", mode="wb", fileobj=output_file, mtime=0
            ) as compressed,
            tarfile.open(fileobj=compressed, mode="w") as bundle,
        ):
            for member, path, mode in members:
                data = path.read_bytes()
                info = tarfile.TarInfo(f"{prefix}/{member}")
                info.size, info.mode, info.mtime = len(data), mode, 0
                bundle.addfile(info, io.BytesIO(data))
    inspect_archive(archive, version, target)
    return archive


def inspect_archive(archive, version, target):
    if archive.name != archive_name(version, target):
        raise ValueError(f"unexpected archive name: {archive.name}")
    prefix = f"igit_{version}_{target}/"
    expected = {prefix + name for name in (*binary_names(target), *DOCUMENTS)}
    data = {}
    if "windows" in target:
        with zipfile.ZipFile(archive) as bundle:
            members = bundle.infolist()
            if (
                len(members) != len(expected)
                or {m.filename for m in members} != expected
            ):
                raise ValueError("archive does not contain the exact expected files")
            for member in members:
                if stat.S_ISLNK(member.external_attr >> 16) or member.is_dir():
                    raise ValueError("archive members must be regular files")
                data[member.filename[len(prefix) :]] = bundle.read(member)
    else:
        with tarfile.open(archive, "r:gz") as bundle:
            members = bundle.getmembers()
            if len(members) != len(expected) or {m.name for m in members} != expected:
                raise ValueError("archive does not contain the exact expected files")
            for member in members:
                if not member.isfile():
                    raise ValueError("archive members must be regular files")
                name = member.name[len(prefix) :]
                if name in binary_names(target) and member.mode & 0o111 != 0o111:
                    raise ValueError(f"binary is not executable: {name}")
                data[name] = bundle.extractfile(member).read()
    if any(not content for content in data.values()):
        raise ValueError("archive contains an empty file")
    return data


def checksums(directory, version, verify=False):
    expected = {archive_name(version, target) for target in TARGETS}
    actual = {p.name for p in directory.iterdir()}
    if actual - {"SHA256SUMS"} != expected:
        raise ValueError(
            f"release asset mismatch: missing={sorted(expected - actual)}, "
            f"unexpected={sorted(actual - expected - {'SHA256SUMS'})}"
        )
    lines = []
    for target in TARGETS:
        inspect_archive(directory / archive_name(version, target), version, target)
    for name in sorted(expected):
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"asset must be a regular file: {name}")
        with path.open("rb") as asset:
            lines.append(
                f"{hashlib.file_digest(asset, 'sha256').hexdigest()}  {name}\n"
            )
    content = "".join(lines)
    path = directory / "SHA256SUMS"
    if verify:
        if path.read_text(encoding="utf-8") != content:
            raise ValueError("SHA256SUMS does not match release assets")
    else:
        path.write_text(content, encoding="utf-8", newline="\n")


def smoke(archive, version, target):
    files = inspect_archive(archive, version, target)
    with tempfile.TemporaryDirectory(prefix="igit-release-") as temporary:
        root = Path(temporary).resolve()
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("GIT_")
        }
        env.update(
            IGIT_CONFIG_HOME=str(root / "config"),
            IGIT_STATE_DIR=str(root / "state"),
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_NOSYSTEM="1",
        )
        repo = root / "workspace" / "demo"
        repo.mkdir(parents=True)
        subprocess.run(
            ["git", "init", "-q", "-b", "main", str(repo)],
            check=True,
            env=env,
            timeout=30,
        )
        for name in binary_names(target):
            binary = root / name
            binary.write_bytes(files[name])
            binary.chmod(0o755)

            def run(*args, binary=binary):
                result = subprocess.run(
                    [str(binary), *args],
                    check=True,
                    env=env,
                    cwd=root,
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    timeout=30,
                )
                return result.stdout

            expected = f"IGit {version} — terminal Git workbench"
            if run("--version").strip() != expected or not run("--help").startswith(
                expected + "\n"
            ):
                raise ValueError(f"incorrect executable version: {name}")
            if run("--print-default-config") != files["config/default.toml"].decode(
                "utf-8"
            ):
                raise ValueError(f"default config does not match archive: {name}")
            run("--check-config")
            if "demo" not in run("--list-roots", str(repo.parent)):
                raise ValueError(f"repository discovery failed: {name}")
        if (root / "state").exists():
            raise ValueError("noninteractive smoke test wrote application state")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("package")
    pack.add_argument("target", choices=TARGETS)
    pack.add_argument("--root", type=Path, default=Path.cwd())
    pack.add_argument("--bin-dir", type=Path, required=True)
    pack.add_argument("--output", type=Path, required=True)
    for name in ("inspect", "smoke"):
        command = commands.add_parser(name)
        command.add_argument("archive", type=Path)
        command.add_argument("version")
        command.add_argument("target", choices=TARGETS)
    sums = commands.add_parser("checksums")
    sums.add_argument("directory", type=Path)
    sums.add_argument("version")
    sums.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "package":
            print(
                package(
                    args.root,
                    args.bin_dir,
                    args.output,
                    args.target,
                    cargo_version(args.root),
                )
            )
        elif args.command == "inspect":
            inspect_archive(args.archive, args.version, args.target)
        elif args.command == "smoke":
            smoke(args.archive, args.version, args.target)
        elif args.command == "checksums":
            checksums(args.directory, args.version, args.verify)
    except (
        ValueError,
        OSError,
        subprocess.SubprocessError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
