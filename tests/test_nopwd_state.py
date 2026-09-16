"""免密盘检测/防重复写入/备份打标/make 感知提示 测试。"""
import io, os, unittest
from unittest import mock
from contextlib import redirect_stdout

from tests.helpers import FIXTURES, fixture_bin, load_disk_image, read_fn_of, TmpDir
from nopwd import cli, diskio
from nopwd.common import restore_cmd, apply_cmd
from nopwd.sectors import convert, looks_nopwd
from nopwd.common import SECTOR

NETAC_ID = FIXTURES['netac'][1]

def converted_image(key='netac'):
    """合成免密盘镜像(原备份 + 改造后 LBA0/6/7/12, LBA9 清零) → (bytes, device_id)。"""
    data = load_disk_image(key)
    did = FIXTURES[key][1]
    r = convert(read_fn_of(data), did, None, verbose=False)
    conv = [data[i*SECTOR:(i+1)*SECTOR] for i in range(14)]
    for lba, k in ((0, 'lba0'), (6, 'lba6'), (7, 'lba7'), (12, 'lba12')):
        conv[lba] = r[k]
    conv[9] = bytes(SECTOR)
    return b''.join(conv), did

@unittest.skipIf(any(fixture_bin(k) is None for k in FIXTURES), '真实备份不可用')
class TestLooksNopwd(unittest.TestCase):
    def test_originals_all_negative(self):
        for key in FIXTURES:
            self.assertFalse(looks_nopwd(read_fn_of(load_disk_image(key)),
                                         FIXTURES[key][1]), key)

    def test_converted_all_positive(self):
        for key in FIXTURES:
            img, did = converted_image(key)
            self.assertTrue(looks_nopwd(read_fn_of(img), did), key)

    def test_wrong_device_id_negative(self):
        img, _ = converted_image()
        self.assertFalse(looks_nopwd(read_fn_of(img), 'disk&ven_bogus&prod_x'))

    def test_core_signals_required(self):
        # MBR / LBA12 任一恢复原盘即判否(主信号)。
        # LBA6 0x1CA 实测仅部分型号有区分度(netac/lexar 原盘本就=128480,
        # aigo 原盘=20417), 有区分度的型号恢复 LBA6 也须判否。
        img, did = converted_image()
        conv = [img[i*SECTOR:(i+1)*SECTOR] for i in range(14)]
        orig = load_disk_image('netac')
        for lba in (0, 12):                         # 主信号: 缺一即否
            mixed = conv[:]
            mixed[lba] = orig[lba*SECTOR:(lba+1)*SECTOR]
            self.assertFalse(looks_nopwd(lambda l, m=mixed: m[l], did),
                             f'LBA{lba} 恢复原盘后仍误判为免密')
        img_a, did_a = converted_image('aigo')
        mixed = [img_a[i*SECTOR:(i+1)*SECTOR] for i in range(14)]
        mixed[6] = load_disk_image('aigo')[6*SECTOR:7*SECTOR]
        self.assertFalse(looks_nopwd(lambda l, m=mixed: m[l], did_a),
                         'aigo LBA6 恢复原盘后仍误判为免密')

class TestCmdHints(unittest.TestCase):
    def test_make_vs_python_forms(self):
        with mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': '/x/backup'}):
            self.assertEqual(restore_cmd('/p.bin', disk=6, apply=True),
                             'sudo make restore DISK=6 RESTORE="/p.bin" APPLY=1')
            self.assertEqual(apply_cmd(4), 'sudo make apply DISK=4')
        env = {k: v for k, v in os.environ.items() if k != 'NOPWD_BACKUP_DIR'}
        with mock.patch.dict(os.environ, env, clear=True):
            self.assertEqual(restore_cmd('/p.bin', disk=6, apply=True),
                             'sudo python3 -m nopwd --restore "/p.bin" --disk 6 --apply')
            self.assertEqual(apply_cmd(), 'sudo python3 -m nopwd --apply')

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestBackupTagging(unittest.TestCase):
    def _backup(self, img_bytes):
        with TmpDir() as d:
            img = os.path.join(d, 'disk.img')
            open(img, 'wb').write(img_bytes)
            with mock.patch.object(diskio, '_raw_path', return_value=img), \
                 mock.patch.object(diskio, '_disk_total_sectors', return_value='122880000'), \
                 mock.patch.object(diskio, '_usb_vid_pid', return_value=('0dd8', '2005')), \
                 mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': d}):
                buf = io.StringIO()
                with redirect_stdout(buf):
                    path = diskio.backup_disk(99, NETAC_ID)
            return os.path.basename(path), buf.getvalue()

    def test_converted_backup_tagged_nopwd(self):
        img, _ = converted_image()
        name, out = self._backup(img)
        self.assertIn('_nopwd_', name)
        self.assertIn('免密状态', out)
        self.assertIn('make restore', out)          # make 运行 → make 等价命令

    def test_original_backup_untagged(self):
        name, out = self._backup(load_disk_image('netac'))
        self.assertNotIn('_nopwd', name)
        self.assertNotIn('免密状态', out)

    def test_backup_is_nopwd_reads_content(self):
        img, _ = converted_image()
        with TmpDir() as d:
            p = os.path.join(d, 'x.bin')
            open(p, 'wb').write(img)
            self.assertTrue(diskio.backup_is_nopwd(p, NETAC_ID))
            self.assertFalse(diskio.backup_is_nopwd(p, 'disk&ven_bogus&prod_x'))
        self.assertFalse(diskio.backup_is_nopwd('/nonexistent.bin', NETAC_ID))

