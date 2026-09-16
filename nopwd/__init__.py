"""nopwd — cems 加密 U 盘 → 无密码盘 一键改造工具包。

仅依赖 Python3 标准库。模块分层(无环):
  common   公共常量(SECTOR)与容量显示
  crypto   逆向得到的加密原语(CRC32 bare / AES-128 变体 / 滚动 XOR / LBA6 校验)
  sectors  扇区格式与转换(MBR / SAFE6 / EDPF), convert() 主编排
  identify device_id 识别(ioreg INQUIRY + 传输模式, LBA7 magic 判真)
  diskio   真盘 IO、原子写入(单fd+读回校验+失败回滚)、备份/还原、盘枚举
  cli      命令行入口(python3 -m nopwd)

写入原子性(2026-09-16): 硬件无跨扇区事务, 以"单fd全程持有 + LBA0最后写 +
逐扇读回校验 + 失败自动回滚"逼近全有或全无。
"""

__version__ = '2.0.0'
