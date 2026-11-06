#!/usr/bin/env python3
"""IGit release metadata; Python 3.11+, Git, and no third-party dependencies."""

import argparse
import datetime
import importlib.util
import json
import re
import subprocess
import tomllib
from pathlib import Path

PACKAGE = "igit-workbench"
REPOSITORY = "https://github.com/yelog/igit"
spec = importlib.util.spec_from_file_location(
    "next_version", Path(__file__).with_name("next-version.py")
)
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], text=True, encoding="utf-8"
    ).rstrip("\n")


def version_info(version):
    base, beta = policy.parse_version(version)
    normalized = ".".join(map(str, base)) + (
        f"-beta.{beta}" if beta is not None else ""
    )
    return {
        "version": normalized,
        "tag": f"v{normalized}",
        "channel": "beta" if beta is not None else "stable",
        "prerelease": beta is not None,
    }


def cargo_version(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    if manifest["package"]["name"] != PACKAGE:
        raise ValueError(f"expected package {PACKAGE}")
    version = manifest["package"]["version"]
    matches = [p for p in lock["package"] if p["name"] == PACKAGE and "source" not in p]
    if len(matches) != 1 or matches[0]["version"] != version:
        raise ValueError("Cargo.toml and Cargo.lock package versions must match")
    version_info(version)
    return version


def set_version(root, version):
    version = version_info(version)["version"]
    cargo_version(root)
    changes = []
    for filename, table in [("Cargo.toml", "[package]"), ("Cargo.lock", "[[package]]")]:
        path = root / filename
        original = path.read_text(encoding="utf-8")
        pattern = re.compile(r"(?ms)^" + re.escape(table) + r"\s*\n.*?(?=^\[|\Z)")
        count = 0

        def replace(match, filename=filename):
            nonlocal count
            block = match.group()
            data = tomllib.loads(block)["package"]
            package = data[0] if isinstance(data, list) else data
            if package["name"] != PACKAGE or "source" in package:
                return block
            updated, replacements = re.subn(
                r'(?m)^(version\s*=\s*)"[^"]+"', lambda m: m[1] + f'"{version}"', block
            )
            if replacements != 1:
                raise ValueError(f"expected one package version in {filename}")
            count += 1
            return updated

        updated = pattern.sub(replace, original)
        if count != 1:
            raise ValueError(f"expected one local package in {filename}")
        tomllib.loads(updated)
        changes.append((path, updated))
    # Prepare both edits before writing; never resolve or update dependencies.
    for path, updated in changes:
        path.write_text(updated, encoding="utf-8", newline="\n")
    return cargo_version(root)


def changelog_section(text, version):
    version = version_info(version)["version"]
    headers = list(re.finditer(r"(?m)^## \[" + re.escape(version) + r"\].*$", text))
    if len(headers) != 1:
        raise ValueError(f"expected exactly one Changelog section for {version}")
    header = headers[0]
    match = re.fullmatch(
        r"## \[" + re.escape(version) + r"\] - (\d{4}-\d{2}-\d{2})", header.group()
    )
    if not match:
        raise ValueError("expected a dated Changelog heading")
    datetime.date.fromisoformat(match[1])
    end = re.search(r"(?m)^## ", text[header.end() :])
    section = text[
        header.start() : header.end() + end.start() if end else len(text)
    ].strip()
    categories = re.findall(r"(?m)^### (.+)$", section)
    allowed = {
        "Added",
        "Changed",
        "Fixed",
        "Security",
        "Deprecated",
        "Removed",
        "Internal",
        "Commits",
    }
    if len(categories) != len(set(categories)) or set(categories) - allowed:
        raise ValueError("unknown or repeated Changelog category")
    if "Commits" not in categories or len(categories) < 2:
        raise ValueError("Changelog needs a summary category and a Commits list")
    for subsection in re.split(r"(?m)^### .+\n", section)[1:]:
        if not re.search(r"(?m)^- \S", subsection):
            raise ValueError("empty Changelog category")
    commits = section.split("### Commits\n", 1)[1].split("\n### ", 1)[0]
    links = re.findall(
        r"(?m)^- \[`([0-9a-f]{7,40})`\]\("
        + re.escape(REPOSITORY)
        + r"/commit/([0-9a-f]{7,40})\) .+",
        commits,
    )
    bullets = re.findall(r"(?m)^- .+", commits)
    if (
        not links
        or len(links) != len(bullets)
        or len({sha for _, sha in links}) != len(links)
    ):
        raise ValueError("Commits must contain unique linked Git commits")
    if any(not sha.startswith(short) for short, sha in links):
        raise ValueError("commit link does not match its short SHA")
    return section + "\n"


def update_changelog(root, version, body, date):
    version = version_info(version)["version"]
    datetime.date.fromisoformat(date)
    path = root / "CHANGELOG.md"
    text = path.read_text(encoding="utf-8")
    if re.search(r"(?m)^## \[" + re.escape(version) + r"\]", text):
        raise ValueError(f"Changelog section already exists: {version}")
    if re.search(r"(?m)^## ", body):
        raise ValueError("release body must contain subsections, not release headings")
    section = f"## [{version}] - {date}\n\n{body.strip()}\n"
    changelog_section(section, version)
    marker = re.search(r"(?m)^## ", text)
    if not marker:
        raise ValueError("Changelog must contain an Unreleased or release section")
    updated = text[: marker.start()] + section + "\n" + text[marker.start() :]
    path.write_text(updated, encoding="utf-8", newline="\n")


def release_notes(root, version):
    section = changelog_section(
        (root / "CHANGELOG.md").read_text(encoding="utf-8"), version
    )
    return (
        re.sub(
            r"(?ms)^### Commits\n(.*?)(?=^### |\Z)",
            lambda m: (
                "### Commits\n\n<details>\n<summary>Show commits</summary>\n\n"
                + m[1].strip()
                + "\n\n</details>\n\n"
            ),
            section,
        ).rstrip()
        + "\n"
    )


def collect_commits(root, channel, base_version):
    base, beta = policy.parse_version(base_version)
    if beta is not None:
        raise ValueError("collect requires a base version without a Beta suffix")
    stable = []
    betas = []
    for tag in git(root, "tag", "--list", "v*").splitlines():
        try:
            line, number = policy.parse_version(tag)
        except ValueError:
            continue
        if number is None and line < base:
            stable.append((line, tag))
        elif line == base and number is not None:
            betas.append((number, tag))
    baseline = (
        max(betas)[1]
        if channel == "beta" and betas
        else max(stable)[1]
        if stable
        else None
    )
    if baseline:
        git(root, "merge-base", "--is-ancestor", baseline, "HEAD")
    revisions = f"{baseline}..HEAD" if baseline else "HEAD"
    raw = git(
        root,
        "log",
        "--reverse",
        "--format=%H%x00%h%x00%an%x00%aI%x00%s%x00%b%x00",
        revisions,
    )
    fields = raw.split("\0")
    if fields[-1].strip():
        raise ValueError("unable to parse commit records")
    fields.pop()
    if len(fields) % 6:
        raise ValueError("incomplete commit record")
    commits = []
    for start in range(0, len(fields), 6):
        sha, short, author, date, subject, body = fields[start : start + 6]
        commits.append(
            {
                "sha": sha.strip(),
                "short_sha": short,
                "author": author,
                "date": date,
                "subject": subject,
                "body": body.rstrip(),
            }
        )
    return {
        "baseline": baseline,
        "head": git(root, "rev-parse", "HEAD"),
        "commits": commits,
    }


def validate(root, tag, pre_tag=False):
    info = version_info(tag)
    if tag != info["tag"]:
        raise ValueError("release tags must start with v")
    if cargo_version(root) != info["version"]:
        raise ValueError("tag does not match Cargo.toml/Cargo.lock")
    release_notes(root, info["version"])
    if not pre_tag:
        ref = f"refs/tags/{tag}"
        if git(root, "cat-file", "-t", ref) != "tag":
            raise ValueError("release tag must be annotated")
        head = git(root, "rev-parse", "HEAD")
        if git(root, "rev-parse", f"{ref}^{{commit}}") != head:
            raise ValueError("HEAD must be the exact tagged commit")
        git(root, "merge-base", "--is-ancestor", head, "refs/remotes/origin/main")
        if git(root, "status", "--porcelain", "--untracked-files=normal"):
            raise ValueError("tagged source must have a clean worktree and index")
        info["sha"] = head
    return info


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("version")
    commands.add_parser("info")
    setter = commands.add_parser("set-version")
    setter.add_argument("version")
    validator = commands.add_parser("validate")
    validator.add_argument("tag")
    validator.add_argument("--pre-tag", action="store_true")
    notes = commands.add_parser("notes")
    notes.add_argument("version")
    update = commands.add_parser("update-changelog")
    update.add_argument("version")
    update.add_argument("body", type=Path)
    update.add_argument("date", help="release date in YYYY-MM-DD")
    collect = commands.add_parser("collect")
    collect.add_argument("channel", choices=("beta", "stable"))
    collect.add_argument("base_version")
    args = parser.parse_args()
    try:
        root = args.root.resolve()
        if args.command == "version":
            print(cargo_version(root))
        elif args.command == "info":
            print(json.dumps(version_info(cargo_version(root))))
        elif args.command == "set-version":
            print(set_version(root, args.version))
        elif args.command == "validate":
            print(json.dumps(validate(root, args.tag, args.pre_tag)))
        elif args.command == "notes":
            print(release_notes(root, args.version), end="")
        elif args.command == "update-changelog":
            update_changelog(
                root, args.version, args.body.read_text(encoding="utf-8"), args.date
            )
        elif args.command == "collect":
            print(json.dumps(collect_commits(root, args.channel, args.base_version)))
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
