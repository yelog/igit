#!/usr/bin/env python3
"""Exercise search, worksets, Activity, and restart through a disposable PTY."""

import argparse
import codecs
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unicodedata


CSI_PATTERN = re.compile(r"\x1b\[([0-?]*)([ -/]*)([@-~])")


class TerminalScreen:
    """Small ANSI terminal model for assertions against rendered text."""

    def __init__(self, columns, rows):
        self.columns = columns
        self.rows = rows
        self.grid = [[" "] * columns for _ in range(rows)]
        self.row = 0
        self.column = 0
        self.saved = None
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending_text = ""

    def resize(self, columns, rows):
        grid = []
        for row in range(rows):
            cells = self.grid[row][:columns] if row < self.rows else []
            grid.append(cells + [" "] * (columns - len(cells)))
        self.columns = columns
        self.rows = rows
        self.grid = grid
        self.row = min(self.row, rows - 1)
        self.column = min(self.column, columns - 1)

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.grid)

    def feed(self, data):
        text = self.pending_text + self.decoder.decode(data)
        self.pending_text = ""
        index = 0
        while index < len(text):
            char = text[index]
            if char == "\x1b":
                consumed = self._escape(text, index)
                if consumed is None:
                    self.pending_text = text[index:]
                    break
                index = consumed
                continue
            if char == "\r":
                self.column = 0
            elif char == "\n":
                self._next_line()
            elif char == "\b":
                self.column = max(0, self.column - 1)
            elif char == "\t":
                self.column = min(self.columns - 1, (self.column // 8 + 1) * 8)
            elif char >= " " and char != "\x7f":
                self._write(char)
            index += 1

    def _escape(self, text, index):
        if index + 1 >= len(text):
            return None
        next_char = text[index + 1]
        if next_char == "[":
            match = CSI_PATTERN.match(text, index)
            if match is None:
                return None
            self._csi(match.group(1), match.group(3))
            return match.end()
        if next_char == "]":
            end = text.find("\x07", index + 2)
            escaped_end = text.find("\x1b\\", index + 2)
            if end < 0 and escaped_end < 0:
                return None
            if end < 0 or (escaped_end >= 0 and escaped_end < end):
                return escaped_end + 2
            return end + 1
        if next_char == "7":
            self.saved = (self.row, self.column)
            return index + 2
        if next_char == "8":
            if self.saved is not None:
                self.row, self.column = self.saved
            return index + 2
        if next_char == "c":
            self.grid = [[" "] * self.columns for _ in range(self.rows)]
            self.row = 0
            self.column = 0
            return index + 2
        if index + 2 >= len(text):
            return None
        return index + 3

    def _csi(self, parameters, command):
        private = parameters.startswith("?")
        raw = parameters[1:] if private else parameters
        values = []
        for value in raw.split(";"):
            try:
                values.append(int(value) if value else 0)
            except ValueError:
                values.append(0)

        def value(position, default=1):
            return values[position] if position < len(values) and values[position] else default

        if command in ("H", "f"):
            self.row = min(self.rows - 1, max(0, value(0) - 1))
            self.column = min(self.columns - 1, max(0, value(1) - 1))
        elif command in ("G", "`"):
            self.column = min(self.columns - 1, max(0, value(0) - 1))
        elif command == "d":
            self.row = min(self.rows - 1, max(0, value(0) - 1))
        elif command in ("A", "B", "C", "D"):
            amount = value(0)
            if command == "A":
                self.row = max(0, self.row - amount)
            elif command == "B":
                self.row = min(self.rows - 1, self.row + amount)
            elif command == "C":
                self.column = min(self.columns - 1, self.column + amount)
            else:
                self.column = max(0, self.column - amount)
        elif command in ("E", "F"):
            amount = value(0)
            if command == "E":
                self.row = min(self.rows - 1, self.row + amount)
            else:
                self.row = max(0, self.row - amount)
            self.column = 0
        elif command == "J":
            self._erase_display(values[0] if values else 0)
        elif command == "K":
            self._erase_line(values[0] if values else 0)
        elif command == "s":
            self.saved = (self.row, self.column)
        elif command == "u" and self.saved is not None:
            self.row, self.column = self.saved
        elif command in ("h", "l") and private and 1049 in values:
            if command == "h":
                self.grid = [[" "] * self.columns for _ in range(self.rows)]
                self.row = 0
                self.column = 0
        elif command == "S":
            for _ in range(value(0)):
                self._scroll_up()
        elif command == "T":
            for _ in range(value(0)):
                self.grid.pop()
                self.grid.insert(0, [" "] * self.columns)
        elif command == "L":
            for _ in range(value(0)):
                self.grid.insert(self.row, [" "] * self.columns)
                self.grid.pop()
            self.column = 0
        elif command == "M":
            for _ in range(value(0)):
                self.grid.pop(self.row)
                self.grid.append([" "] * self.columns)
            self.column = 0
        elif command == "@":
            amount = min(value(0), self.columns - self.column)
            self.grid[self.row][self.column : self.column] = [" "] * amount
            del self.grid[self.row][self.columns :]
        elif command == "P":
            amount = min(value(0), self.columns - self.column)
            del self.grid[self.row][self.column : self.column + amount]
            self.grid[self.row].extend([" "] * amount)
        elif command == "X":
            end = min(self.columns, self.column + value(0))
            self.grid[self.row][self.column : end] = [" "] * (end - self.column)

    def _write(self, char):
        width = 0 if unicodedata.combining(char) else (
            2 if unicodedata.east_asian_width(char) in ("F", "W") else 1
        )
        if width == 0:
            return
        if self.column + width > self.columns:
            self._next_line()
        self.grid[self.row][self.column] = char
        if width == 2:
            self.grid[self.row][self.column + 1] = ""
        self.column += width
        if self.column >= self.columns:
            self.column = self.columns - 1

    def _next_line(self):
        if self.row + 1 >= self.rows:
            self._scroll_up()
            self.row = self.rows - 1
        else:
            self.row += 1

    def _scroll_up(self):
        self.grid.pop(0)
        self.grid.append([" "] * self.columns)

    def _erase_display(self, mode):
        if mode in (2, 3):
            self.grid = [[" "] * self.columns for _ in range(self.rows)]
            return
        if mode == 1:
            for row in range(self.row):
                self.grid[row] = [" "] * self.columns
            self.grid[self.row][: self.column + 1] = [" "] * (self.column + 1)
            return
        self.grid[self.row][self.column :] = [" "] * (self.columns - self.column)
        for row in range(self.row + 1, self.rows):
            self.grid[row] = [" "] * self.columns

    def _erase_line(self, mode):
        if mode == 1:
            self.grid[self.row][: self.column + 1] = [" "] * (self.column + 1)
        elif mode == 2:
            self.grid[self.row] = [" "] * self.columns
        else:
            self.grid[self.row][self.column :] = [" "] * (self.columns - self.column)


class App:
    def __init__(self, binary, workspace, config, state, panel=None):
        self.output = bytearray()
        self.screen = TerminalScreen(120, 36)
        self.screen_baseline = self.screen.text()
        self.requires_change = False
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.environ.update(
                TERM="xterm-256color",
                IGIT_STATE_DIR=str(state),
                GIT_CONFIG_NOSYSTEM="1",
                GIT_CONFIG_GLOBAL="/dev/null",
            )
            args = [str(binary), "--icons", "ascii", "--config", str(config)]
            if panel:
                args.append(panel)
            os.execv(str(binary), args + [str(workspace)])
        self.resize(120, 36)

    def resize(self, columns, rows):
        self.screen.resize(columns, rows)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)

    def send(self, keys):
        self.screen_baseline = self.screen.text()
        self.requires_change = True
        os.write(self.fd, keys.encode())

    def expect(self, text, timeout=12):
        deadline = time.monotonic() + timeout
        while True:
            rendered = self.screen.text()
            if self.requires_change and rendered != self.screen_baseline:
                self.requires_change = False
            if not self.requires_change and text in rendered:
                return time.monotonic()
            if time.monotonic() >= deadline:
                raise AssertionError(f"PTY did not render {text!r}; screen:\n{rendered.rstrip()}")
            ready, _, _ = select.select([self.fd], [], [], 0.05)
            if ready:
                try:
                    block = os.read(self.fd, 65536)
                except OSError as error:
                    raise AssertionError(f"PTY exited before rendering {text!r}") from error
                if not block:
                    raise AssertionError(f"PTY closed before rendering {text!r}")
                self.output.extend(block)
                self.screen.feed(block)

    def quit(self):
        self.send("q")
        deadline = time.monotonic() + 8
        retry = time.monotonic() + 0.75
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], 0.02)
            if ready:
                try:
                    block = os.read(self.fd, 65536)
                except OSError:
                    block = b""
                self.output.extend(block)
                self.screen.feed(block)
            if self.pid is not None and time.monotonic() >= retry:
                os.write(self.fd, b"q")
                retry = time.monotonic() + 0.75
            child, status = os.waitpid(self.pid, os.WNOHANG)
            if child:
                assert os.waitstatus_to_exitcode(status) == 0, "IGit exited with an error"
                self.pid = None
                os.close(self.fd)
                return
        tail = bytes(self.output[-6000:]).decode(errors="replace")
        raise AssertionError(f"IGit did not quit; output tail:\n{tail}")

    def close(self):
        if self.pid is not None:
            os.kill(self.pid, signal.SIGKILL)
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                child, _ = os.waitpid(self.pid, os.WNOHANG)
                if child:
                    break
                select.select([], [], [], 0.02)
            self.pid = None
            os.close(self.fd)