def _run_cli(argv, conv_img, did, baks=(), backup_mock=None, atomic_mock=None):
    """在进程内跑 cli.main(), 真盘 IO 全部替换为合成免密盘镜像。"""
    sectors = [conv_img[i*SECTOR:(i+1)*SECTOR] for i in range(14)]
    backup_m = backup_mock or mock.Mock()
    atomic_m = atomic_mock or mock.Mock()
    with mock.patch('sys.argv', ['nopwd'] + argv), \
         mock.patch.object(cli, 'identify', return_value=(did, 1, 2)), \
         mock.patch.object(cli, 'read_lba_disk', side_effect=lambda d, l: sectors[l]), \
         mock.patch.object(cli, 'find_backups', return_value=list(baks)), \
         mock.patch.object(cli, 'backup_disk', backup_m), \
         mock.patch.object(cli, 'atomic_write_sectors', atomic_m), \
         mock.patch.object(cli, '_disk_total_sectors', return_value='122880000'), \
         mock.patch.object(cli, '_usb_vid_pid', return_value=('0dd8', '2005')), \
         mock.patch('builtins.input', return_value='YES'), \
         mock.patch.object(cli.subprocess, 'run', mock.Mock()):
        buf = io.StringIO()
        try:
            with redirect_stdout(buf):
                cli.main()
            ret = None
        except SystemExit as e:
            ret = e
    return ret, buf.getvalue(), backup_m, atomic_m

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestApplyGuard(unittest.TestCase):
    def setUp(self):
        self.img, self.did = converted_image()

    def test_refuses_without_force(self):
        ret, out, backup_m, atomic_m = _run_cli(['--disk', '6', '--apply'], self.img, self.did)
        self.assertIsNotNone(ret)                              # SystemExit
        self.assertIn('sudo python3 -m nopwd --apply --disk 6 --force', str(ret))
        self.assertIn('免密盘', out)
        backup_m.assert_not_called()                           # 未备份未写盘
        atomic_m.assert_not_called()

    def test_refusal_hint_make_aware(self):
        # make 运行时拒绝提示必须给 FORCE=1 等价形式, 而非无效的 make ... --force
        with mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': '/x/backup'}):
            ret, out, _, _ = _run_cli(['--disk', '6', '--apply'], self.img, self.did)
        self.assertIsNotNone(ret)
        self.assertIn('sudo make apply DISK=6 FORCE=1', str(ret))
        self.assertNotIn('--force', str(ret))

    def test_force_proceeds_and_writes_same_sectors(self):
        ret, out, backup_m, atomic_m = _run_cli(['--disk', '6', '--apply', '--force'],
                                                self.img, self.did)
        self.assertIsNone(ret, out)
        backup_m.assert_called_once()
        atomic_m.assert_called_once()
        patch = atomic_m.call_args[0][1]
        self.assertEqual(sorted(patch), [0, 6, 7, 12])         # LBA9 已零不写
        self.assertIn('_nopwd', out)                           # 提醒备份将打标

    def test_original_disk_not_blocked(self):
        orig = load_disk_image('netac')
        ret, out, backup_m, atomic_m = _run_cli(['--disk', '6', '--apply'], orig, self.did)
        self.assertIsNone(ret, out)
        self.assertNotIn('--force', out)
        atomic_m.assert_called_once()

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestRestoreHints(unittest.TestCase):
    def setUp(self):
        self.img, self.did = converted_image()

    def test_single_backup_path_filled_make_form(self):
        with TmpDir() as d, mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': d}):
            p = os.path.join(d, 'the_only.bin')
            open(p, 'wb').write(load_disk_image('netac'))
            ret, out, _, _ = _run_cli(['--disk', '26', '--restore'], self.img, self.did, baks=[p])
        self.assertIsNone(ret, out)
        self.assertIn(f'sudo make restore DISK=26 RESTORE="{p}" APPLY=1', out)

    def test_multiple_backups_use_placeholder(self):
        with TmpDir() as d, mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': d}):
            baks = []
            for i in range(2):
                p = os.path.join(d, f'b{i}.bin')
                open(p, 'wb').write(load_disk_image('netac'))
                baks.append(p)
            ret, out, _, _ = _run_cli(['--disk', '26', '--restore'], self.img, self.did, baks=baks)
        self.assertIn('RESTORE="<上面任一路径>"', out)

    def test_nopwd_backup_annotated_in_listing(self):
        with TmpDir() as d, mock.patch.dict(os.environ, {'NOPWD_BACKUP_DIR': d}):
            p = os.path.join(d, 'conv_state.bin')
            open(p, 'wb').write(self.img)                      # 免密状态备份
            ret, out, _, _ = _run_cli(['--disk', '26', '--restore'], self.img, self.did, baks=[p])
        self.assertIn('[免密状态]', out)
        # 单备份 + 免密状态: 路径直填且带警示
        self.assertIn(f'RESTORE="{p}"', out)

    def test_explicit_nopwd_backup_preview_warns(self):
        with TmpDir() as d:
            p = os.path.join(d, 'conv.bin')
            open(p, 'wb').write(self.img)
            ret, out, _, _ = _run_cli(['--disk', '26', '--restore', p], self.img, self.did)
        self.assertIn('免密状态', out)
        self.assertIn('不会回到加密原盘', out)

if __name__ == '__main__':
    unittest.main()
