"""真盘 IO、原子写入、备份/还原与盘枚举。"""
import os, sys, glob, re, time, errno, hashlib, subprocess

from .common import SECTOR, fmt_gb, restore_cmd
from .sectors import looks_nopwd

def _raw_path(disk):
    return f'/dev/rdisk{disk}'

def read_lba_disk(disk, lba):
    fd = os.open(_raw_path(disk), os.O_RDONLY)
    try:
        return os.pread(fd, SECTOR, lba * SECTOR)
    finally:
        os.close(fd)

# ══════════════════════════════════════════════════════════════════
# 1. 原子写入(全有或全无)
# ══════════════════════════════════════════════════════════════════
def open_rdwr(disk, wait_s=10.0):
    """以 O_RDWR 打开 raw 设备, 成功后由调用方持有到全部写完校验完。
    EBUSY(16) 多发生在写 LBA0 改 MBR 后 macOS 重扫/挂载的瞬间; 单 fd 全程
    持有、不再中途重开后, 该窗口即不复存在(旧版逐扇重开曾实测撞 EBUSY)。"""
    deadline = time.monotonic() + wait_s
    while True:
        try:
            return os.open(_raw_path(disk), os.O_RDWR)
        except OSError as e:
            if e.errno != errno.EBUSY or time.monotonic() >= deadline:
                raise
            time.sleep(0.2)

def pwrite_full(fd, data, offset):
    """写满 data(处理短写; pwrite<=0 视为失败)。旧版不查返回值, 短写会静默丢数据。"""
    mv, off = memoryview(data), offset
    while len(mv):
        n = os.pwrite(fd, mv, off)
        if n <= 0:
            raise OSError(errno.EIO, f'pwrite 未写完(offset {off:#x}, 剩 {len(mv)}B)')
        mv, off = mv[n:], off + n

def _write_and_verify(fd, sectors, order):
    """按 order 逐扇写入 sectors={lba:512B}, 全部写完后逐扇读回比对。"""
    for lba in order:
        pwrite_full(fd, sectors[lba], lba * SECTOR)
    for lba in sorted(sectors):
        if os.pread(fd, SECTOR, lba * SECTOR) != sectors[lba]:
            raise OSError(errno.EIO, f'LBA{lba} 读回校验不符')

def atomic_write_sectors(disk, patch):
    """全有或全无写盘(patch={lba:512B 新内容})。

    USB 盘硬件没有跨扇区事务, 严格原子不可得; 本函数以四层逼近:
      1) 单 fd 打开后全程持有 → 不存在中途重开撞 EBUSY/系统重扫的窗口;
      2) LBA0(唯一改 MBR 的扇区)最后写 → 未到它之前系统视角的 MBR 仍是旧的;
      3) 写完逐扇读回校验, 落盘与否以读回为准;
      4) 任一失败 → 以写前内存镜像自动回滚全部扇区并再校验。
    回滚成功 → 盘仍为写前状态, 可安全重试; 回滚失败 → 明确报告中间态,
    指引重插后用 --restore 从备份文件还原(写前 backup_disk 已落盘一份)。"""
    order = [l for l in sorted(patch) if l != 0] + ([0] if 0 in patch else [])
    fd = open_rdwr(disk)
    try:
        mirror = {l: os.pread(fd, SECTOR, l * SECTOR) for l in patch}
        try:
            _write_and_verify(fd, patch, order)
        except OSError as e:
            print(f'!! 写入失败: {e}', file=sys.stderr)
            print('!! 自动回滚到本次写前状态 ...', file=sys.stderr)
            for i in range(3):
                try:
                    _write_and_verify(fd, mirror, order)
                    break
                except OSError as e2:
                    if i == 2:
                        sys.exit(f'错误: 回滚亦失败({e2}) — 盘处于中间状态! '
                                 f'请重插后立即 python3 -m nopwd --restore 从备份还原。')
                    time.sleep(0.5)
            sys.exit('错误: 已完整回滚, 盘仍为写前状态(未改造)。'
                     '可换 USB 口/线后重试, 或 --restore 走还原流程。')
    finally:
        os.close(fd)

# ══════════════════════════════════════════════════════════════════
# 2. 盘信息
# ══════════════════════════════════════════════════════════════════
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

