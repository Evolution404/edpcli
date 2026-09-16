"""磁盘 IO 测试: 原子写入三态(成功/回滚/短写) + 快照读取。
全部跑在文件镜像上, 不碰真盘。"""
import os, unittest
from unittest import mock

from tests.helpers import fixture_bin, load_disk_image, TmpDir
from nopwd import diskio
from nopwd.common import SECTOR

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestAtomicWrite(unittest.TestCase):
    def setUp(self):
        self.base = load_disk_image('netac')
        self._tmp = TmpDir()
        self.img = os.path.join(self._tmp.__enter__(), 'disk.img')
        open(self.img, 'wb').write(self.base)
        self.patch = {6: b'\x11'*SECTOR, 7: b'\x22'*SECTOR,
                      12: b'\x33'*SECTOR, 0: b'\x44'*SECTOR}

    def tearDown(self):
        self._tmp.__exit__(None, None, None)

    def img_bytes(self):
        with open(self.img, 'rb') as f:
            return f.read()

    def test_success_writes_all_and_verifies(self):
        with mock.patch.object(diskio, '_raw_path', return_value=self.img):
            diskio.atomic_write_sectors(99, self.patch)
        got = self.img_bytes()
        self.assertEqual(got[0:SECTOR], self.patch[0])
        for lba in (6, 7, 12):
            self.assertEqual(got[lba*SECTOR:(lba+1)*SECTOR], self.patch[lba])
        # 未列入的扇区一律不动
        self.assertEqual(got[SECTOR:6*SECTOR], self.base[SECTOR:6*SECTOR])
        self.assertEqual(got[8*SECTOR:12*SECTOR], self.base[8*SECTOR:12*SECTOR])

    def test_midway_failure_rolls_back(self):
        orig = diskio.pwrite_full
        calls = {'n': 0}
        def flaky(fd, data, offset):
            calls['n'] += 1
            if calls['n'] == 3:
                raise OSError(5, '注入的写入失败')
            return orig(fd, data, offset)
        with mock.patch.object(diskio, '_raw_path', return_value=self.img), \
             mock.patch.object(diskio, 'pwrite_full', flaky):
            with self.assertRaises(SystemExit) as cm:
                diskio.atomic_write_sectors(99, self.patch)
        self.assertIn('回滚', str(cm.exception))
        self.assertEqual(self.img_bytes(), self.base)   # 逐字节回到写前

    def test_verify_failure_rolls_back(self):
        # 读回不符同样触发回滚: 在校验阶段(4 扇已写完)对 LBA12 返回错误数据
        real_pread, real_pwrite = os.pread, os.pwrite
        state = {'written': 0, 'tampered': False}
        def counting_pwrite(fd, data, offset):
            n = real_pwrite(fd, data, offset)
            state['written'] += 1
            return n
        def spying_pread(fd, n, off):
            data = real_pread(fd, n, off)
            if state['written'] >= 4 and off == 12 * SECTOR and not state['tampered']:
                state['tampered'] = True
                return bytes(n)                        # 模拟"读回与写入不符"
            return data
        with mock.patch.object(diskio, '_raw_path', return_value=self.img), \
             mock.patch.object(os, 'pwrite', counting_pwrite), \
             mock.patch.object(os, 'pread', spying_pread):
            with self.assertRaises(SystemExit) as cm:
                diskio.atomic_write_sectors(99, self.patch)
        self.assertIn('回滚', str(cm.exception))
        self.assertEqual(self.img_bytes(), self.base)  # 回滚后逐字节写前状态

class TestPwriteFull(unittest.TestCase):
    def test_short_writes_are_looped(self):
        with TmpDir() as d:
            p = os.path.join(d, 'x.bin')
            open(p, 'wb').write(bytes(14*SECTOR))
            real = os.pwrite
            def half(fd, data, offset):                # 每次只写一半
                return real(fd, memoryview(data)[:max(1, len(data)//2)], offset)
            fd = os.open(p, os.O_RDWR)
            try:
                with mock.patch.object(os, 'pwrite', half):
                    diskio.pwrite_full(fd, b'\xAA'*SECTOR, 6*SECTOR)
            finally:
                os.close(fd)
            with open(p, 'rb') as f:
                f.seek(6*SECTOR)
                self.assertEqual(f.read(SECTOR), b'\xAA'*SECTOR)

class TestReadLbaFile(unittest.TestCase):
    def test_naming_compat_and_missing(self):
        with TmpDir() as d:
            open(os.path.join(d, 'LBA7.bin'), 'wb').write(b'7' * SECTOR)
            open(os.path.join(d, 'LBA12.bin'), 'wb').write(b'c' * SECTOR)
            self.assertEqual(diskio.read_lba_file(d, 7), b'7' * SECTOR)     # 无前导零
            self.assertEqual(diskio.read_lba_file(d, 12), b'c' * SECTOR)
            self.assertEqual(diskio.read_lba_file(d, 9), bytes(SECTOR))     # 缺失→全零

if __name__ == '__main__':
    unittest.main()
