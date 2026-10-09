"""Pure regression tests for two historical static FAT16 template xref audits."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'audit_vrvaud_lce_template_xrefs.py'
SPEC = importlib.util.spec_from_file_location('vrvaud_static_xref', SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StaticXrefAuditTests(unittest.TestCase):
    def test_synthetic_x86_immediate_and_memory_reference(self):
        # mov eax, 0x10002000; mov edx, [0x10002004]; or eax, 0xc00
        code = bytes.fromhex('b8 00 20 00 10 8b 15 04 20 00 10 0d 00 0c 00 00')
        refs, c00 = MODULE.x86_find_static_refs(code, 0x10001000, 0x10002000, 16)
        self.assertEqual([item[-1] for item in refs], [0x10002000, 0x10002004])
        self.assertEqual([(m, op) for _, m, op in c00], [('or', 'eax, 0xc00')])

    def test_invalid_file_rejected_before_pe_parser(self):
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / 'invalid.dll'
            fake.write_bytes(b'MZ' + bytes(100))
            with self.assertRaisesRegex(ValueError, 'incorrect original OEM file SHA256'):
                MODULE.audit_original_dll(fake, b'X' * 3072, 'legacy_2022')

    def test_version_rejected(self):
        with self.assertRaisesRegex(ValueError, 'unrecognized OEM binary version'):
            MODULE.audit_original_dll(Path('nonexistent'), b'', 'other_edition')

    def test_original_oem_variants_if_available(self):
        names = {'legacy_2022': os.environ.get('EDP_OEM_VRVAUD_LEGACY'),
                 'current_2026': os.environ.get('EDP_OEM_VRVAUD_CURRENT')}
        if not all(names.values()):
            self.skipTest('original vendor binaries are optional; set EDP_OEM_VRVAUD_{LEGACY,CURRENT}')
        gold = MODULE.TEMPLATE_PATH.read_bytes()
        for version, path in names.items():
            with self.subTest(version=version):
                result = MODULE.audit_original_dll(Path(path), gold, version)
                self.assertTrue(result['template_matches_history'])
                self.assertEqual(result['template_static_text_ref_count'], 0)
                self.assertEqual(result['template_relocated_pointer_count'], 0)
                self.assertEqual(result['template_data_export_count'], 0)
                self.assertEqual(len(result['code_3072_immediates']), 2)
                self.assertEqual(len(result['direct_kernel32_writefile_calls']), {'legacy_2022': 12, 'current_2026': 13}[version])
                self.assertTrue(all(mnemonic == 'or' for _, mnemonic, _ in result['code_3072_immediates']))


if __name__ == '__main__':
    unittest.main()
