# EDP LBA0-LBA12 逆向实时状态

> 用途：这是 **唯一的实时进展页**，用于让后续 AI 先确认“现在做到哪里、哪些结论已验证、哪些改动尚未提交、下一步追什么”。
>
> 详细证据、逐字节定义和最终 COMPLETE/PARTIAL 判定的唯一权威仍是
> `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`。本页不复制完整分析链，避免再次形成多份相互漂移的协议文档。
>
> 更新规则：每出现可复核的实质进展，立即更新本页；形成稳定证据后同步主协议文档和测试门禁，并及时提交。本页之外不要再新建同类 handoff/progress 文档。

## 1. 当前严格进度

截至 2026-09-21，严格口径仍为：

- **COMPLETE：5586 / 6656B = 83.9%**
- **PARTIAL：1070 / 6656B = 16.1%**
- **UNKNOWN：0B**

| LBA | COMPLETE | PARTIAL | UNKNOWN | 当前状态 |
|---:|---:|---:|---:|---|
| LBA0 | 142 | 370 | 0 | 继续追 bootstrap/profile selector |
| LBA1 | 512 | 0 | 0 | COMPLETE |
| LBA2 | 512 | 0 | 0 | COMPLETE |
| LBA3 | 0 | 512 | 0 | 继续追 Phison MP/FW manufacturer sector |
| LBA4 | 487 | 25 | 0 | 剩 HSerialCRC[5] + MyHardinfo + bDataToServer |
| LBA5 | 512 | 0 | 0 | COMPLETE |
| LBA6 | 497 | 15 | 0 | 剩 join59 1B + legacy MBR fragment 14B |
| LBA7 | 512 | 0 | 0 | COMPLETE |
| LBA8 | 492 | 20 | 0 | 剩 legacy HDSerialInfo/UsbOnlyInfo profile closure |
| LBA9 | 384 | 128 | 0 | 剩 legacy join59 Dept continuation writer |
| LBA10 | 512 | 0 | 0 | COMPLETE |
| LBA11 | 512 | 0 | 0 | COMPLETE |
| LBA12 | 512 | 0 | 0 | COMPLETE |

**禁止把相关性、零样本或非 exact-generation writer 直接计入 COMPLETE。**

## 2. 最新已验证进展

### 2.1 LBA4：HSerialCRC[5] 的 legacy ABI 已向上推进一层

历史 `CEMSUsbRegsiter.dll` v19.11.4.1 中，
`ISUdiskRegsiterObj::virtual_8@0x1000B9C0` 会把 legacy request
`+0x150..+0x160` 的 5 个 DWORD 逐项复制进 object HSerial 槽，随后 SAFE6 writer
原样写入 LBA4 `+0x020..+0x033`。

2020 `busManage.dll::WriteNormalULabel` 的实际 caller 会先把 0x184B request 清零，
其转换函数又不写 `+0x150..+0x160`，因此该已取得版本组合仍只能生成 5×0。

**剩余 blocker：找到更早 exact-generation caller，以及非零 5×DWORD 的输入来源/算法。**

### 2.2 LBA4 MyHardinfo：已找到直接历史 producer family，但 exact profile 仍未闭合

2019 `CEMSUsbRegsiter.dll` SAFE6 writer 在 `0x1000D189` 调
`UsbTools.dll` ordinal4=`EDP_DiskNumber`，返回0时 fallback ordinal3=`EDP_DeviceNumber`，
并在 `0x1000D19B` 直接写入 `node+0x1D MyHardinfo`。

同一 DLL 的 LBA8 writer 独立调用同一 ordinal4/3 并写 `HDSerialInfo`，从 producer
机制解释了 strict originals 的 `LBA4.MyHardinfo == LBA8.HDSerialInfo`。

但该 2019 profile 同时会生成非零 `UsbOnlyInfo`，与 strict legacy originals 的零态不完全一致，
因此 MyHardinfo **仍保持 PARTIAL**，等待更早 exact-generation writer/profile selection。

### 2.3 LBA4 historical consumer：HSerial/MyHardinfo 属于 preserve/value-ignore

2020 `BusManageImp::ActiveNormalUDev` 经 `ReadUsbHserialsInfo` 读回完整 0x2F restore node；
2019 三套主/备 reader 都完整复制 node，但值相关校验只检查 `OnlyIdXor8`。

后续 `RestoreRegsiterUsb` 对传入 node 只读取 `node+0x04 OnllyID2Nd` 作为恢复 blob key，
不读取 HSerial/MyHardinfo/server flags。因此这些字段的该历史 activation 链消费语义已闭合为
**structural preserve / value-ignore**。

