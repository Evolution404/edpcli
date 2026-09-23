# Netac EESI 启用状态物理采集来源

产物：`netac_onlydisk_20260804_lba0_12.bin`

- 长度：6656 字节（精确为 LBA0-LBA12）。
- SHA-256：`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`。
- 原始工作站来源：`/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`。
- 原始配套元数据保存为 `netac_onlydisk_20260804_lba0_12.meta.json`；记录 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`、CRC32 `5088ee37`、采集时间 `20260804_080927`、大小 6656、MD5 `db17edf8246ad55e9800b36701afd8e4`。
- 采集工具：`/Users/zhangyuxi/Desktop/u_disk/utils/make_big_boot.py`；来源审计时 SHA-256 为 `d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd`。

采集工具的 `read_lba()` 使用 `O_RDONLY` 打开 `/dev/rdiskN` 并调用 `pread`。`backup()` 会先读取 LBA0-LBA12，再写出 `.bin`、`.md5`、`.meta.json`。在 `--apply` 路径中，`backup()` 发生在第二次 `YES` 确认之前，也发生在 `unmountDisk` 和任何 `write_lba()` / `pwrite` 之前。因此，该文件代表那次运行看到的**写入前真实物理状态**，不是修改代码合成的输出。

对 LBA10，CRC32(device_id)=`0x5088EE37`；解密前 `0x80` 字节后得到 `EESI`、DWORD 标志 `1`、GBK `交换区`、GBK `保密区`，随后是 88 个零字节。本次采集中物理尾部 `+0x80..+0x1FF` 也为全零。

适用范围：该产物是 EESI 启用配置类型的特定用途正向物理参考。它有意不加入“19 份严格加密 + 1 份真实免密”的通用统计集，也不能识别最初制造该磁盘的具体可执行文件。
