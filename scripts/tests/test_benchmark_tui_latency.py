"""Fail-closed checks for PTY first-output latency benchmark parsing."""
import importlib.util
from pathlib import Path
import unittest

script=Path(__file__).resolve().parents[1]/'benchmark-tui-latency.py'
spec=importlib.util.spec_from_file_location('benchmark_tui_latency',script)
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class LatencyBenchTests(unittest.TestCase):
    def test_interpolated_percentile(self):
        self.assertEqual(module.percentile([20., 10., 30.],0.5),20.)
        self.assertEqual(module.percentile([1.,2.,3.,4.,5.],0.95),4.8)
        with self.assertRaises(ValueError):module.percentile([],0.95)
    def test_missing_output_or_action_is_failure(self):
        steps=[{'wait':0.5},{'keys':'j','wait':0.2},{'keys':'q','wait':0.1}]
        frames=[{'ansi_offset':300}, {'ansi_offset':320,'first_pty_output_ms':25.}, {'ansi_offset':320}]
        manifest={'frames':frames,'steps':steps,'exit_code':0,'timed_out':False}
        self.assertEqual(module.samples_for(manifest,'test'),[25.])
        with self.assertRaises(ValueError):module.samples_for(manifest,'test','settled_pty_burst_ms')
        frames[1]['settled_pty_burst_ms']=26.
        self.assertEqual(module.samples_for(manifest,'test','settled_pty_burst_ms'),[26.])
        for broken in [dict(frames=[frames[0]]+frames[2:]), dict(frames=[frames[0],{'ansi_offset':300,'first_pty_output_ms':5.},frames[2:]]),dict(frames=[frames[0],{'ansi_offset':321,'first_pty_output_ms':None},frames[2:]]),dict(exit_code=1)]:
            with self.assertRaises(ValueError):module.samples_for({**manifest,**broken},'test')
if __name__=='__main__':unittest.main()
