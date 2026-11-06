#!/usr/bin/env python3
"""Recommend a release from local tags after the caller has fetched origin."""

import argparse
import json
import re
import subprocess

VERSION = re.compile(
    r"v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-beta\.([1-9][0-9]*))?"
)


def parse_version(value):
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError(f"invalid version: {value}")
    return tuple(int(part) for part in match.groups()[:3]), (
        int(match[4]) if match[4] else None
    )


def recommend(tags, channel, override=None):
    releases = {}
    for tag in tags:
        if not tag.startswith("v"):
            continue
        try:
            releases[tag] = parse_version(tag)
        except ValueError:
            continue
    stable = max(
        (base for base, beta in releases.values() if beta is None), default=None
    )
    baseline = "v" + ".".join(map(str, stable)) if stable else None
    if override is not None:
        base, beta = parse_version(override)
        if (channel == "beta") != (beta is not None):
            raise ValueError("override does not match release channel")
        if stable is not None and base <= stable:
            raise ValueError("override must be newer than the latest stable release")
        reason = "explicit version override"
    else:
        if stable is None:
            raise ValueError("no stable tag; specify an explicit version")
        major, minor, patch = stable
        if minor > 9 or patch > 9 or (minor == 9 and patch == 9):
            raise ValueError(
                "undefined rollover or major boundary; specify an explicit version"
            )
        base = (major, minor, patch + 1) if patch < 9 else (major, minor + 1, 0)
        active = {
            line
            for line, number in releases.values()
            if number is not None and line > stable
        }
        if active - {base}:
            raise ValueError(
                "conflicting unpublished Beta line; specify an explicit version"
            )
        beta = None
        reason = "PATCH + 1" if patch < 9 else "PATCH 9 -> 0; MINOR + 1"
        if channel == "beta":
            beta = (
                max(
                    (
                        number
                        for line, number in releases.values()
                        if line == base and number is not None
                    ),
                    default=0,
                )
                + 1
            )
            reason += "; increment same-line Beta number"
        elif base in active:
            reason += "; promote same-line Beta to stable"
    version = ".".join(map(str, base))
    if beta is not None:
        version += f"-beta.{beta}"
    tag = "v" + version
    if tag in tags:
        raise ValueError(f"tag already exists: {tag}")
    if beta is not None and any(
        line == base and number is not None and number >= beta
        for line, number in releases.values()
    ):
        raise ValueError(
            "Beta override must advance the existing same-line Beta number"
        )
    return {
        "baseline": baseline,
        "channel": channel,
        "version": version,
        "base_version": ".".join(map(str, base)),
        "tag": tag,
        "reason": reason,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("channel", choices=("stable", "beta"))
    parser.add_argument("--override", help="explicit version, with optional v prefix")
    args = parser.parse_args()
    try:
        tags = subprocess.check_output(["git", "tag", "--list"], text=True).splitlines()
        result = recommend(tags, args.channel, args.override)
        if result["baseline"]:
            subprocess.run(
                ["git", "merge-base", "--is-ancestor", result["baseline"], "HEAD"],
                check=True,
            )
        print(json.dumps(result))
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
