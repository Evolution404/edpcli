"""Regression tests for the SHA-bound OEM virtual mount-to-write trace."""
import os
from pathlib import Path
import tempfile
import unittest
from scripts.protocol.audit_edpediskex_mount_chain import EX_SHA, audit, verified_pe


class EdpExMountTests(unittest.TestCase):
    def test_rejects_wrong_original_before_pe_parser(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sample.dll"
            path.write_bytes(b"not an OEM original")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                verified_pe(path, EX_SHA, 0x14c, 0x10000000)

    def test_original_oem_imports_and_opcode_boundaries(self):
        ex = os.environ.get("EDP_OEM_EDPEDISKEX")
        driver = os.environ.get("EDP_OEM_EDPEDISK64")
        if not ex or not driver:
            self.skipTest("set both EDP_OEM_EDPEDISKEX and EDP_OEM_EDPEDISK64")
        proof = audit(Path(ex), Path(driver))
        self.assertTrue(proof["read_only"])
        self.assertEqual(len(proof["verified_user_mount_instructions"]), 5)
        self.assertEqual(len(proof["verified_kernel_dispatch_and_write_instructions"]), 13)
        self.assertIn("last 1024B owner", proof["unproven"])


if __name__ == "__main__":
    unittest.main()
