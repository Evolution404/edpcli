from __future__ import annotations
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('p2_benchmark', ROOT / 'scripts/benchmark-p2.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class P2BenchmarkTests(unittest.TestCase):
    def test_parse_result(self):
        row = benchmark.parse_measure('count=65536 plan_ms=1.200 transaction_ms=45.006 '
                                      'rss_after_plan_kib=40000 peak_rss_kib=45678 peak_delta_kib=5678')
        self.assertEqual(row['sectors'], 65536)
        self.assertEqual(row['peak_delta_kib'], 5678)

    def test_fail_closed_on_missing_fields(self):
        with self.assertRaises(ValueError):
            benchmark.parse_measure('count=1024 elapsed_ms=14.1')
        with self.assertRaises(AssertionError):
            benchmark.parse_measure('count=1024 plan_ms=1 transaction_ms=1 '
                                    'rss_after_plan_kib=4 peak_rss_kib=9 peak_delta_kib=99')
