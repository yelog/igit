"""Exercise the command lock using a fake Cargo, without compiling Rust."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

WRAPPER = Path(__file__).with_name("cargo_serial.py")
ROOT = WRAPPER.parent.parent


@unittest.skipUnless(os.name == "posix", "local POSIX command lock")
class SerialCargoTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.events = base / "events.jsonl"
        fake = base / "cargo"
        fake.write_text("#!" + sys.executable + "\n" + '''import json, os, sys, time
from pathlib import Path
p = Path(os.environ['FAKE_EVENTS'])
with p.open('a') as f:
    f.write(json.dumps({'kind':'start', 'time':time.monotonic(), 'pid':os.getpid(),
                       'args':sys.argv[1:], 'cwd':os.getcwd(), 'sid':os.getsid(0),
                       'target':os.environ['CARGO_TARGET_DIR'],
                       'jobs':os.environ['CARGO_BUILD_JOBS'],
                       'threads':os.environ['RUST_TEST_THREADS']})+'\\n')
time.sleep(float(os.environ.get('FAKE_SLEEP', '.2')))
with p.open('a') as f:
    f.write(json.dumps({'kind':'end', 'time':time.monotonic(), 'pid':os.getpid()})+'\\n')
sys.exit(int(os.environ.get('FAKE_EXIT', '0')))
''')
        fake.chmod(0o700)
        self.env = dict(os.environ, PATH=str(base) + os.pathsep + os.environ.get("PATH", ""),
                        XDG_CACHE_HOME=str(base / "cache"), FAKE_EVENTS=str(self.events),
                        CARGO_TARGET_DIR="target", CARGO_BUILD_JOBS="99", RUST_TEST_THREADS="99")

    def start(self, *args, **extra):
        p = subprocess.Popen([sys.executable, str(WRAPPER), *args],
                             env=dict(self.env, **extra), stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
        self.addCleanup(lambda: p.kill() if p.poll() is None else None)
        return p

    def read_events(self):
        return [json.loads(line) for line in self.events.read_text().splitlines()]

    def test_commands_do_not_overlap_and_force_defaults(self):
        a = self.start("test", "--locked", "--lib", "filter")
        b = self.start("clippy", "--locked", "--", "-D", "warnings")
        self.assertEqual(a.wait(timeout=10), 0)
        self.assertEqual(b.wait(timeout=10), 0)
        rows = self.read_events()
        self.assertEqual([r["kind"] for r in rows], ["start", "end", "start", "end"])
        for row in (rows[0], rows[2]):
            self.assertEqual(row["cwd"], str(ROOT))
            self.assertEqual(row["target"], str(ROOT / "target-serial"))
            self.assertEqual((row["jobs"], row["threads"]), ("1", "1"))
        self.assertEqual({tuple(r["args"]) for r in rows if r["kind"] == "start"},
                         {("test", "--locked", "--lib", "filter"),
                          ("clippy", "--locked", "--", "-D", "warnings")})

    def test_failure_releases_lock(self):
        self.assertEqual(self.start("check", FAKE_EXIT="7").wait(timeout=10), 7)
        self.assertEqual(self.start("build").wait(timeout=10), 0)

    def test_run_preserves_terminal_session(self):
        self.assertEqual(self.start("run", "--bin", "ig").wait(timeout=5), 0)
        self.assertEqual(self.read_events()[0]["sid"], os.getsid(0))

    def test_interrupt_reaps_child_and_releases_lock(self):
        p = self.start("test", FAKE_SLEEP="30")
        deadline = time.monotonic() + 5
        while not self.events.exists() and time.monotonic() < deadline:
            time.sleep(.02)
        self.assertTrue(self.events.exists())
        child_pid = self.read_events()[0]["pid"]
        p.send_signal(signal.SIGINT)
        self.assertEqual(p.wait(timeout=5), 130)
        with self.assertRaises(ProcessLookupError):
            os.kill(child_pid, 0)
        self.assertEqual(self.start("check").wait(timeout=5), 0)

    def test_rejects_bypasses_and_clean(self):
        for args in [("build", "-j8"), ("test", "--target-dir=target"),
                     ("test", "--", "--test-threads=8"), ("clean",)]:
            self.assertEqual(self.start(*args).wait(timeout=5), 2)
        self.assertFalse(self.events.exists())


if __name__ == "__main__":
    unittest.main()
