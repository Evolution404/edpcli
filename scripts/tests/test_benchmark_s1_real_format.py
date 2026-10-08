from __future__ import annotations
import importlib.util
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('benchmark_s1',ROOT/'scripts/benchmark-s1-real-format.py')
bench=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

class RealFormatBenchmarkParsing(unittest.TestCase):
    def test_real_format_schema(self):
        fields={'fs':'fat16','volume':'20417','mode':'plain','metadata_sectors':'2',
            'payload_bytes':'1024','estimate_working_bytes':'8192'}
        fields.update({key:'1.0' for key in bench.METRICS})
        result=bench.parse(' '.join(f'{k}={v}' for k,v in fields.items()))
        self.assertEqual(result['metadata_sectors'],2)
        self.assertEqual(result['build_ms'],1.)
    def test_malformed_accounting_rejected(self):
        fields={'fs':'fat16','volume':'20417','mode':'plain','metadata_sectors':'3',
            'payload_bytes':'1024','estimate_working_bytes':'8192'}
        fields.update({key:'1.0' for key in bench.METRICS})
        with self.assertRaises(ValueError):
            bench.parse(' '.join(f'{k}={v}' for k,v in fields.items()))
