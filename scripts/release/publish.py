#!/usr/bin/env python3
"""Publish verified artifacts; resume matching uploads without replacing assets."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

from distribution import checksums
from metadata import git, release_notes, validate


def gh(*args):
    return subprocess.check_output(
        ["gh", *args], text=True, encoding="utf-8", stderr=subprocess.PIPE, timeout=120
    )


def api(path, missing_ok=False):
    try:
        return json.loads(gh("api", path))
    except subprocess.CalledProcessError as error:
        if missing_ok and "(HTTP 404)" in error.stderr:
            return None
        raise


def digest(path):
    with path.open("rb") as source:
        return "sha256:" + hashlib.file_digest(source, "sha256").hexdigest()


def find_release(repository, tag):
    release = api(f"repos/{repository}/releases/tags/{tag}", missing_ok=True)
    if release is not None:
        return release
    # The tag endpoint only finds published releases. Drafts are visible to a
    # writer through the release list; do not create a duplicate on resume.
    pages = json.loads(
        gh("api", "--paginate", "--slurp", f"repos/{repository}/releases?per_page=100")
    )
    matches = [
        release for page in pages for release in page if release["tag_name"] == tag
    ]
    if len(matches) > 1:
        raise ValueError("multiple releases use the requested tag")
    return matches[0] if matches else None


def matching_assets(release, expected, tag, repository):
    """Return missing names, refusing unknown, duplicate, or different assets."""
    assets = release["assets"]
    names = [asset["name"] for asset in assets]
    if len(names) != len(set(names)) or set(names) - expected.keys():
        raise ValueError("release contains unexpected or duplicate assets")
    for asset in assets:
        actual = asset.get("digest")
        if not actual:
            with tempfile.TemporaryDirectory(prefix="igit-upload-check-") as temporary:
                gh(
                    "release",
                    "download",
                    tag,
                    "--repo",
                    repository,
                    "--pattern",
                    asset["name"],
                    "--dir",
                    temporary,
                )
                actual = digest(Path(temporary) / asset["name"])
        if actual != expected[asset["name"]]:
            raise ValueError(
                f"existing asset differs; refusing to overwrite: {asset['name']}"
            )
    return sorted(expected.keys() - set(names))


def publish(root, tag, directory, repository):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("expected an owner/repository name")
    info = validate(root, tag)
    checksums(directory, info["version"], verify=True)
    files = sorted(directory.iterdir())
    expected = {path.name: digest(path) for path in files}

    def check_remote_tag():
        remote = api(f"repos/{repository}/git/ref/tags/{tag}")["object"]
        if remote["type"] != "tag" or remote["sha"] != git(
            root, "rev-parse", f"refs/tags/{tag}"
        ):
            raise ValueError(
                "remote tag differs from the validated local annotated tag"
            )

    check_remote_tag()
    release = find_release(repository, tag)
    if release is None:
        with tempfile.TemporaryDirectory(prefix="igit-release-notes-") as temporary:
            notes = Path(temporary) / "notes.md"
            notes.write_text(release_notes(root, info["version"]), encoding="utf-8")
            gh(
                "release",
                "create",
                tag,
                "--repo",
                repository,
                "--verify-tag",
                "--draft",
                "--target",
                info["sha"],
                "--title",
                f"IGit {tag}",
                "--notes-file",
                str(notes),
                *(["--prerelease"] if info["prerelease"] else []),
            )
        # GitHub may acknowledge creation before the draft appears in listings.
        release = find_release(repository, tag)
        for delay in (1, 2, 4, 8, 16):
            if release is not None:
                break
            time.sleep(delay)
            release = find_release(repository, tag)
        if release is None:
            raise ValueError("created draft release was not visible after retries")
    endpoint = f"repos/{repository}/releases/{release['id']}"
    if release["tag_name"] != tag or release["target_commitish"] != info["sha"]:
        raise ValueError("existing release does not identify this exact tagged commit")
    if release["prerelease"] != info["prerelease"]:
        raise ValueError("existing release channel differs")
    if (release.get("body") or "").replace("\r\n", "\n").strip() != release_notes(
        root, info["version"]
    ).strip():
        raise ValueError("existing release notes differ from the tagged Changelog")
    missing = matching_assets(release, expected, tag, repository)
    if not release["draft"]:
        if missing:
            raise ValueError(
                "published release is missing assets; refusing to modify it"
            )
        return release["html_url"]
    if missing:
        gh(
            "release",
            "upload",
            tag,
            "--repo",
            repository,
            *(str(directory / name) for name in missing),
        )
    if matching_assets(api(endpoint), expected, tag, repository):
        raise ValueError("uploaded asset set is incomplete")
    check_remote_tag()
    gh(
        "release",
        "edit",
        tag,
        "--repo",
        repository,
        "--draft=false",
        *(["--latest=false"] if info["prerelease"] else []),
    )
    release = api(endpoint)
    if release["draft"] or matching_assets(release, expected, tag, repository):
        raise ValueError("release publication could not be verified")
    return release["html_url"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        print(
            publish(
                Path.cwd(),
                args.tag,
                args.directory,
                os.environ.get("GH_REPO", "yelog/igit"),
            )
        )
    except subprocess.CalledProcessError as error:
        parser.exit(1, f"release: GitHub command failed: {error.stderr or error}\n")
    except (ValueError, OSError, KeyError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
