"""Offline SHA-bound OEM format/provision source graph tests."""
from __future__ import annotations
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_oem_lce_format_producer import (
    OEM_REGISTRAR_SHA256, audit, read_original, scan_oem_binary_fixtures,
)


class OEMLCEFormatProducerTest(unittest.TestCase):
    def test_wrong_binary_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            file = Path(d) / "fake.dll"
            file.write_bytes(b"not original registrar")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                read_original(file, OEM_REGISTRAR_SHA256)

    def test_original_registrar_if_available(self):
        filename = os.getenv("EDP_OEM_USBREGSITER")
        if not filename:
            self.skipTest("set EDP_OEM_USBREGSITER to original SHA-bound DLL")
        result = audit(Path(filename))
        chain = result["format_chain"]
        self.assertEqual(chain["dynamic_module"], "fmifs.dll")
        self.assertEqual(chain["dynamic_export"], "FormatEx")
        self.assertEqual(chain["format_ex_call_va"], "0x1004592c")
        self.assertTrue(any(row["target"] == "0x10044210" for row in chain["named_call_edges"]))
        self.assertFalse(chain["direct_lce_byte_address_in_verified_format_ex_call"])
        self.assertFalse(result["physical_device_io"])

    def test_writer_absolute_pointer_survey_if_available(self):
        registrar = os.getenv("EDP_OEM_USBREGSITER")
        current = os.getenv("EDP_OEM_VRVAUD_CURRENT")
        if not registrar or not current:
            self.skipTest("set EDP_OEM_USBREGSITER and EDP_OEM_VRVAUD_CURRENT")
        result = audit(Path(registrar), current_vrvaud=Path(current))
        writer = result["vrvaud_writer_address_scan"]
        self.assertFalse(any(writer["absolute_pe_dword_occurrences"].values()))
        self.assertEqual(writer["highlow_relocated_writer_pointers"], [])
        self.assertFalse(writer["runtime_computed_dispatch_excluded"])

    def test_source_binary_golden_scan_if_available(self):
        root = os.getenv("EDP_OEM_VRV_ROOT")
        if not root:
            self.skipTest("set EDP_OEM_VRV_ROOT for native PE files read-only scan")
        gold = (Path(__file__).resolve().parents[3] /
                "audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin").read_bytes()
        rows = scan_oem_binary_fixtures(Path(root), gold)
        self.assertEqual(sum(row["file_count"] for row in rows), 170)
        self.assertEqual(sum(len(row["matches"]) for row in rows), 3)
        self.assertEqual({match["file"] for row in rows for match in row["matches"]}, {"vrvaud_c.dll"})


if __name__ == "__main__":
    unittest.main()
