"""公共常量与工具。"""
import os

SECTOR = 512

def fmt_gb(num_bytes):
    """容量显示: GB(10^9) 两位小数, 四舍五入(整数运算, 与 macOS 显示一致)。"""
    return f'{(num_bytes + 5 * 10**6) // 10**7 / 100:.2f}GB'

def via_make():
    """是否经 make 运行(以 Makefile 传入的 NOPWD_BACKUP_DIR 为信号)。
    决定给用户的提示命令用 make 等价形式还是 python3 -m 形式。"""
    return bool(os.environ.get('NOPWD_BACKUP_DIR'))

def restore_cmd(path='<上面任一路径>', disk=None, apply=False):
    """还原命令提示, 按运行方式给等价形式。"""
    if via_make():
        parts = ['make', 'restore']
        if disk is not None:
            parts.append(f'DISK={disk}')
        parts.append(f'RESTORE="{path}"')
        if apply:
            parts.append('APPLY=1')
        return 'sudo ' + ' '.join(parts)
    parts = ['python3', '-m', 'nopwd', '--restore', f'"{path}"']
    if disk is not None:
        parts.append(f'--disk {disk}')
    if apply:
        parts.append('--apply')
    return 'sudo ' + ' '.join(parts)

def apply_cmd(disk=None, force=False):
    """写入命令提示, 按运行方式给等价形式(make 下旗标经 FORCE=1/EXTRA 透传)。"""
    if via_make():
        parts = ['sudo', 'make', 'apply']
        if disk is not None:
            parts.append(f'DISK={disk}')
        if force:
            parts.append('FORCE=1')
        return ' '.join(parts)
    parts = ['sudo', 'python3', '-m', 'nopwd', '--apply']
    if disk is not None:
        parts.append(f'--disk {disk}')
    if force:
        parts.append('--force')
    return ' '.join(parts)
