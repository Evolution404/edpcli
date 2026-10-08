from __future__ import annotations
import importlib.util
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('p2_format',ROOT/'scripts/benchmark-p2-format.py')
bench=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

class FormatBenchParsing(unittest.TestCase):
    def test_valid_reading(self):
        value=bench.parse('variant=borrowed sectors=32768 prepare_ms=1.236 transaction_ms=134.567 image_rss_kib=1234 prepared_rss_kib=1235 peak_rss_kib=1254 peak_delta_kib=20')
        self.assertEqual(value['sectors'],32768)
        self.assertEqual(value['peak_delta_kib'],20)
        self.assertEqual(value['variant'],'borrowed')
    def test_fail_bad_output(self):
        with self.assertRaises(ValueError): bench.parse('edpcli 2.5.0')
        with self.assertRaises(AssertionError):
            bench.parse('variant=borrowed sectors=32768 prepare_ms=1 transaction_ms=1 image_rss_kib=3 prepared_rss_kib=3 peak_rss_kib=4 peak_delta_kib=9')
