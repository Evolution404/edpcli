"""备份/还原体系测试: 命名迁移、按盘匹配(LBA4 终验)、备份落盘、labelOnlyId 解析。"""
import os, glob, hashlib, unittest
from unittest import mock

from tests.helpers import fixture_bin, load_disk_image, TmpDir, BAK_DIR, FIXTURES
from nopwd import diskio

NETAC_ID = FIXTURES['netac'][1]

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestLabelId(unittest.TestCase):
    def test_positive_and_negative_ids(self):
        # 正 id: netac; 负 10 位 id 连分隔符 17B: aigo hd806
        self.assertEqual(diskio._lba4_label_id_from(b'$$$1402259934$$$' + bytes(18)), '1402259934')
        self.assertEqual(diskio._lba4_label_id_from(b'$$$-1833210541$$$' + bytes(15)), '-1833210541')

    def test_malformed_returns_none(self):
        self.assertIsNone(diskio._lba4_label_id_from(b''))
        self.assertIsNone(diskio._lba4_label_id_from(bytes(32)))
        self.assertIsNone(diskio._lba4_label_id_from(b'@@@1@@@'))

    def test_real_backup_label_id(self):
        # 备份文件名中的 onlyid 段 == 从其 LBA4 读出的值
        path = fixture_bin('netac')
        self.assertEqual(diskio._backup_label_id(path), '1402259934')

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestMigrateBackupNames(unittest.TestCase):
    def setUp(self):
        self.data = load_disk_image('netac')
        self._tmp = TmpDir(); self.d = self._tmp.__enter__()

    def tearDown(self):
        self._tmp.__exit__(None, None, None)

    def write(self, name):
        p = os.path.join(self.d, name)
        open(p, 'wb').write(self.data)
        open(p + '.md5', 'w').write(hashlib.md5(self.data).hexdigest() + '\n')
        return p

    def test_old_lid_and_no_id_names_are_migrated(self):
        old1 = self.write('disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_lid1402259934_20250101_000000.bin')
        old2 = self.write('disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin')
        diskio.migrate_backup_names(self.d)
        self.assertFalse(os.path.exists(old1))
        self.assertFalse(os.path.exists(old2))
        news = sorted(os.path.basename(p) for p in glob.glob(os.path.join(self.d, '*.bin')))
        self.assertTrue(all('_onlyid1402259934_' in n for n in news), news)
        # .md5 同步改名
        self.assertEqual(len(glob.glob(os.path.join(self.d, '*.md5'))), 2)

    def test_idempotent(self):
        self.write('disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_000000.bin')
        diskio.migrate_backup_names(self.d)
        first = sorted(os.listdir(self.d))
        diskio.migrate_backup_names(self.d)          # 第二次应无变化
        self.assertEqual(sorted(os.listdir(self.d)), first)

@unittest.skipIf(fixture_bin('netac') is None or fixture_bin('lexar') is None, '真实备份不可用')
class TestFindBackups(unittest.TestCase):
    def setUp(self):
        self.netac = load_disk_image('netac')
        self._tmp = TmpDir(); self.d = self._tmp.__enter__()

    def tearDown(self):
        self._tmp.__exit__(None, None, None)

    def ctx(self):
        return [mock.patch.object(diskio, '_raw_path', return_value=fixture_bin('netac')),
                mock.patch.object(diskio, '_disk_total_sectors', return_value='122880000'),
                mock.patch.object(diskio, '_usb_vid_pid', return_value=('0dd8', '2005')),
                mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': self.d})]

    def test_same_model_foreign_disk_filtered_by_lba4(self):
        # 同型号模式但 LBA4 是他盘(lexar)的备份 → 被 LBA4 终验剔除
        real = os.path.join(self.d, 'disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin')
        open(real, 'wb').write(self.netac)
        fake = os.path.join(self.d, 'disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_20260910_173000.bin')
        open(fake, 'wb').write(load_disk_image('lexar'))
        for p in (real, fake):
            open(p + '.md5', 'w').write('\n')
        for m in self.ctx(): m.start()
        try:
            found = diskio.find_backups(99, NETAC_ID)
        finally:
            for m in self.ctx(): m.stop()
        self.assertEqual(found, [real])

    def test_no_match_returns_empty(self):
        for m in self.ctx(): m.start()
        try:
            self.assertEqual(diskio.find_backups(99, NETAC_ID), [])
        finally:
            for m in self.ctx(): m.stop()

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestBackupDisk(unittest.TestCase):
    def test_backup_written_with_md5_and_onlyid(self):
        data = load_disk_image('netac')
        with TmpDir() as d, \
             mock.patch.object(diskio, '_raw_path', return_value=fixture_bin('netac')), \
             mock.patch.object(diskio, '_disk_total_sectors', return_value='122880000'), \
             mock.patch.object(diskio, '_usb_vid_pid', return_value=('0dd8', '2005')), \
             mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': d}):
            path = diskio.backup_disk(99, NETAC_ID)
            self.assertTrue(path.startswith(d), path)
            self.assertIn('_onlyid1402259934_', os.path.basename(path))
            with open(path, 'rb') as f:
                self.assertEqual(f.read(), data)      # LBA0-13 全量
            with open(path + '.md5') as f:
                self.assertEqual(f.read().strip(), hashlib.md5(data).hexdigest())

if __name__ == '__main__':
    unittest.main()
