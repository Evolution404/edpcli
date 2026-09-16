#!/usr/bin/env python3
"""nopwd.py — cems 加密 U 盘 → 无密码盘 一键改造工具(独立版, 零外部依赖)

仅需 Python3 标准库。不依赖本仓库任何其他文件。

原理(2026-08-27 定案, 内网实测成功):
  把 3 分区加密盘(Boot/Share/Encrypt)"原盘原地 patch"成 2 分区免密盘:
    LBA0  : MBR 分区1 → type=0x07 @63 × Share扇数(数据区直挂, 系统原生挂载)
    LBA6  : 0x1CA=128,480 + 0x1D4-0x1ED 清零, 身份字段保留, 重算校验和
    LBA7  : EDPF 2条版本2 [Share@63, type4指针(原盘保留)] + 表尾终止符保留
    LBA12 : EDPF 2条版本2 + 表尾终止符保留 + 尾部144B原盘保留
    LBA9  : 非零则清零(EETU)
    其余扇区(LBA4/8/11 及全零保留区)一律不动
  三条铁律: EDPF 表尾终止符必须保留(LBA7@0xC0/LBA12@0x120);
            LBA12 尾部144B(0x170-0x200)不可清零; 不发明原盘没有的状态。
  分区参数按实际物理盘计算: Encrypt 从原盘 LBA12 type=4 读取, Share 占满其前。

加密算法(逆向 cemsusbregsiter.dll / sectormanage64.dll 得到, 均内置于本文件):
  device_id → CRC32(bare: init=0, poly 0xEDB88320, 无final-xor)
  LBA7  K0 = low16(CRC) ^ high16(CRC), 整扇 16位字滚动 XOR(逐字递减1)
  LBA12 key = CRC的4字节×4 ^ "EDPSECDISK200709" → AES-128 变体(A6B0/a7f0,
         counter=块号×16), 仅加密前 368B, 尾部 144B 为原始数据
  LBA6  固定 K0=0x4DAA 滚动 XOR; 0x1FC 校验和 = 密文CRC32(bare)×10轮
         ((v>>15)+(v<<1)) 变换, 小端写入

用法:
  离线(快照目录, 供验证): python3 nopwd.py --dir <快照目录> --id <device_id> [--out <目录>]
  真盘 dry-run:          sudo python3 nopwd.py [--disk N] [--size GB]  # N 缺省自动检测 USB 盘
  真盘写入:             sudo python3 nopwd.py --apply
  列出本盘备份(不写入): sudo python3 nopwd.py --restore
  还原预检(不写入):     sudo python3 nopwd.py --restore <备份.bin>
  还原写入(须 --apply): sudo python3 nopwd.py --restore <备份.bin> --apply

实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
Kingston DT3.0 64G (每盘改前自动备份, 可随时 --restore 还原)。
"""
import os, sys, struct, argparse, glob, hashlib, subprocess, re, time

SECTOR = 512
LBA6_K0 = 0x4DAA            # LBA6 SAFE6 固定滚动 XOR key(跨盘通用)
EDPF_ENC_LEN = 368          # LBA12 前 368B A6B0 加密, 后 144B 不加密
E7, E12 = 0x40, 0x60        # entry stride: LBA7=64B, LBA12=96B
PWD_CRC = 0x0429735D        # CRC32_bare("0000aaaa") 免密盘默认密码
NOPWD_LBA6_1CA = 128480     # 免密盘 LBA6 0x1CA 模板默认值
LBA6_CLEAR = (0x1D4, 0x1ED) # LBA6 清零区间(含 0x1EC)
PART_TYPES = {1: 'Boot', 2: 'Share', 4: 'Encrypt/IIR指针'}

def fmt_gb(num_bytes):
    """容量显示: GB(10^9) 两位小数, 四舍五入(整数运算, 与 macOS 显示一致)。"""
    return f'{(num_bytes + 5 * 10**6) // 10**7 / 100:.2f}GB'

# ══════════════════════════════════════════════════════════════════
# 1. CRC32 (bare: init=0, poly 0xEDB88320, 无 final-xor) — sub_10001130
# ══════════════════════════════════════════════════════════════════
_crc_table = [0] * 256
for _i in range(256):
    _c = _i
    for _ in range(8):
        _c = (_c >> 1) ^ 0xEDB88320 if _c & 1 else _c >> 1
    _crc_table[_i] = _c

def crc32_bare(data):
    c = 0
    for b in data:
        c = _crc_table[(c ^ b) & 0xFF] ^ (c >> 8)
    return c

