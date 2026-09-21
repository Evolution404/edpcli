# EDP Protocol Live Status

> 这是 LBA0-LBA12 逆向的短状态页；完整证据与推理只维护在
> `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`，本页不作为第二份协议总文档。

更新时间：2026-09-21

## 严格进度

- COMPLETE：5458 / 6656 B = 82.0%
- PARTIAL：1198 B
- UNKNOWN：0 B
- 本轮进入时仓库 HEAD：`43c7227c6e9952fe52559c3d963173f0d0279a20`
- 本轮没有增加 COMPLETE 字节；LBA4 `0x020..0x033` 继续保持 PARTIAL。

## 最新结论：LBA4 HSerial / HDSerial

2020 `busManage.dll` 的 `ReadUsbHserialsInfo` 调用 ABI 已闭合到可复核地址：

- `fcn.10010730` 在 `0x100108C3` 调历史 CEMSUsbRegsiter 接口 `vtable+0x2C`；
- v19.11.4.1 `ISUdiskRegsiterObj` vtable=`0x1019DB54`，`+0x2C` 精确对应
  `virtual_44@0x100054A0`；
- 第一个输出是从 LBA4/两份尾部镜像读取并 rolling decode 得到的完整 0x2F-byte
  restore node；HSerialCRC[5] 位于该 node `+0x08..+0x1B`，属于盘面持久化数据；
- reader 成功后才单独调用 `UsbTools` ordinal3=`EDP_DeviceNumber`、
  ordinal4=`EDP_DiskNumber`，将结果写到另外两个 4B 输出；
- 2020 caller 不消费这两个 scalar 输出，只把 0x2F node 传给 `vtable+0x44`；该槽在
  v19.11.4.1 精确对应 `RestoreRegsiterUsb/virtual_68@0x1000E650`，恢复端只取
  `node+0x04 OnllyID2Nd` 作为恢复密钥。

因此已经**直接排除**“DeviceNumber/HDSerialCRC 单 DWORD 就是 LBA4 HSerialCRC[5] 20B”
这一等价关系。但仍不能排除更早 writer 使用主机身份材料经过未知转换生成 5×DWORD；
strict legacy 非零 `request+0x150..+0x160` 的真正 producer 仍缺失，所以不能升级 COMPLETE。

## 当前阻塞点与下一步

本机可见的 `/private/tmp/ijinshan_edp` 三件套与已审组件 SHA-256 完全重复：BusManage
仍是 2020 build，CEMSUsbRegsiter 仍是 v19.11.4.1，RegManage 仍是 v20.1.2.2；Spotlight
也未发现更早的 `busmanage.dll/cemsusbregsiter.dll`。因此 LBA4 当前阻塞在**更早的 caller /
配套组件缺失**：需要找到真正给 legacy request `+0x150..+0x160` 五个 DWORD 赋非零值的
writer 或其上游输入算法。

在没有新的历史组件证据时，下一分析优先级切换为 LBA6/LBA9：继续追
join59 reader 与同代 writer ABI，寻找能够实际生成 Dept[59] 拼接形态的历史 producer。

## 固定门禁

任何新增 COMPLETE 必须同时满足现有 source + consumer + physical gold/model 门禁，并通过
`scripts/protocol/audit_baseline.py`、`protocol_byte_ledger`、
`protocol_documentation_contract`、格式检查与 `git diff --check`。推断、命名相似、相邻字段
相关性或单个反编译变量名都不能增加 COMPLETE 数量。