def git(repo, *args, data=None):
    env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL="/dev/null")
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        input=data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        check=True,
    ).stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    metrics = {}
    with tempfile.TemporaryDirectory(prefix="igit-reliability-") as temporary:
        root = Path(temporary)
        workspace = root / "workspace"
        workspace.mkdir()
        repo = workspace / "repo"
        repo.mkdir()
        git(repo, "init", "-q", "-b", "main")
        git(repo, "config", "core.hooksPath", "/dev/null")
        records = bytearray()
        for index in range(400):
            message = "first\n\nold body needle" if index == 0 else f"recent-{index}"
            body = message.encode()
            records.extend(
                f"commit refs/heads/main\ncommitter Probe <probe@example.invalid> {1700000000 + index} +0000\ndata {len(body)}\n".encode()
                + body
                + b"\n\n"
            )
        git(repo, "fast-import", "--quiet", data=records)
        (repo / "pending-smoke.txt").write_text("Untracked content stays unchanged.\n")
        before = (git(repo, "rev-parse", "HEAD"), git(repo, "status", "--porcelain=v1"))
        config = root / "settings.toml"
        config.write_text("[fetch]\nenabled=false\n[updates]\ncheck_on_startup=false\n")
        state = root / "state"
        apps = []
        try:
            started = time.monotonic()
            app = App(binary, workspace, config, state)
            apps.append(app)
            metrics["startup_ready_seconds"] = app.expect("pending-smoke.txt") - started
            app.send("r")
            app.expect("Workspace roots")
            app.send("s")
            app.expect("Name:")
            app.send("Focused\r")
            app.expect("Workspace roots")
            app.send(" \r")
            app.expect("0 roots in scope")
            app.quit()
            session_path = next(state.glob("*/session.json"))
            saved = json.loads(session_path.read_text())
            assert saved["roots"] == []
            assert "Focused" in saved["worksets"]

            app = App(binary, workspace, config, state)
            apps.append(app)
            app.expect("0 files selected · 0 roots")
            app.send("r")
            app.expect("Workspace roots")
            app.send("w")
            app.expect("Roots worksets")
            app.send("\r")
            app.expect("Workspace roots")
            app.send("\r")
            app.expect("1 roots in scope")
            app.send("2")
            app.expect("recent-399")
            started = time.monotonic()
            app.send("/old body needle\r")
            metrics["complete_search_seconds"] = app.expect("first") - started
            app.send("\x1b[19~")
            app.expect("Activity / Recovery")
            app.send("\x1b")
            app.expect("old body needle")
            app.resize(44, 12)
            app.send("\x1b[19~")
            app.expect("Activity / Recovery")
            app.send("\x1b")
            app.expect("old body needle")
            app.resize(120, 36)
            app.quit()
            saved = json.loads(session_path.read_text())
            assert saved["tab"] == "Log"
            assert saved["log"]["views"][0]["query"] == "old body needle"

            app = App(binary, workspace, config, state)
            apps.append(app)
            app.expect("old body needle")
            app.expect("first")
            app.quit()
            app = App(binary, workspace, config, state, "commit")
            apps.append(app)
            app.expect("pending-smoke.txt")
            app.quit()
            assert json.loads(session_path.read_text())["tab"] == "Commit"
            after = (git(repo, "rev-parse", "HEAD"), git(repo, "status", "--porcelain=v1"))
            assert before == after, "PTY browsing changed repository state"
            print(json.dumps({"status": "passed", **metrics}, indent=2))
        finally:
            for app in apps:
                app.close()


if __name__ == "__main__":
    main()
