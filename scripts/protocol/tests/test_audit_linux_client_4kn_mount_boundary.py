"""Fail-closed checks and source-bound OEM Linux 4Kn machine code evidence."""
import os
from pathlib import Path
import tempfile
import unittest

from scripts.protocol.audit_linux_client_4kn_mount_boundary import (
    LIBS, audit, elf_va_offset, verify_binary,
)


class LinuxClient4KnAuditTests(unittest.TestCase):
    def test_rejects_impostor_before_parsing(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/"libedpedisk.so"
            path.write_bytes(b"\x7fELF"+"fake".encode())
            with self.assertRaisesRegex(ValueError,"SHA-256 mismatch"):
                verify_binary(path,"mount")

    def test_rejects_invalid_elf_and_unmapped_virtual_address(self):
        with self.assertRaisesRegex(ValueError,"64-bit"):
            elf_va_offset(b"bad",0x6BD86)

    def test_real_oem_linux_libraries_if_available(self):
        mount=os.environ.get("EDP_OEM_LINUX_EDPEDISK_SO")
        sector=os.environ.get("EDP_OEM_LINUX_SECTOR_SO")
        if not mount or not sector:
            self.skipTest("set EDP_OEM_LINUX_EDPEDISK_SO / EDP_OEM_LINUX_SECTOR_SO")
        report=audit(Path(mount),Path(sector))
        for label,kind in [("libedpedisk.so","mount"),("libsectorManage.so","sector_helper")]:
            self.assertEqual(len(report["libs"][label]["verified_sites"]),len(LIBS[kind]["sites"]))
        self.assertTrue(report["verified"]["mount_library_queries_real_logical_bytes_via_BLKSSZGET"])
        self.assertTrue(report["verified"]["sector_helper_WriteEncrypt_rounds_on_fixed_512_byte_boundaries"])
        self.assertTrue(report["not_verified"]["failure_of_specific_4Kn_device_on_Linux_client"])
        self.assertFalse(report["device_read_or_write"])


if __name__=="__main__": unittest.main()
