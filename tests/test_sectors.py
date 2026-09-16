"""扇区转换测试: 输出金标(锁死行为漂移) + 三条铁律 + 字段语义。"""
import unittest, hashlib, struct

from tests.helpers import FIXTURES, fixture_bin, load_disk_image, read_fn_of
from nopwd.sectors import (convert, convert_lba0, make_entry, E7, E12, EDPF_ENC_LEN,
                           PWD_CRC, NOPWD_LBA6_1CA)
from nopwd.crypto import xor_rolling, a6b0_full, crc32_bare
from nopwd.common import SECTOR

md5 = lambda b: hashlib.md5(b).hexdigest()

# 金标 = 2026-09-16 拆包前单文件版对下列真实备份的实测输出
GOLDEN = {
    'netac': dict(share=116707265, enc_start=116707328, enc_size=3143761920,
                  crc='F1A78819', k0='79BE', lba9_none=False,
                  lba0='fd76dc67f22b770b9083c1d048188726', lba6='7088f76eb9cd19bf10e3d98d4a6049c2',
                  lba7='ff53ab1c18053c783250d0b8c1a9283f', lba12='c36f05a29d9877a25681051594c557ce'),
    'lexar': dict(share=231423937, enc_start=231424000, enc_size=6234963968,
                  crc='6BBAEEFB', k0='8541', lba9_none=False,
                  lba0='18c2b6e793f78329bf159b3726632070', lba6='56f8012133790126707c79af0ca0e662',
                  lba7='59fe2195d7c80d9c73c0fd98876bb0a5', lba12='7ec3ddb9ac51c246ec7fb4742b8689cc'),
    'aigo':  dict(share=243115997, enc_start=243116060, enc_size=1340720640,
                  crc='2EEB4CE1', k0='620A', lba9_none=True,
                  lba0='52e8a2a54bfe646b236ee0f84d79d32d', lba6='769de2441ba38f39e03297c3d928ec71',
                  lba7='02d920e71c328dc4df5b206fa51f232c', lba12='f865701657d7bd000752574546d50b4e'),
    'aigo_size50': dict(share=97656248, enc_start=243116060,
                        lba0='2a0fabe2b49e644867befdec33bdf9d7', lba6='769de2441ba38f39e03297c3d928ec71',
                        lba7='42a2778336ead2a313f2bdfe6e3352c2', lba12='c8fc3272ede87ba259a4d4e4d99f08fa'),
}

