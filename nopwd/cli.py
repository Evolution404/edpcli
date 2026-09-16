"""命令行入口。

用法:
  列出外接盘(不写入):   python3 -m nopwd --list   # sudo 可多显示 cems 识别/备份
  离线(快照目录, 供验证): python3 -m nopwd --dir <快照目录> --id <device_id> [--out <目录>]
  真盘 dry-run:          sudo python3 -m nopwd [--disk N] [--size GB]  # N 缺省自动检测 USB 盘
  真盘写入:             sudo python3 -m nopwd --apply
  列出本盘备份(不写入): sudo python3 -m nopwd --restore
  还原预检(不写入):     sudo python3 -m nopwd --restore <备份.bin>
  还原写入(须 --apply): sudo python3 -m nopwd --restore <备份.bin> --apply
  (Makefile 提供 make list / run / apply / restore 等快捷目标)

实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
Kingston DT3.0 64G (每盘改前自动备份, 可随时 --restore 还原)。
"""
import os, sys, argparse, hashlib, subprocess, time

from .common import SECTOR, fmt_gb, restore_cmd, apply_cmd, via_make
from .sectors import convert, looks_nopwd
from .identify import identify
from .diskio import (backup_disk, backup_is_nopwd, find_backups, atomic_write_sectors,
                     read_lba_file, read_lba_disk, list_external_disks, _disk_label_id,
                     _disk_total_sectors, _usb_vid_pid, auto_pick_disk)

def scan_disks():
    """外接盘一览数据: 编号/容量/接口; USB 盘再尽力识别 cems 身份与备份份数。
    未 sudo 时 raw 设备无读权限 → denied=True(基本列仍可显示)。"""
    rows = []
    for n, size, vid, pid, proto in list_external_disks():
        row = dict(disk=n, size=size, vid=vid, pid=pid, proto=proto,
                   device_id=None, onlyid=None, n_baks=0, denied=False)
        if proto == 'USB':
            try:
                did, _, _ = identify(n)
                row['device_id'] = did
                row['onlyid'] = _disk_label_id(n)
                if did:
                    row['n_baks'] = len(find_backups(n, did))
            except OSError:
                row['denied'] = True
        rows.append(row)
    return rows

def print_disk_table(rows):
    w = max(len(str(r['disk'])) for r in rows) if rows else 1
    print(f'外接盘 {len(rows)} 个:' if rows else '未检测到外接盘。')
    for r in rows:
        head = f"  disk{r['disk']:<{w}}  {fmt_gb(r['size']):>8}  {r['proto']}"
        if r['proto'] != 'USB':
            print(f'{head}  (非USB, 本工具不支持)')
        elif r['denied']:
            print(f'{head} {r["vid"]}:{r["pid"]}  (加 sudo 可识别 cems 盘/备份)')
        elif not r['device_id']:
            print(f'{head} {r["vid"]}:{r["pid"]}  非cems盘')
        else:
            oid = f'  onlyid={r["onlyid"]}' if r['onlyid'] else ''
            baks = f'  备份{r["n_baks"]}份' if r['n_baks'] else '  无备份'
            print(f'{head} {r["vid"]}:{r["pid"]}  cems盘{oid}{baks}')

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
    ap.add_argument('--force', action='store_true',
                    help='已改造(免密)盘仍强制重写(默认拒绝, 实测重写幂等无害)')
    ap.add_argument('--restore', nargs='?', const='AUTO', metavar='[BIN]',
                    help='从备份还原 LBA0-13(不带值=自动匹配本盘最新备份)')
    ap.add_argument('--list', action='store_true',
                    help='列出外接盘: 编号/容量/接口/cems识别/备份(sudo 更全)')
    args = ap.parse_args()

    if args.list:                                   # 只看盘, 不选盘不写盘
        print_disk_table(scan_disks())
        return

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
                tag = '  [免密状态]' if backup_is_nopwd(b, did) else ''
                print(f'  {mt}{tag}  {b}')
            # 唯一备份直接把路径填进命令, 多个才用占位符
            path_hint = baks[0] if len(baks) == 1 else '<上面任一路径>'
            print(f'\n还原执行: {restore_cmd(path_hint, disk=args.disk, apply=True)}')
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
        try:
            did_now, _, _ = identify(args.disk)
            nopwd_snap = did_now and backup_is_nopwd(path, did_now)
        except OSError:
            nopwd_snap = False
        if not args.apply:
            note = ('\n注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。'
                    if nopwd_snap else '')
            print(f'[dry-run] 将还原 {path} → disk{args.disk} LBA0-13 ({len(data)}B)。确认后加 --apply。{note}')
            return
        if nopwd_snap:
            print('注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。')
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

    already = looks_nopwd(lambda lba: read_lba_disk(args.disk, lba), did)
    if already:
        print('\n提示: 该盘已是改造后的免密盘 — 再次写入只会重写相同内容(实测幂等)。')
    if not args.apply:
        tail = ''
        if already:
            how = 'FORCE=1' if via_make() else '--force'
            tail = f'(该盘已是免密盘, 须加 {how})'
        print(f'操作 : 以上为预览(dry-run), 未写盘。执行写入: {apply_cmd(args.disk)}{tail}')
        return
    if already and not args.force:
        sys.exit('错误: 该盘已是免密盘, 拒绝重复写入(重写内容相同, 实测幂等无害)。'
                 f'确需重写: {apply_cmd(args.disk, force=True)}')
    if already:
        print('--force: 继续重写。本次自动备份将标记为免密状态(文件名含 _nopwd); '
              '加密原盘备份是更早时间戳那份。')

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
