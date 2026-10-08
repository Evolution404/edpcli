"""P0 benchmark parser and optional, manual regression gate."""
from __future__ import annotations
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('p0_benchmark', ROOT / 'scripts/benchmark-p0.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class BaselineTests(unittest.TestCase):
    def test_tui_rows_require_all_three_sizes(self):
        line = 'count={n} groups=2 build_ms=1.1 cached_snapshot_us=0.1 frame_ms=2.5 sort_ms=0.6 filter_ms=0.1 search_ms=1.2'
        with self.assertRaises(ValueError):
            benchmark.read_tui(line.format(n=100))
        rows = benchmark.read_tui('\n'.join(line.format(n=n) for n in (100, 1000, 10000)))
        self.assertEqual(rows['10000']['sort_ms'], 0.6)

    def test_csv_can_strip_cancellation_note(self):
        parsed = benchmark.read_csv('dataset,count,cold_ms,warm_p50_ms,peak_rss_kib\nvalid,10,5,1,200\ncancel_result=Some(error)',
                                    {'dataset', 'count', 'warm_p50_ms'})
        self.assertEqual(parsed[0]['count'], '10')

    def test_comparison_uses_absolute_and_relative_guard(self):
        baseline = {'catalog': {'valid-1000': {'warm_p50_ms': 25}}, 'tui': {'10000': {
            'frame_ms': 8, 'sort_ms': 1.0, 'search_ms': 30}}}
        current = {'catalog': {'valid-1000': {'warm_p50_ms': 24}}, 'tui': {'10000': {
            'frame_ms': 8, 'sort_ms': 2.4, 'search_ms': 70}}}
        self.assertEqual(benchmark.regression_issues(current, baseline, 2.0),
                         ['tui[10000].search_ms: 70.000ms > 2.0x 30.000ms'])
