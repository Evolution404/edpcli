"""OEM disk upgrade and external-process boundary, read-only."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_vrvaud_disk_upgrade_launch import inspect_oem, load_original, optional_bundle_inventory


class OEMUpgradeChainTest(unittest.TestCase):
    def test_fail_closed_wrong_original(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/"vrvaud_c.dll"
            p.write_bytes(b"fake")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                load_original(p, "current_2026")

    def test_sha_bound_upgrade_chain_if_available(self):
        old = os.environ.get("EDP_OEM_VRVAUD_LEGACY")
        current = os.environ.get("EDP_OEM_VRVAUD_CURRENT")
        if not old or not current:
            self.skipTest("set EDP_OEM_VRVAUD_LEGACY and EDP_OEM_VRVAUD_CURRENT")
        r = inspect_oem(Path(old), Path(current))
        self.assertEqual(r["policy_to_disk_update_boundary"]["external_process"],
                         "sub_100A7230 invokes CreateProcessA at 0x100A7620")
        self.assertFalse(any(r["bounded_direct_WriteFile_sites"].values()))
        self.assertFalse(r["extent_image_producer_proven"])
        self.assertFalse(r["physical_disk_access"])

    def test_limited_bundle_inventory_if_available(self):
        root = os.environ.get("EDP_OEM_VRV_ROOT")
        if not root:
            self.skipTest("set EDP_OEM_VRV_ROOT")
        data = optional_bundle_inventory(Path(root), {})
        self.assertIn("EdpUUpdate.exe", data["original_bundle_file_inventory"])
        self.assertGreaterEqual(data["original_bundle_file_inventory"]["EdpEDisk.exe"]["count"], 1)
        self.assertTrue(all(x["full_3072_gold_occurrences"] == 0 for x in data["external_legacy_app_binary_asset_checks"]))
        self.assertTrue(all(x["warning_82_byte_GBK_occurrences"] == 0 for x in data["external_legacy_app_binary_asset_checks"]))


if __name__ == "__main__":
    unittest.main()
