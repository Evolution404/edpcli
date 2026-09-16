"""测试公用: 路径常量、真实备份夹具定位、临时'盘'镜像。"""
import os, sys, tempfile, shutil

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

BAK_DIR = os.path.join(ROOT, 'backup')

# (键, 备份文件名, device_id) — 覆盖三种型号, 含 BOT 带 &rev_ 的 aigo
FIXTURES = {
    'netac': ('disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin',
              'disk&ven_netac&prod_onlydisk'),
    'lexar': ('disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin',
              'disk&ven_lexar&prod_usb_flash_drive'),
    'aigo':  ('disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin',
              'disk&ven_aigo&prod_u335&rev_pmap'),
}
AIGO_NEG_ID = 'disk4_1953525168_vid174c_pid55aa_disk&ven_aigo&prod_hd806_onlyid-1833210541_20260903_121552.bin'

def fixture_bin(key):
    """备份 bin 的完整路径; 文件不存在返回 None(用例自行 skip)。"""
    p = os.path.join(BAK_DIR, FIXTURES[key][0])
    return p if os.path.exists(p) else None

def load_disk_image(key):
    """整份 14 扇备份 → bytes(即一张'盘'的 LBA0-13 镜像)。"""
    with open(fixture_bin(key), 'rb') as f:
        return f.read()

def read_fn_of(data):
    return lambda lba: data[lba*512:(lba+1)*512]

class TmpDir:
    """临时目录上下文: with TmpDir() as d: ..."""
    def __enter__(self):
        self.d = tempfile.mkdtemp(prefix='nopwd_test_')
        return self.d
    def __exit__(self, *exc):
        shutil.rmtree(self.d, ignore_errors=True)
