# 只读盘面分析真实物理盘 HIL 验收记录（2026-09-23）

## 1. 验收对象

- 分支：`feat/provision-new-usb-20260919`
- 基线提交：`6a85673fe6a35bab12fe41a41b19c8d972231d59`
- edpcli：`2.3.0`
- 平台：macOS
- 设备：`Lexar USB Flash Drive`，VID:PID=`21c4:0cd1`
- 容量：124,736,503,808 B
- 逻辑扇区：243,625,984 * 512 B
- 最后合法 LBA：243,625,983
- 全部验收命令均为只读 `inspect raw/decode/meta`；没有执行格式化、修复或裸盘写入。

## 2. 协议与区域定位

`inspect meta --disk 4 --lba 4,6,7,12` 成功，确认：

- LBA4：`labelOnlyId=3164177653`；
- LBA6：SAFE6 校验通过，`device_id CRC32=0x6BBAEEFB`；
- LBA7：LCE 起始 LBA=243,623,933，长度 6 扇区；
- LBA12 type1：start=63，sectors=20,417，`NeedEncrypt=0`，`EncryptMode=0`；
- LBA12 type2：start=20,480，sectors=231,401,728，`NeedEncrypt=1`，`EncryptMode=2`；
- LBA12 type4：start=231,424,000，sectors=12,177,664，`NeedEncrypt=1`，`EncryptMode=2`。

## 3. LCE

对 LBA 243,623,933 起连续 6 个扇区执行 `raw`、`decode`、`meta`：

- 三种模式均返回 0；
- `meta` 正确识别 `LCE +0/6`；
- 解码方法为 `EDPSECDISK zero8 + 64 位物理字节偏移 tweak`；
- 6 个扇区全部成功解码；
- 第 1 个解码扇区以 `EB 3C 90 4D 53 44 4F 53 35 2E 30` 开头，对应 `MSDOS5.0` FAT 兼容镜像。

## 4. type2

对 LBA 20,480 执行 `raw`、`decode`、`meta`：

- 三种模式均返回 0；
- raw SHA-256：`e02f31b0e9da1630f2e8e71b549cca9315e82004e466e683fddb322718ac859e`；
- `FileKeyCRC=PASS`；
- 物理状态判定为 `SM4 mode2 密文`；
- decode 后 SHA-256：`8430543b721cc35c82fbbc9676c48f8bb6442f2cbea5a188da69023ad6b86ad4`；
- decode 后出现标准 `EXFAT` OEM 标识和末尾 `55 AA`，文件系统识别为 exFAT。

对非起始 LBA 20,481 执行 decode/meta 同样返回 0，证明实现能够先读取分区起始扇区作为物理状态证据，再正确解码后续扇区。

## 5. type4

对 LBA 231,424,000 执行 `raw`、`decode`、`meta`：

- 三种模式均返回 0；
- raw SHA-256：`34cc8ee7c254a021aa9c93a835b3fd10b97979551d86fe82f4b29e6a99f79a47`；
- `FileKeyCRC=PASS`；
- 物理状态判定为 `SM4 mode2 密文`；
- decode 后 SHA-256：`00a307ae11862b343d3449a8e83cb01b69565b14dba496bcb87aadc7e3bff2a8`；
- decode 后出现标准 `EXFAT` OEM 标识和末尾 `55 AA`，文件系统识别为 exFAT。

对非起始 LBA 231,424,001 执行 decode/meta 同样返回 0，证明非起始扇区证据链有效。

## 6. type1：HIL 发现并修复 FAT12 缺口

首次验收 LBA63 时：

- raw 返回 0；
- MBR 明确暴露 `P1 type=0x0E start=63 sectors=20417`；
- boot sector 自身包含 `FAT12`；
- 按 BPB 计算的数据簇数约 2545，符合 FAT12 范围；
- 旧实现只支持 FAT16/FAT32/exFAT/NTFS，因此 `meta` 判 Unknown、`decode` fail-closed。

修复方式：

- 新增 `FilesystemBootKind::Fat12`；
- 对传统 FAT BPB 按簇数量区分：
  - FAT12：cluster_count < 4,085；
  - FAT16：4,085 <= cluster_count < 65,525；
  - FAT32：cluster_count >= 65,525，并继续满足已有 FAT32 结构约束；
- 不放宽现有 boot signature、BPB、分区边界等严格校验。

修复后真实 LBA63：

- `meta` 返回 0；
- 识别为 `物理明文文件系统 (FAT12)`；
- `decode` 返回 0；
- 明确标记“不执行 SM4，decode=raw”；
- decoded SHA-256 与 raw 相同：`18559330303fbe2b6c6efb82da0442b04428181999fae598a599ba2681bda038`。

## 7. 容量边界

- `inspect raw --lba 243625983`：返回 0，最后合法 LBA 可读；
- `inspect raw --lba 243625984`：返回 3，并在读盘前报告越界；
- 错误信息明确给出合法范围 `0..243625983`。

## 8. 结论

真实物理盘 HIL 已覆盖：

- 协议区 LBA4/LBA6/LBA7/LBA12；
- LCE 6 扇区；
- type1 明文 FAT12；
- type2 SM4 mode2 + exFAT；
- type4 SM4 mode2 + exFAT；
- type2/type4 非起始扇区解码；
- 最后合法 LBA；
- 首个越界 LBA 的读前拒绝。

本次 HIL 发现的唯一产品缺口为 FAT12 boot-sector 识别，修复后真实盘复验通过。
