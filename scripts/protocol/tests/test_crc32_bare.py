"""Behavior tests for the standalone CRC-32 calculator."""

from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import sys
import unittest


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "crc32_bare.py"
SPEC = importlib.util.spec_from_file_location("crc32_bare_cli", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class TestCrc32Bare(unittest.TestCase):
    def test_reference_values(self) -> None:
        self.assertEqual(MODULE.crc32_bare(b""), 0)
        self.assertEqual(MODULE.crc32_bare(b"0000aaaa"), 0x0429735D)
        self.assertEqual(MODULE.crc32_bare(b"\x00"), 0)

    def test_text_and_hex_are_identical(self) -> None:
        text = subprocess.run(
            [sys.executable, str(SCRIPT), "--text", "0000aaaa"],
            check=True, text=True, capture_output=True,
        )
        encoded = subprocess.run(
            [sys.executable, str(SCRIPT), "--hex", "3030303061616161"],
            check=True, text=True, capture_output=True,
        )
        self.assertEqual(text.stdout.strip(), "0x0429735D")
        self.assertEqual(text.stdout, encoded.stdout)

    def test_stdin_preserves_trailing_newlines(self) -> None:
        received = subprocess.run(
            [sys.executable, str(SCRIPT), "--stdin"],
            input=b"0000aaaa\n", check=True, capture_output=True,
        )
        self.assertEqual(
            received.stdout.decode().strip(),
            f"0x{MODULE.crc32_bare(b'0000aaaa' + bytes([10])):08X}",
        )
        self.assertNotEqual(received.stdout.decode().strip(), "0x0429735D")

    def test_hex_rejects_invalid_encoding(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--hex", "g"],
            text=True, capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid hexadecimal input", result.stderr)


if __name__ == "__main__":
    unittest.main()
