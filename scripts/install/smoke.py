#!/usr/bin/env python3
"""Install an actual archive or public channel into an isolated temporary root."""

import argparse
import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/release"))
from distribution import TARGETS, archive_name  # noqa: E402

from pages import unix_script  # noqa: E402


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def execute(command, env, input_text=None):
    result = subprocess.run(command, input=input_text, text=True, encoding="utf-8", errors="replace",
                            capture_output=True, env=env, timeout=300)
    if result.returncode:
        raise ValueError("Command failed: %s\n%s\n%s" % (command[0], result.stdout, result.stderr))
    return result.stdout.strip()


def launcher(command, arguments, env, powershell):
    if os.name != "nt":
        return execute([str(command), *arguments], env)
    # Literal PowerShell strings avoid cmd.exe quoting of paths containing spaces.
    quoted = ["'" + str(value).replace("'", "''") + "'" for value in [command, *arguments]]
    script = "[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false); & " + " ".join(quoted) + "; exit $LASTEXITCODE"
    encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    return execute([powershell, "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded], env)


def snapshot(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in root.rglob("*") if path.is_file() and not path.is_symlink()}


def smoke(args, temporary, site_url):
    root, commands = temporary / "data [literal]", temporary / "bin [literal]"
    env = dict(os.environ)
    for key in ("IGIT_CHANNEL", "IGIT_CHANNEL_LOCKED", "IGIT_VERSION", "IGIT_INSTALL_ROOT", "IGIT_INSTALL_DIR"):
        env.pop(key, None)
    env["IGIT_CHANNEL_BASE_URL"] = site_url + "/channels"
    windows = args.target.endswith("windows-msvc")
    installer = temporary / ("install.ps1" if windows else "install.sh")
    filename = "install.ps1.txt" if windows else ("install-beta.sh" if args.channel == "beta" else "install.sh")
    execute(["curl.exe" if windows else "curl", "--fail", "--silent", "--show-error", "--location",
             "--connect-timeout", "15", "--max-time", "120", "--output", str(installer), site_url + "/" + filename], env)

    def install(check=False):
        if windows:
            command = [args.powershell, "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(installer),
                       "-Channel", args.channel, "-Version", args.version, "-InstallRoot", str(root),
                       "-InstallDir", str(commands), "-NoModifyPath", "-Json"]
            return execute(command + (["-Check"] if check else []), env)
        command = ["sh", "-s", "--", "--version", args.version, "--install-root", str(root),
                   "--install-dir", str(commands), "--no-modify-path", "--json"]
        return execute(command + (["--check"] if check else []), env, installer.read_text(encoding="utf-8"))

    report = json.loads(install())
    if report["status"] != "installed" or report["target_version"] != args.version:
        raise ValueError("Installer did not activate the expected version")
    before = snapshot(root)
    update_env = dict(env, IGIT_CHANNEL="stable" if args.channel == "beta" else "beta",
                      IGIT_CHANNEL_LOCKED="invalid", IGIT_VERSION="invalid",
                      IGIT_INSTALL_ROOT=str(temporary / "foreign-root"), IGIT_INSTALL_DIR=str(temporary / "foreign-bin"))
    legacy = False
    for name in ("igit", "ig"):
        command = commands / (name + ".cmd" if windows else name)
        if launcher(command, ["--version"], env, args.powershell) != "IGit %s — terminal Git workbench" % args.version:
            raise ValueError("Installed command reports a different version: " + name)
        help_text = launcher(command, ["update", "--help"], env, args.powershell)
        if "Usage: igit update" not in help_text:
            if not args.allow_legacy:
                raise ValueError("The built archive does not provide the update command")
            legacy = True
            continue
        for flags in (["--check", "--json"], ["--json"]):
            report = json.loads(launcher(command, ["update", *flags], update_env, args.powershell))
            if report["status"] != "up_to_date" or report["channel"] != args.channel or report["current_version"] != args.version:
                raise ValueError("Installed updater returned an unexpected result")
    if json.loads(install(check=True))["status"] != "up_to_date" or snapshot(root) != before:
        raise ValueError("A check or no-op update changed the installation")
    if (temporary / "foreign-root").exists() or (temporary / "foreign-bin").exists():
        raise ValueError("Ambient installer variables redirected the update")
    print("Installed and verified both commands: %s (%s, %s)%s" % (
        args.version, args.target, args.channel,
        "; legacy release uses installer checks" if legacy else "; built-in update verified",
    ))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--archive", type=Path)
    source.add_argument("--site-url")
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--channel", choices=("stable", "beta"))
    parser.add_argument("--powershell", default="powershell.exe")
    parser.add_argument("--allow-legacy", action="store_true", help="Allow published binaries that predate the update command")
    args = parser.parse_args()
    args.channel = args.channel or ("beta" if "-beta." in args.version else "stable")
    with tempfile.TemporaryDirectory(prefix="igit-install-smoke-") as temp:
        temporary = Path(temp).resolve()
        if args.site_url:
            smoke(args, temporary, args.site_url.rstrip("/"))
            return
        site = temporary / "site"
        (site / "channels").mkdir(parents=True)
        server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(site)))
        url = "http://127.0.0.1:%s" % server.server_port
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            assets = {}
            for target in TARGETS:
                relative = "yelog/igit/releases/download/v%s/%s" % (args.version, archive_name(args.version, target))
                digest = "0" * 64
                if target == args.target:
                    destination = site / relative
                    destination.parent.mkdir(parents=True)
                    shutil.copyfile(args.archive, destination)
                    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
                assets[target] = {"url": url + "/" + relative, "sha256": digest}
            manifest = dict(schema=1, product="igit", channel=args.channel, version=args.version,
                            tag="v" + args.version, prerelease=args.channel == "beta", assets=assets)
            (site / "channels" / (args.channel + ".json")).write_text(json.dumps(manifest), encoding="utf-8")
            for channel, name in (("stable", "install.sh"), ("beta", "install-beta.sh")):
                (site / name).write_text(unix_script(ROOT, channel), encoding="utf-8")
            shutil.copyfile(ROOT / "pages/install.ps1", site / "install.ps1.txt")
            smoke(args, temporary, url)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


if __name__ == "__main__":
    main()
