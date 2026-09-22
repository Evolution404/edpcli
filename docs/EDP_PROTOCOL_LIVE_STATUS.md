# EDP Protocol Live Status

> 这是 LBA0-LBA12 逆向的短状态页；完整证据与推理只维护在
> `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`，本页不作为第二份协议总文档。

更新时间：2026-09-21

## 严格进度

- COMPLETE：5457 / 6656 B = 82.0%
- PARTIAL：1199 B
- UNKNOWN：0 B
- 本轮进入时仓库 HEAD：`63b76da1fed351f1e7bbe45f0f12f5c22765e3dc`
- 本轮因真实免密 SanDisk flag 表示证据纠错，LBA4 `+0x046` 从 COMPLETE 降回
  PARTIAL；LBA4 `0x020..0x033` 继续保持 PARTIAL。

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
- 历史写入链已继续闭合：`request+0x150..+0x160 -> object+0x2488..+0x2498 -> node+0x08..+0x1B -> fcn.10006090 -> LBA4`；但五个非零 DWORD 在进入 request 之前的生成算法仍未知。
- `fcn.10008800` 会把 LBA4-LBA12 共9扇区原样备份到 `disk_end-0x80000`；该地址与第三 restore-node reader `fcn.1000DB90` 精确一致。

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

## 最新结论：LBA6/LBA9 join59

- `cems/Edp/fileophook.dll` x86 SHA-256=`db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65`，
  `fileophook64.dll` x64 SHA-256=`93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9`；
  两者均为 2022-12-13 build，版本 `8.1.2211.2811 / 1.0.0.11`，不能冒充2019 writer。
- x86 `fcn.10022F80` 与 x64 `fcn.180026D70` 都固定执行 join59：marker 后先取60B prefix，
  再把 LBA9 continuation 覆盖到 prefix `+0x3B`，没有 current 的 prefix[59] 分支。
- 但两个架构的唯一 live `\\.\\PhysicalDrive%u` raw path——x86 `fcn.10026280` 与
  x64 `fcn.18002B880`——都只以 `GENERIC_READ` 打开盘并调用 `ReadFile`；marker 在两个
  二进制也都只有一个 `cmp` 命中。因此这两份
  CEMS2.0-lineage FileOpHook 是 join59 reader 证据，**不是**缺失的 join59 producer。
- 扩大到 Desktop + `/private/tmp` 的 PE marker 扫描以及 `VRV.zip` 归档检查仍没有得到
  新的 earlier writer；2024 `EdpEDiskCtrl.dll` 新命中仍只是兼容 reader。

所以 LBA6 `+0x03F` 与 LBA9 `+0x080..+0x0FF` 保持 PARTIAL；当前 blocker 已进一步缩成
“取得独立的同代 `safeudisklabeltool/cemsusbregsiter` writer，并直接看到 Dept[59] 被置 NUL、
continuation 从 Dept[59] 开始的 producer/选择条件”。没有该 writer 前不得增加 COMPLETE。

## 最新结论：真实免密 SanDisk 的 LBA4 flag 表示

`tests/protocol_gold_crosscheck.rs` 对当前19份加密金标重算：HSerial=6份零、12份
固定tuple、1份其它非零；Dept=12短/3 join59/4 join60；非零MBR fragment只有1份。
join59样本与非零fragment样本不重合，因此历史writer四类指纹不能作为必须同时命中的
筛选条件；这也不证明它们一定来自不同writer。

真实免密 SanDisk 完整金标 LBA4：main=`794661040`，second=`0x4A32BA39`、HSerial非零，
物理 flags=`00 00`，current official `ReadSector4` rolling-reader view=`D4 D9`；guard、
LLGB、Version=1、sector tuple=`08 04 0C 01` 全部通过。旧 dec 文件的零 flags 来自
逐字节 raw-zero 强制归零的错误 decoder，继续禁止作为独立证据。

本轮新增两条关键闭环：

- v19.11.4.1 `CEMSUsbRegsiter.dll` SHA-256
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814` 的
  `fcn.10006090` 在 rolling 后于 `0x10006249..0x10006257` 同样执行
  `node+0x2D/+0x2E -> physical+0x45/+0x46`。其 SAFE6 node 同时允许从 request/object
  注入 nonzero HSerial，所以 **legacy identity 不能再作为 rolling-wire flag 的判据**；
- `scripts/protocol/probe_lba4_reader.py` 隔离执行 current official
  `ReadSector4@RVA 0x15090`，固定 current DLL hash 与 authentic gold hash，实际执行到
  `0x15295`、返回0、无 emulator exception，复现 wire=`0000` -> reader=`d4d9`。
  onlyid 的 K0=`0xBFED`，两个位置的 rolling key byte 正好为 `D4/D9`。

因此 `inspect` 已删除 `second==main && HSerial==0` 的 representation heuristic：
`decoded` 始终保持官方 reader view，flag 字段同时展示 wire/reader，producer-side 值
必须按 writer provenance 判断。current Windows/Linux 与 v19.11.4.1 SAFE6 writer 中
`bConnetServer` producer-side=0 仍是稳定的**限定结论**；但 authentic no-password
sample 的 exact producer、`disk_end-4 sectors`/`disk_end-0x80000` 同盘镜像和可能的
历史免密/恢复路径尚缺，因此 `+0x046` 的跨 profile dormant-zero COMPLETE 已撤销。

已审 current `RegsiterUsb` 的 LBA4 写入只经过 BuildSector4；`UnRegsiterUsb` 的逐扇写回
不含 LBA4，current repair 路径也未找到只改两个 flag byte 的证据。该假设对现行路径
已排除，但未知 historical converter 仍不能凭空排除。完整证据见主文档第1.3节。

## 固定门禁

任何新增 COMPLETE 必须同时满足现有 source + consumer + physical gold/model 门禁，并通过
`scripts/protocol/audit_baseline.py`、`protocol_byte_ledger`、
`protocol_documentation_contract`、格式检查与 `git diff --check`。推断、命名相似、相邻字段
相关性或单个反编译变量名都不能增加 COMPLETE 数量。