@unittest.skipIf(any(fixture_bin(k) is None for k in FIXTURES), '真实备份不可用')
class TestConvertGolden(unittest.TestCase):
    def test_default_size_all_disks(self):
        for key in FIXTURES:
            r = convert(read_fn_of(load_disk_image(key)), FIXTURES[key][1], None, verbose=False)
            g = GOLDEN[key]
            self.assertEqual(r['share'], g['share'], key)
            self.assertEqual(r['enc_start'], g['enc_start'], key)
            self.assertEqual(r['enc_size'], g['enc_size'], key)
            self.assertEqual(f"{r['crc']:08X}", g['crc'], key)
            self.assertEqual(f"{r['k0']:04X}", g['k0'], key)
            self.assertEqual(r['lba9'] is None, g['lba9_none'], key)
            for lba in (0, 6, 7, 12):
                self.assertEqual(md5(r[f'lba{lba}']), g[f'lba{lba}'], f'{key} LBA{lba}')

    def test_size_gb_path(self):
        r = convert(read_fn_of(load_disk_image('aigo')), FIXTURES['aigo'][1], 50, verbose=False)
        g = GOLDEN['aigo_size50']
        self.assertEqual(r['share'], g['share'])            # 8 扇对齐: 97,656,248
        self.assertEqual(r['enc_start'], g['enc_start'])
        for lba in (0, 6, 7, 12):
            self.assertEqual(md5(r[f'lba{lba}']), g[f'lba{lba}'], f'LBA{lba}')

    def test_size_overflow_rejected(self):
        # 60GB 盘放不下 100GB Share → SystemExit, 而非越界写
        with self.assertRaises(SystemExit):
            convert(read_fn_of(load_disk_image('netac')), FIXTURES['netac'][1], 100, verbose=False)

    def test_wrong_device_id_rejected(self):
        with self.assertRaises(SystemExit):
            convert(read_fn_of(load_disk_image('netac')), 'disk&ven_bogus&prod_x', None, verbose=False)

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestIronRules(unittest.TestCase):
    """三条铁律的机械检查: 终止符/尾部 144B 保留, 其余扇区不动。"""
    def setUp(self):
        self.data = load_disk_image('netac')
        self.r = convert(read_fn_of(self.data), FIXTURES['netac'][1], None, verbose=False)

    def test_lba7_terminator_preserved(self):
        # 0xC0 表尾终止符区: 明文未动 → 滚动 XOR 密文也不变
        self.assertEqual(self.r['lba7'][0xC0:], self.data[7*SECTOR+0xC0:8*SECTOR])

    def test_lba12_terminator_and_tail144_preserved(self):
        # 0x120 终止符区在加密区内, 明文不动则密文不动
        self.assertEqual(self.r['lba12'][0x120:EDPF_ENC_LEN], self.data[12*SECTOR+0x120:12*SECTOR+EDPF_ENC_LEN])
        # 尾部 144B 原盘密文原样拼接
        self.assertEqual(self.r['lba12'][EDPF_ENC_LEN:], self.data[12*SECTOR+EDPF_ENC_LEN:13*SECTOR])

    def test_encrypt_entry_start_size_from_original(self):
        # 改造后 entry1(type4 指针)的 start/size 取自原盘 type=4 entry, 不发明
        from nopwd.sectors import find_type_entry
        key = struct.pack('<I', crc32_bare(FIXTURES['netac'][1].encode()))
        dec_old = a6b0_full(self.data[12*SECTOR:12*SECTOR+EDPF_ENC_LEN], key, 0)
        dec_new = a6b0_full(self.r['lba12'][:EDPF_ENC_LEN], key, 0)
        e_old = dec_old[find_type_entry(dec_old, E12, 4)*E12:(find_type_entry(dec_old, E12, 4)+1)*E12]
        e_new = dec_new[E12:2*E12]
        self.assertEqual(struct.unpack_from('<Q', e_new, 0x18)[0], struct.unpack_from('<Q', e_old, 0x18)[0])
        self.assertEqual(struct.unpack_from('<Q', e_new, 0x28)[0], struct.unpack_from('<Q', e_old, 0x28)[0])

    def test_lba0_mbr_shape(self):
        out = self.r['lba0']
        self.assertEqual(out[0x1FE:0x200], b'\x55\xaa')
        self.assertEqual(out[0x1BE+4], 0x07)                       # type=07
        self.assertEqual(struct.unpack_from('<I', out, 0x1BE+8)[0], 63)
        self.assertEqual(struct.unpack_from('<I', out, 0x1BE+12)[0], self.r['share'])
        self.assertEqual(out[0x1CE:0x1FE], bytes(0x1FE-0x1CE))     # 分区2-4 清零

    def test_lba9_zeroed_when_present(self):
        self.assertEqual(self.r['lba9'], bytes(SECTOR))

@unittest.skipIf(fixture_bin('aigo') is None, '真实备份不可用')
class TestMakeEntry(unittest.TestCase):
    def test_fields(self):
        data = load_disk_image('aigo')
        crc = crc32_bare(FIXTURES['aigo'][1].encode())
        dec = xor_rolling(data[7*SECTOR:8*SECTOR], (crc & 0xFFFF) ^ (crc >> 16))
        src = dec[E7:2*E7]                                       # 原 type=4 entry 作壳
        e = make_entry(src, 2, 63, 12345 * SECTOR)
        self.assertEqual(e[:4], b'EDPF')
        self.assertEqual(struct.unpack_from('<I', e, 0x08)[0], 2)          # 版本2
        self.assertEqual(struct.unpack_from('<I', e, 0x0c)[0], 2)          # type=Share
        self.assertEqual(struct.unpack_from('<I', e, 0x10)[0], 1)          # 激活
        self.assertEqual(struct.unpack_from('<I', e, 0x14)[0], 1)          # 加密使能
        self.assertEqual(struct.unpack_from('<Q', e, 0x18)[0], 63)
        self.assertEqual(struct.unpack_from('<Q', e, 0x20)[0], 0x200)      # bps=512
        self.assertEqual(struct.unpack_from('<Q', e, 0x28)[0], 12345 * SECTOR)
        self.assertEqual(struct.unpack_from('<I', e, 0x30)[0], PWD_CRC)    # CRC32("0000aaaa")
        self.assertEqual(e[0x34:], src[0x34:])                   # 其余字段原样保留

if __name__ == '__main__':
    unittest.main()