# ══════════════════════════════════════════════════════════════════
# 3. 备份/还原
# ══════════════════════════════════════════════════════════════════
def backup_dir():
    """备份目录: $NOPWD_BACKUP_DIR 显式优先, 缺省 CWD/backup。
    (旧单文件版取脚本所在目录; 拆包后脚本目录是包目录, 不再适用。
    Makefile 以 NOPWD_BACKUP_DIR 固定为仓库 backup/。)"""
    d = os.environ.get('NOPWD_BACKUP_DIR')
    return d if d else os.path.join(os.getcwd(), 'backup')

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


def backup_is_nopwd(path, device_id):
    """备份文件是否为免密状态快照(按内容检测, 与文件名无关)。"""
    try:
        with open(path, 'rb') as f:
            data = f.read(14 * SECTOR)
        return looks_nopwd(lambda lba: data[lba*SECTOR:(lba+1)*SECTOR], device_id)
    except OSError:
        return False


def backup_disk(disk, device_id, n=14):
    bak_dir = backup_dir()
    os.makedirs(bak_dir, exist_ok=True)
    migrate_backup_names(bak_dir)
    data = b''.join(read_lba_disk(disk, l) for l in range(n))
    ts = time.strftime('%Y%m%d_%H%M%S')
    secs = _disk_total_sectors(disk)
    vid, pid = _usb_vid_pid(disk)
    onlyid = _disk_label_id(disk)
    onlyid_part = f'_onlyid{onlyid}' if onlyid else ''
    # 免密状态快照打 _nopwd 标: 区别于加密原盘备份, 防止还原时拿错
    state_part = '_nopwd' if looks_nopwd(lambda lba: data[lba*SECTOR:(lba+1)*SECTOR],
                                         device_id) else ''
    base = f'disk{disk}_{secs}_vid{vid}_pid{pid}_{device_id}{onlyid_part}{state_part}_{ts}'
    path = os.path.join(bak_dir, base + '.bin')
    with open(path, 'wb') as f:
        f.write(data)
    with open(path + '.md5', 'w') as f:
        f.write(hashlib.md5(data).hexdigest() + '\n')
    print(f'备份: {path}')
    if state_part:
        print('注意: 本份备份为【免密状态】快照 — 还原它不会回到加密原盘。')
    print(f'还原: {restore_cmd(path, disk=disk, apply=True)}')
    return path

def find_backups(disk, device_id=None):
    """匹配 backup/ 中本盘备份, 新→旧排序。
    注意: device_id/总扇区/VID/PID 均非盘唯一(同型号盘全同), 最终以
    LBA4 labelOnlyId(每盘随机唯一, 明文) 终验剔除他盘备份。
    兼容旧命名(device_id 中 & 被替换为 _)。"""
    bak_dir = backup_dir()
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

# ══════════════════════════════════════════════════════════════════
# 4. 盘枚举 + 快照读取
# ══════════════════════════════════════════════════════════════════
def list_external_disks():
    """枚举全部外接整盘(disk≥2) → [(disk号, 字节数, vid, pid, 总线协议)]。
    非USB盘 vid/pid='xxxx'; 系统盘(disk<2)与虚拟盘(DMG等)不进入。"""
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
        if not info.get('WholeDisk') or info.get('Internal'):
            continue
        if info.get('VirtualOrPhysical') == 'Virtual':
            continue                                   # DMG 等虚拟盘, 非物理介质
        proto = info.get('BusProtocol') or '?'
        size = info.get('TotalSize') or info.get('DiskSize') or info.get('Size') or 0
        vid, pid = _usb_vid_pid(n) if proto == 'USB' else ('xxxx', 'xxxx')
        disks.append((n, size, vid, pid, proto))
    return disks

def list_usb_disks():
    """本工具可操作的外接 USB 整盘子集(供 auto_pick_disk)。"""
    return [d for d in list_external_disks() if d[4] == 'USB']

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

def read_lba_file(path, lba):
    """快照目录读扇区, 兼容 LBA7.bin / LBA07.bin 命名。缺失返回全零扇区。"""
    for name in (f'LBA{lba}.bin', f'LBA{lba:02d}.bin'):
        p = os.path.join(path, name)
        if os.path.exists(p):
            with open(p, 'rb') as f:
                return f.read(SECTOR)
    return bytes(SECTOR)
