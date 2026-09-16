"""cems 盘内置加密原语(逆向 cemsusbregsiter.dll / sectormanage64.dll 得到)。

  device_id → CRC32(bare: init=0, poly 0xEDB88320, 无final-xor)
  LBA7  K0 = low16(CRC) ^ high16(CRC), 整扇 16位字滚动 XOR(逐字递减1)
  LBA12 key = CRC的4字节×4 ^ "EDPSECDISK200709" → AES-128 变体(A6B0/a7f0,
         counter=块号×16), 仅加密前 368B, 尾部 144B 为原始数据
  LBA6  固定 K0=0x4DAA 滚动 XOR; 0x1FC 校验和 = 密文CRC32(bare)×10轮
         ((v>>15)+(v<<1)) 变换, 小端写入
"""
import struct

LBA6_K0 = 0x4DAA            # LBA6 SAFE6 固定滚动 XOR key(跨盘通用)

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