# ══════════════════════════════════════════════════════════════════
# 2. AES-128 变体 (A6B0 解密 / a7f0 加密) — sub_1003a6b0 / sub_1003a7f0
#    key = CRC32(device_id) 4字节重复4遍 ^ "EDPSECDISK200709"
#    counter XOR 进 round keys (counter = 块号*16)
# ══════════════════════════════════════════════════════════════════
KEY_TABLE = b"EDPSECDISK200709"
SBOX = [99,124,119,123,242,107,111,197,48,1,103,43,254,215,171,118,202,130,201,125,250,89,71,240,173,212,162,175,156,164,114,192,183,253,147,38,54,63,247,204,52,165,229,241,113,216,49,21,4,199,35,195,24,150,5,154,7,18,128,226,235,39,178,117,9,131,44,26,27,110,90,160,82,59,214,179,41,227,47,132,83,209,0,237,32,252,177,91,106,203,190,57,74,76,88,207,208,239,170,251,67,77,51,133,69,249,2,127,80,60,159,168,81,163,64,143,146,157,56,245,188,182,218,33,16,255,243,210,205,12,19,236,95,151,68,23,196,167,126,61,100,93,25,115,96,129,79,220,34,42,144,136,70,238,184,20,222,94,11,219,224,50,58,10,73,6,36,92,194,211,172,98,145,149,228,121,231,200,55,109,141,213,78,169,108,86,244,234,101,122,174,8,186,120,37,46,28,166,180,198,232,221,116,31,75,189,139,138,112,62,181,102,72,3,246,14,97,53,87,185,134,193,29,158,225,248,152,17,105,217,142,148,155,30,135,233,206,85,40,223,140,161,137,13,191,230,66,104,65,153,45,15,176,84,187,22]
SBOX2 = [82,9,106,213,48,54,165,56,191,64,163,158,129,243,215,251,124,227,57,130,155,47,255,135,52,142,67,68,196,222,233,203,84,123,148,50,166,194,35,61,238,76,149,11,66,250,195,78,8,46,161,102,40,217,36,178,118,91,162,73,109,139,209,37,114,248,246,100,134,104,152,22,212,164,92,204,93,101,182,146,108,112,72,80,253,237,185,218,94,21,70,87,167,141,157,132,144,216,171,0,140,188,211,10,247,228,88,5,184,179,69,6,208,44,30,143,202,63,15,2,193,175,189,3,1,19,138,107,58,145,17,65,79,103,220,234,151,242,207,206,240,180,230,115,150,172,116,34,231,173,53,133,226,249,55,232,28,117,223,110,71,241,26,113,29,41,197,137,111,183,98,14,170,24,190,27,252,86,62,75,198,210,121,32,154,219,192,254,120,205,90,244,31,221,168,51,136,7,199,49,177,18,16,89,39,128,236,95,96,81,127,169,25,181,74,13,45,229,122,159,147,201,156,239,160,224,59,77,174,42,245,176,200,235,187,60,131,83,153,97,23,43,4,126,186,119,214,38,225,105,20,99,85,33,12,125]
RCON = [0,1,2,4,8,16,32,64,128,27,54]

