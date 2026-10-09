"""Fail-closed checks for multi-partition resource accounting benchmark."""
from pathlib import Path
import importlib.util
import unittest
path=Path(__file__).resolve().parents[1]/'benchmark-s4-multi-format.py'
spec=importlib.util.spec_from_file_location('benchmark_s4_multi_format',path)
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class MultiFormatTests(unittest.TestCase):
    def test_strict_schema_and_aliasing(self):
        row={'scenario':'official-large','status':'ok','selected_partitions':3,
            'metadata_sectors':3,'plan_sectors':3,'estimate_payload_bytes':1536,
            'estimate_working_bytes':12288,'actual_image_payload_bytes':1536,
            'shared_images':1,'encrypted_images':2,'resource_ms':0.5,'plan_ms':3.0,
            'rss_start_kib':5000,'rss_estimate_kib':5000,'rss_plan_kib':6000,'rss_peak_kib':6000}
        raw=' '.join(f'{k}={v}' for k,v in row.items())
        self.assertEqual(module.parse(raw,'official-large')['metadata_sectors'],3)
        for invalid in [{**row,'plan_sectors':2},{**row,'encrypted_images':3},{**row,'estimate_working_bytes':10}]:
            with self.assertRaises(ValueError):module.parse(' '.join(f'{k}={v}' for k,v in invalid.items()),'official-large')
        with self.assertRaises(ValueError):module.parse(raw+' extra=1','official-large')
    def test_reject_before_alloc(self):
        raw='scenario=budget-reject status=rejected-before-alloc metadata_sectors=100000 estimate_payload_bytes=100000000 estimate_working_bytes=800000000 peak_rss_kib=5000'
        self.assertEqual(module.parse(raw,'budget-reject')['peak_rss_kib'],5000)
        with self.assertRaises(ValueError):module.parse(raw.replace('peak_rss_kib=5000','peak_rss_kib=300000'),'budget-reject')
if __name__=='__main__':unittest.main()
