"""2022/2026 embedded mount-request ABI SHA-bound offline regression."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_embedded_edpediskex_versions import (
    HOSTS, audit, audit_embedded_ex, sha_guard,
)


class EmbeddedOEMMountVersionTests(unittest.TestCase):
    def test_sha_invalid_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "EdpEDiskCtrl.dll"
            fake.write_bytes(b"invalid")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                sha_guard(fake, HOSTS["2026"]["sha256"])

    def test_real_2022_2026_mount_abi_if_available(self):
        old = os.environ.get("EDP_OEM_CTRL_LEGACY")
        current = os.environ.get("EDP_OEM_CTRL_CURRENT")
        if not old or not current:
            self.skipTest("set EDP_OEM_CTRL_LEGACY and EDP_OEM_CTRL_CURRENT")
        report = audit(Path(old), Path(current))
        self.assertEqual(report["versions"]["2022"]["edp_mount_file_export_rva"], "0x98b0")
        self.assertEqual(report["versions"]["2026"]["edp_mount_file_export_rva"], "0x9a10")
        self.assertEqual(len(report["versions"]["2022"]["x86_sites"]), 11)
        self.assertEqual(len(report["versions"]["2026"]["x86_sites"]), 11)
        self.assertEqual(report["versions"]["2022"]["mount_descriptor_length"],
                         report["versions"]["2026"]["mount_descriptor_length"])
        self.assertFalse(report["first_3072_byte_upgrade_notice_writer_identified"])
        self.assertFalse(report["accessed_physical_device"])

    def test_embedded_legacy_zip_exact_controller_if_available(self):
        old = os.environ.get("EDP_OEM_CTRL_LEGACY")
        current = os.environ.get("EDP_OEM_CTRL_CURRENT")
        legacy_exe = os.environ.get("EDP_OEM_EDPEDISK_EXE_LEGACY")
        if not old or not current or not legacy_exe:
            self.skipTest("set EDP_OEM_CTRL_LEGACY/CURRENT and EDP_OEM_EDPEDISK_EXE_LEGACY")
        report = audit(Path(old), Path(current), Path(legacy_exe))
        self.assertTrue(report["historical_edpedisk_installer"]["content_equal_to_standalone_controller"])
        self.assertFalse(report["historical_edpedisk_installer"]["contains_updater_executables"])


if __name__ == "__main__":
    unittest.main()
