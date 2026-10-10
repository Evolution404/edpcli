"""Historical OEM LBA0 MBR patch call-chain and pure 4Kn sector tests."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_vrvaud_mbr_rmw import (
    ENTRY_START, GOLD, audit, audit_original, simulated_lba0_patch,
)


class HistoricalMbrWriterTests(unittest.TestCase):
    def test_mbr_patch_matches_both_native_logical_sector_widths(self):
        for width in (512,4096):
            with self.subTest(width=width):
                prior=bytes((n*13+7)%256 for n in range(width))
                after=simulated_lba0_patch(prior,width)
                self.assertEqual(after[:462], prior[:462])
                self.assertEqual(after[462:510], GOLD)
                self.assertEqual(after[510:], prior[510:])
                self.assertEqual(len(after),width)

    def test_invalid_size_rejected(self):
        for width,block in [(512,b"X"*4096),(4096,b"X"*512),(1024,b"X"*1024)]:
            with self.subTest(width=width):
                with self.assertRaisesRegex(ValueError,"exactly one"):
                    simulated_lba0_patch(block,width)

    def test_fail_closed_on_forged_oem(self):
        with tempfile.TemporaryDirectory() as tmp:
            sample=Path(tmp)/"vrvaud_c.dll"
            sample.write_bytes(b"MZ"+b"0"*4096)
            with self.assertRaisesRegex(ValueError,"SHA-256 mismatch"):
                audit_original(sample,"current_2026")

    def test_both_real_original_mbr_paths_when_available(self):
        old=os.environ.get("EDP_OEM_VRVAUD_LEGACY")
        current=os.environ.get("EDP_OEM_VRVAUD_CURRENT")
        if not old or not current:
            self.skipTest("set EDP_OEM_VRVAUD_LEGACY/CURRENT")
        evidence=audit(Path(old),Path(current))
        self.assertFalse(evidence["first_LCE_FAT16_physical_writer_identified"])
        self.assertTrue(evidence["no_device_access"])
        for item in evidence["editions"].values():
            self.assertEqual(item["MBR_copy_relative_offset"],ENTRY_START)
            self.assertEqual(item["MBR_copy_length_bytes"],len(GOLD))
            self.assertEqual(len(item["verified_machine_opcodes"]),20)
            self.assertEqual([e["start_lba"] for e in item["MBR_slots_2_to_4"]],[63,64,65])
            self.assertEqual([e["partition_type"] for e in item["MBR_slots_2_to_4"]],["0x04","0x08","0x08"])


if __name__=="__main__":
    unittest.main()
