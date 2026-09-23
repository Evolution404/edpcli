# DeviceNumber.dll 主机身份算法

状态：**已验证的静态写入依赖**（2026-09-21）。

本文记录历史 `CEMSUsbRegsiter.dll` 路径用于填充 `MyHardinfo` / LBA8 `HDSerialInfo` 的主机身份组件。本文**不**声称该组件会生成五个 `HDOnlySerial[5]` / `HSerialCRC[5]` 值。

## 二进制身份

- 路径：`~/Desktop/u_disk/VRV/cems/ydcc/devicenumber.dll`
- SHA-256：`0ef94c3679da6f27eac75959cf299bbad19676c251d88f7554fbc305407d6041`
- 二进制记录的 PE COFF 时间戳：`2008-12-03 06:51:10`
- 架构：x86 / i386
- 相关导出：
  - `EDP_DeviceNumber @ 0x10011E00`
  - `EDP_LicenseNumber @ 0x10012A90`
  - `EDP_DiskNumber @ 0x10012C90`
- 内嵌采集字符串包括 `\\\\.\\PhysicalDrive%d`、`MACCount`、`MACAddress`。

旧时间戳可以用于来源判断，但不能单独证明某块严格旧版 U 盘在 2008 年制造。下面的证据是恢复出的机器码算法，而不是时间戳。

## CRC 实现

`EDP_DiskNumber@0x10012C90` 会枚举物理磁盘身份材料，再把组合后的字节串压缩为一个 DWORD。

在 `0x10013051` 调用 `fcn.10013170` 初始化 CRC 查找表。表生成器反复右移，并按条件异或 `0xEDB88320`，即反射形式的 IEEE CRC-32 多项式。

在 `0x100130A2` 调用 `fcn.10013210`，参数为：

- 初始 CRC = `0`；
- 组合身份字节的指针；
- 身份字节长度；
- 上述已初始化查找表。

`fcn.10013210` 执行：

```text
crc = ~initial
for each byte:
    crc = (crc >> 8) ^ table[(crc ^ byte) & 0xff]
return ~crc
```

因此，`EDP_DiskNumber` 返回组合主机物理磁盘身份材料的标准反射 IEEE CRC-32。`EDP_DeviceNumber` 的复合主机身份路径也调用相同的 `fcn.10013170 / fcn.10013210` 组合。

## 对协议的影响

历史 `CEMSUsbRegsiter.dll v19.11.4.1` 会先调用配套 `UsbTools.dll` 的磁盘编号入口，失败时回退到设备编号入口。得到的单个 DWORD 同时写入恢复节点的 `MyHardinfo` 家族和 LBA8 `HDSerialInfo` 家族。因此，这些字段更准确的含义是“主机身份 CRC 家族”，而不是早期笼统的“硬件序列号”。

字段级闭环与整代身份来源必须分开：

1. LBA4 `MyHardinfo` 与 LBA8 `HDSerialInfo` 已为完全闭环：v19 独立把同一 `EDP_DiskNumber`/回退 `EDP_DeviceNumber` 结果写入两处；当前版本写零；22/22 严格原始样本中两个 DWORD 互相镜像；同一非零主机值可在不同目标 USB 厂商间重复出现；已审计消费端不按该值分支。
2. LBA8 `UsbOnlyInfo[0..15]` 也已作为可选兼容槽完全闭环：当前版本写 `main-onlyid + 0`，v19 过渡版本写 `main-onlyid + host-hardinfo`，严格旧版物理样本包含缺失全零配置类型，语义读取端忽略该槽。这不意味着严格旧版盘由 v19 制造。
3. LBA4 `HSerialCRC[5]` 已作为调用方负责的五 DWORD 身份向量完全闭环，对应 `UsbLabelParam::HDOnlySerial[5]`。字段边界、当前缺失全零配置类型、v19 请求注入 ABI、官方写入传输、三个恢复节点镜像读取端、忽略值的恢复消费端以及真实非零配置类型均已闭环；v19 内存后端写入器还能在保留真实 SanDisk 非零 20B 向量的同时逐字节复现其 LBA4。`DeviceNumber.dll` 仍只返回一个 DWORD，明确**不是** HSerial 生成算法。更早调用方的值生成算法属于来源研究，不是盘面字段语义缺口。
