"""命令行入口。

用法:
  离线(快照目录, 供验证): python3 -m nopwd --dir <快照目录> --id <device_id> [--out <目录>]
  真盘 dry-run:          sudo python3 -m nopwd [--disk N] [--size GB]  # N 缺省自动检测 USB 盘
  真盘写入:             sudo python3 -m nopwd --apply
  列出本盘备份(不写入): sudo python3 -m nopwd --restore
  还原预检(不写入):     sudo python3 -m nopwd --restore <备份.bin>
  还原写入(须 --apply): sudo python3 -m nopwd --restore <备份.bin> --apply
  (Makefile 提供 make run / make apply / make restore 等快捷目标)

实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
Kingston DT3.0 64G (每盘改前自动备份, 可随时 --restore 还原)。
"""
import os, sys, argparse, hashlib, subprocess, time

from .common import SECTOR, fmt_gb
from .sectors import convert
from .identify import identify
from .diskio import (backup_disk, find_backups, atomic_write_sectors, read_lba_file,
                     read_lba_disk, _disk_total_sectors, _usb_vid_pid, auto_pick_disk)

def main():
    ap = argparse.ArgumentParser(
        prog='nopwd',
        description='cems 加密 U 盘 → 无密码盘(仅 Python3 标准库, 零外部依赖)')
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
            print(f'\n还原执行: python3 -m nopwd --disk {args.disk} --restore "<上面任一路径>" --apply')
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
        atomic_write_sectors(args.disk,
                             {lba: data[lba*SECTOR:(lba+1)*SECTOR] for lba in range(14)})
        print('已还原, 读回校验通过。请拔出重插。')
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
        print('操作 : 以上为预览(dry-run), 未写盘。执行写入: sudo python3 -m nopwd --apply (或 sudo make apply)')
        return
    backup_disk(args.disk, did)
    if input(f'将改写 disk{args.disk} LBA0/6/7/12/9。输入 YES: ').strip() != 'YES':
        sys.exit('已取消(未写盘)')
    subprocess.run(['diskutil', 'unmountDisk', 'force', f'disk{args.disk}'], capture_output=True)
    # 写序由 atomic_write_sectors 保证: LBA0(唯一改 MBR 的扇区)最后写 —
    # 写它才触发 macOS 重扫/挂载; 且单 fd 全程持有, 不再存在中途重开窗口
    writes = {6: result['lba6'], 7: result['lba7'], 12: result['lba12']}
    if result['lba9'] is not None:
        writes[9] = result['lba9']
    writes[0] = result['lba0']
    atomic_write_sectors(args.disk, writes)
    print('已写入, 读回校验通过。请拔出 U 盘重新插入, 数据区格式化 exFAT/NTFS 即得免密可写区。')
