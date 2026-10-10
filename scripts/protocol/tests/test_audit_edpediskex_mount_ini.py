"""SHA-bound historical mount configuration writer boundaries."""
import os
from pathlib import Path
import tempfile
import unittest
from scripts.protocol.audit_edpediskex_mount_ini import audit, validate_original


class EDPDiskMountINItest(unittest.TestCase):
    def test_rejects_wrong_binary(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/"EdpEDiskEx.dll"
            p.write_bytes(b"not valid")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                validate_original(p)

    def test_actual_mount_helper_if_available(self):
        pa = os.environ.get("EDP_OEM_EDPEDISKEX")
        if not pa:
            self.skipTest("set EDP_OEM_EDPEDISKEX")
        report = audit(Path(pa))
        self.assertEqual(len(report["actual_ini_write_call_sites"]), 5)
        self.assertEqual(report["ini_entry_names"], ["GLOBAL", "LETTER", "INDEX", "IMAGE", "SESSION"])
        self.assertFalse(report["compatibility_LCE_first_writer_identified"])
        self.assertFalse(report["direct_disk_WriteFile_or_DeviceIoControl_within_helper"])


if __name__ == "__main__":
    unittest.main()
