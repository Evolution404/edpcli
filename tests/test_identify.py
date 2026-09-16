"""device_id 识别测试: 纯函数逻辑 + 以真实备份当'盘'的 identify 判真。"""
import unittest
from unittest import mock

from tests.helpers import fixture_bin, FIXTURES
from nopwd import identify, diskio
from nopwd.identify import build_device_id, _norm

class TestNorm(unittest.TestCase):
    def test_trailing_space_lower_underscore(self):
        self.assertEqual(_norm('Netac  '), 'netac')
        self.assertEqual(_norm('My Prod'), 'my_prod')
        self.assertEqual(_norm(None), '')
        self.assertEqual(_norm(''), '')

class TestBuildDeviceId(unittest.TestCase):
    def test_bot_includes_rev(self):
        self.assertEqual(build_device_id('AIGO', 'U335', 'PMAP', 'BOT'),
                         'disk&ven_aigo&prod_u335&rev_pmap')

    def test_uas_drops_rev(self):
        self.assertEqual(build_device_id('AIGO', 'U335', 'PMAP', 'UAS'),
                         'disk&ven_aigo&prod_u335')

    def test_bot_without_rev_falls_back_to_base(self):
        self.assertEqual(build_device_id('V', 'P', '', 'BOT'), 'disk&ven_v&prod_p')

    def test_unknown_transport_drops_rev(self):
        self.assertEqual(build_device_id('V', 'P', 'R1', 'UNKNOWN'), 'disk&ven_v&prod_p')

@unittest.skipIf(fixture_bin('netac') is None, '真实备份不可用')
class TestIdentify(unittest.TestCase):
    def test_picks_edpf_verified_candidate(self):
        real = FIXTURES['netac'][1]
        with mock.patch.object(diskio, '_raw_path', return_value=fixture_bin('netac')), \
             mock.patch.object(identify, 'generate_candidates',
                               return_value=['disk&ven_bogus&prod_x', real]):
            did, crc, k0 = identify.identify(99)
        self.assertEqual(did, real)
        self.assertEqual(crc, 0xF1A78819)
        self.assertEqual(k0, 0x79BE)

    def test_no_candidate_matches(self):
        with mock.patch.object(diskio, '_raw_path', return_value=fixture_bin('netac')), \
             mock.patch.object(identify, 'generate_candidates',
                               return_value=['disk&ven_bogus&prod_x']):
            self.assertEqual(identify.identify(99), (None, None, None))

if __name__ == '__main__':
    unittest.main()
