"""Tests for warning-image semantics in six-sector historical LCE gold."""
from pathlib import Path
import os
import tempfile
import unittest

from scripts.protocol.audit_lce_upgrade_notice_image import (
    GOLD_PATH, EXPECTED_FILENAME, EXPECTED_FOLDER, EXPECTED_TEXT, analyze_image, audit, inspect_embedded_oem,
)


class LCEWarningImageTest(unittest.TestCase):
    def test_recovers_original_filename_and_82_byte_notice(self):
        r = analyze_image(GOLD_PATH.read_bytes())
        notice = r["warning_txt"]
        self.assertEqual(notice["long_filename"], EXPECTED_FILENAME)
        self.assertEqual(r["warning_folder"]["long_filename"], EXPECTED_FOLDER)
        self.assertEqual(r["warning_folder"]["cluster"], 2)
        self.assertEqual(notice["content"], EXPECTED_TEXT)
        self.assertEqual(notice["data_sector"], 5)
        self.assertEqual(notice["size_bytes"], 82)
        self.assertEqual(r["cluster_count_implied_by_bpb"], 2)
        self.assertFalse(r["physical_lce_write_event_proven"])

    def test_invalid_image_rejected_before_interpretation(self):
        data = bytearray(GOLD_PATH.read_bytes())
        data[5*512+3] ^= 1
        with self.assertRaisesRegex(ValueError, "SHA-256/size mismatch"):
            analyze_image(bytes(data))

    def test_binary_strict_sha_guard(self):
        with tempfile.TemporaryDirectory() as d:
            bad = Path(d)/"vrvaud_c.dll"
            bad.write_bytes(b"not original")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                inspect_embedded_oem(bad, "current_2026", GOLD_PATH.read_bytes())

    def test_both_real_embedded_oem_images(self):
        old = os.environ.get("EDP_OEM_VRVAUD_LEGACY")
        new = os.environ.get("EDP_OEM_VRVAUD_CURRENT")
        if not old or not new:
            self.skipTest("set EDP_OEM_VRVAUD_LEGACY and EDP_OEM_VRVAUD_CURRENT")
        r = audit(Path(old), Path(new))
        self.assertTrue(all(x["fully_embedded_image_matches_gold"] for x in r["oem_origins"]))
        self.assertFalse(r["accessed_physical_device"])


if __name__ == "__main__":
    unittest.main()
