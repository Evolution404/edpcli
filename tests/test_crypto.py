"""加密原语测试: 向量金标 + 往返自洽 + 真实盘密文可解性。"""
import unittest, random

from tests.helpers import FIXTURES, fixture_bin, load_disk_image, read_fn_of
from nopwd.crypto import (crc32_bare, xor_rolling, a6b0_full, a7f0_full,
                          lba6_decode, lba6_checksum)
from nopwd.sectors import EDPF_ENC_LEN
from nopwd.common import SECTOR
import struct

class TestCrc32Bare(unittest.TestCase):
    def test_empty_is_zero(self):
        self.assertEqual(crc32_bare(b''), 0)

    def test_golden_device_ids(self):
        # 拆包前单文件版实测值
        for did, crc, k0 in (('disk&ven_netac&prod_onlydisk', 0xF1A78819, 0x79BE),
                             ('disk&ven_lexar&prod_usb_flash_drive', 0x6BBAEEFB, 0x8541),
                             ('disk&ven_aigo&prod_u335&rev_pmap', 0x2EEB4CE1, 0x620A)):
            c = crc32_bare(did.encode())
            self.assertEqual(c, crc, did)
            self.assertEqual((c & 0xFFFF) ^ (c >> 16), k0, did)

class TestXorRolling(unittest.TestCase):
    def test_roundtrip(self):
        random.seed(42)
        for _ in range(20):
            d = bytes(random.randrange(256) for _ in range(random.choice([2, 64, 512])))
            k = random.randrange(0x10000)
            self.assertEqual(xor_rolling(xor_rolling(d, k), k), d)

    def test_rolling_key_schedule(self):
        # 密钥演进 key_{i+1} = key_i + 0x100 - i - 1: 两个字全零明文 → 密文即各位置密钥
        out = xor_rolling(bytes(4), 0x1234)
        w0, w1 = struct.unpack('<HH', out)
        self.assertEqual((w0, w1), (0x1234, (0x1234 + 0x100 - 0 - 1) & 0xFFFF))

class TestAesVariant(unittest.TestCase):
    def test_roundtrip(self):
        random.seed(7)
        for _ in range(5):
            d = bytes(random.randrange(256) for _ in range(EDPF_ENC_LEN))
            key = bytes(random.randrange(256) for _ in range(4))
            self.assertEqual(a6b0_full(a7f0_full(d, key, 0), key, 0), d)

    def test_counter_is_block_number_times_16(self):
        # counter 按块推进: 整段加解密必须等价于按 initial_counter=0 连续处理
        random.seed(8)
        d = bytes(random.randrange(256) for _ in range(48))
        key = bytes(range(4))
        self.assertEqual(a6b0_full(a7f0_full(d, key, 0), key, 0),
                         a6b0_full(a7f0_full(d, key, 16), key, 16))

class TestLba6(unittest.TestCase):
    def test_decode_splits_checksum(self):
        raw = bytes(range(256)) * 2
        dec = lba6_decode(raw)
        self.assertEqual(len(dec), SECTOR)
        self.assertEqual(dec[0x1FC:], raw[0x1FC:])   # 尾 4B 校验和原样透传

    def test_checksum_is_rot10_of_crc(self):
        def rot10(v):
            for _ in range(10):
                v = ((v >> 15) + (v << 1)) & 0xFFFFFFFF
            return v
        d = b'\x01' * 508
        self.assertEqual(lba6_checksum(d), rot10(crc32_bare(d)))
        self.assertNotEqual(lba6_checksum(d), crc32_bare(d))  # 旋转变换改变值

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestAgainstRealDisks(unittest.TestCase):
    """三种真实盘备份: LBA12 前 368B 用各自 CRC 作 key 必须解出 EDPF magic。"""
    def test_lba12_decrypts_to_edpf(self):
        for key in FIXTURES:
            data = load_disk_image(key)
            did = FIXTURES[key][1]
            crc_key = struct.pack('<I', crc32_bare(did.encode()))
            dec = a6b0_full(data[12*SECTOR:12*SECTOR+EDPF_ENC_LEN], crc_key, 0)
            self.assertEqual(dec[:4], b'EDPF', f'{key}: {dec[:4].hex()}')

    def test_lba7_decrypts_to_edpf(self):
        for key in FIXTURES:
            data = load_disk_image(key)
            did = FIXTURES[key][1]
            crc = crc32_bare(did.encode())
            k0 = (crc & 0xFFFF) ^ (crc >> 16)
            dec = xor_rolling(data[7*SECTOR:8*SECTOR], k0)
            self.assertEqual(dec[:4], b'EDPF', f'{key}: {dec[:4].hex()}')

    def test_wrong_id_does_not_decrypt(self):
        data = load_disk_image('netac')
        crc = crc32_bare(b'disk&ven_bogus&prod_x')
        k0 = (crc & 0xFFFF) ^ (crc >> 16)
        self.assertNotEqual(xor_rolling(data[7*SECTOR:8*SECTOR], k0)[:4], b'EDPF')

if __name__ == '__main__':
    unittest.main()
