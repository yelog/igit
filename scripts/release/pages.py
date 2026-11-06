#!/usr/bin/env python3
"""Build the public installer site from published releases, never from draft tags."""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from distribution import TARGETS, archive_name
from metadata import policy, version_info


def select_releases(releases):
    selected = {}
    for release in releases:
        if release.get("draft") or not release.get("published_at"):
            continue
        try:
            info = version_info(release["tag_name"])
        except (KeyError, ValueError):
            continue
        if release["tag_name"] != info["tag"] or release.get("prerelease") is not info["prerelease"]:
            raise ValueError("published release channel differs from its version")
        channel = info["channel"]
        base, beta = policy.parse_version(info["version"])
        key = base + (beta or 0,)
        if channel not in selected or key > selected[channel][0]:
            selected[channel] = (key, release)
    return {channel: item[1] for channel, item in selected.items()}


def manifest(release, checksum_text):
    info = version_info(release["tag_name"])
    expected = {archive_name(info["version"], target) for target in TARGETS}
    assets = release["assets"]
    names = [asset["name"] for asset in assets]
    if len(set(names)) != len(names) or set(names) != expected | {"SHA256SUMS"}:
        raise ValueError("published release has an unexpected asset set")
    digests = {}
    for line in checksum_text.splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (\S+)", line)
        if not match or match[2] in digests:
            raise ValueError("invalid or duplicate published checksum")
        digests[match[2]] = match[1]
    if digests.keys() != expected:
        raise ValueError("published checksums do not cover the exact target set")
    by_name = {asset["name"]: asset for asset in assets}
    mapping = {}
    for name in names:
        asset = by_name[name]
        url = "https://github.com/yelog/igit/releases/download/%s/%s" % (info["tag"], name)
        if asset.get("browser_download_url") != url or asset.get("state") != "uploaded" or asset.get("size", 0) <= 0:
            raise ValueError("published asset identity mismatch: " + name)
        digest = hashlib.sha256(checksum_text.encode()).hexdigest() if name == "SHA256SUMS" else digests[name]
        if asset.get("digest") and asset["digest"] != "sha256:" + digest:
            raise ValueError("published asset digest differs from SHA256SUMS: " + name)
    for target in TARGETS:
        name = archive_name(info["version"], target)
        mapping[target] = {"url": by_name[name]["browser_download_url"], "sha256": digests[name]}
    return dict(schema=1, product="igit", published_at=release["published_at"], assets=mapping, **info)


def unix_script(root, channel):
    if channel not in {"stable", "beta"}:
        raise ValueError("invalid installer channel")
    source = (root / "scripts/install/install.py").read_text(encoding="utf-8")
    return (
        "#!/bin/sh\nset -eu\n"
        "export IGIT_CHANNEL=%s\nexport IGIT_CHANNEL_LOCKED=%s\n" % (channel, channel)
        + "command -v python3 >/dev/null 2>&1 || { printf '%s\\n' 'IGit requires Python 3.9+.' >&2; exit 1; }\n"
        + "exec python3 - \"$@\" <<'IGIT_INSTALLER_PY'\n"
        + source + "\nIGIT_INSTALLER_PY\n"
    )


def assemble(root, output, releases, checksums):
    selected = select_releases(releases)
    if not selected:
        raise ValueError("no published IGit release is available")
    manifests = {channel: manifest(release, checksums[release["tag_name"]]) for channel, release in selected.items()}
    if output.exists() and any(output.iterdir()):
        raise ValueError("Pages output must be empty")
    output.mkdir(parents=True, exist_ok=True)
    (output / "channels").mkdir()
    for channel, data in manifests.items():
        (output / "channels" / (channel + ".json")).write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    for channel, name in (("stable", "install.sh"), ("beta", "install-beta.sh")):
        (output / name).write_text(unix_script(root, channel), encoding="utf-8", newline="\n")
    for name in ("install.ps1", "index.html", "CNAME"):
        shutil.copyfile(root / "pages" / name, output / name)
    shutil.copyfile(root / "pages/install.ps1", output / "install.ps1.txt")
    (output / ".nojekyll").touch()
    if (output / "CNAME").read_text().strip() != "igit.yelog.org":
        raise ValueError("unexpected Pages domain")
    return [{"channel": channel, "version": data["version"]} for channel, data in sorted(manifests.items())]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        pages = json.loads(subprocess.check_output([
            "gh", "api", "--paginate", "--slurp", "repos/yelog/igit/releases?per_page=100",
        ], text=True, timeout=120))
        releases = [release for page in pages for release in page]
        checksums = {}
        for release in select_releases(releases).values():
            with tempfile.TemporaryDirectory(prefix="igit-channel-") as temporary:
                subprocess.run([
                    "gh", "release", "download", release["tag_name"], "--repo", "yelog/igit",
                    "--pattern", "SHA256SUMS", "--dir", temporary,
                ], check=True, timeout=120)
                checksums[release["tag_name"]] = (Path(temporary) / "SHA256SUMS").read_text(encoding="utf-8")
        print(json.dumps(assemble(Path(__file__).resolve().parents[2], args.output, releases, checksums)))
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, "Pages: %s\n" % error)


if __name__ == "__main__":
    main()
