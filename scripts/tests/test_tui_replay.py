"""Capture validation and process ownership regression; uses a fake CLI, no devices."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "tui-replay.py"
SPEC = importlib.util.spec_from_file_location("tui_replay", SCRIPT)
REPLAY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPLAY)


class ReplayTests(unittest.TestCase):
    def test_live_mode_rejects_write_keys_before_launch(self):
        with tempfile.TemporaryDirectory() as temp:
            steps = Path(temp) / "steps.json"
            for key in ["p", "R", "d", ":format\r", "rR"]:
                steps.write_text(json.dumps([{"keys": key}]))
                with self.assertRaises(ValueError):
                    REPLAY.load_steps(steps, True)
            steps.write_text(json.dumps([{"keys": ":backups\r", "resize": [200, 60]}]))
            self.assertEqual(len(REPLAY.load_steps(steps, True)), 1)

    def test_timeout_cleans_only_its_owned_process_and_records_final_status(self):
        unrelated = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        try:
            with tempfile.TemporaryDirectory() as temp:
                fake = Path(temp) / "fake-edpcli"
                fake.write_text(f"#!{sys.executable}\nimport sys,time\nif sys.argv[1]=='version': print('fake test binary')\nelse: time.sleep(30)\n")
                fake.chmod(0o755)
                result = subprocess.run([sys.executable, str(SCRIPT), "--binary", str(fake), "--timeout", "1", "--output", temp], capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 124, result.stderr)
                manifest = json.loads(Path(result.stdout.strip()).read_text())
                self.assertTrue(manifest["timed_out"])
                self.assertIsNotNone(manifest["exit_code"])
                self.assertEqual(manifest["size"], [200, 60])
                self.assertEqual(len(manifest["sha256"]), 64)
                self.assertIsNone(unrelated.poll())
        finally:
            unrelated.terminate()
            unrelated.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