def _aes_expand(key16):
    w = [list(key16[i*4:(i+1)*4]) for i in range(4)]
    for i in range(4, 44):
        t = w[i-1][:]
        if i % 4 == 0:
            t = t[1:] + t[:1]; t = [SBOX[b] for b in t]; t[0] ^= RCON[i//4]
        w.append([w[i-4][j] ^ t[j] for j in range(4)])
    return w

def _shift_rows(s): return [s[0],s[5],s[10],s[15],s[4],s[9],s[14],s[3],s[8],s[13],s[2],s[7],s[12],s[1],s[6],s[11]]
def _inv_shift(s):  return [s[0],s[13],s[10],s[7],s[4],s[1],s[14],s[11],s[8],s[5],s[2],s[15],s[12],s[9],s[6],s[3]]

def _gf_mul(a, b):
    p = 0
    for _ in range(8):
        if b & 1: p ^= a
        h = a & 0x80; a = (a << 1) & 0xFF
        if h: a ^= 0x1B
        b >>= 1
    return p

def _mix_column(col):
    a,b,c,d = col
    return [_gf_mul(a,2)^_gf_mul(b,3)^_gf_mul(c,1)^_gf_mul(d,1),
            _gf_mul(a,1)^_gf_mul(b,2)^_gf_mul(c,3)^_gf_mul(d,1),
            _gf_mul(a,1)^_gf_mul(b,1)^_gf_mul(c,2)^_gf_mul(d,3),
            _gf_mul(a,3)^_gf_mul(b,1)^_gf_mul(c,1)^_gf_mul(d,2)]

def _imix(col):
    a,b,c,d = col
    return [_gf_mul(a,14)^_gf_mul(b,11)^_gf_mul(c,13)^_gf_mul(d,9),
            _gf_mul(a,9)^_gf_mul(b,14)^_gf_mul(c,11)^_gf_mul(d,13),
            _gf_mul(a,13)^_gf_mul(b,9)^_gf_mul(c,14)^_gf_mul(d,11),
            _gf_mul(a,11)^_gf_mul(b,13)^_gf_mul(c,9)^_gf_mul(d,14)]

def a6b0_decrypt(data, key_raw, counter=0):
    expanded = bytes([key_raw[i % len(key_raw)] ^ KEY_TABLE[i] for i in range(16)])
    rk = _aes_expand(expanded)
    cb = struct.pack("<I", counter) + b'\x00\x00\x00\x00'
    for wi in range(44):
        for bi in range(4): rk[wi][bi] ^= cb[(wi*4+bi) % 8]
    s = list(data)
    lk = []; [lk.extend(b) for b in rk[40:44]]; s = [s[i]^lk[i] for i in range(16)]
    for rnd in range(9, 0, -1):
        s = _inv_shift(s); s = [SBOX2[b] for b in s]
        kr = []; [kr.extend(b) for b in rk[rnd*4:(rnd+1)*4]]; s = [s[i]^kr[i] for i in range(16)]
        for c in range(0, 16, 4): s[c:c+4] = _imix(s[c:c+4])
    s = _inv_shift(s); s = [SBOX2[b] for b in s]
    k0 = []; [k0.extend(b) for b in rk[0:4]]; s = [s[i]^k0[i] for i in range(16)]
    return bytes(s)

def a7f0_encrypt(data, key_raw, counter=0):
    expanded = bytes([key_raw[i % len(key_raw)] ^ KEY_TABLE[i] for i in range(16)])
    rk = _aes_expand(expanded)
    cb = struct.pack("<I", counter) + b'\x00\x00\x00\x00'
    for wi in range(44):
        for bi in range(4): rk[wi][bi] ^= cb[(wi*4+bi) % 8]
    s = list(data)
    k0 = []; [k0.extend(b) for b in rk[0:4]]; s = [s[i]^k0[i] for i in range(16)]
    for rnd in range(1, 10):
        s = [SBOX[b] for b in s]; s = _shift_rows(s)
        for c in range(0, 16, 4): s[c:c+4] = _mix_column(s[c:c+4])
        kr = []; [kr.extend(b) for b in rk[rnd*4:(rnd+1)*4]]; s = [s[i]^kr[i] for i in range(16)]
    s = [SBOX[b] for b in s]; s = _shift_rows(s)
    lk = []; [lk.extend(b) for b in rk[40:44]]; s = [s[i]^lk[i] for i in range(16)]
    return bytes(s)

def a6b0_full(data, key16, initial_counter=0):
    out = bytearray(); ctr = initial_counter
    for i in range(0, len(data), 16):
        out += a6b0_decrypt(data[i:i+16], key16, ctr); ctr += 16
    return bytes(out)

def a7f0_full(data, key_raw, initial_counter=0):
    out = bytearray(); ctr = initial_counter
    for i in range(0, len(data), 16):
        out += a7f0_encrypt(data[i:i+16], key_raw, ctr); ctr += 16
    return bytes(out)

# ══════════════════════════════════════════════════════════════════
# 3. 滚动 XOR (16位字, key 逐字递减1) — sub_10013de0 / sub_10013fd0
# ══════════════════════════════════════════════════════════════════
def xor_rolling(data, k0):
    r = bytearray(data); key = k0 & 0xFFFF
    for i in range(len(r) // 2):
        w = struct.unpack_from("<H", r, i*2)[0] ^ key
        struct.pack_into("<H", r, i*2, w)
        key = (key + 0x100 - i - 1) & 0xFFFF
    return bytes(r)

# ══════════════════════════════════════════════════════════════════
# 4. LBA6 (SAFE6): 解密 + 校验和
# ══════════════════════════════════════════════════════════════════
def lba6_checksum(cipher_508b):
    """对 XOR 加密后的密文: CRC32(bare) 后 10 轮 ((v>>15)+(v<<1)) 变换。"""
    c = 0
    for b in cipher_508b:
        c = (c >> 8) ^ _crc_table[(c & 0xFF) ^ b]
    for _ in range(10):
        c = ((c >> 15) + (c << 1)) & 0xFFFFFFFF
    return c

def lba6_decode(raw):
    """前 508B 滚动 XOR 解密, 后 4B 校验和(写入在加密之后, 不参与)。"""
    return xor_rolling(raw[:0x1FC], LBA6_K0) + raw[0x1FC:0x200]

# ══════════════════════════════════════════════════════════════════
# 5. device_id 识别 (macOS ioreg INQUIRY + 传输模式; LBA7 EDPF magic 判真)
# ══════════════════════════════════════════════════════════════════
def _norm(s):
    return (s or '').rstrip(' ').replace(' ', '_').lower() if s else ''

def _ioreg_fields(cls, disk, keys):
    try:
        out = subprocess.check_output(['ioreg', '-r', '-c', cls, '-l'],
                                      text=True, errors='ignore', timeout=15)
    except Exception:
        return {}
    blocks = re.split(r'(?=^\s*\+-o ' + re.escape(cls) + r')', out, flags=re.M)
    want = f'"BSD Name" = "disk{disk}"'
    for b in blocks:
        if want not in b: continue
        d = {}
        for k in keys:
            m = re.search(r'"' + re.escape(k) + r'"\s*=\s*"([^"]*)"', b)
            if m: d[k] = m.group(1)
        return d
    return {}

def detect_transport(disk):
    present = []
    for cls in ('IOUSBMassStorageUASDriver', 'IOUSBMassStorageInterfaceNub', 'IOUSBMassStorageDriver'):
        try:
            out = subprocess.check_output(['ioreg', '-r', '-c', cls, '-l'],
                                          text=True, errors='ignore', timeout=15)
            if f'"BSD Name" = "disk{disk}"' in out: present.append(cls)
        except Exception:
            pass
    if 'IOUSBMassStorageUASDriver' in present: return 'UAS'
    if 'IOUSBMassStorageInterfaceNub' in present or 'IOUSBMassStorageDriver' in present: return 'BOT'
    return 'UNKNOWN'

def build_device_id(vendor, product, revision='', transport='UNKNOWN'):
    """Windows InstanceId 中间段: BOT(usbstor)含 &rev_, UAS(uaspstor)通常不含。"""
    v, p = _norm(vendor), _norm(product)
    base = f"disk&ven_{v}&prod_{p}"
    if transport == 'BOT':
        r = _norm(revision)
        if r: return base + f"&rev_{r}"
    return base

def generate_candidates(disk):
    cs = []
    def add(c):
        if c and c not in cs: cs.append(c)
    transport = detect_transport(disk)
    for cls in ('IOSCSITargetDevice', 'IOSCSILogicalUnitNub', 'IOSCSIPeripheralDeviceNub'):
        d = _ioreg_fields(cls, disk, ('Vendor Identification', 'Product Identification', 'Product Revision Level'))
        if d.get('Vendor Identification'):
            v, p, rev = d['Vendor Identification'], d.get('Product Identification', ''), d.get('Product Revision Level', '')
            long_id = build_device_id(v, p, rev, 'BOT')
            short_id = build_device_id(v, p, rev, 'UAS')
            if transport == 'UAS':
                add(short_id); add(long_id)
            else:
                add(long_id); add(short_id)
            break
    return cs

def identify(disk):
    """返回 (device_id, crc32, k0); 两候选 LBA7 EDPF magic 判真。"""
    raw = read_lba_disk(disk, 7)
    for c in generate_candidates(disk):
        crc = crc32_bare(c.encode())
        k0 = (crc & 0xFFFF) ^ ((crc >> 16) & 0xFFFF)
        if xor_rolling(raw, k0)[:4] == b'EDPF':
            return c, crc, k0
    return None, None, None

# ══════════════════════════════════════════════════════════════════
# 6. EDPF entry 工具
# ══════════════════════════════════════════════════════════════════
def ent(dec, i, stride):
    return dec[i * stride:(i + 1) * stride]

def find_type_entry(dec, stride, ptype, n=3):
    for i in range(n):
        e = ent(dec, i, stride)
        if e[:4] == b'EDPF' and struct.unpack_from('<I', e, 0x0c)[0] == ptype:
            return i
    sys.exit(f'错误: EDPF 中未找到 type={ptype}({PART_TYPES.get(ptype)}) entry')

def make_entry(src_e, ptype, start, size):
    """以原 type=4 entry 为壳, 改 type/start/size + 免密规则字段, 其余原样。
    entry0(Share)与 entry1 由此天然成对(材料字段相同)。"""
    e = bytearray(src_e)
    e[0:4] = b'EDPF'
    struct.pack_into('<I', e, 0x08, 2)             # 版本 2
    struct.pack_into('<I', e, 0x0c, ptype)
    struct.pack_into('<I', e, 0x10, 1)             # 激活=1 (原 Encrypt 为 0)
    struct.pack_into('<I', e, 0x14, 1)             # 加密使能=1 (原 Boot 为 0)
    struct.pack_into('<Q', e, 0x18, start)
    struct.pack_into('<Q', e, 0x20, 0x200)         # bps=512
    struct.pack_into('<Q', e, 0x28, size)
    struct.pack_into('<I', e, 0x30, PWD_CRC)       # pwdCRC=CRC32("0000aaaa")
    return bytes(e)

# ══════════════════════════════════════════════════════════════════
# 7. 扇区转换
# ══════════════════════════════════════════════════════════════════
def convert_lba0(raw, share_sectors):
    out = bytearray(raw)
    for i in range(4):
        out[0x1BE + i*16: 0x1BE + (i+1)*16] = b'\x00' * 16
    out[0x1BE + 4] = 0x07
    struct.pack_into('<I', out, 0x1BE + 8, 63)
    struct.pack_into('<I', out, 0x1BE + 12, share_sectors)
    out[0x1FE:0x200] = b'\x55\xaa'
    return bytes(out)

def convert_lba6(raw):
    dec = bytearray(lba6_decode(raw))
    if dec[0x188:0x190] == b'\x00' * 8:
        sys.exit('错误: LBA6 解密后 0x188 magic 为零 — 非法 SAFE6')
    struct.pack_into('<I', dec, 0x1CA, NOPWD_LBA6_1CA)
    lo, hi = LBA6_CLEAR
    dec[lo:hi] = b'\x00' * (hi - lo)
    cipher = xor_rolling(bytes(dec[:0x1FC]), LBA6_K0)
    csum = lba6_checksum(cipher)
    new = cipher + struct.pack('<I', csum)
    if lba6_decode(new)[:0x1FC] != bytes(dec[:0x1FC]):
        sys.exit('错误: LBA6 往返自检失败')
    return new, bytes(dec)

def convert_lba7(raw, k0, share_sectors):
    dec = bytearray(xor_rolling(raw, k0))
    if dec[:4] != b'EDPF':
        sys.exit(f'错误: LBA7 解密后非 EDPF magic({dec[:4].hex()}) — device_id/K0 不符')
    src = ent(dec, find_type_entry(dec, E7, 4), E7)
    dec[0:E7] = make_entry(src, 2, 63, share_sectors * SECTOR)
    e1 = ent(dec, find_type_entry(dec, E7, 4), E7)
    s1, z1 = struct.unpack_from('<Q', e1, 0x18)[0], struct.unpack_from('<Q', e1, 0x28)[0]
    dec[E7:2*E7] = make_entry(src, 4, s1, z1)
    dec[2*E7:3*E7] = bytes(E7)                      # entry2 区清零(3条→2条)
    # 0xC0 表尾终止符及之后不动
    return xor_rolling(bytes(dec), k0), bytes(dec)

def convert_lba12(raw, crc_key, share_sectors):
    dec = bytearray(a6b0_full(raw[:EDPF_ENC_LEN], crc_key, 0))
    if dec[:4] != b'EDPF':
        sys.exit(f'错误: LBA12 解密后非 EDPF magic({dec[:4].hex()}) — device_id/CRC 不符')
    src = ent(dec, find_type_entry(dec, E12, 4), E12)
    dec[0:E12] = make_entry(src, 2, 63, share_sectors * SECTOR)
    s1, z1 = struct.unpack_from('<Q', src, 0x18)[0], struct.unpack_from('<Q', src, 0x28)[0]
    dec[E12:2*E12] = make_entry(src, 4, s1, z1)
    dec[2*E12:3*E12] = bytes(E12)                   # entry2 区清零
    # 0x120 表尾终止符区不动; 尾部 144B 从原盘密文原样拼接
    enc = a7f0_full(bytes(dec), crc_key, 0) + raw[EDPF_ENC_LEN:]
    if a6b0_full(enc[:EDPF_ENC_LEN], crc_key, 0) != bytes(dec):
        sys.exit('错误: LBA12 A6B0/a7f0 往返自检失败')
    return enc, bytes(dec)

# ══════════════════════════════════════════════════════════════════
# 8. 主转换
# ══════════════════════════════════════════════════════════════════
def convert(read_fn, device_id, size_gb=None, verbose=True):
    crc = crc32_bare(device_id.encode())
    k0 = (crc & 0xFFFF) ^ ((crc >> 16) & 0xFFFF)
    crc_key = struct.pack('<I', crc)
    if verbose:
        print(f'标识 : {device_id}  (CRC32 0x{crc:08X}, K0 0x{k0:04X})')

    raw12 = read_fn(12)
    dec12 = a6b0_full(raw12[:EDPF_ENC_LEN], crc_key, 0)
    if dec12[:4] != b'EDPF':
        sys.exit(f'错误: LBA12 解密后非 EDPF({dec12[:4].hex()}) — device_id 不符或非 cems 盘')
    enc_e = ent(dec12, find_type_entry(dec12, E12, 4), E12)
    enc_start = struct.unpack_from('<Q', enc_e, 0x18)[0]
    enc_size = struct.unpack_from('<Q', enc_e, 0x28)[0]
    if size_gb is not None:
        share = (round(size_gb * 10**9 / SECTOR) // 8) * 8    # GB(10^9), 8 扇对齐
    else:
        share = enc_start - 63
    if 63 + share > enc_start:
        sys.exit(f'错误: Share@63+{share:,} 越过 Encrypt@{enc_start:,}')
    if verbose:
        enc_end = enc_start + enc_size // SECTOR - 1
        print(f'布局 : Share   LBA 63 ~ {63 + share - 1:,}   {fmt_gb(share * SECTOR)}  明文数据区, 系统直接挂载读写')
        print(f'       Encrypt LBA {enc_start:,} ~ {enc_end:,}   {fmt_gb(enc_size)}  原样保留不动')

    new12, plain12 = convert_lba12(raw12, crc_key, share)
    new7, plain7 = convert_lba7(read_fn(7), k0, share)
    new0 = convert_lba0(read_fn(0), share)
    new6, dec6 = convert_lba6(read_fn(6))
    raw9 = read_fn(9)
    new9 = bytes(SECTOR) if any(raw9) else None

    if verbose:
        print()
        print('将写入 5 个扇区')
        print(f'  LBA0   MBR    → 单分区(type=07) 指向 Share: @LBA63 × {share:,} 扇')
        print( '  LBA6   盘标签 → 按免密盘模板改写(0x1CA=128480, 清25B), 重算校验和')
        print( '  LBA7   分区表 → 2 条目: Share@63 + Encrypt')
        print( '  LBA12  分区表 → 2 条目: Share@63 + Encrypt')
        print(f'  LBA9   临时区 → {"清零(当前存在)" if new9 else "已是零, 不写"}')
        print('不改动 : LBA4/8/11(盘身份) · 其余保留扇区 · 表尾终止符 · LBA12 尾部144B · 盘尾区域')

    return dict(lba0=new0, lba6=new6, lba7=new7, lba12=new12, lba9=new9,
                share=share, enc_start=enc_start, enc_size=enc_size,
                k0=k0, crc=crc, plain7=plain7, plain12=plain12, raw12=raw12)

# ══════════════════════════════════════════════════════════════════
# 9. 真盘 IO + 备份/恢复
# ══════════════════════════════════════════════════════════════════
def read_lba_disk(disk, lba):
    fd = os.open(f'/dev/rdisk{disk}', os.O_RDONLY)
    try:
        return os.pread(fd, SECTOR, lba * SECTOR)
    finally:
        os.close(fd)

def write_lba_disk(disk, lba, data):
    if len(data) != SECTOR: sys.exit(f'内部错误: 写入非 512 对齐 ({len(data)})')
    # EBUSY(16) 重试: 写 LBA0 改 MBR 会触发 macOS 重扫/挂载新分区, 短暂独占 raw 设备
    for i in range(15):  # 0.2s × 15 ≈ 3s
        try:
            fd = os.open(f'/dev/rdisk{disk}', os.O_RDWR)
            break
        except OSError as e:
            if e.errno != 16 or i == 14: raise
            time.sleep(0.2)
    try:
        return os.pwrite(fd, data, lba * SECTOR)
    finally:
        os.close(fd)

def _disk_total_sectors(disk):
    """盘总扇区数(diskutil DiskSize/512); 失败返回 'unknown'。"""
    try:
        import plistlib
        info = plistlib.loads(subprocess.check_output(
            ['diskutil', 'info', '-plist', f'disk{disk}'], timeout=10))
        ds = info.get('DiskSize') or info.get('TotalSize') or 0
        if ds:
            return str(ds // SECTOR)
    except Exception:
        pass
    return 'unknown'

def _usb_vid_pid(disk):
    """USB VID/PID(hex4); 失败返回 ('xxxx','xxxx')。"""
    try:
        out = subprocess.check_output(['ioreg', '-r', '-c', 'IOUSBHostDevice', '-l'],
                                      text=True, errors='ignore', timeout=15)
    except Exception:
        return 'xxxx', 'xxxx'
    want = f'"BSD Name" = "disk{disk}"'
    for b in re.split(r'(?=^\s*\+-o IOUSBHostDevice)', out, flags=re.M):
        if want not in b:
            continue
        mv = re.search(r'"idVendor"\s*=\s*(\d+)', b)
        mp = re.search(r'"idProduct"\s*=\s*(\d+)', b)
        if mv and mp:
            return f'{int(mv.group(1)):04x}', f'{int(mp.group(1)):04x}'
    return 'xxxx', 'xxxx'

def _lba4_label_id_from(head):
    """LBA4 开头的 `$$$<labelOnlyId>$$$` → 十进制字符串; 非法返回 None。

    labelOnlyId 在部分盘上以有符号 32 位十进制文本保存；负的 10 位数连同
    分隔符需要 17B，因此不能只截取 16B。
    """
    m = re.match(rb'\$\$\$(-?\d+)\$\$\$', head)
    return m.group(1).decode() if m else None

def _disk_label_id(disk):
    try:
        return _lba4_label_id_from(read_lba_disk(disk, 4)[:32])
    except OSError:
        return None

def _backup_label_id(path):
    """直接从备份快照的 LBA4 读取 labelOnlyId，不依赖当前插入的真盘。"""
    try:
        with open(path, 'rb') as f:
            f.seek(4 * SECTOR)
            return _lba4_label_id_from(f.read(32))
    except OSError:
        return None


def migrate_backup_names(bak_dir):
    """把历史备份文件名统一为 `_onlyid<labelOnlyId>_`，并同步改名 .md5。

    兼容早期 `_lid..._` 命名以及完全没有 onlyid 段的历史备份。onlyid 始终
    从该备份自身的 LBA4 读取，避免依赖当前磁盘或按型号猜测。
    """
    if not os.path.isdir(bak_dir):
        return []
    renamed = []
    for path in glob.glob(os.path.join(bak_dir, '*.bin')):
        name = os.path.basename(path)
        if re.search(r'_onlyid-?\d+_', name):
            continue
        onlyid = _backup_label_id(path)
        if onlyid is None:
            continue
        if re.search(r'_lid-?\d+_', name):
            new_name = re.sub(r'_lid-?\d+_', f'_onlyid{onlyid}_', name, count=1)
        else:
            new_name = re.sub(r'_(\d{8}_\d{6}\.bin)$',
                              f'_onlyid{onlyid}_\\1', name, count=1)
            if new_name == name:
                continue
        new_path = os.path.join(bak_dir, new_name)
        if os.path.exists(new_path):
            print(f'警告: 历史备份改名目标已存在，跳过: {new_path}')
            continue
        try:
            os.rename(path, new_path)
            old_md5, new_md5 = path + '.md5', new_path + '.md5'
            if os.path.exists(old_md5):
                os.rename(old_md5, new_md5)
        except OSError as e:
            print(f'警告: 历史备份无法改名: {path} ({e})')
            continue
        renamed.append((path, new_path))
    return renamed


def backup_disk(disk, device_id, n=14):
    bak_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'backup')
    os.makedirs(bak_dir, exist_ok=True)
    migrate_backup_names(bak_dir)
    data = b''.join(read_lba_disk(disk, l) for l in range(n))
    ts = time.strftime('%Y%m%d_%H%M%S')
    secs = _disk_total_sectors(disk)
    vid, pid = _usb_vid_pid(disk)
    onlyid = _disk_label_id(disk)
    onlyid_part = f'_onlyid{onlyid}' if onlyid else ''
    base = f'disk{disk}_{secs}_vid{vid}_pid{pid}_{device_id}{onlyid_part}_{ts}'
    path = os.path.join(bak_dir, base + '.bin')
    with open(path, 'wb') as f:
        f.write(data)
    with open(path + '.md5', 'w') as f:
        f.write(hashlib.md5(data).hexdigest() + '\n')
    print(f'备份: {path}')
    print(f'还原: sudo python3 {os.path.basename(__file__)} --restore {path}')
    return path

def list_usb_disks():
    """枚举外部 USB 整盘 → [(disk号, 字节数, vid, pid)]; 系统盘(disk<2)不进入候选。"""
    import plistlib
    disks = []
    try:
        all_disks = plistlib.loads(subprocess.check_output(
            ['diskutil', 'list', '-plist'], timeout=10)).get('AllDisks', [])
    except Exception:
        return []
    for name in all_disks:
        m = re.fullmatch(r'disk(\d+)', name)          # 只要整盘, 排除 disk4s1 等分区
        if not m:
            continue
        n = int(m.group(1))
        if n < 2:                                      # 系统盘防护
            continue
        try:
            info = plistlib.loads(subprocess.check_output(
                ['diskutil', 'info', '-plist', name], timeout=10))
        except Exception:
            continue
        if not info.get('WholeDisk') or info.get('Internal') or info.get('BusProtocol') != 'USB':
            continue
        size = info.get('TotalSize') or info.get('DiskSize') or info.get('Size') or 0
        disks.append((n, size, *_usb_vid_pid(n)))
    return disks

def auto_pick_disk():
    """自动选定 USB 盘: 唯一候选直接用, 多个交互选择。返回 disk 号。"""
    disks = list_usb_disks()
    if not disks:
        sys.exit('错误: 未检测到外部 USB 盘。插入后重试, 或 --disk N 手动指定。')
    if len(disks) == 1:
        return disks[0][0]
    print('检测到多个 USB 盘:')
    for i, (n, size, vid, pid) in enumerate(disks, 1):
        print(f'  {i}) disk{n}  {fmt_gb(size)}  {vid}:{pid}')
    while True:
        c = input(f'选择 [1-{len(disks)}] (回车取消): ').strip()
        if not c:
            sys.exit('已取消')
        if c.isdigit() and 1 <= int(c) <= len(disks):
            return disks[int(c) - 1][0]
        print('无效输入')

def find_backups(disk, device_id=None):
    """匹配 backup/ 中本盘备份, 新→旧排序。
    注意: device_id/总扇区/VID/PID 均非盘唯一(同型号盘全同), 最终以
    LBA4 labelOnlyId(每盘随机唯一, 明文) 终验剔除他盘备份。
    兼容旧命名(device_id 中 & 被替换为 _)。"""
    bak_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'backup')
    if not os.path.isdir(bak_dir):
        return []
    migrate_backup_names(bak_dir)
    secs, (vid, pid) = _disk_total_sectors(disk), _usb_vid_pid(disk)
    tiers = []
    if device_id:
        tiers.append([f'disk*_{secs}_vid{vid}_pid{pid}_{device_id}_*.bin',
                      f'disk*_{secs}_vid{vid}_pid{pid}_{device_id.replace("&", "_")}_*.bin'])
    tiers.append([f'disk*_{secs}_vid{vid}_pid{pid}_*.bin'])   # 兜底(identify 失败时)
    for pats in tiers:                                        # 分层: 精确有果则不兜底
        out = []
        for p in pats:
            out += [f for f in glob.glob(os.path.join(bak_dir, p)) if not f.endswith('.md5')]
        if not out:
            continue
        # LBA4 终验: 剔除同型号他盘的备份
        try:
            my_tag = read_lba_disk(disk, 4)[:16]
        except OSError:
            my_tag = None
        if my_tag and any(my_tag):
            def tag_ok(f):
                with open(f, 'rb') as fh:
                    fh.seek(4 * SECTOR)
                    return fh.read(16) == my_tag
            out = [f for f in out if tag_ok(f)]
        return sorted(set(out), key=os.path.getmtime, reverse=True)
    return []

def read_lba_file(path, lba):
    """快照目录读扇区, 兼容 LBA7.bin / LBA07.bin 命名。缺失返回全零扇区。"""
    for name in (f'LBA{lba}.bin', f'LBA{lba:02d}.bin'):
        p = os.path.join(path, name)
        if os.path.exists(p):
            with open(p, 'rb') as f:
                return f.read(SECTOR)
    return bytes(SECTOR)

# ══════════════════════════════════════════════════════════════════
# 10. CLI
# ══════════════════════════════════════════════════════════════════
def main():
    ap = argparse.ArgumentParser(description='cems 加密 U 盘 → 无密码盘(独立版, 零依赖)')
    ap.add_argument('--disk', type=int, help='真盘号(缺省自动检测外部 USB 盘)')
    ap.add_argument('--dir', help='离线快照目录(LBA0-13 bin 文件)')
    ap.add_argument('--id', help='device_id(离线模式必须; 真盘模式自动识别)')
    ap.add_argument('--size', type=float, help='Share 大小 GB(默认占满到 Encrypt 前)')
    ap.add_argument('--out', help='输出改造后扇区目录(离线模式)')
    ap.add_argument('--apply', action='store_true', help='真盘实际写入(默认 dry-run)')
    ap.add_argument('--restore', nargs='?', const='AUTO', metavar='[BIN]',
                    help='从备份还原 LBA0-13(不带值=自动匹配本盘最新备份)')
    args = ap.parse_args()

    if args.disk is None and not args.dir:
        args.disk = auto_pick_disk()
    if args.disk is not None and args.disk < 2:
        sys.exit(f'错误: 拒绝系统盘 disk{args.disk}(须 disk2+)')

    if args.restore:
        path = args.restore
        if path == 'AUTO':
            did, _, _ = identify(args.disk)
            baks = find_backups(args.disk, did)
            if not baks:
                sys.exit('错误: backup/ 未找到本盘备份; 可 --restore <备份.bin> 显式指定')
            print(f'disk{args.disk} 匹配备份 {len(baks)} 个(新→旧):')
            for b in baks:
                mt = time.strftime('%Y-%m-%d %H:%M', time.localtime(os.path.getmtime(b)))
                print(f'  {mt}  {b}')
            print(f'\n还原执行: sudo python3 {os.path.basename(__file__)} --restore "<上面任一路径>"')
            return
        data = open(path, 'rb').read()
        if len(data) != 14 * SECTOR:
            sys.exit(f'错误: 备份大小 {len(data)} ≠ {14*SECTOR}')
        if os.path.exists(path + '.md5'):
            want = open(path + '.md5').read().strip()
            got = hashlib.md5(data).hexdigest()
            if want != got:
                sys.exit(f'错误: 备份 MD5 不符(期望 {want}, 实际 {got}) — 文件损坏?')
            print(f'MD5 校验通过: {got}')
        if not args.apply:
            print(f'[dry-run] 将还原 {path} → disk{args.disk} LBA0-13 ({len(data)}B)。确认后加 --apply。')
            return
        if input(f'还原 {path} → disk{args.disk} LBA0-13? 输入 YES: ').strip() != 'YES':
            sys.exit('已取消')
        subprocess.run(['diskutil', 'unmountDisk', 'force', f'disk{args.disk}'], capture_output=True)
        for lba in range(14):
            write_lba_disk(args.disk, lba, data[lba*SECTOR:(lba+1)*SECTOR])
        print('已还原。请拔出重插。')
        return

    if args.dir:                                    # ── 离线模式 ──
        if not args.id:
            sys.exit('错误: 离线模式需 --id <device_id>')
        result = convert(lambda lba: read_lba_file(args.dir, lba), args.id, args.size)
        if args.out:
            os.makedirs(args.out, exist_ok=True)
            for lba, key in ((0, 'lba0'), (6, 'lba6'), (7, 'lba7'), (12, 'lba12')):
                with open(os.path.join(args.out, f'LBA{lba:02d}.bin'), 'wb') as f:
                    f.write(result[key])
            if result['lba9']:
                with open(os.path.join(args.out, 'LBA09.bin'), 'wb') as f:
                    f.write(result['lba9'])
            print(f'\n产物已写入 {args.out}/')
        return

    # ── 真盘模式 ──
    if args.disk < 2:
        sys.exit(f'错误: 拒绝系统盘 disk{args.disk}(须 disk2+)')
    secs = _disk_total_sectors(args.disk)
    vid, pid = _usb_vid_pid(args.disk)
    sz = fmt_gb(int(secs) * SECTOR) if secs.isdigit() else f'{secs} 扇'
    print(f'盘   : disk{args.disk}  {sz}  USB {vid}:{pid}')
    did, crc, k0 = identify(args.disk)
    if not did:
        sys.exit('错误: 无法识别 device_id(LBA7 两候选均未解出 EDPF); 可插好盘重试')
    result = convert(lambda lba: read_lba_disk(args.disk, lba), did, args.size)

    baks = find_backups(args.disk, did)
    if baks:
        print(f'\n备份 : 本盘已有 {len(baks)} 份(--apply 时会自动再备份):')
        for b in baks:
            t = time.strftime('%Y-%m-%d %H:%M', time.localtime(os.path.getmtime(b)))
            print(f'  {t}  {os.path.basename(b)}')
    else:
        print('\n备份 : 尚无; --apply 时自动创建首个备份')

    if not args.apply:
        print('操作 : 以上为预览(dry-run), 未写盘。执行写入: sudo python3 nopwd.py --apply')
        return
    backup_disk(args.disk, did)
    if input(f'将改写 disk{args.disk} LBA0/6/7/12/9。输入 YES: ').strip() != 'YES':
        sys.exit('已取消(未写盘)')
    subprocess.run(['diskutil', 'unmountDisk', 'force', f'disk{args.disk}'], capture_output=True)
    # LBA0 最后写: 它是唯一改 MBR 的扇区, 写后 macOS 重扫/挂载会短暂锁盘(EBUSY),
    # 放最后则没有后续写会被波及(2026-08-28 实测 LBA9 曾在旧写序下撞 EBUSY)
    writes = [(6, 'lba6'), (7, 'lba7'), (12, 'lba12')]
    if result['lba9'] is not None:
        writes.append((9, 'lba9'))
    writes.append((0, 'lba0'))
    for lba, key in writes:
        write_lba_disk(args.disk, lba, result[key])
    print('已写入。请拔出 U 盘重新插入, 数据区格式化 exFAT/NTFS 即得免密可写区。')


if __name__ == '__main__':
    main()
