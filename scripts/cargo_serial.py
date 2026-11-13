#!/usr/bin/env python3
"""Serialize local Cargo commands and keep IGit application builds paired."""
from fnmatch import fnmatchcase
import os
from pathlib import Path
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = {"build", "check", "clippy", "fmt", "install", "run", "test"}
BINARIES = ("igit", "ig")


def validate(args):
    if not args or args[0] not in COMMANDS:
        raise ValueError("Usage: python3 scripts/cargo_serial.py {build,check,clippy,fmt,install,run,test} [args]")
    cargo_args = args[:args.index("--")] if "--" in args else args
    for arg in cargo_args:
        if arg.startswith(("-j", "--jobs", "--target-dir", "--config")):
            raise ValueError("Do not override jobs, target-dir or Cargo config in the serial entry point.")
    if args[0] == "test":
        for index, arg in enumerate(args):
            if arg == "--test-threads":
                if index + 1 == len(args) or args[index + 1] != "1":
                    raise ValueError("The serial entry point requires --test-threads=1.")
            elif arg.startswith("--test-threads=") and arg != "--test-threads=1":
                raise ValueError("The serial entry point requires --test-threads=1.")


def paired_build_args(args):
    if args[0] != "build":
        return args
    boundary = args.index("--") if "--" in args else len(args)
    cargo_args = args[:boundary]
    if "--bins" in cargo_args or "--all-targets" in cargo_args:
        return args
    selected = set()
    for index, arg in enumerate(cargo_args):
        if arg == "--bin" and index + 1 < len(cargo_args):
            selector = cargo_args[index + 1]
        elif arg.startswith("--bin="):
            selector = arg[len("--bin="):]
        else:
            continue
        selected.update(name for name in BINARIES if fnmatchcase(name, selector))
    if len(selected) != 1:
        return args
    companion = next(name for name in BINARIES if name not in selected)
    return [*cargo_args, "--bin", companion, *args[boundary:]]


def main(args=None):
    args = sys.argv[1:] if args is None else args
    try:
        validate(args)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2
    if os.name != "posix":
        print("This local command lock supports macOS/Linux; use Cargo directly on Windows.", file=sys.stderr)
        return 2
    requested_args = args
    args = paired_build_args(args)
    import fcntl

    default_cache = (Path.home() / "Library/Caches" if sys.platform == "darwin"
                     else Path.home() / ".cache")
    cache = Path(os.environ.get("XDG_CACHE_HOME") or default_cache) / "igit"
    cache.mkdir(parents=True, exist_ok=True)
    # Shared by all IGit checkouts for this user. Never unlink a lock inode.
    lock_path = cache / "cargo-serial.lock"
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target-serial"),
               CARGO_BUILD_JOBS="1", RUST_TEST_THREADS="1")
    with lock_path.open("a+b") as lock:
        print(f"Waiting for IGit Cargo lock: {lock_path}", flush=True)
        try:
            fcntl.flock(lock, fcntl.LOCK_EX)
        except KeyboardInterrupt:
            return 130
        if args != requested_args:
            print("Building both IGit commands (igit and ig) to keep local executables in sync.", flush=True)
        print(f"Running cargo {' '.join(args)}\nTarget: {env['CARGO_TARGET_DIR']}; build jobs: 1; test threads: 1", flush=True)
        # The parent owns the lock; Cargo descendants must not inherit its fd.
        child = None
        pending = []
        interactive = args[0] == "run"

        def send(signum):
            try:
                if interactive:
                    child.send_signal(signum)
                else:
                    os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

        def forward(signum, _frame):
            pending.append(signum)
            if child is not None:
                send(signum)

        previous = {sig: signal.signal(sig, forward) for sig in (signal.SIGINT, signal.SIGTERM)}
        try:
            if pending:
                return 128 + pending[-1]
            child = subprocess.Popen(["cargo", *args], cwd=ROOT, env=env,
                                     start_new_session=not interactive, close_fds=True)
            for sig in pending:
                send(sig)
            status = child.wait()
            return status if status >= 0 else 128 - status
        except OSError as error:
            print(f"Cannot run Cargo: {error}", file=sys.stderr)
            return 1
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)


if __name__ == "__main__":
    sys.exit(main())
