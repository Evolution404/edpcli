"""扇区格式与转换: 把 3 分区加密盘(Boot/Share/Encrypt)"原盘原地 patch"成
2 分区免密盘。

原理(2026-08-27 定案, 内网实测成功):
    LBA0  : MBR 分区1 → type=0x07 @63 × Share扇数(数据区直挂, 系统原生挂载)
    LBA6  : 0x1CA=128,480 + 0x1D4-0x1ED 清零, 身份字段保留, 重算校验和
    LBA7  : EDPF 2条版本2 [Share@63, type4指针(原盘保留)] + 表尾终止符保留
    LBA12 : EDPF 2条版本2 + 表尾终止符保留 + 尾部144B原盘保留
    LBA9  : 非零则清零(EETU)
    其余扇区(LBA4/8/11 及全零保留区)一律不动
  三条铁律: EDPF 表尾终止符必须保留(LBA7@0xC0/LBA12@0x120);
            LBA12 尾部144B(0x170-0x200)不可清零; 不发明原盘没有的状态。
  分区参数按实际物理盘计算: Encrypt 从原盘 LBA12 type=4 读取, Share 占满其前。
"""
import sys, struct

from .common import SECTOR, fmt_gb
from .crypto import (xor_rolling, a6b0_full, a7f0_full, crc32_bare,
                     lba6_decode, lba6_checksum, LBA6_K0)

EDPF_ENC_LEN = 368          # LBA12 前 368B A6B0 加密, 后 144B 不加密
E7, E12 = 0x40, 0x60        # entry stride: LBA7=64B, LBA12=96B
PWD_CRC = 0x0429735D        # CRC32_bare("0000aaaa") 免密盘默认密码
NOPWD_LBA6_1CA = 128480     # 免密盘 LBA6 0x1CA 模板默认值
LBA6_CLEAR = (0x1D4, 0x1ED) # LBA6 清零区间(含 0x1EC)
PART_TYPES = {1: 'Boot', 2: 'Share', 4: 'Encrypt/IIR指针'}

# ══════════════════════════════════════════════════════════════════
# 1. EDPF entry 工具
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
# 2. 单扇区转换
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
# 3. 已改造(免密)盘检测
# ══════════════════════════════════════════════════════════════════
def looks_nopwd(read_fn, device_id):
    """已是免密盘? LBA6 / MBR / LBA12 三处信号须同时成立(缺一即否):
      LBA6  解密后 0x1CA 已是免密模板值 128480 (辅助信号: 实测 netac/lexar
            原盘本就=128480 无区分度, 仅 aigo 原盘=20417 不同)
      MBR   分区1 = type=0x07 @LBA63 带 55AA (原盘实测为 0x0e)
      LBA12 以 device_id 派生 key 解密后: entry0=Share(type2,@63,active=1,enc=1),
            entry1=Encrypt指针(type4,active=1), entry2 区已清零 (主信号:
            原盘恒为 3 条 EDPF, entry0 enc=0, entry2 type4/active=0;
            注意 aigo 原盘 entry0 也是 type=2@63, 故不能只看 type/start)"""
    dec6 = lba6_decode(read_fn(6))
    if struct.unpack_from('<I', dec6, 0x1CA)[0] != NOPWD_LBA6_1CA:
        return False
    mbr = read_fn(0)
    if not (mbr[0x1BE + 4] == 0x07
            and struct.unpack_from('<I', mbr, 0x1BE + 8)[0] == 63
            and mbr[0x1FE:0x200] == b'\x55\xaa'):
        return False
    crc = crc32_bare(device_id.encode())
    dec12 = a6b0_full(read_fn(12)[:EDPF_ENC_LEN], struct.pack('<I', crc), 0)
    if dec12[:4] != b'EDPF':
        return False
    e0, e1 = dec12[0:E12], dec12[E12:2*E12]
    ok_e0 = (struct.unpack_from('<I', e0, 0x0c)[0] == 2       # type=Share
             and struct.unpack_from('<I', e0, 0x10)[0] == 1   # active
             and struct.unpack_from('<I', e0, 0x14)[0] == 1   # enc 使能
             and struct.unpack_from('<Q', e0, 0x18)[0] == 63)
    ok_e1 = (e1[:4] == b'EDPF'
             and struct.unpack_from('<I', e1, 0x0c)[0] == 4   # Encrypt/IIR 指针
             and struct.unpack_from('<I', e1, 0x10)[0] == 1)  # active
    return ok_e0 and ok_e1 and not any(dec12[2*E12:3*E12])

# ══════════════════════════════════════════════════════════════════
# 4. 主转换
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
