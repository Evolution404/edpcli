"""Regression tests for the historical nested kernel driver's 4Kn boundaries."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_oem_embedded_driver_mapping import audit, audit_release


class EmbeddedKernelMappingTest(unittest.TestCase):
    def test_fail_closed_sha_for_invalid_host(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/"EdpEDiskCtrl.dll"
            p.write_bytes(b"fake driver container")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                audit_release(p,"2026")

    def test_real_embedded_driver_mapping_if_available(self):
        old = os.environ.get("EDP_OEM_CTRL_LEGACY")
        current = os.environ.get("EDP_OEM_CTRL_CURRENT")
        standalone = os.environ.get("EDP_OEM_EDPEDISK64")
        if not old or not current or not standalone:
            self.skipTest("set EDP_OEM_CTRL_LEGACY/CURRENT and EDP_OEM_EDPEDISK64")
        result = audit(Path(old), Path(current), Path(standalone))
        a = result["versions"]["2022"]["x64"]
        b = result["versions"]["2026"]["x64"]
        self.assertEqual(len(a["verified_machine_opcodes"]), 14)
        self.assertEqual(len(b["verified_machine_opcodes"]), 14)
        self.assertNotEqual(a["sha256"],b["sha256"])
        self.assertEqual(a["direct_ZwWriteFile_site"],"0x122dd")
        self.assertEqual(b["direct_ZwWriteFile_site"],"0x1232e")
        self.assertTrue(result["current_x64_matches_existing_standalone_original"])
        self.assertEqual(a["device_default_block_size_constant"],512)
        self.assertFalse(result["accessed_physical_disk"])
        self.assertFalse(b["native_logical_sector_bytes_from_device_identified"])


if __name__ == "__main__":
    unittest.main()