### 2.4 LBA6：已确认 2019 legacy SAFE6 一体化 writer，并排除它作为非零 entry3 fragment 的 exact producer

`CEMSUsbRegsiter.dll` v19.11.4.1 的 `fcn.10006370` 已确认是旧 SAFE6 的
**构造 + rolling + checksum + LBA6 写盘**一体化 writer：

1. 从 `UsbMainBSec@0x101BA790` 复制完整 512B 到扇区缓冲；
2. 覆盖 SAFE6 字段槽；
3. 对完整扇区执行 rolling；
4. 对前 `0x1FC` 计算 CRC/checksum；
5. 定位 LBA6 并写盘。

其字符串 helper `fcn.101473FE` 已确认是 NUL 截断的 bounded C-string copy，遇到 NUL 即停止，
不会清理目标余下 capacity，因此可以直接解释旧 GSerial/BeiZhu 后出现 MBR template underlay。

但该 2019 内嵌 `UsbMainBSec` 的 `+0x1E0..+0x1EF` 为 16B 全零，且 writer 不覆盖
`+0x1E0..+0x1ED`。所以它只能生成该区零态，**不能**生成 Aigo/SanDisk 两份真实盘上的
非零 legacy MBR entry3 fragment。

结论：那 14B 的 exact producer 必须属于**更早或不同的 legacy profile**，其 LBA6 构造前模板
已经带有真实动态 MBR partition geometry。

### 2.5 LBA6/LBA9 join59：2019 DLL 不是 current long-Dept marker writer

2019 v19.11.4.1 DLL 中没有 `0x40245E2A` long-Dept marker。它不是 current join60
long-Dept builder 的同构版本。strict originals 中 join59 唯一 LBA6 分叉仍是 `+0x03F=0`，
但 exact legacy join59 writer 尚未取得，因此 LBA6 `+0x03F` 和 LBA9 `+0x080..0x0FF`
继续保持 PARTIAL。

### 2.6 新增真实 SanDisk 完整 LBA0-LBA12 只读证据

工作区新增：

- `tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_front_lba0_12.bin`
- `tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_front_lba0_12.provenance.txt`

完整 6656B SHA-256：
`764fc0bbbdf5a9980498d710cb90bce3f8b623f66dec07cb580debc4869538b4`

provenance 明确记录 `os.open(..., O_RDONLY)` + `os.pread(...)`，设备为
`0781:5591 SanDisk Ultra USB 3.0`。其 LBA10 与既有 EESI fixture 逐字节一致。

## 3. 当前工作区状态

创建本页时，分支为：

`feat/provision-new-usb-20260919`

创建本页前基线 HEAD / origin 均为：

`38f41623abff4133e71b5e8728f9bffd4adcc6a3`

除本实时状态页外，以下逆向成果仍处于工作区、**尚未统一提交**：

- `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`
- `tests/protocol_documentation_contract.rs`
- `tests/provision_protocol_audit.rs`
- `tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_front_lba0_12.bin`
- `tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_front_lba0_12.provenance.txt`

最近已验证门禁：

- `cargo test --test provision_protocol_audit`：73/73 PASS
- `cargo test --test protocol_documentation_contract`：4/4 PASS
- `cargo fmt --all -- --check`：PASS
- `git diff --check`：PASS

## 4. 下一步，按优先级执行

1. **LBA6 `+0x1E0..+0x1ED` 14B**：寻找早于/不同于 2019 v19.11.4.1 的 CEMS2.0/legacy 制标 writer，重点查“动态 MBR partition table → SAFE6 template”路径。
2. **LBA6 `+0x03F` / LBA9 join59**：锁定旧版 `0x40245E2A` marker writer 或同代 long-Dept builder，证明为何 inline 第60字节被写成0并从 Dept[59] 接 continuation。
3. **LBA4 HSerialCRC[5]**：从 `request+0x150..+0x160` 继续向更早 caller 追，找到非零 5×DWORD 的 producer/algorithm。
4. **LBA4/LBA8 MyHardinfo/HDSerialInfo**：寻找比 2019 v19.11.4.1 更早且 `UsbOnlyInfo=0` 的 exact-generation writer/profile。
5. 每取得可复核证据：先更新本页，再同步主协议文档与测试；只有满足严格 COMPLETE 门槛才增加字节数。

## 5. 后续 AI 开工前必须做

1. `git status --short --branch`
2. `git fetch --prune origin`
3. 对比 local HEAD 与 `origin/feat/provision-new-usb-20260919`
4. 先读本页，再读 `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md` 对应字段详细证据
5. 不重建已删除的散乱协议分析/handoff 文档
6. 不写真实 raw USB；原盘证据只读
7. 任何 COMPLETE 增量必须同时更新主文档、严格进度表和回归门禁
