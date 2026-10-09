"""Fail-closed checks and optional authentic OEM x86 locator regression."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "probe_oem_lce_chs_emulation.py"
SPEC = importlib.util.spec_from_file_location("oem_chs_probe", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
OEM_DLL = Path(os.environ.get("EDP_OEM_DLL_PATH", ""))


class TestIsolatedOEMCHSEmulator(unittest.TestCase):
    def test_wrong_or_truncated_binary_rejected_before_emulation(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            fake = Path(folder) / "wrong-oem.dll"
            fake.write_bytes(b"MZ" + bytes(30))
            with self.assertRaisesRegex(ValueError, "wrong OEM DLL identity"):
                MODULE.load_original_instructions(fake)

    def test_fail_closed_for_invalid_chs_and_native_width(self) -> None:
        with self.assertRaisesRegex(ValueError, "CHS geometry out of range"):
            MODULE.emulate_real_chs_locator(b"", -1)
        with self.assertRaisesRegex(ValueError, "CHS geometry out of range"):
            MODULE.emulate_real_chs_locator(b"", 1 << 64)
        for bad in [0, -512, 65537]:
            with self.assertRaisesRegex(ValueError, "invalid logical sector width"):
                MODULE.emulate_real_compat_extent_rounding(b"", bad)

    @unittest.skipUnless(OEM_DLL.is_file(), "original Windows OEM DLL not installed on this machine")
    def test_exact_oem_machine_code_vectors_and_rounding(self) -> None:
        locator, rounding = MODULE.load_original_instructions(OEM_DLL)
        cases = MODULE.test_oem_vectors(locator, rounding)
        self.assertEqual([label for label, *_ in cases], ["U391_4Kn", "Lexar_512B"])
        self.assertEqual(MODULE.emulate_real_compat_extent_rounding(rounding, 512), 3072)
        self.assertEqual(MODULE.emulate_real_compat_extent_rounding(rounding, 4096), 4096)
        self.assertEqual(MODULE.emulate_real_chs_locator(locator, 0x123456789ABC), 0x1234566A9ABC)


if __name__ == "__main__":
    unittest.main()
