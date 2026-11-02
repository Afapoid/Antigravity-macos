"""Exercise the real POSIX launcher in a controlling terminal with offline assets."""
import hashlib
import io
import os
from pathlib import Path
import pty
import select
import subprocess
import tarfile
import tempfile
import time
import unittest
import fcntl
import termios

ROOT = Path(__file__).resolve().parents[1]


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ag launcher ")
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.home = self.base / "home with spaces"
        self.bin = self.home / "Library/Application Support/AGUnlocker/ag_unlocker"
        self.mocks = self.base / "mocks"
        self.mocks.mkdir()
        self.home.mkdir()
        self.env = dict(os.environ, HOME=str(self.home),
                        PATH=f"{self.mocks}:{os.environ['PATH']}",
                        FIXTURES=str(self.base), FAKE_ARCH="arm64", FAKE_OS="Darwin")
        self.mock("uname", 'case "$1" in -s) echo "$FAKE_OS";; -m) echo "$FAKE_ARCH";; esac')
        self.mock("sysctl", 'echo "${FAKE_ROSETTA:-0}"')
        self.mock("file", 'echo "Mach-O 64-bit executable ${ASSET_ARCH:-arm64}"')
        self.mock("codesign", 'exit "${SIGN_FAIL:-0}"')
        self.mock("curl", '''exec python3 - "$@" <<'PY'
import os, sys, shutil
from pathlib import Path
args=sys.argv[1:]
if os.environ.get('OFFLINE'): sys.exit(22)
url=args[-1]
if url.endswith('/latest'):
 print('https://github.com/Afapoid/Antigravity-macos/releases/tag/v2.28.2.1',end='')
else:
 shutil.copyfile(Path(os.environ['FIXTURES'])/url.rsplit('/',1)[-1],args[args.index('-o')+1])
PY''')
        self.mock("shasum", '''exec python3 - "$@" <<'PY'
import hashlib,sys
from pathlib import Path
expected,name=Path(sys.argv[-1]).read_text().split()
sys.exit(0 if hashlib.sha256(Path(name).read_bytes()).hexdigest()==expected else 1)
PY''')
        self.asset = self.base / "AG_2.28.2.1_macos_arm64.tar.gz"
        data = b'#!/bin/sh\nprintf "LAUNCHED:%s\\n" "$1"\n'
        with tarfile.open(self.asset, "w:gz") as archive:
            member = tarfile.TarInfo("ag_unlocker")
            member.size = len(data)
            member.mode = 0o755
            archive.addfile(member, io.BytesIO(data))
        self.checksum = self.asset.with_name(self.asset.name + ".sha256")
        self.checksum.write_text(f"{hashlib.sha256(self.asset.read_bytes()).hexdigest()}  {self.asset.name}\n")

    def mock(self, name, body):
        path = self.mocks / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def launch(self):
        master, slave = pty.openpty()
        def terminal():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        proc = subprocess.Popen(["sh", str(ROOT / "tui.sh")], env=self.env,
                                stdin=slave, stdout=slave, stderr=slave,
                                preexec_fn=terminal)
        os.close(slave)
        chunks = []
        deadline = time.monotonic() + 15
        try:
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    if not data:
                        break
                    chunks.append(data)
                elif proc.poll() is not None:
                    break
            if proc.poll() is None:
                proc.wait(timeout=1)
            return proc.returncode, b"".join(chunks).decode(errors="replace")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            os.close(master)

    def test_native_install_and_cached_offline_launch(self):
        code, text = self.launch()
        self.assertEqual(code, 0, text)
        self.assertIn("LAUNCHED:--tui", text)
        self.assertEqual(Path(str(self.bin) + ".platform").read_text().strip(), "macos_arm64")
        self.env["OFFLINE"] = "1"
        code, text = self.launch()
        self.assertEqual(code, 0, text)
        self.assertIn("LAUNCHED:--tui", text)

    def test_rosetta_downloads_native_arm(self):
        self.env.update(FAKE_ARCH="x86_64", FAKE_ROSETTA="1")
        code, text = self.launch()
        self.assertEqual(code, 0, text)
        self.assertIn("LAUNCHED:--tui", text)

    def test_intel_mac_is_rejected(self):
        self.env["FAKE_ARCH"] = "x86_64"
        self.assertNotEqual(self.launch()[0], 0)
        self.assertFalse(self.bin.exists())

    def test_linux_is_rejected(self):
        self.env["FAKE_OS"] = "Linux"
        self.assertNotEqual(self.launch()[0], 0)

    def test_corrupt_download_keeps_previous_binary(self):
        self.bin.parent.mkdir(parents=True)
        self.bin.write_bytes(b"previous version")
        self.checksum.write_text(f"{'0' * 64}  {self.asset.name}\n")
        self.assertNotEqual(self.launch()[0], 0)
        self.assertEqual(self.bin.read_bytes(), b"previous version")
        self.assertEqual(list(self.bin.parent.glob(".download.*")), [])

    def test_wrong_architecture_is_not_installed(self):
        self.env["ASSET_ARCH"] = "x86_64"
        self.assertNotEqual(self.launch()[0], 0)
        self.assertFalse(self.bin.exists())

    def test_invalid_signature_is_not_installed(self):
        self.env["SIGN_FAIL"] = "1"
        self.assertNotEqual(self.launch()[0], 0)
        self.assertFalse(self.bin.exists())

    def test_offline_without_cache_fails(self):
        self.env["OFFLINE"] = "1"
        self.assertNotEqual(self.launch()[0], 0)


if __name__ == "__main__":
    unittest.main()
