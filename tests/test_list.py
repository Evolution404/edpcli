"""外接盘一览测试: 枚举解析(diskutil mock) + 行型展示 + CLI 冒烟。"""
import io, os, plistlib, subprocess, sys, unittest
from unittest import mock
from contextlib import redirect_stdout

from tests.helpers import ROOT
from nopwd import diskio
from nopwd.cli import scan_disks, print_disk_table

# diskutil mock: disk4=USB(未识别), disk6=USB(cems), disk7=Thunderbolt,
#                disk0/1=系统盘, disk8=DMG虚拟盘
FAKE_DISKS = {'AllDisks': ['disk0', 'disk1', 'disk4', 'disk4s1', 'disk6', 'disk7', 'disk8']}
def _info(name, proto, internal=False, size=64_000_000_000, **extra):
    d = dict(WholeDisk=not name.endswith('s1'), Internal=internal,
             BusProtocol=proto, TotalSize=size)
    d.update(extra)
    return d
FAKE_INFO = {
    'disk0': _info('disk0', 'Apple Fabric', internal=True),
    'disk1': _info('disk1', 'Apple Fabric', internal=True),
    'disk4': _info('disk4', 'USB', size=64_000_000_000),
    'disk4s1': _info('disk4s1', 'USB'),                      # 分区, 应被排除
    'disk6': _info('disk6', 'USB', size=62_914_560_000),
    'disk7': _info('disk7', 'Thunderbolt', size=500_107_862_016),
    'disk8': _info('disk8', 'Disk Image', VirtualOrPhysical='Virtual'),
}
def fake_check_output(cmd, **kw):
    if cmd[:2] == ['diskutil', 'list']:
        return plistlib.dumps(FAKE_DISKS)
    return plistlib.dumps(FAKE_INFO[cmd[-1]])

class TestListExternalDisks(unittest.TestCase):
    def test_enumeration_filters_and_protocol(self):
        with mock.patch.object(diskio.subprocess, 'check_output', fake_check_output), \
             mock.patch.object(diskio, '_usb_vid_pid', return_value=('0dd8', '2005')):
            disks = diskio.list_external_disks()
        self.assertEqual([d[0] for d in disks], [4, 6, 7])          # 系统盘/分区排除
        self.assertEqual(disks[0][4], 'USB')
        self.assertEqual(disks[0][2:4], ('0dd8', '2005'))
        self.assertEqual(disks[2][4], 'Thunderbolt')
        self.assertEqual(disks[2][2:4], ('xxxx', 'xxxx'))           # 非USB 无 VID/PID
        with mock.patch.object(diskio.subprocess, 'check_output', fake_check_output), \
             mock.patch.object(diskio, '_usb_vid_pid', return_value=('0dd8', '2005')):
            usb = diskio.list_usb_disks()
        self.assertEqual([d[0] for d in usb], [4, 6])               # 子集仅 USB

class TestScanAndPrint(unittest.TestCase):
    ROWS_SRC = [(4, 64_000_000_000, '0951', '1666', 'USB'),
                (6, 62_914_560_000, '0dd8', '2005', 'USB'),
                (7, 500_107_862_016, 'xxxx', 'xxxx', 'Thunderbolt')]

    def _run(self, ident_map, label_ids=None, baks=None, denied_disks=()):
        label_ids = label_ids or {}
        baks = baks or {}
        def fake_identify(disk):
            if disk in denied_disks:
                raise PermissionError(13, 'Permission denied')
            return ident_map.get(disk, (None, None, None))
        import nopwd.cli as cli
        with mock.patch.object(cli, 'list_external_disks', return_value=self.ROWS_SRC), \
             mock.patch.object(cli, 'identify', fake_identify), \
             mock.patch.object(cli, '_disk_label_id', lambda d: label_ids.get(d)), \
             mock.patch.object(cli, 'find_backups', lambda d, did: ['x'] * baks.get(d, 0)):
            buf = io.StringIO()
            with redirect_stdout(buf):
                print_disk_table(scan_disks())
        return buf.getvalue()

    def test_all_row_kinds(self):
        out = self._run({6: ('disk&ven_netac&prod_onlydisk', 1, 2)},
                        label_ids={6: '1402259934'}, baks={6: 3})
        lines = out.strip().splitlines()
        self.assertEqual(lines[0], '外接盘 3 个:')
        self.assertIn('disk4', lines[1]); self.assertIn('非cems盘', lines[1])
        self.assertIn('disk6', lines[2])
        self.assertIn('cems盘', lines[2]); self.assertIn('onlyid=1402259934', lines[2])
        self.assertIn('备份3份', lines[2])
        self.assertIn('disk7', lines[3]); self.assertIn('非USB', lines[3])

    def test_denied_hint_without_sudo(self):
        out = self._run({}, denied_disks=(4, 6))
        self.assertIn('sudo 可识别', out)

    def test_cems_without_backup_shows_wubak(self):
        out = self._run({4: ('disk&ven_x&prod_y', 1, 2)})
        self.assertIn('无备份', out)

    def test_empty(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            print_disk_table([])
        self.assertEqual(buf.getvalue().strip(), '未检测到外接盘。')

class TestListCli(unittest.TestCase):
    def test_smoke_exit_zero(self):
        # 本机 diskutil 真跑: 无论有没有插盘, 都应正常退出并给出可辨认输出
        r = subprocess.run([sys.executable, '-m', 'nopwd', '--list'],
                           cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue('外接盘' in r.stdout or '未检测到外接盘' in r.stdout, r.stdout)

if __name__ == '__main__':
    unittest.main()
