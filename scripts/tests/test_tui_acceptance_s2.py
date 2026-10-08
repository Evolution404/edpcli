"""No subprocesses; asserts the S2 PTY acceptance harness itself is fail-closed."""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('s2_pty_acceptance', ROOT/'scripts/tui-acceptance-s2.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class PtyAcceptanceSteps(unittest.TestCase):
    def test_resize_must_precede_quit(self):
        steps=module.steps_for(['j','q'])
        self.assertEqual(steps[-3]['resize'],[40,12])
        self.assertEqual(steps[-2]['resize'],[160,45])
        self.assertEqual(steps[-1]['keys'],'q')
        self.assertEqual(steps[1]['keys'],'j')
        with self.assertRaises(ValueError):module.steps_for(['q','j'])
    def test_capture_requires_real_redraw_during_both_resizes(self):
        steps=module.steps_for(['j','q'])
        with tempfile.TemporaryDirectory() as td:
            output=Path(td)/'terminal.ansi'
            output.write_bytes(b'x'*250+b'a'*5+b'y'*40+b'z'*30+b'w'*5)
            frames=[{'ansi_offset': x} for x in [250,255,295,325,330]]
            manifest={'frames':frames}
            self.assertEqual(module.check_capture(manifest,output,steps,'fixture'),(40,30))
            for bad in [frames[:3], [frames[0],frames[1],frames[1],frames[3],frames[4]]]:
                with self.assertRaises(ValueError):
                    module.check_capture({'frames':bad},output,steps,'fixture')
