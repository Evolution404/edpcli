"""OEM vrvaud policy callbacks and WriteDiskEx argument trace."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_vrvaud_writer_provenance import (
    VERSIONS, original_pe, audit,
)


class VrvAudWriterProvenanceTests(unittest.TestCase):
    def test_rejects_mismatched_oem_binary_early(self):
        with tempfile.TemporaryDirectory() as folder:
            p = Path(folder) / "bogus.dll"
            p.write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                original_pe(p, VERSIONS["current_2026"]["sha"])

    def test_original_oem_matrix_if_available(self):
        legacy = os.environ.get("EDP_OEM_VRVAUD_LEGACY")
        current = os.environ.get("EDP_OEM_VRVAUD_CURRENT")
        if not legacy or not current:
            self.skipTest("set both EDP_OEM_VRVAUD_LEGACY and EDP_OEM_VRVAUD_CURRENT")
        report = audit(Path(legacy), Path(current))
        for obj in report["policy_callbacks"]:
            self.assertEqual(obj["dynamic_module"], "DeviceNumber.dll")
            self.assertEqual(obj["following_1024_pe_bytes_nonzero"], 275)
            self.assertTrue(obj["following_bytes_include_rtti_not_disk_payload"])
        writer = report["writer_2026"]
        self.assertFalse(writer["static_caller_found"])
        self.assertFalse(writer["exported_writer"])
        self.assertEqual(writer["payload_buffer"], "arg4 at [ebp+0x18] (verified push before Win32 WriteFile)")
        self.assertFalse(report["disk_access"])


if __name__ == "__main__":
    unittest.main()
