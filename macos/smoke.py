"""Open the actual release TUI in a bounded pseudo-terminal session."""
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve())


def timed_out(signum, frame):
    raise TimeoutError("native TUI smoke test exceeded 40 seconds")


signal.signal(signal.SIGALRM, timed_out)
signal.alarm(40)
print("Smoke: creating terminal", flush=True)
with tempfile.TemporaryDirectory(prefix="ag-native-smoke-") as home:
    env = dict(os.environ, HOME=home, TERM="xterm-256color")
    env.pop("AG_UNLOCKER_DEV_SKIP_KEY", None)
    pid, master = pty.fork()
    if pid == 0:
        os.execve(binary, [binary, "--tui"], env)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    os.set_blocking(master, False)
    print(f"Smoke: started child {pid}", flush=True)
    transcript = bytearray()
    rendered = False
    exited = False
    try:
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                try:
                    data = os.read(master, 65536)
                except BlockingIOError:
                    continue
                except OSError:
                    data = b""
                if data:
                    transcript.extend(data)
                    # Answer terminal probes emitted by crossterm.
                    if b"\x1b[6n" in data:
                        os.write(master, b"\x1b[1;1R")
                    if b"\x1b[?u" in data:
                        os.write(master, b"\x1b[?0u")
                    if b"\x1b[c" in data:
                        os.write(master, b"\x1b[?1;2c")
                    if not rendered and b"Antigravity" in transcript:
                        rendered = True
                        print("Smoke: TUI rendered; sending Ctrl+C", flush=True)
                        os.write(master, b"\x03")
            child, status = os.waitpid(pid, os.WNOHANG)
            if child:
                exited = True
                assert rendered, f"TUI did not render: {transcript[-2000:]!r}"
                assert os.waitstatus_to_exitcode(status) == 0, f"TUI failed: {status}, {transcript[-2000:]!r}"
                print("Native ARM64 TUI rendered and exited cleanly", flush=True)
                break
        assert exited, f"TUI did not exit in 25 seconds: {transcript[-2000:]!r}"
    finally:
        if not exited:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            # No blocking wait: even cleanup must honor the overall deadline.
            os.waitpid(pid, os.WNOHANG)
        os.close(master)
signal.alarm(0)
