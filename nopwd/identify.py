"""device_id 识别 (macOS ioreg INQUIRY + 传输模式; LBA7 EDPF magic 判真)。"""
import subprocess, re

from .crypto import crc32_bare, xor_rolling
from .diskio import read_lba_disk

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
