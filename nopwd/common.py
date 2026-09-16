"""公共常量与工具。"""

SECTOR = 512

def fmt_gb(num_bytes):
    """容量显示: GB(10^9) 两位小数, 四舍五入(整数运算, 与 macOS 显示一致)。"""
    return f'{(num_bytes + 5 * 10**6) // 10**7 / 100:.2f}GB'
