"""Open the actual release TUI in a terminal, then leave with Ctrl+C."""
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="ag-native-smoke-") as home:
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))

    def terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    env = dict(os.environ, HOME=home, TERM="xterm-256color")
    env.pop("AG_UNLOCKER_DEV_SKIP_KEY", None)
    proc = subprocess.Popen([binary, "--tui"], env=env, stdin=slave, stdout=slave,
                            stderr=slave, preexec_fn=terminal)
    os.close(slave)
    transcript = bytearray()
    rendered = False
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    break
                if not data:
                    break
                transcript.extend(data)
                if b"Antigravity" in transcript:
                    rendered = True
                    os.write(master, b"\x03")
                    break
            elif proc.poll() is not None:
                break
        assert rendered, f"TUI did not render: {transcript[-2000:]!r}"
        assert proc.wait(timeout=10) == 0, f"TUI exit code: {proc.returncode}"
        print("Native ARM64 TUI rendered and exited cleanly")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        os.close(master)
