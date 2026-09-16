"""CLI 端到端测试(子进程, 不碰真盘): 离线模式产物与金标一致; 系统盘防护。"""
import os, sys, hashlib, subprocess, unittest

from tests.helpers import ROOT, fixture_bin, load_disk_image, TmpDir
from tests.test_sectors import GOLDEN, FIXTURES

md5 = lambda b: hashlib.md5(b).hexdigest()

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestOfflineCli(unittest.TestCase):
    def setUp(self):
        self._tmp = TmpDir()
        self.d = self._tmp.__enter__()
        data = load_disk_image('netac')
        self.snap = os.path.join(self.d, 'snap')
        os.makedirs(self.snap)
        for lba in range(14):
            with open(os.path.join(self.snap, f'LBA{lba:02d}.bin'), 'wb') as f:
                f.write(data[lba*512:(lba+1)*512])

    def tearDown(self):
        self._tmp.__exit__(None, None, None)

    def test_offline_run_matches_golden(self):
        out = os.path.join(self.d, 'out')
        r = subprocess.run([sys.executable, '-m', 'nopwd', '--dir', self.snap,
                            '--id', FIXTURES['netac'][1], '--out', out],
                           cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        g = GOLDEN['netac']
        for lba in (0, 6, 7, 12):
            with open(os.path.join(out, f'LBA{lba:02d}.bin'), 'rb') as f:
                self.assertEqual(md5(f.read()), g[f'lba{lba}'], f'LBA{lba}')
        with open(os.path.join(out, 'LBA09.bin'), 'rb') as f:      # netac LBA9 非零 → 清零产物
            self.assertEqual(f.read(), bytes(512))
        self.assertIn('59.75GB', r.stdout)                          # 布局回显

    def test_offline_requires_id(self):
        r = subprocess.run([sys.executable, '-m', 'nopwd', '--dir', self.snap],
                           cwd=ROOT, capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn('--id', r.stderr)

class TestSystemDiskGuard(unittest.TestCase):
    def test_disk_below_2_refused(self):
        r = subprocess.run([sys.executable, '-m', 'nopwd', '--disk', '1'],
                           cwd=ROOT, capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn('系统盘', r.stderr)

if __name__ == '__main__':
    unittest.main()
