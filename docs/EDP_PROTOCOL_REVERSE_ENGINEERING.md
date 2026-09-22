# EDP LBA0–LBA12 协议逆向与验证总文档

> **唯一真相源（Single Source of Truth）**
>
> 本文档汇总 Phase 0 以来 LBA0–LBA12 的当前结论、逐字节账本、producer/consumer、
> 真实盘证据、官方/历史二进制取证、已证伪假设和剩余 blocker。
> `COMPLETE` 只允许由可复核证据升级；`PARTIAL` 表示边界/部分语义已经验证但仍有
> 明确缺口；任何候选解释必须标注为候选或已证伪，不得写成事实。
>
> **当前 EDP 字节语义闭环进度：6513 / 6656B COMPLETE（97.9%），143B PARTIAL（2.1%），UNKNOWN=0。**
> `COMPLETE` 与“每个已知 profile 都有真实物理正例”是两个独立维度；物理 profile 覆盖见 `audit/protocol/profile_coverage.tsv`。
>
> 文末“验证历程附录”用于保留详细推导和纠错记录；若附录中的历史阶段判断与本文前半
> canonical 账本冲突，**一律以前半当前账本为准**。


> 目的：把 LBA0–12 的每一个已识别字段追到“官方生产者 → 盘面字节 → 官方消费者 → 实盘验证”，并以严格完成率衡量逆向进度。
>
> 本文是长期维护的**唯一协议分析主账本**。研究过程、历史误判、专项取证和更长的
> 证据讨论已全部迁入本文第10–12节，不再维护并行分析文档。

## 1. 完成判定：语义闭环与物理 profile 覆盖分层

字段的**语义状态**只有三种：

- **COMPLETE**：字段边界、producer/序列化或明确的 caller-owned 输入边界、consumer/行为语义已经闭合；若字段值本身由协议规定可推导算法，则该算法也必须闭合。真实原盘仍是最高价值的正向证据，但当某个 profile 缺少 real-device capture 时，若 first-party 官方二进制已直接生成该 profile 的 positive wire、consumer 可独立正向消费且算法/边界可重放，该字段可以在**语义维度**保持 COMPLETE，同时必须把该 profile 单独标记为 `MISSING_PHYSICAL`，不得把 virtual output 写成 physical capture。对于明确的 caller-owned identity/material，必须证明 writer 只负责透明接收/序列化、consumer 不依赖某个未证明的隐藏派生关系，此时更上游“业务为何选择这个值”的 provenance 不属于盘面字段语义缺口。存在代际/profile 差异时，差异也必须解释到不会影响字段语义。
- **PARTIAL**：至少一项关键语义证据缺失。例如只有字段名、只有 producer、只有 consumer、只有样本规律、只有解密公式，均只能算 PARTIAL。
- **UNKNOWN**：尚不能稳定划定语义边界，或只知道“当前样本为零/固定值”。

与上述状态独立，`audit/protocol/profile_coverage.tsv` 维护已知 profile 的**物理正例覆盖**：`COVERED` 表示有对应 real-device positive capture；`MISSING_PHYSICAL` 表示当前只有 static/first-party virtual positive evidence。语义 COMPLETE **不等于**所有 profile 已完成物理采样，物理覆盖也不能替代 producer/consumer 语义证明。

以下内容**永远不能单独把字段升级为 COMPLETE**：

1. 金标样本全部相同、全零或固定值；
2. DWARF/变量名看起来合理；
3. 可以正确解密；
4. edpcli Provision 可以生成；
5. 只有 writer 没有 reader；
6. 只有 reader 没有 producer；
7. 免密转换结果、自生成盘或实验盘与预期一致。

### 1.1 实盘证据规则

从 2026-09-21 起，协议分析实际使用的**唯一金标字节集**固定为仓库内
`audit/protocol/gold/`，确保 clean clone/CI/后续 AI 不依赖采集机本地目录即可重放：

- `audit/protocol/gold/strict-encrypted/`：19 份 SHA-256 唯一的 6656B 严格
  original-generation 加密原盘 LBA0-LBA12；
- `audit/protocol/gold/authentic-nopwd/`：1 份 6656B 的 SanDisk Ultra 真实免密盘
  LBA0-LBA12，只读采集，作为免密行为/profile 的唯一金标。

原始采集 provenance 仍分别保留为 `/Users/zhangyuxi/.edpcli-backup` 与
`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4`，但这两个目录已不再
是审计运行依赖。其中 `.edpcli-backup` 的 `_nopwd_` 文件是 edpcli 自制免密盘，只能
用于产品回归，**没有协议参考价值**，不得与真实免密金标混用。
`nopwd_tool/backup`、`utils/backup`、散落的历史快照以及仓库裁剪夹具均不得再作为
金标统计来源。仓库 `tests/fixtures/protocol` 只保留从金标提取的 CI 回归子集；附录中
残留的 22/57/58 份历史 census 仅记录当时研究过程，不能覆盖本节口径，也不能独立
支撑 COMPLETE。canonical 账本中涉及真实盘的结论必须能回到上述 19 份加密原盘或
1 份真实免密盘；否则降为 PARTIAL。

**禁止把免密转换盘、自生成盘、文件名带 `_nopwd` 的夹具、或内容已经呈现自制免密
状态的备份作为“原始 writer 协议”证据。**
**金标按完整 6656B SHA-256 去重；字节完全相同的重复只读采集只保留一份，不得按
独立真实盘重复计权。**

### 1.2 可复现审计基线（2026-09-21）

为防止“文档结论已更新、临时 harness / 样本口径仍停留在旧阶段”，仓库新增
`audit/protocol/` 作为本文的机器可校验伴随账本，而不是第二份真相源：

- `gold_samples.tsv`：冻结当前19份唯一严格加密原盘 + 1份真实免密盘 LBA0-LBA12
  的来源名、仓库相对路径、长度和 SHA-256；`audit/protocol/gold/` 保存对应完整
  6656B 字节，因此基线审计从 clean clone 即可直接重放；
- `evidence_manifest.tsv`：把 physical / virtual / static 三类证据分开记录，固定
  官方二进制版本、SHA-256、关键函数地址、实验边界和不能证明的内容；
- `byte_ledger.tsv`：按 profile 标注并覆盖全部6656B，状态表示 EDP 字节**语义闭环**；
- `profile_coverage.tsv`：把已知 profile 的 real-device positive coverage 与语义状态分开维护，禁止把 official virtual output 冒充 physical capture；
- `historical_matrix.tsv`：集中记录历史 DLL 版本与 join59、HSerialCRC、
  UsbOnlyInfo、动态 MBR 四类指纹的命中/排除结果；
- `scripts/protocol/audit_baseline.py`：取代旧 `/private/tmp/audit22` 的样本 census
  作用，只按本节两类金标重放；旧 harness 的 source SHA-256
  `c9fb7ba50d715e8c0d53611c73076b3c50e7b40a23f05693001d33c17f05abba`
  仅保留为 lineage 记录；
- `tests/protocol_byte_ledger.rs`：自动展开所有 range，拒绝遗漏、重叠、证据 ID
  漂移和 6000/656 统计偏差。

本轮实际重放现行20份 general-census 唯一金标后：LBA10 **20/20 整扇全零**；LBA3 为19份全零 +
1份 strict Kingston 非零 profile，后者 `+0x020..0x027=b57e9c4500800014`、
`+0x1F0..0x1FF="this is mp mark\0"`。旧 `/tmp/audit22` 曾混入的第三来源 SanDisk
EESI 正例仍不得作为 general census 的第21份样本。

但这不再等同于“没有合格的 EESI 正向物理证据”。本轮重新审计
`/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`：完整6656B
SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`，companion
metadata 固定 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`、CRC32=`5088ee37`、
size=6656、MD5=`db17edf8246ad55e9800b36701afd8e4`。产生这组三件套的
`make_big_boot.py`（审计时 SHA-256=`d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd`）
在 `read_lba()` 中以 `O_RDONLY/pread` 读取，`--apply` 主流程先调用 `backup()` 保存
LBA0-LBA12，再进行第二次 `YES` 确认，之后才 unmount 并通过 `O_RDWR/pwrite` 修改
LBA0/LBA12。因此该文件是对应运行的**写前真实物理快照**。仓库现将它完整保存为
`audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin`，登记为
`P-EESI-NETAC`，只作为 EESI-enabled profile 的 purpose-specific physical evidence，不纳入19+1 general census。
对其重放得到 `CRC32(device_id)=0x5088EE37`，LBA10 前0x80解密为 `EESI`、flag=1、
GBK“交换区”、GBK“保密区”、88B零 compatibility extension；它与独立旧 SanDisk
EESI 正例的明文布局一致。结合两套官方 EESI producer/consumer 生命周期，LBA10
`0x000..0x07F` 的 real-device gate 现已满足并升级 COMPLETE。

### 1.3 去重金标交叉复核：指纹不能合并为单一筛选条件

对当前仓库19份 strict-encrypted 金标重新解码 LBA4/LBA6/LBA8，
`tests/protocol_gold_crosscheck.rs` 固定以下 census（不再沿用旧22份计数）：

| 观察项 | 当前19份加密金标 |
|---|---|
| HSerialCRC[5] | 6份全零、12份固定 `1D29/7B/4DD/79/7C`、1份其它非零 |
| Dept | 12份短串、3份 join59、4份 join60 |
| LBA6 `+0x1E0..+0x1ED` | 18份全零、1份非零（Aigo U335 rev_pmap） |
| LBA0 前400B | 8份全零、10份 UsbMainBSec、1份 Netac |
| LBA4 MyHardinfo / LBA8 HDSerialInfo | 19/19 相等；所有样本 LBA4 guard 与 LBA6 checksum 均通过 |

3份 join59 均携带固定 HSerial，且 MBR fragment 为零；唯一非零 fragment 样本
则是短 Dept + 其它非零 HSerial。**这不证明它们必然来自不同 writer**，但表明目前没有
同时展示 join59 与非零 fragment 的金标。因此历史组件矩阵的四类指纹应作为独立
调查入口，不能要求候选同时命中四项才予保留。LBA0 模板同样不是 HSerial 世代的
单值判别器：current-zero HSerial 与 legacy 非零 HSerial 都存在 UsbMainBSec/zero
bootstrap 实例。

历史 writer 的 acquisition target 也已从“未知中间代”收紧到可复核的本机运行基线。`Product_audit` 与 `VUpdateReplace.log` 已证明 2025-05-13 部署了 CEMS base `8.1.2502.2116`，且其更新树包含 `ydcc/cemsusbregsiter.dll(.zip)`；新增审计 `VUpdateService.log`（SHA-256=`5ec3b53e34573646b29dde6cee5595fccb527c385f0b0f571d7b8b74ebf8c517`）进一步证明，在 2026-04-30 升级发生前，更新服务仍将本机 `LocalVersionBase` 报告为 `8.1.2502.2116`，之后才看到 `ServiceVersionBase=8.1.2604.0917`，并对新版 `ydcc/cemsusbregsiter.dll.zip` 逐文件下载、校验 CRC/size。这说明 2025 generation 是升级前实际运行的 CEMSUsbRegsiter 家族候选，而非单纯历史数据库记录。由于旧 DLL bytes/hash 仍未恢复，这条证据只收紧 join59/legacy-MBR producer 的获取目标，不改变 `LBA6+0x03F/+0x1E0..+0x1ED` 与 `LBA9+0x080..+0x0FF` 的 PARTIAL 状态。

#### 真实免密 SanDisk LBA4：wire / reader / producer 三层必须分开

仓库真实免密金标 SHA-256=`d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258`
的 LBA4，已与原始 `raw/LBA04.bin` 及 concat 对应扇区逐字节比对一致：

- main onlyid=`794661040`，rolling 后 `OnlyIdXor8` guard 正确；
- second onlyid=`0x4A32BA39`，HSerial 五 DWORD 非零；
- node 中 `LLGB`、Version=1、sector tuple=`08 04 0C 01` 正确；
- 物理 `+0x45/+0x46=00 00`，完整 rolling-reader 视图却为 **`D4 D9`**；
- backing `+0x47..+0x1FB` 并非整段零，不能套 raw-zero gap 规则。

本轮已把这个分叉进一步闭合到“表示规则已知，并且该实盘的 512B wire image 可由
已取得的官方 historical writer 以 zero server flags 精确重建；物理盘当年究竟由哪个
具体可执行文件生成仍不作无证据归因”：

- current Windows `CEMSUsbRegsiter.dll` SHA-256=
  `122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb` 的
  `BuildSector4/sub_10014550`，以及 Linux `BuildSector4@0x1D08E`，都会先对完整
  `+0x18..+0x1FF` rolling，再把 `node+0x2D/+0x2E` **post-XOR 覆盖**到物理
  `+0x45/+0x46`；
- 更关键的是 historical Windows v19.11.4.1，SHA-256=
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814`，其
  LBA4 encoder `fcn.10006090` 也在 rolling 后于 `0x10006249..0x10006257`
  执行相同两次覆盖。对应 SAFE6 `virtual_56@0x1000CC50` 先清零0x2F node，
  `0x1000D128..0x1000D15E` 又允许把对象中的5个 HSerial DWORD复制到
  `node+0x08..+0x1B`，而两个 flag 仍保持初始化值。这直接否定了
  “second!=main/HSerial!=0 就一定采用普通 rolling flag 表示”的旧分类器；
- current 官方 reader `ReadSector4/sub_10015090` 已由仓库
  `scripts/protocol/probe_lba4_reader.py` 在 Unicorn 隔离内存中直接执行。probe 固定
  上述 current DLL hash 与真实免密金标 hash，实际覆盖函数 RVA
  `0x15090..0x15295`，只 stub allocator/MSVC string/`atoi`/security-cookie 运行库边界，
  无 unmapped/exception，返回0并得到 onlyid=`794661040`、正确 guard、
  second=`0x4A32BA39`、非零 HSerial、`LLGB`、Version=1、tuple=`08040c01`、
  `wire_flags=0000`、`reader_flags=d4d9`；
- 新增 `scripts/protocol/probe_lba4_v19_writer.py` 对 historical v19.11.4.1 writer 做
  **memory-backed 原生执行**：固定 DLL SHA-256 与 authentic gold SHA-256，保留样本
  second=`0x4A32BA39` 与原20B非零 HSerial，仅把 restore node 的两个 server flag
  设为 `00 00`；caller-owned backing 则按 rolling 逆变换回 writer 输入形态。probe
  只 stub PhysicalDrive 路径格式化与 debug 文本格式化两个 CRT 边界，并把
  `CreateFileA/ReadFile/WriteFile` 重定向到内存；`fcn.10006090@RVA 0x06090` 的
  rolling loop、node copy、post-XOR stores 全部执行官方机器码。结果 `ret=1`、无
  unmapped/exception，唯一一次 write 为 `offset=2048,size=512`，输出 SHA-256
  `c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8` 与真实
  SanDisk LBA4 **512/512 完全一致**。这证明 `wire=00 00 / reader=D4 D9` 不需要
  非零 producer flag 才能产生；它不声称 v19.11.4.1 就是当年制造该物理盘的 exact EXE；
- 对 onlyid=`794661040`，rolling K0=`0xBFED`。在两个 flag 物理位置上的 key byte
  分别为 `D4`、`D9`，所以 reader 的 `D4 D9` 精确等于 `00^D4, 00^D9`。
  它是**官方 reader view**，不是已证明的 producer-side flag 值。

因此 LBA4 flag 的真实规则不能再按 HSerial 世代绑定，而必须按 **writer
representation family** 区分：已取得的 current Windows/Linux 和 v19.11.4.1 writer
均属于 post-XOR family；若某盘由这类 writer 生成，则 wire byte 就是 producer node
flag，而官方 reader 会再 XOR 一次。另一些历史加密实盘则观察到 physical 非零而
reader view 为 `00 00`/`0B 00`，与 ordinary rolling representation 相容，但其 exact
writer 仍未取得；在该 family 中 reader view 才对应 node flag。仅凭盘面与身份字段，
目前不能无歧义选择两种 family。

真实免密 SanDisk 的 `00 00 -> D4 D9` 现在不再只是“与 post-XOR family 相容”：上述
v19 virtual execution 已证明，在保留其 non-mirrored second ID 与 nonzero HSerial 的条件下，
**producer node flags=`00 00` 可以由官方 historical writer 精确重建整扇真实 wire image**。
但仍不能声称它一定由 v19.11.4.1 生成：已取得的 paired 2020 BusManage 会把 HSerial
request 区清零，仍缺真正给该盘 nonzero HSerial 的上游 caller；同时也没有该盘
`disk_end-4 sectors` / `disk_end-0x80000` 两份 restore-node 镜像的精确捕获。
2026-08-23 原采集只保存 LBA0-LBA13 与 `size-0xE0000` 等其它尾区，不能拿来替代这两个
镜像地址。current `RegsiterUsb` 的 LBA4 内部写路径只经过 BuildSector4，
`UnRegsiterUsb` 的逐扇写回序列不含 LBA4；current repair 路径也没有找到只 patch
`+0x45/+0x46` 的证据。另取得 historical `UDiskLabelRepair.dll` 2021-12-08 build，
SHA-256=`f5e6ddbb4e3097c9968296b43627543ecacdc24b174e52f8f049b289d7264efc`：
`RepairSafe6Label@0x10008B20` 从 `disk_end-0x80000` **整块读取9扇区并原样写回
LBA4-LBA12**，`RewriteSafe6BakLabel@0x100094E0` 反向整块备份；其独立 LBA4 reader
`fcn.10006DA0` 做完整0xF4 rolling、复制0x2F node、只校验 `OnlyIdXor8`。因此这条
历史修复链同样不会凭空制造/清零单独两个 flag byte。

实现仍然不猜 producer：`inspect` 的 `decoded` 对 LBA4 始终保持官方 rolling-reader
view，两个 flag 字段同时展示 `reader=` 与 `wire=`，不会把 `D9` 强行改成0。字段状态则
按 producer/representation/consumer 生命周期分别记账：`bDataToServer@+0x045` 仍有
真实 `0B` reader profile，保持 PARTIAL；`bConnetServer@+0x046` 的已取得 current
Windows/Linux 与 v19.11.4.1 constructors 都 zero-init 且无赋值，historical rolling-form
实盘的正式 reader view 为0，真实免密 SanDisk 又已由 v19 official writer 以
producer-side zero 精确重建整扇，而所有已审 reader/restore 上层均无该 byte 的值相关
业务分支。因此 `+0x046` 重新闭合为 **producer-side dormant-zero compatibility byte，
COMPLETE**。这里 COMPLETE 不等于“reader 总返回0”，也不宣称已知道 SanDisk 当年的
exact manufacturing executable；它只表示该 byte 的已知 producer 值、两类 wire/reader
表示关系、repair 边界和 negative semantic consumer 已闭合。该阶段当时的严格统计为
6000 COMPLETE / 656 PARTIAL；随后 LBA3 preserve-only 生命周期闭合，当前总计以第3节的
6513 COMPLETE / 143 PARTIAL 为准。

原采集目录的 `dec/LBA04_dec.bin` 虽显示 flags=0，但不能作为独立反证：当时的
`analyze/scripts/read_metadata.py::lba4_decode` 对每一个 raw-zero byte 强制把解码值
归零。同一错误还把物理 `+0x03C=00` 正确解出的 `LLGB` 最后一个 `B` 清成零。
因此本次回归只使用 checked-in raw 金标和正式 rolling 规则，不依赖历史 dec 文件。

另外，金标中的 Netac `onlyid=949028302 @17:24:33` 与 `@17:23:49` 仍只在 LBA7
不同；第6节已将前者 LBA7 认定为局部实验/中间态。SHA-256去重并不消除该证据限制，
后续 manifest 应显式记录 per-LBA 的生成证据排除范围，不能仅靠整镜像
`strict-encrypted` 标签把该 LBA7 当原始生成正例。

## 2. 官方制盘工具链：已经确认

### 2.1 官方 GUI

官方制盘/写标签前端：

`cemssafeudisklabeltool.exe`

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemssafeudisklabeltool.exe.m`

关键证据：

- `sub_42fdb0`，反编译行 **1457–1480**：
  - 加载 `/usbtoolBusManage.dll`；
  - `QLibrary::resolve("CreateBusManageImp")`；
  - 初始化 BusManage 对象。
- Qt 元对象注册：
  - `WriteLabel`：反编译行 **15534–15539**；
  - 写标签页面 `StartSlot()`：反编译行 **13404–13407**；
  - UI 状态文本 `"WriteLabel,Please wait..."`：反编译行 **13396**。
- 服务/业务字段中存在 `labelOnlyId`：反编译行 **7542**。

伪代码：

```text
UsbLabelTool.StartSlot()
    -> show("WriteLabel,Please wait...")
    -> bus = load("usbtoolBusManage.dll").CreateBusManageImp()
    -> bus.WriteNormalULabel(label_request)
```

### 2.2 BusManage 业务层

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/usbtoolbusmanage.dll.m`

`BusManageImp::Init` 的原源码日志位置直接保留在二进制中：

- `busManageImp.cpp:0x214`：加载 `sectorManage.dll`；
- `busManageImp.cpp:0x21A`：加载 `sectorManageHd.dll`；
- `busManageImp.cpp:0x22A`：加载 `SafeUsbRegsiterCems.dll`；
- `busManageImp.cpp:0x23C`：加载 `CEMSUsbRegsiter.dll`；
- 随后 resolve `CreateNormalSectorManageImp`。

`BusManageImp::WriteNormalULabel`：

- 反编译行 **103842–103865**；
- 原源码日志位置 `busManageImp.cpp:0xC4D` 明确写出
  `"UsbToolsRegsiterUsb ERROR_REGSUCESS"`；
- vtable `+0x08` 调用注册；
- vtable `+0x0C` 调用 `ChkRegsiterUsb` 校验。

伪代码：

```text
WriteNormalULabel(request):
    reg = CEMSUsbRegsiter interface
    ret = reg.UsbToolsRegsiterUsb(mapped_request)
    if ret != success:
        return error
    if reg.ChkRegsiterUsb(mapped_request) != valid:
        return verify_error
    return success
```

### 2.3 CEMSUsbRegsiter：真正的 LBA0–12 producer

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll.m`

`CUsbRegsiter::RegsiterUsb`：

- 反编译函数 `sub_1003b560`，起始行 **71006**；
- 原源码日志位置 `usbregsiter.cpp:0x915..0xA04`；
- SAFE6 分支中明确调用：
  - `sub_10014550(..., sector4)` → LBA4；
  - `sub_10013fd0(..., sector6)` → LBA6；
  - `sub_100148d0(..., sector8)` → LBA8；
  - `sub_10014720(..., sector11)` → LBA11；
  - 其它 helper 生成剩余标签扇区；
- 最后 `WriteSectorData(..., count=0x0D)` 一次写 **13 个扇区，即 LBA0–12**；
- 写失败日志原源码位置 `usbregsiter.cpp:0x9B3`。

这条调用链是后续字段 producer 追踪的根。

### 2.4 官方 cross-platform 源码位置索引

Linux `libcemsfilesyscheck.so` 带 DWARF，可恢复原工程文件和行号。后续字段追踪
优先以这些位置作为 producer/reader 的源码锚点，再用 Windows 当前实现和实盘
交叉验证：

| 区域 | Producer | Consumer/reader | 原源码位置 |
|---|---|---|---|
| LBA4 | `CLabelManage::BuildSector4` | `CLabelManage::ReadSector4` | `diskfile.cpp:740 / 956` |
| LBA6 | `CLabelManage::BuildSector6` | `CLabelManage::ReadSector6` | `diskfile.cpp:672 / 1005` |
| LBA7 | `CLabelManage::BuildSector7` | LBA7/EDPF reader链 | `diskfile.cpp:895` |
| LBA8 | `CLabelManage::BuildSector8` | `CLabelManage::ReadSector8` | `diskfile.cpp:805 / 1102,1143` |
| LBA11 | `CLabelManage::BuildSector11` | `CLabelManage::ReadSector11` | `diskfile.cpp:783 / 1168` |
| LBA12 | `CLabelManage::BuildSector12` | `CLabelManage::ReadSector12` | `diskfile.cpp:921 / 1195` |
| LBA11随机源 | `CDataSecrity::RandBuffer256` | 作为 DataEncrypt/Decrypt KDF 输入 | `datasecrity.cpp:14` |
| LBA11加/解密 | `CDataSecrity::DataEncrypt` | `CDataSecrity::DataDecrypt` | `datasecrity.cpp:34 / 61` |
| LBA11历史容量兼容 helper | `CDisk::GetWindowsDiskSizeFromLinux` | 当前 build 无静态 caller；仅作为兼容函数存在 | `DiskInterface.cpp:1196` |

结构定义的主要 DWARF 源位置：

- `tagEdpPartionInfo`：`global/inc/edpdiskglobal.h:76`；
- `tagNewEdpPartionInfo`：`edpdiskglobal.h:101`；
- `tagEdpPartionPassInfo`：`edpdiskglobal.h:151`；
- `UsbLabelParam`：`diskfile.h:125`；
- `UsbWriteParam`：`diskfile.h:142`。

以上“源码位置”来自 DWARF，本仓库没有复制这些第三方源码；文档只记录定位信息、
反编译证据和伪代码。

### 2.5 LBA6 producer/reader 伪代码：为什么不能按固定字符串槽粗暴判完成

Linux producer：

`CLabelManage::BuildSector6(UsbWriteParam&, char*) @ diskfile.cpp:672`

机器码已恢复出的关键行为：

```text
BuildSector6(p, out):
    out = UsbMainBSec template

    if strlen(p.department) <= 63:
        memcpy(out+0x000, p.department_buffer, 0x40)
    else:
        out+0x000 = overflow_marker(0x40245E2A) + first_60_bytes
        write remaining bytes into extension area

    if strlen(p.owner) <= 31:
        memcpy(out+0x050, p.owner_buffer, 0x20)
    else:
        out+0x050 = overflow_marker(0x40245E2A) + first_28_bytes
        write remaining bytes into extension area

    memcpy(out+0x070, p.autoid, 0x10)
    memcpy(out+0x080, p.office, 0x40)
    ...

    tmp16 = zero[16]
    memcpy(tmp16, p.m_usbGSerial, 15)
    memcpy(out+0x1C0, tmp16, 16)

    tmp16 = zero[16]
    memcpy(tmp16, p.BeiZhu, 15)
    memcpy(out+0x1D0, tmp16, 16)

    u32(out+0x1F0) = p.m_encrypt

    rolling_xor(out[0x000..0x1FB])
    checksum = rol(CRC32(out[0x000..0x1FB]), 10)
    u32(out+0x1FC) = checksum
```

Consumer：

`CLabelManage::ReadSector6(char*, UsbLabelParam&) @ diskfile.cpp:1005`

reader 会识别 `0x40245E2A` 溢出 marker，并从扩展位置重建长 Dept/User；
GSerial/BeiZhu 则直接按 C 字符串读回。注意 `UsbLabelParam` **没有**
`m_encrypt` 成员；`m_encrypt` 只存在于 writer 的 `UsbWriteParam+0x258`。

因此：

- GSerial / BeiZhu 的 **C 字符串语义**可以闭合，但固定 16B 物理槽不能整体计
  COMPLETE：writer 固定复制输入对象的前 15B 再补第 16B NUL，consumer 只按
  C 字符串读取；输入对象在 NUL 后的 backing bytes 并没有稳定生成语义；
- Dept/User 虽字段含义明确，但“本槽 + overflow extension”必须作为一个整体继续追，
  不能仅看到 `0x000..0x03F` 或 `0x050..0x06F` 就把整槽算 COMPLETE；
- **LBA6 m_encrypt current producer !SAFE gate** 已继续追到 `RegsiterUsb` 的
  current Windows 赋值点，而不再只停留在 `BuildSector6` 的落盘动作：
  `sub_100139f0(this+0x2E0 -> temp)` 构造临时 `UsbWriteParam` 后，临时对象基址
  精确为 `ebp-0x3F4`；DWARF 已知 `m_encrypt@UsbWriteParam+0x258`，因此对应
  `ebp-0x19C`。原始 PE 机器码在 `RegsiterUsb@0x1003BA94` 的相等分支把该字节
  写为 `1`，`0x1003BAC7` 的非相等分支写为 `0`；前面的5字节比较目标
  `0x100C9A90` 在 PE 中为 ASCII `!SAFE`。随后 `0x1003BAE2..0x1003BAF5`
  立即把同一临时对象传给 `BuildSector6/sub_10013FD0`，后者再把该 BYTE 扩成
  DWORD 写到 `sector6+0x1F0`。Linux `BuildSector6@diskfile.cpp:672` 独立给出
  同一落盘映射。也就是说 current producer 的 1/0 来源已经闭合，不再是
  “22/22 恰好为1”的样本推断。继续核对读取侧后，`UsbLabelParam` 正式结构
  没有 `m_encrypt` 成员；Windows `CheckLabel/sub_100152A0` 在校验前508B
  checksum 后会显式解析 Dept/User/GSerial/Label 等字段，却不读取
  `+0x1F0`；Linux `ReadSector6` 同样不返回该字段，已扫 runtime 也无值相关
  consumer。因此这4B可闭合为 **write-only `!SAFE` label-generation metadata**：
  writer记录当时的 `!SAFE` 匹配结果，物理字节参与整扇checksum，但不是读取侧控制量。

22份原始盘进一步给出了不能把两个 16B 字符串槽整体标 COMPLETE 的直接反例：

- GSerial：16/22 的 C 字符串为 `"322CA28A"`，6/22 为
  `"322CA28A-D7D144"`；前一组 **16/16 都在 NUL 后仍有非零字节**；
- BeiZhu：20/22 为空、2/22 为 GBK `"普通"`；总计 **8/22 在首个 NUL 后仍有
  非零 backing bytes**；
- 两份旧 profile（Aigo U335、SanDisk）还同时在 `+0x1E0..0x1EF`
  留有非零材料。本轮已证明它不是独立“扩展字段”，而是旧版 MBR
  partition-table underlay 的幸存片段：`+0x1DE..0x1ED` 原本是第3条
  16B MBR entry，BeiZhu 覆盖其前2B 后，`+0x1E0..0x1ED` 仍保留
  CHS/type/start/count；`+0x1EE..0x1EF` 已进入第4条 entry。

**LBA6 C-string slots have profile-dependent post-NUL backing bytes**。因此本轮主动回撤此前对
`+0x1C0..0x1DF` 的过度 COMPLETE 认定。这里是
“字符串含义已知 + 固定槽尾跨 writer profile 未闭合”的 PARTIAL，而不是
32B 完整字段。current writer 的槽尾来自输入对象 backing bytes；两份 legacy
profile 则能看到被短 C 字符串局部覆盖后的 MBR 几何残留。

#### m_autoid / Autonum：固定 16B 槽已闭合，包括非语义 post-NUL backing

producer：

```text
UsbWriteParam::UsbWriteParam(UsbLabelParam&):
    strcpy_s(writer.m_autoid /* +0x259 */, 16,
             label.m_autoid  /* +0x258 */)

BuildSector6:
    memcpy(sector6 + 0x70, writer.m_autoid, 16)
```

consumer：

```text
ReadSector6:
    strcpy_s(label.m_autoid /* +0x258 */, 16,
             decoded_sector6 + 0x70)

BuildSector8(label):
    ELABEL += "Autonum=" + label.m_autoid + "||"
```

22 份原始实盘只读交叉：

- **22/22**：LBA6 `0x70` 起的 C 字符串 == LBA8 `Autonum=`；
- 分布：`YD000001` 14份、空串6份、`1` 2份；
- 但固定 16B 槽在第一个 NUL 之后经常保留非零字节，例如
  `YD000001\0\0 73 05 A0 B6 07 EB`；
- 后续进一步反汇编 `strcpy_s@0x1B9B0` 证明它遇到 NUL 后立即停止，
  **不会清 destination 剩余 capacity**；同时
  `UsbWriteParam(UsbLabelParam&)@0x1C362` 入口没有整体 memset，
  随后 `BuildSector6` 又固定 `memcpy 16B`，因此 NUL 后内容的 producer
  已闭合为 **writer-uninitialized backing**；
- committed originals 还保留了更强反例：同一个空 autoid 字符串至少存在
  2种不同且非零的 post-NUL backing，证明这些字节不是第二个隐藏字段。

`ReadSector6` 只解释首个 NUL 前字符串，整扇 checksum 又保护完整16B物理值。
因此这里现在**增加16B COMPLETE**；COMPLETE 表示“每个字节的存储/消费行为已知”，
并不表示 post-NUL 字节具有固定值。Provision 不需要模拟未初始化内存泄漏。

### LBA4 `+0x45/+0x46`：writer representation 与 reader view 分叉

Linux DWARF 给出 restore node 的正式字段名：

```text
tagEdpPartionRestorInfoNode @ edpdiskglobal.h:220, sizeof=0x2F
  +0x2D BYTE bDataToServer
  +0x2E BYTE bConnetServer
```

已取得的三套 writer 证据证明 **post-XOR 覆盖并非 current identity 专属规则**：

- Windows `cemsusbregsiter.dll::sub_10014550`：先把 0x2F node 复制到
  LBA4 `+0x18`，对 `+0x18..+0x1FF` 执行完整 0xF4-word rolling XOR，随后
  明确执行 `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 完成同一 0xF4-word rolling loop 后，
  `0x1D302..0x1D329` 再把 `pSerinfo+0x2D/+0x2E` 原样写回
  `buffer+0x45/+0x46`；
- historical Windows v19.11.4.1（SHA-256
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814`）的
  LBA4 encoder `fcn.10006090` 先做完整 rolling，随后在
  `0x10006249..0x10006257` 从 `node+0x2D/+0x2E` 覆盖到物理 flag bytes。
  同版本 SAFE6 `virtual_56@0x1000CC50` 的 0x2F node 先整体 zero-init，随后
  `0x1000D128..0x1000D15E` 可把对象5个 HSerial DWORD复制到 node；两个 flags
  没有后续赋值。因此即使 second/HSerial 呈 legacy identity，也可以走 post-XOR
  representation。旧 inspect 用 identity shape 判断 wire rule 的前提至此被静态证伪。

reader 则存在一个必须明确记录的非对称行为：

- Windows `sub_10015090` 对 `+0x18..+0x1FF` 统一 rolling 解码后，直接复制
  `decoded+0x18` 的 0x2F node，只校验 `OnlyIdXor8`；没有把物理
  `+0x45/+0x46` 恢复回来；
- Linux `CLabelManage::ReadSector4@0x1E048, diskfile.cpp:956` 同样在
  `0x1E18C..0x1E1D8` 完整 rolling，再 `memcpy(decoded+0x18, 0x2F)`，随后仅
  比较 `OnlyIdXor8 == onlyid ^ 0x88888888`；也没有 post-decode 修正。

因此必须按 **writer representation family** 区分三层事实：

1. **wire bytes**：真实盘物理 `raw[0x45]/raw[0x46]`，只说明盘面是什么；
2. **official ReadSector4 transformed bytes**：reader 对物理字节统一执行 rolling，
   本身不会补偿 post-XOR writer 的覆盖；
3. **producer-side node flags**：只有 writer provenance 已知时才能判定。对 current
   Windows/Linux 与 v19.11.4.1 post-XOR family，producer node byte == wire byte；
   对历史 ordinary-rolling-compatible 实盘，若其缺失 writer 确实按普通 rolling
   生成，则 producer node byte == reader view。identity shape 本身不能选择二者。

旧阶段严格22份 original-generation reference set 的复算仍作为历史观测保留：

- **22/22** 的 physical bytes 与 generic rolling-decoded bytes 不相等；
- **6/22 current-identity**：同时满足
  `OnllyID2Nd == main onlyid && HSerialCRC[5] == 0`；physical=`00 00`，
  generic reader 输出为6组不同非零值。该组与 post-XOR writer family 一致；
- **14/22 legacy-identity**：`OnllyID2Nd != main onlyid && HSerialCRC[5] != 0`，
  physical 为非零字节，generic reader 输出=`00 00`；
- **2/22 legacy-identity 特殊 profile**（Aigo U335 rev_pmap + 独立 SanDisk）：同属
  legacy identity profile，physical 非零，generic reader 输出=`0B 00`。

新增反例进一步证明 **raw-zero/full-rolling 物理表示与 current/legacy identity 不是同一个维度**：
严格原始 Kingston `2026-08-27 17:30:24` 的 restore node 已是 current identity
（`OnllyID2Nd==main onlyid && HSerialCRC[5]==0`），但 `+0x47..0x1FB` 仍为
physical raw-zero；同盘 `17:28:57` 的14扇区只读快照与其逐字节完全一致，排除两次采集
之间临时改写。CI 夹具
`kingston_20260827_current_identity_raw_zero_lba4.bin`（LBA4 单扇区，SHA-256
`85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec`）与
`lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile` 锁定该反例。
因此后续追 raw-zero 历史 producer 时，**禁止**再用 OnllyID2Nd/HSerialCRC 代际形态作为
short/full 表示的选择条件；真实选择条件仍未定位。

新增真实免密 SanDisk 又给出决定性反例：second=`0x4A32BA39`、HSerial非零，
但 physical flags=`00 00`，current 官方 reader 输出=`D4 D9`。所以此前两个极端模型
以及 identity classifier 都不成立：既不能把 generic rolling 结果对所有盘都当
producer flags，也不能把 physical bytes 对所有盘都当 producer flags，更不能用
second/HSerial 决定取哪一层。进一步的 v19 writer probe 已保留同一 second/HSerial，
以 node flags=`00 00` 原生执行 `fcn.10006090`，并把该真实 LBA4 **512/512 精确重建**；
所以 `D4 D9` 已被正向证明可以只是 post-XOR writer 遇到非对称 reader 后的 transformed
bytes，而不是必须存在的 nonzero producer flags。`src/inspect.rs` 仍保持 reader-faithful：

```text
reader = rolling_decode(raw[0x18..])
if historical raw-zero gap:
    reader[0x47..0x1FB] = 0
decoded = reader
flag display = { reader byte, physical wire byte, producer=requires writer provenance }
```

这使 `decoded` 与官方 ReadSector4 行为保持一致，不再让 inspect 替用户猜 producer。
current/v19 的 post-XOR producer 语义仍保留在审计证据与 Provision writer 中，而不是
偷偷改写 reader view。当前已审 Windows/Linux 上层：

- Windows `ReadRestorInfo/sub_10041290` 只负责读取/重试，不修正这2B；
- `ActiveNormalUDev -> sub_1003CEB0` 不读取 `+0x2D/+0x2E`；
- `GetUpLoadInformation/sub_10039C30` 会把 restore node 暴露给上层，但本 DLL
  内只用身份材料，不读取两个 server flag；
- Linux `libcemsfilesyscheck.so` 除 BuildSector4 的两次 post-XOR store 外，没有
  对 restore-node `+0x2D/+0x2E` 的直接字段访问。
- historical `UDiskLabelRepair.dll` 2021 build 的 `RepairSafe6Label@0x10008B20` /
  `RewriteSafe6BakLabel@0x100094E0` 只在 LBA4-LBA12 与 `disk_end-0x80000` 间整块复制
  9扇区 raw bytes；独立 reader `fcn.10006DA0` 也只是 rolling + 完整 node copy +
  `OnlyIdXor8` guard，没有单字节 flag patch 或值相关分支。

这段历史阶段判断已被后续证据取代：`+0x45` 现已由 v19 official writer 的 caller-owned 注入实验、strict Aigo rolling decode=`0B` 与跨代 negative consumer 闭合为 **COMPLETE**；`+0x46` 则由 current Windows/Linux + v19.11.4.1 direct producer zero、
historical rolling-form reader-zero、真实免密 physical wire=`00`、v19 exact-512 virtual
writer reconstruction、跨代 reader/repair negative consumer 一起闭合为
**COMPLETE**。其中 `+0x46` 的字段语义是 producer-side dormant-zero compatibility
byte；官方 reader 对 post-XOR wire 可返回非零 transformed byte（真实样本即 `D9`），
inspect 因而绝不能为了 COMPLETE 状态把 reader view 清成0。物理盘当年的 exact EXE
provenance 仍未知，但这不再构成该1B生命周期的缺口；它继续构成 HSerial upstream 等
其它字段的 provenance 缺口。

同时，current SAFE6 Provision 已从历史 raw-zero short representation 改为官方
current writer 的 full representation：完整 `+0x18..+0x1FF` rolling，然后再
post-XOR 写回 `+0x45/+0x46`。历史 raw-zero 实盘仅作为兼容读取 profile 保留。

### 已验证的远端证据同步（2026-09-21）

本节只收录已经在独立二进制、真实盘或现有回归中核验过、但此前分散在交接/专项
笔记中的证据；它们不会因为“合并文档”而自动增加 COMPLETE 字节。

- **LBA0 / Netac 格式化链**：current 主制标链已静态闭合到
  `BusManageImp::WriteLabelImp -> CCEMSSafeUsbRegsiter::UsbFormat`，但该
  `UsbFormat` 只执行 sectorManage / IIR / password 兼容预处理，并不调用
  Netac MBR format。另一条独立、已验证的兼容层调用链为
  `usb20dll.dll!_IF_DiskFormat -> NewUsb20.dll!FormatExA_NetacAPI`。
  当前安装包仍未找到两条链的连接点，因此 historical formatter selector 继续作为**调用链 provenance 开放问题**保留，禁止因函数名同含 “Format” 就强行拼接。它不再是 LBA0 byte-semantic blocker：20份 general census 的 wire 状态已经被 explicit-zero、UsbMainBSec 与 Netac 三个 first-party producer profile 穷尽。
- **LBA3 / Phison 制造链**：离线静态结果除
  `CBaseController::WriteF2Mark` 外，还确认
  `CU32SSBaseContoller::WriteF2Mark` 与字符串 `F1-F2 MARK`；不同 controller
  class 的 F2 staging 长度/包装不同，因此不能把同名 `WriteF2Mark` 当成统一
  512B LBA3 wire writer。对 strict Kingston `0951:1666`、62008590336B 的公开
  交叉记录还满足“**公开同 identity/capacity 记录同时存在 PS2307 与 PS2309**”，
  所以 controller 型号不能仅凭 VID/PID/model/capacity 锁死。
- **LBA4 / current node layout**：已有 **LBA4 current writer machine-code node layout**
  证据，current `RegsiterUsb` 逐字段把对象成员写入 0x2F restore node；
  本地新增的历史 v19.11.4.1 writer 又进一步闭合 `OnllyID2Nd` 的独立
  `CoCreateGuid -> CRC32_bare` legacy producer。
- **LBA6 / legacy underlay**：`+0x1E0..+0x1ED` 已按标准 MBR entry 对齐为
  **legacy MBR partition-table fragment**，Aigo/SanDisk 两块真实盘均能与同盘
  LBA12 type4 的 type/start/count 交叉核对；当前仍缺 exact legacy writer 与直接
  consumer，因此保持 PARTIAL。

## 3. 严格逐字节进度

> 每个 LBA 固定 512B；总计 13 × 512 = 6656B。
>
> 本表的完成率只统计**语义 COMPLETE**，不把 PARTIAL 计入完成；它不是 physical-profile coverage 百分比。

<!-- STRICT_PROGRESS_BEGIN -->
| LBA | COMPLETE | PARTIAL | UNKNOWN | 严格完成率 |
|---:|---:|---:|---:|---:|
| LBA0 | 512 | 0 | 0 | 100.0% |
| LBA1 | 512 | 0 | 0 | 100.0% |
| LBA2 | 512 | 0 | 0 | 100.0% |
| LBA3 | 512 | 0 | 0 | 100.0% |
| LBA4 | 512 | 0 | 0 | 100.0% |
| LBA5 | 512 | 0 | 0 | 100.0% |
| LBA6 | 497 | 15 | 0 | 97.1% |
| LBA7 | 512 | 0 | 0 | 100.0% |
| LBA8 | 512 | 0 | 0 | 100.0% |
| LBA9 | 384 | 128 | 0 | 75.0% |
| LBA10 | 512 | 0 | 0 | 100.0% |
| LBA11 | 512 | 0 | 0 | 100.0% |
| LBA12 | 512 | 0 | 0 | 100.0% |
<!-- STRICT_PROGRESS_END -->

当前语义总计：

- **COMPLETE：6513B / 6656B = 97.9%**
- **PARTIAL：143B / 6656B = 2.1%**
- **UNKNOWN：0B / 6656B = 0.0%**

物理 profile 正例覆盖不再混入上述百分比。当前 `profile_coverage.tsv` 明确记录：GPT enabled profile 以及 LBA12 mode1/mode3 尚无 EDP real-device positive capture；对应语义由 first-party runtime positive wire + consumer/算法重放闭合。

LBA11 已完整闭合为 512B COMPLETE。此前卡住的后半 252B 不是“某型号盘偶尔使用
CHS”的未知 profile，而是来自另一条官方 writer/reader 路径：正常注册 writer 使用
`DISK_GEOMETRY_EX.DiskSize`；`UDiskLabelRepair` 的 LBA11 检查与重写使用传统
`DISK_GEOMETRY` 计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector`
作为 `ullSize`。同一 Aigo U335 `rev_pmap` 已同时保留 CHS 与 exact DiskSize 两种真实
捕获，分别匹配这两条官方路径。

### 3.1 Linux DWARF 原源码索引

`libcemsfilesyscheck.so` 带有可用 DWARF。以下位置不是按函数名猜测，而是
DWARF 中恢复出的原始声明文件/行号与本地函数地址：

| 功能 | 本地地址 | 原始源码位置 |
|---|---:|---|
| `CLabelManage::BuildSector6` | `0x1CAAC` | `/mnt/git/cross_platform/src/global/src/diskfile.cpp:672` |
| `CLabelManage::BuildSector4` | `0x1D08E` | `diskfile.cpp:740` |
| `CLabelManage::BuildSector11` | `0x1D350` | `diskfile.cpp:783` |
| `CLabelManage::BuildSector8` | `0x1D602` | `diskfile.cpp:805` |
| `CLabelManage::BuildSector7` | `0x1DCDA` | `diskfile.cpp:895` |
| `CLabelManage::BuildSector12` | `0x1DEB8` | `diskfile.cpp:921` |
| `CLabelManage::ReadSector4` | `0x1E048` | `diskfile.cpp:956` |
| `CLabelManage::ReadSector6` | `0x1E2CC` | `diskfile.cpp:1005` |
| `CLabelManage::ReadSector8(UsbLabelParam&)` | `0x1E994` | `diskfile.cpp:1102` |
| `CLabelManage::ReadSector8(BYTE*)` | `0x1F0F0` | `diskfile.cpp:1143` |
| `CLabelManage::ReadSector11` | `0x1F220` | `diskfile.cpp:1168` |
| `CLabelManage::ReadSector12` | `0x1F426` | `diskfile.cpp:1195` |
| `CLabelManage` 构造器 | `0x1C538` | `diskfile.cpp:576` |
| `CDataSecrity::RandBuffer256` | `0x20204` | `/mnt/git/cross_platform/src/global/src/datasecrity.cpp:14` |
| `CDataSecrity::DataEncrypt` | `0x202D4` | `datasecrity.cpp:34` |
| `CDataSecrity::DataDecrypt` | `0x20476` | `datasecrity.cpp:61` |

这些路径是二进制编译时记录的原始源码位置；本机没有对应原厂源码正文。
审计把它们作为函数来源定位证据，不把 DWARF 行号误写成“已取得源码”。

## 4. 字段证据账本

表中 COMPLETE 行必须同时有 producer、consumer、实盘验证。CI 会解析本表，缺任一列即失败。

<!-- FIELD_LEDGER_BEGIN -->
| LBA | 范围 | 状态 | 字段/区域 | Producer 证据 | Consumer 证据 | 实盘验证 | 当前结论 |
|---|---|---|---|---|---|---|---|
| LBA0 | 0x000–0x17A excluding 0x0E1/0x0E8/0x101/0x103/0x10B/0x10D/0x124/0x143/0x162 | COMPLETE | profile-level MBR bootstrap blob / absent-zero profile | current `CUsbRegsiter::RegsiterUsb` 在最终13扇区 `WriteSectorData` 前无条件 `memset(LBA0+0x000,0,0x190)`，形成 explicit-zero absent profile；同一 first-party binary 的 `UsbMainBSec@0x100E7220` 提供完整 legacy bootstrap，且 `UnRegsiterUsb` 有模板直接写回 LBA0 的路径；Netac profile 由 `Netac_USB_API.dll` 1.3.1.16（SHA-256 `b12a249a...`）`sub_10003880` 从 `0x1014BA58` 以 `rep movsd, ECX=0x80` 精确复制512B模板再重建分区项。两份静态 producer 的前400B已提交为 clean-clone fixtures `official_usb_main_bsec_lba0_prefix.hex` / `official_netac_mbr_lba0_prefix.hex`；Netac静态证据登记为 `S-NETAC-MBR`，两份 prefix SHA-256分别为 `4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed` / `00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec` | 两种非零状态都是完整、可自洽的16-bit MBR bootstrap payload：legacy `UsbMainBSec` 的指令/消息链和 Netac 模板的独立 bootstrap 已静态核对；current zero profile表示该 bootstrap absent，EDP 对前400B只按可清除/可替换 opaque bootstrap 处理，不存在另一个值相关 EDP consumer | 现行20份 general census 被严格穷尽为 **8×zero + 11×UsbMainBSec + 1×Netac**，不存在第四种前400B wire 状态；真实免密 SanDisk仍属于 UsbMainBSec。两个非零物理 prefix 均与对应 first-party static template 逐字节相等 | 字段语义按 profile-level blob 闭合：每一种已观测 wire 状态都有 producer、consumer/absent 行为和 physical evidence。historical 上游 formatter selector 仍是部署/调用链 provenance 的开放问题，但它只选择三个已知 producer state 之一，不再构成这370B的字节语义缺口；未来第四种 profile 必须重新开账 |
| LBA0 | 0x0E1/0x0E8/0x101/0x103/0x10B/0x10D/0x124 | COMPLETE | seven profile-invariant zero instruction-operand bytes | current SAFE6 explicitly clears all seven; legacy `UsbMainBSec` fixes them as zero operands in executable instructions: three `mov dl,[bp+0]` displacements (+0x0E1/+0x0E8/+0x124), three `push 0` immediates (+0x101/+0x103/+0x10B), and the low byte of `push 0x7C00` (+0x10D); Aigo/Netac template is zero padding at all seven offsets | legacy 16-bit bootstrap executes the corresponding instructions, so each zero participates in a decoded operand; Netac bootstrap's code/messages end before these offsets and has no references into them; current EDP clears/does not parse the bootstrap | committed current-zero + legacy fixtures and dedicated Aigo/Netac prefix evidence are all zero at all seven exact offsets; regression locks the instruction byte windows | profile selection changes surrounding code but cannot change these seven physical bytes; their per-profile producer/consumer behavior is closed |
| LBA0 | 0x143/0x162 | COMPLETE | first/second legacy MBR error-message NUL terminators / other-profile zero padding | current SAFE6 clears both; legacy `UsbMainBSec` stores `Invalid partition table` at +0x12C..+0x142 then NUL@+0x143, and `Error loading operating system` at +0x144..+0x161 then NUL@+0x162; Aigo/Netac template is zero padding at both offsets | legacy print loop consumes the NULs as C-string terminators through the existing message-pointer path; Netac uses earlier message copies and does not reference these offsets; current EDP has no bootstrap consumer after clearing | all committed current-zero/legacy fixtures plus dedicated Aigo/Netac prefix evidence keep both bytes zero; regression checks the exact strings and terminators | 2B are profile-invariant physical zeros with fully explained legacy string semantics and padding semantics in the other profiles |
| LBA0 | 0x17B | COMPLETE | legacy third MBR error-message NUL terminator / other-profile zero padding | current SAFE6 writer explicitly clears through +0x18F; legacy `UsbMainBSec` fixes `Missing operating system` at +0x163..+0x17A followed by NUL at +0x17B; Aigo/Netac embedded MBR template is already zero throughout this offset | legacy relocated bootstrap's print loop terminates on the NUL after the third error string; Netac bootstrap uses its three messages at +0x08B/+0x0A3/+0x0C2 (last NUL at +0x0DA), so +0x17B is not read there; current EDP has no bootstrap consumer after clearing | committed current-zero + legacy fixtures and dedicated Aigo L8302 Netac prefix evidence are all zero at +0x17B | profile selection cannot change this byte: it is the legacy message terminator and a zero-padding byte in the other known producer families |
| LBA0 | 0x17C–0x18F | COMPLETE | cross-profile fixed-zero bootstrap tail padding | current SAFE6 writer explicitly clears this20B; legacy `UsbMainBSec` has zero padding immediately after the third message terminator; Aigo/Netac embedded template also contains zeros here | legacy 16-bit bootstrap has no data/code reference into +0x17C..+0x18F after its final message; Netac bootstrap relocates its code but its last message ends at +0x0DA and no code/data reference targets this tail; current EDP does not parse it | committed current-zero + legacy fixtures plus `aigo_l8302_netac_lba0_prefix.hex` all preserve 20B zero; three known physical profiles agree byte-for-byte | producer ownership, negative consumer/reference boundary, and all known real-device profiles close 20B as fixed-zero bootstrap padding; future unknown profiles must still be treated compatibly rather than blindly cleaned |
| LBA0 | 0x190–0x19F | COMPLETE | cross-profile unowned preserve / historical-zero compatibility region | current SAFE6 `RegsiterUsb` 最终只清 `+0x000..+0x18F`，因此本16B保持 pre-read backing；Linux `BuildSector0@diskfile.cpp:625` 只重建 `+0x1BE` MBR entry，不写本区；legacy `UsbMainBSec` 本16B固定为零；Netac `sub_10003880` 整扇复制的 Aigo producer 模板在本16B同样为零 | 16-bit `UsbMainBSec` bootstrap 的直接数据引用落在 `+0x1B5/+0x1B6/+0x1B7` 和分区表/签名，不读取本区；current 注册/准入与 `UDiskLabelRepair::ReCreate0Sector/sub_10003960` 均不解释本16B，repair 新建只清前0x190和分区表，保持本区 unowned | 严格22份原始参考本16B 22/22 全零；扩展 `nopwd_tool/backup + utils/backup` 共57份完整历史快照也 57/57 全零；既有 `lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix` 门禁锁定 committed originals | COMPLETE 表示跨已知 profile 的**无业务 payload / preserve-existing**生命周期闭合，而不是规定未来盘面必须为零；遇到未知非零值应兼容保留 |
| LBA0 | 0x1A0–0x1A3 | COMPLETE | optional SAFE1 / legacy `SectorSize` compatibility overlay | Windows `BuildSector0/sub_10013F10@0x10013FB6` 在 **SAFE1** 分支明确把 `m_nSectorSize` 写到 `+0x1A0`；`UsbMainBSec` 完整模板同样携带512。Aigo/Netac `sub_10003880` 的整扇 MBR template 在该槽显式为0；current SAFE6 不调用 `BuildSector0` 而 preserve 现有 backing，Linux `BuildSector0@diskfile.cpp:625` 只用 `m_nSectorSize` 计算 partition sector count、不序列化该槽 | legacy 16-bit bootstrap 不读取该槽；current `CEMSUsbRegsiter.dll` 全模块对 `+0x1A0` 的唯一 sector-data 访问就是上述 writer store，另一个 `push 0x1A0` 只是临时 `memset` 长度；`UDiskLabelRepair` 没有盘面 `+0x1A0` 读点，Linux 没有 `ReadSector0` consumer；`EdpEDiskCtrl` 的 `+0x1A0` 命中已逐项确认是 vtable/object 偏移而非 LBA0 数据 | 严格22份原始盘仅0/512双态；committed fixtures 同时保留0和512。扩展只读历史扫描中，47份相同 legacy bootstrap 快照分为10×0、37×512；把 SectorSize、disk signature、partition table 三个独立变量归一化后47/47整个LBA0 SHA-256均为 `2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f`，证明该DWORD只是独立可选 overlay | 512表示 SAFE1/legacy writer 填充的 sector-size compatibility metadata；0表示该 overlay absent/unowned，后续 SAFE6 只透明保留。producer/preserve、negative consumer、0/512 双真实 profile 与物理独立性均闭合，4B 升 COMPLETE；不要求未来未知值被清洗 |
| LBA0 | 0x1A4–0x1B4 | COMPLETE | cross-profile unowned preserve / historical-zero compatibility region | 与 `+0x190..+0x19F` 相同：current SAFE6 不覆盖、Linux BuildSector0 不写；legacy `UsbMainBSec` 与 Aigo/Netac整扇模板均在本17B生成零 | 16-bit MBR bootstrap 不读取本区；current EDP 准入、partition repair 与 `ReCreate0Sector` 都只处理其它明确区域，不赋予本17B语义 | 严格22份 22/22 全零；扩展57份完整历史快照 57/57 全零；committed original 门禁已有精确零断言 | 17B 的跨 profile unowned/preserve、negative consumer 与真实盘已闭合；未来非零兼容值必须 preserve，不得机械清零 |
| LBA0 | 0x1B5–0x1B7 | COMPLETE | **LBA0 legacy MBR message-pointer bytes** | 官方 `UsbMainBSec@0x100E7220` 固定为 `2C 44 63`；`sub_10013FD0`/旧模板写路径整扇复制该模板 | 模板先把 `+0x1B..` 搬到 `0x061B` 后执行；runtime `mov al,[0x07B5/0x07B6/0x07B7]` 分别组成 `SI=0x072C/0x0744/0x0763`，指向原模板 `+0x12C/+0x144/+0x163` 三条错误消息 | 22盘严格统计：14/22=`2C 44 63`，8/22=`00 00 00`，无第三种值；CI夹具同时保留两种 profile | 三字节是 legacy MBR 错误消息指针低字节；零态表示该 legacy tail 未存在/已清空，不再当“未知随机尾巴” |
| LBA0 | 0x1B8–0x1BB | COMPLETE | standard Windows MBR disk signature | `CreateDiskMbr` 取 `GetSystemTimePreciseAsFileTime`（fallback `GetSystemTimeAsFileTime`）→ FILETIME 转 Unix seconds → 低32位填 `CREATE_DISK_MBR.Signature` → `IOCTL_DISK_CREATE_DISK`；Windows `DRIVE_LAYOUT_INFORMATION_MBR.Signature` 正式定义该 DWORD 为唯一标识 MBR disk 的 drive signature | Windows `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 将该值作为 `DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 返回。当前 `CEMSUsbRegsiter.dll::fcn.10046320` 的唯一 `0x70050` 调用已逐指令复核：返回 buffer 只比较 `+0x04 PartitionCount==1` 与 `+0x00 PartitionStyle==MBR`，不读取 `+0x08 Mbr.Signature`，证明 EDP 对该标准字段没有附加业务语义 | 22/22非零，19个值；同一 onlyid 的重复备份保持不变，按LE解释与历史初始化日期吻合；curated protocol fixtures 又锁定 signature 非零且至少存在两个不同真实值 | producer、Windows 标准 consumer/字段语义、EDP negative-semantic-consumer 与真实盘变化均闭合；4B 升 COMPLETE，不能把“EDP 不读取”误当成字段语义未知 |
| LBA0 | 0x1BC–0x1BD | COMPLETE | standard MBR reserved / unowned compatibility word | current SAFE6 `RegsiterUsb` 只清 `+0x000..+0x18F` 并在 `+0x1BE` 起重建分区表，因此这2B保持 pre-read backing；Linux `BuildSector0@diskfile.cpp:625` 同样不写本槽；legacy `UsbMainBSec` 与 Aigo/Netac 整扇模板都在此显式携带 `00 00` | 16-bit `UsbMainBSec` bootstrap 的已确认尾部引用不包含 `+0x1BC/+0x1BD`；current 注册/准入、`UDiskLabelRepair::ReCreate0Sector` 与已审 `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 路径均不把这2B作为业务字段读取，分区语义从 `+0x1BE` 开始 | 严格22份原始参考 22/22=`00 00`；扩展 `nopwd_tool/backup + utils/backup` 的57份完整历史快照同样 57/57=`00 00`，且相邻 disk signature 明确多值，排除“整段尾部碰巧固定”的误判 | 2B 的跨已知 profile producer/preserve 生命周期、negative semantic consumer 与实盘均闭合；COMPLETE 表示该 reserved word 当前无业务 payload，未来未知非零值应兼容 preserve，不得机械清零 |
| LBA0 | 0x1BE–0x1FD | COMPLETE | 4×MBR partition entry | `UsbMainBSec` 模板；SAPF 恢复项也直接写回此处 | `UDiskLabelRepair.dll::Repair0Sector` 直接恢复该 64B 区域 | 22/22 可按标准 MBR 解码 | 分区表边界和消费闭合 |
| LBA0 | 0x1FE–0x1FF | COMPLETE | MBR 55AA | 官方模板直接写 `55 AA` | MBR 校验/修复链检查签名 | 22/22 | 完成 |
| LBA1 | 0x000–0x1FF | COMPLETE | optional GPT primary header / absent-profile sector | Linux官方 `CLabelManage::BuildSector1_Gpt@0x1FDAA` 原生构造完整512B `GPT_Header`：模板先完整覆盖512B，动态写 backup/last-usable/disk-GUID/table-CRC/header-CRC；`+0x5C..+0x1FF` 由模板明确为零 | Windows current `IsAllowRegisterCommonLabel/sub_1002AB70` 在 protective MBR 命中后，以 sector_size 跳到 LBA1 并检查 `EFI PART` 与 `header_lba==1`；本轮把 official Linux builder 输出直接喂给该 Windows consumer，原生返回 `2=GPT` | 22/22 physical originals 与扩展3896候选均为 absent-GPT 全零 profile；另新增 **official-binary virtual writer** 正向fixture：2GiB/512B配置下 `EFI PART`, version=0x10000, header size=92, backup=4194303, first/last usable=34/4194270, table LBA=2, 128×128B；独立 IEEE CRC32 同时命中 header CRC `0xA4B46C72` 与16KiB array CRC `0xD32CFEA7`；CI锁定 | 两种生命周期均闭合：非GPT时 protective-MBR gate 不进入LBA1语义，当前样本为零；GPT时 first-party producer完整拥有512B并被独立 Windows first-party consumer正向识别。虚拟fixture不冒充physical capture |
| LBA2 | 0x000–0x07F | COMPLETE | GPT partition entry0 | Linux官方 `BuildSector2_Gpt@0x1FFF6` 每次完整写一个128B `GPT_Partition`：Basic Data type GUID固定；partition GUID为caller输入；start固定63；end=`63+caller_size`；attr/name来自零模板 | Windows `sub_1002B2F0` 与 Linux `AnalyzeGptPartitionTable` 均按128B stride比较 type GUID，命中后读取 `start@+0x20/end@+0x28/attr@+0x30`；GPT header又规定 entry-size=128 | official-binary virtual writer 以 GUID `001122...eeff`、2GiB容量直接生成 entry0，得到 start=63/end=4194270/attr=0/name全零；同一输出参与 LBA1 partition-array CRC 并被 Windows GPT header consumer接受 | 128B writer→wire→consumer字段闭合；partition GUID 16B 为显式caller-owned身份材料，name/attr为模板零。该正例与physical census分层 |
| LBA2 | 0x080–0x08F / 0x100–0x10F / 0x180–0x18F | COMPLETE | GPT entries1..3 的 unused `PartitionTypeGUID` | current Windows `WriteNormalULabel` 大盘分支调用 GPT creator `sub_10037160(..., partition_count=1)`；该函数向 `IOCTL_DISK_CREATE_DISK` 传 `PartitionStyle=GPT(1)`，并向 `IOCTL_DISK_SET_DRIVE_LAYOUT_EX` 提交 `DRIVE_LAYOUT_INFORMATION_EX.PartitionCount=1`。因此 entry0 后三个槽在 current 一分区 profile 中均为 unused GPT entry；UEFI 2.10 §5.3.3 定义 unused entry 的 `PartitionTypeGUID=00000000-0000-0000-0000-000000000000` | Windows/Linux GPT parser 都以16B type GUID 判定 entry 是否有效；type GUID 为0时该 entry 不进入 start/end/attr 语义解析 | official Linux virtual GPT fixture 的 entries1..3 三个 type GUID 均为0；另对本机20,538个候选文件只读扫描得到1份真实 GPT image（Ubuntu 26.04 ISO），其3个已用 entry 后至少125个 unused entry 的 type GUID/完整entry均为0。physical EDP originals仍为 absent-GPT 全零 profile | 这里只升级每条 unused entry 的16B type discriminator。其余112B仍不借助 harness 预清或通用规范推断；三条共48B从PARTIAL升COMPLETE |
| LBA2 | 0x090–0x0FF / 0x110–0x17F / 0x190–0x1FF | COMPLETE | **GPT entries1..3 unused-entry unowned residual** | current Windows GPT creator明确只提交 `PartitionCount=1`，因此 entries1..3 的 `PartitionTypeGUID` 为零时整条 entry 已处于 unused 状态；这336B不是 EDP 自定义 payload，Windows kernel 是否把 residual 具体初始化为零不影响其协议语义。Linux `BuildSector2_Gpt` 只负责 active entry，同样不赋予 unused residual 独立字段语义 | current Windows `CPartitionType::AnalyzeGptPartitionTable/sub_1002B2F0` 的机器码先比较每条 entry 的16B TypeGUID；仅在匹配受支持非零 GUID 后才读取 `+0x20/+0x28/+0x30` 等 residual 字段。Linux `AnalyzeGptPartitionTable@0xFB36` 独立同构：`memcmp(type_guid, entry,16)` 命中后才读 start/end/attr。隔离 Unicorn 进一步让 Windows official constructor 原生建立 supported-GUID map（仅映射 SEH 零页并 stub `HeapAlloc/HeapFree` CRT 边界），再喂4条 `TypeGUID=0 + residual=0xA5` 的 entry；parser `ret=0`、无异常，memory-read hook 对336B residual **0次读取**，四条 entry 均在 TypeGUID 起始比较即短路 | Linux first-party virtual GPT fixture与独立 Ubuntu GPT实盘中的 unused residual均为零；新增 Windows first-party consumer probe又证明任意非零 residual 不进入语义消费。这里不把 `0xA5` probe冒充 writer output，而是用于证明“unused后 residual 值无业务意义” | 按与 LBA4/LBA5 unowned backing 一致的 COMPLETE 口径闭合：决定 entry 是否存在的是16B TypeGUID；为零后其余112B/entry属于 unowned residual，兼容读取不得赋予隐藏语义或强制依赖零值。至此 LBA2 512/512 COMPLETE |
| LBA3 | 0x000–0x1FF | COMPLETE | **manufacturer-owned opaque MP metadata / EDP preserve-only sector；不得与 MPALL F2 INFO page 直接等同** | current Windows `CUsbRegsiter::RegsiterUsb` 先读完整13扇区，SAFE6注册链没有 LBA3 builder，最终把同一 staging image 整段写回，因此 LBA3 的 EDP producer 语义是 preserve-existing；Linux 独立不存在 `BuildSector3`。historical v19.11.4.1 又由 `scripts/protocol/audit_v19_lba3_preserve.py` 固定 SHA-256 后枚举全 DLL 31 个 `SetFilePointer`，可恢复的固定 sector-size 倍率精确为 `{1,2,4,6,7,8,12}`、无3，SAFE6 `virtual_56@0x1000CC50` 也不直接调用 generic absolute-seek wrapper | current Windows/Linux 注册、登录路径没有 LBA3 payload parser；2021 `UDiskLabelRepair.dll` 的 `RepairSafe6Label/RewriteSafe6BakLabel` 只在 LBA4-LBA12 与尾部镜像之间整块复制9扇区，天然绕过 LBA3。Phison `WriteF2Mark/GetInfo` 分析继续作为边界反证：F2 INFO 不是 host LBA3，不能把厂商内部 bit 强行赋予 EDP 语义 | 22份原始参考21/22全零、1份 strict Kingston 非零；扩展历史另有第二种不同非零 MP profile。两种非零 profile 均保留 `+0x001=01` 与 `+0x1F0..1FF="this is mp mark\0"`，但 `+0x020..027` 不同，证明 EDP 必须透明保留而不能零填或套固定模板 | **EDP协议层生命周期闭合**：这512B属于外部制造商拥有的 opaque metadata，EDP 的逐字节规则是 preserve/ignore。exact Phison host serializer、controller/firmware 私有字段含义继续作为 manufacturer provenance 研究问题，不再构成 EDP LBA0-LBA12 字节语义 blocker；未来任意未知非零 LBA3 都必须原样保留 |
| LBA4 | 0x000–0x017 | COMPLETE | `$$$onlyid$$$` clear header | 当前注册 writer 根据 main onlyid 格式化 | 识别/解码链从此恢复 onlyid | 22/22 | 完成 |
| LBA4 | 0x018–0x01B | COMPLETE | OnlyIdXor8 | current writer: `main_onlyid ^ 0x88888888` | restore-info 读取该字段 | 22盘 current/legacy 可解 | 完成 |
| LBA4 | 0x01C–0x01F | COMPLETE | `OnllyID2Nd` = backup/activation encryption key seed | **current producer**：Windows `RegsiterUsb@0x1003BBD1..0x1003BBD7` 直接执行 `node+0x04 = object+0x698`，即复用本次注册 main onlyid。**legacy producer**现已从官方归档 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`）恢复：SAFE6 `virtual_56@0x1000CC50` 清零0x2F restore node 后调用 `fcn.100058E0`；该函数 `CoCreateGuid()` 生成16B GUID，初始化协议 CRC table，以初值0逐字节计算同款 reflected `CRC32_bare`，返回DWORD；`0x1000D122` 将结果精确写入 `node+0x04`。因此早期 profile 是“**独立 GUID-CRC key**”，current profile 改为“**复用 main GUID-CRC onlyid**” | active consumer 已闭合为完整 round-trip：`GetUpLoadInformation/sub_10039C30` 以 `restore_node+0x04` 为4B seed 加密 LLGB+EDPF backup blob；`ActiveNormalUDev -> sub_1003CEB0` 取同一 seed 解密 activation/restore blob，校验 `LLGB` 后恢复 LBA8/LBA12。两 helper 的16B key material 均为 `key4[i mod 4] XOR "EDPSECDISK200709"[i]`，随后走同一 key schedule、互为 encrypt/decrypt block transform | strict current profile `OnllyID2Nd==main onlyid && HSerialCRC=0`；legacy profile `OnllyID2Nd!=main`。可复核 legacy second key 不等于本设备/全集 main-onlyid；committed NETAC_A/NETAC_B/LEXAR 精确锁定 `44D9CE02/028EFFD3/7647B1EF`，并新增门禁证明它们也不退化为 `CRC32(device_id)`、MBR disk signature 或 `MyHardinfo`；同一 main 的重复捕获 second key 保持稳定，符合“制标时生成后持久化”的随机 key 生命周期 | 4B 的正式边界、current/legacy 双 producer、随机生成算法、双向密码学 consumer 和真实 profile 均闭合；不同代际只改变 seed 来源，不改变 backup/activation key 语义，升级 COMPLETE |
| LBA4 | 0x020–0x033 | COMPLETE | caller-owned `HSerialCRC[5]` / `HDOnlySerial[5]` identity vector | current Windows PE `RegsiterUsb@0x1003BBF0..0x1003BC79` 精确把 `object+0x558/+55C/+560/+564/+568` 写入 `node+0x08..+0x1B`；`this+0x2E0` 已闭合为内嵌 `UsbLabelParam`，故这些地址正是 `HDOnlySerial[5]@+0x278`。current Windows 两层参数构造以及 Linux caller均不提供这20B，所以 current profile为 zero/absent。historical v19.11.4.1 `ISUdiskRegsiterObj::virtual_8@0x1000B9C0` 则把 legacy ABI `request+0x150..+0x160` 五个DWORD逐项原样复制到 object HSerial 槽，SAFE6 再原样放入 restore node。`scripts/protocol/probe_lba4_v19_writer.py` 固定 v19 DLL/gold SHA，保留 authentic no-password SanDisk 的真实非零20B HSerial 输入，原生执行 `fcn.10006090` 后输出 LBA4 SHA-256=`c2662856...`，512/512与物理 gold 完全一致 | **historical restore-node reader/activation consumer** 已闭合为 structural-preserve / semantic-ignore：2020 `ActiveNormalUDev` 经 vtable `+0x2C` 进入 `ReadUsbHserialsInfo@0x100054A0`，依次从 LBA4、disk_end-4 sectors、disk_end-0x80000 三个镜像读取并 rolling decode完整0x2F node；三套 reader只比较 `OnlyIdXor8`。随后 vtable `+0x44` `RestoreRegsiterUsb` 只取 `node+0x04 OnllyID2Nd` 作为恢复blob密钥，不读取 `+0x08..+0x1B` 的五DWORD值。`EDP_DeviceNumber/EDP_DiskNumber` 是 reader 成功后另行返回的单DWORD scalar，ABI直接排除它与HSerial五槽等价 | 严格22份覆盖14份固定 `1D29,7B,4DD,79,7C`、6份全零、2份高熵，且 authentic no-password SanDisk 的非零20B已由 first-party v19 writer 在不修改该vector的条件下 bit-exact 重建整扇 | 字段级生命周期按 caller-owned identity vector 闭合：正式字段边界、current absent-zero profile、legacy caller injection ABI、官方 writer transport、三镜像 reader、negative semantic consumer和多种真实非零profile均已确定。更早 caller 为什么/如何计算五个DWORD仍是 value-generation provenance 开放问题，类似其它 caller-owned identity material，不再构成这20B盘面语义缺口；不得把 DeviceNumber/DiskNumber 猜成该算法 |
| LBA4 | 0x034 | COMPLETE | fixed restore-node `SingleUsbFlg` metadata = 0 | current Windows restore-node writer 清零 node 后显式保持/写入 `SingleUsbFlg=0`；Linux official node ABI确认该 BYTE 的结构位置 | Windows/Linux `ReadSector4` 都对完整0x2F restore node做 rolling decode 后结构性返回；除 `OnlyIdXor8` 外不对该 BYTE 做值相关分支，因此是 structural-preserve / semantic-ignore metadata | committed original fixtures 全量门禁 + 22份 strict originals 均为0，跨 current/legacy identity profile 无反例 | producer、正式字段边界、结构 consumer、negative semantic consumer 与跨代实盘一致，1B COMPLETE；不是运行时开关 |
| LBA4 | 0x035–0x038 | COMPLETE | `MyHardinfo` = observed mirror of LBA8 `HDSerialInfo` | current SAFE6 `RegsiterUsb` 对完整0x2F node清零且不覆盖 `node+0x1D..20`，因此 current producer=0。历史官方 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`）SAFE6 `virtual_56` 在 `0x1000D189` 调 `UsbTools.dll` ordinal4=`EDP_DiskNumber`，返回0才 fallback ordinal3=`EDP_DeviceNumber`，并在 `0x1000D19B` 把结果直接写到 `node+0x1D MyHardinfo`；同一二进制 LBA8 writer `sub_10007DF0@0x10007E82..` 独立调用同一 ordinal4/3 并把结果写入 `HDSerialInfo`，从 producer 机制解释跨LBA镜像 | 历史三套LBA4 reader完整结构返回node但只比较OnlyIdXor8；2020 `ReadUsbHserialsInfo -> RestoreRegsiterUsb` 链又证明恢复端只消费 `node+0x04`，不读取 `MyHardinfo`。因此 LBA4 副本是 structural-preserve / semantic-ignore compatibility metadata | strict originals逐盘 **22/22 `MyHardinfo == LBA8.HDSerialInfo`**，同时保留 current `0->0` 和 legacy非零 `A017AD78/A68BAE08/8B4613F5/2AB0E33C`；该值不等于device-id CRC或MBR signature | 字段级生命周期现已闭合：current-zero 与 legacy host-identity producer family、LBA4/LBA8 独立镜像写点、22/22 physical mirror、跨不同目标U盘复用同一 host-hardinfo 值以及 negative semantic consumer 共同限定了本 DWORD。v19 的相邻 `UsbOnlyInfo` 与 strict legacy 不同只证明另一个16B字段存在 generation 分叉，不再作为本字段 blocker；exact manufacturing executable 未定位不影响该4B含义闭环 |
| LBA4 | 0x039–0x03C | COMPLETE | fixed restore-node `NewLabFlag = LLGB` | current Windows machine code在 node 清零后显式写 `LLGB`；Linux DWARF恢复正式字段与偏移 | Windows/Linux reader解码并结构性返回完整node，不对该字段做独立行为判断 | committed original fixtures 全量门禁 + strict 22/22 均为 `LLGB`，跨 current/legacy profile 一致 | fixed writer metadata + structural-preserve/semantic-ignore + real-device profile闭合，4B COMPLETE |
| LBA4 | 0x03D–0x040 | COMPLETE | fixed restore-node `Version = 1` | current Windows writer显式写 DWORD 1 到 restore-node Version；Linux ABI给出字段边界 | Windows/Linux reader把 Version 随完整node返回，当前没有值相关准入/行为分支 | committed original fixtures 全量门禁 + strict 22/22 均为1 | 当前已知协议代际中的固定 restore-node version metadata 生命周期闭合，4B COMPLETE；未来新版本非1时应按新profile处理而非强制改写 |
| LBA4 | 0x041–0x044 | COMPLETE | fixed restore-node sector tuple `08 04 0C 01` | current Windows writer在 node 构造阶段显式写四个 sector BYTE `08 04 0C 01` | Windows/Linux reader随完整restore node结构返回这些BYTE，但当前没有独立值相关分支 | committed original fixtures 全量门禁 + strict 22/22 均为 `08 04 0C 01`，跨 current/legacy profile一致 | producer、结构边界、negative semantic consumer 与真实盘全部闭合，4B COMPLETE；按 fixed compatibility metadata 建模 |
| LBA4 | 0x045 | COMPLETE | caller-owned `bDataToServer` compatibility byte；wire 表示随 writer family 变化 | current Windows/Linux 与 historical v19.11.4.1 `fcn.10006090` 都把完整0x2F node作为输入；v19 writer 在 full rolling 后执行 `node+0x2D -> wire+0x45` post-XOR store。扩展后的 `probe_lba4_v19_writer.py --node-flags 0b00` 原生执行 official machine code，除 `+0x45: 00->0B` 外 512B 无任何变化，证明该 BYTE 是透明 caller-owned 输入而非 writer 内部派生 | Windows/Linux ReadSector4 rolling 后结构性返回完整node；2021 repair reader同样完整复制node且只校验OnlyIdXor8。ActiveNormalUDev/RestoreRegsiterUsb只消费OnllyID2Nd，GetUpLoadInformation虽携带node但本DLL没有 `+0x2D` 值相关读取；Linux DWARF中该node只进入BuildSector4/ReadSector4。因此已覆盖 consumer 对该BYTE是 structural-preserve / semantic-ignore | strict Aigo U335 `onlyid=1987718388` 的 physical flags=`64 7A`；按正式rolling精确得到 logical node flags=`0B 00`，其中对应key byte为 `6F/7A`，即 `64^6F=0B`、`7A^7A=00`。同盘免密转换前后LBA4 512/512不变，排除转换工具生成该值；其它 strict/post-XOR profile 与 authentic SanDisk 又覆盖 zero/nonzero reader view | 字段生命周期按 caller-owned compatibility metadata 闭合：逻辑值由上游调用者选择，writer只透明序列化，reader/restore无隐藏派生或值相关语义。更早 ordinary-rolling manufacturing EXE 未取得只影响 wire-representation provenance，不再构成本1B盘面语义缺口；不得把 physical wire byte直接当逻辑flag |
| LBA4 | 0x046 | COMPLETE | `bConnetServer` producer-side dormant-zero compatibility flag；reader view 随 wire representation 可非零 | current Windows/Linux 与 v19.11.4.1 writer都在rolling后post-XOR覆盖 node+0x2E；这些 SAFE6 node constructor 均 zero-init 且无后续 flag store，所以 direct producer-side=0。新增 `scripts/protocol/probe_lba4_v19_writer.py` 保留真实免密 SanDisk 的 second=`0x4A32BA39`/nonzero HSerial，只将 node flags 设为`00 00`，memory-backed 原生执行 v19 `fcn.10006090` 后唯一一次 LBA4 write 与真实扇区512/512完全一致，SHA-256=`c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8` | Windows/Linux ReadSector4统一rolling并返回该byte而不修正；current reader probe 对真实免密盘得到`D9`，证明 reader view 不是 producer value。2021 historical `UDiskLabelRepair::fcn.10006DA0` 独立执行同款rolling、完整复制0x2F node且只校验OnlyIdXor8；其 repair/backup 路径整块复制9扇区，不单独修改flag；ActiveNormalUDev/GetUpLoadInformation等已审上层无该byte值相关业务分支 | strict encrypted historical rolling-form样本的正式 reader view 本byte为0；authentic no-password gold SHA-256 `d6a935...` 为 physical wire=`00`、reader=`D9`，且 v19 official writer 以 producer node byte=0 精确重建整扇 | 生命周期按“producer-side zero + representation-dependent reader transform”闭合；COMPLETE 不意味着 reader 必为0，也不声称 v19 是该物理盘当年的 exact manufacturing EXE。exact provenance 仍属于 HSerial upstream 等其它字段的调查范围，不再构成本1B语义缺口 |
| LBA4 | 0x047–0x1FB | COMPLETE | **unowned backing / representation carrier**；raw-preserve 与 rolling-transformed 两种 wire 表示 | Windows current `sub_10014550` 与 Linux `BuildSector4@diskfile.cpp:741` 都只拥有0x2F restore node。non-null node 分支随后把 rolling XOR 覆盖到 `+0x18..+0x1FF`，因此对这437B只是**可逆变换既有 backing**；Windows `arg0==NULL` 分支则完全跳过 node copy/rolling，对该区逐字节 preserve。两端都没有独立业务字段 store。隔离 Unicorn 直接执行 official Windows writer：预填437B=`0xA5` 时，full 分支 raw bytes改变但独立 rolling decode后437/437恢复 `0xA5`；NULL 分支437/437保持原始 `0xA5` | Windows `ReadSector4/sub_10015090` 与 Linux `ReadSector4@diskfile.cpp:957` 都会为 restore-node 识别需要而滚动处理整段，但最终只返回 `decoded+0x18` 的0x2F node并校验 `OnlyIdXor8`，不暴露/解释 `+0x47..+0x1FB`。同一 official Windows reader 对上述 nonzero-full fixture 动态执行成功，返回 main onlyid、LLGB、Version=1，而437B不进入输出 | strict 22份仍保留18份 raw-zero与4份 rolling-zero物理表示；新增 first-party virtual writer fixtures 又证明 backing 可合法为任意非零值并在 full/null 两分支分别“transform/preserve”。CI `official_virtual_lba4_backing_is_unowned_and_representation_only` 锁定非零正例 | 437B 的含义不是“应该为零但最初 producer 未找到”，而是**没有业务 payload 的 caller/existing backing**。其完整生命周期已由 preserve/可逆transform writer、negative semantic consumer、real双表示和任意非零 first-party 正例闭合；与 LBA5 opaque-preserve 区采用同一 COMPLETE 口径。历史 raw-zero 最初来源不再是语义 blocker，未来未知非零 backing 必须保留/变换而不得清洗 |
| LBA4 | 0x1FC–0x1FF | COMPLETE | trailing LLGB | current writer 继续 rolling key schedule 写 LLGB | reader 作为尾锚点校验 | 22盘可验证 | 完成 |
| LBA5 | 0x000–0x1FF | COMPLETE | opaque preserve / write-protection probe scratch sector | `CUsbRegsiter::RegsiterUsb` 先读取既有 LBA0–12；后续 builder 只重建其它明确扇区，LBA5 不被覆盖，最终随13扇区整体写回；即 producer 语义是 preserve existing bytes | 两版 `EdpDiskCtrl` 的唯一 `base+5` raw-sector consumer 都是：读取整扇→原样写回同一扇区→仅检查 `WriteFile` 是否以 `ERROR_WRITE_PROTECT(0x13)` 失败；`UserLogin` 据此进入只读使用状态，完全不解析内容 | 22/22原始参考整扇512B全零，SHA-256均为 `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；7份原始CI夹具继续锁定 | COMPLETE 表示“整区用途和无payload语义闭合”；全零只是当前实盘状态，不是协议规定，非零内容也应原样保留 |
| LBA6 | 0x000–0x03E | COMPLETE | Dept 主槽前63B：short C-string/backing 或 long marker + Dept[0..58] | Linux `UsbWriteParam(UsbLabelParam&)@0x1C362` 对 department 调 `strcpy_s(dst+0x40,0xBC,src+0x40)`；copy-constructor 不预清对象且自带 `strcpy_s@0x1B9B0` 复制到NUL即停，所以 short profile 的 NUL 后字节是 writer-uninitialized backing。Linux `BuildSector6@0x1CAAC` 在 `strlen<=63` 时固定 memcpy 完整64B；在 long profile 时先清64B临时槽，写 marker `0x40245E2A`，再把 Dept 前60B 放到 marker 后，其中 `+0x04..+0x3E` 正好是 Dept[0..58] | Windows/Linux `ReadSector6`：short profile 按 C-string 读取；long profile 识别 marker 后把 inline prefix 与 LBA9+0x80 continuation 重组。当前/legacy 两种 long reader 均共享 marker + Dept[0..58] 这63B，接缝差异只发生在最后1B `+0x3F` | committed originals 的 short profile 至少3份，LBA6 Dept C-string 与 LBA8 `Dept=` 一致，且在 `+0x00..+0x3E` 内保留真实 post-NUL 非零 backing。严格原始 current Kingston join60 与 legacy Lexar join59 的解密 LBA6 `+0x00..+0x3E` **63B逐字节完全相同**；回归 `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 锁定这一点 | 前63B的两种动态状态均闭合：short = C-string + writer-uninitialized backing；long = marker + Dept[0..58]。已知历史 join59 分叉不触及这63B，因此本段升 COMPLETE |
| LBA6 | 0x03F | PARTIAL | long-Dept inline final byte / short-slot backing | current Linux/Windows/vrvaud long writer 在此写 Dept[59]，当前76B Dept 原盘值为 GBK trail `A8`；short profile 则只是固定64B槽的最后一个 backing byte。历史 v19.11.4.1 `fcn.10006370` 是直接 LBA6 writer，但其实现没有 `0x40245E2A` long-Dept marker，因此也不是 join59 producer | current long reader 用此字节区分接缝：非零则 continuation 接 Dept[60]；为0则按 legacy compatibility 分支从 Dept[59] 覆盖。CEMS2.0 x86 `fileophook.dll` 的 `fcn.10022f80@0x10022F80` 在 `0x100232ED` 比较 marker，先复制 `0x3C=60B` inline prefix，再把 LBA9 continuation 写到目标基址 `+0x3B`，从而覆盖 Dept[59]；x64 build 同构。这是 **CEMS2.0 join59 reader only**，不是 writer | strict-original current Kingston join60 在 `+0x3F=A8`，strict-original Lexar join59 在同一完整 Dept 上 `+0x3F=00`，且两盘前63B完全一致；另对本机 VRV 树按 marker 常量逐二进制扫描，实际 producer 命中只落在 current `cemsusbregsiter.dll` / `vrvaud_c.dll`，两者都固定复制60B，即 **all locally available marker writers use 60** | 兼容 reader ABI 已精确到机器码且本地现有 writer 已排除；但 CEMS2.0 同代 join59 producer/选择条件仍未取得，因此该唯一分叉字节继续 PARTIAL |
| LBA6 | 0x040–0x04F | COMPLETE | UsbMainBSec static template material | Windows `sub_10013FD0` 先从 `UsbMainBSec@0x100E7220` 复制整扇；Linux `BuildSector6@0x1CAAC` 同样从 `UsbMainBSec@0x22BB40` 复制 sector_size；本16B没有后续 overlay | Windows `sub_100152A0` 与 Linux `ReadSector6@0x1E2CC` 都在字段解析前对 raw `+0x000..0x1FB` 计算并校验 SAFE6 checksum，不匹配即拒绝；字段 parser 不另解释本段 | 严格22份原始盘解密后22/22精确等于官方模板 `f0 ac 3c 00 74 fc bb 07 00 b4 0e cd 10 eb f2 88`；CI含独立SanDisk锁定 | fixed producer + whole-sector integrity consumer + real-device evidence，无已知 profile 分叉，16B COMPLETE |
| LBA6 | 0x050–0x06F | COMPLETE | `m_usbowner` / User fixed 32B inline slot | Windows `sub_10047690` 先以 `strcpy_s(dst=UsbWriteParam+0xFC, cap=0x9C, src=request.User)` 写156B User数组；`strcpy_s@0x10097F4F` 逐字节复制到首个NUL即停，不清剩余capacity。注册时 `RegsiterUsb@0x1003B616` 再由 `sub_100139F0` 把持久 `UsbWriteParam` 复制到**未初始化栈局部 `var_3F4`**，该 copy 对 User 仍是同一个不清尾 `strcpy_s`。`BuildSector6` 对 `strlen<32` 固定复制该数组前32B；对 `strlen>=32` 写 `0x40245E2A + User[0..27]`，`User[28..NUL]` 写 LBA9+0x100 | Windows/Linux `ReadSector6`：短值只按 C-string 消费到首NUL；长值检查 marker 后把前28B与 LBA9+0x100 continuation 重组。隔离 Unicorn 又直接执行 current Windows `ReadSector6/sub_100152A0`：通过 checksum/rolling 后命中 `0x15552` marker 分支并把最大155B User 完整恢复到输出 `+0xFC` | committed originals 的短 User 与 LBA8 `User=` 一致，且首个NUL后有真实非零 backing；新增 first-party writer fixture 以最大合法155B User 运行 `BuildSector6@0x10013FD0`，解码后 inline28 精确等于 User[0..27]，LBA9 continuation 为127B剩余字符+NUL；动态官方 reader round-trip 恢复原串。CI `official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 锁定 | 32B 的所有物理状态均闭合：short = C-string + 明确的 writer-uninitialized backing；long = marker + 前28B。post-NUL 非零不是隐藏字段，因此不再因“值不稳定”保留 PARTIAL |
| LBA6 | 0x070–0x07F | COMPLETE | `m_autoid[16]` / Autonum fixed slot, including nonsemantic post-NUL backing | Linux DWARF 定义 `UsbWriteParam.m_autoid char[16]@+0x259`；`UsbWriteParam(UsbLabelParam&)@0x1C362` 调自带 `strcpy_s@0x1B9B0`，该实现只复制到首个 NUL、**不清剩余 capacity**，且构造器入口没有先 memset 整对象；`BuildSector6` 随后固定 memcpy 完整16B 到 `LBA6+0x70` | `ReadSector6` 只用 `strcpy_s(...,16,decoded+0x70)` 消费首个 NUL 前的 C-string；`BuildSector8` 将该字符串序列化为 `Autonum=`；同时 LBA6 前508B checksum 覆盖并保护包括 post-NUL backing 在内的全部物理字节 | 22/22 LBA6 C-string 与 LBA8 Autonum 相同；committed originals 中同一个空字符串至少出现2种不同且非零的 post-NUL backing，直接证明尾字节不属于隐藏字符串语义 | 16B 的逐字节行为已闭合：前缀是 C-string，NUL 后是明确的 **writer-uninitialized backing**；值不固定是协议实现行为本身，不是未知字段，因此整槽 COMPLETE |
| LBA6 | 0x080–0x0BF | COMPLETE | `m_UsbOffice[64]` fixed slot, including nonsemantic post-NUL backing | Linux DWARF 定义 `UsbWriteParam.m_UsbOffice char[64]@+0x198`；copy-constructor 用同一个不清尾 `strcpy_s@0x1B9B0` 写该数组且不预清对象；Windows/Linux `BuildSector6` 再固定复制完整64B到 `out+0x80` | Linux `ReadSector6@0x1E6A3..` 明确 `strcpy_s(UsbLabelParam.m_UsbOffice,64,decoded+0x80)`，只解释首个 NUL 前字符串；Windows reader 同构；整扇 checksum 仍覆盖这64B的全部物理值 | 22份原始盘存在空/非空 Office；committed originals 中同一个空 Office 字符串至少出现3种不同且非零的 post-NUL backing profile，排除隐藏字段/固定 padding 解释 | 64B 槽同样是 **writer-uninitialized backing**：producer bug、C-string consumer、完整性消费和多实盘 profile 均闭合；post-NUL 值允许不稳定但无第二业务字段语义，故整槽 COMPLETE |
| LBA6 | 0x0C0–0x0FF | COMPLETE | UsbMainBSec static bootstrap/template material | Windows/Linux BuildSector6 均先整扇复制官方 `UsbMainBSec`，本64B后续无字段覆盖 | 两端 ReadSector6 的 SAFE6 checksum 在字段解析前覆盖整个前508B；本64B无独立业务字段读取 | 严格22份原始盘22/22逐字节等于官方模板；CI含独立SanDisk精确锁定 | fixed producer + checksum consumer + 22盘闭合，64B COMPLETE |
| LBA6 | 0x100–0x103 | COMPLETE | write-owned `m_crcUsbID[0]` identity/key metadata = CRC32(device_id) | Linux DWARF 明确 `m_crcUsbID[2]@CLabelManage+0x24`；Linux ctor/`Init` 都执行 `CRC32(0,m_strUID.c_str(),m_strUID.length()) -> +0x24`；Windows `sub_10013D20/sub_10013B80` 同构；两端 `BuildSector6` 都把该数组8B复制到扇区 +0x100 | 同源运行时成员 `m_crcUsbID[0]` 被 LBA7 rolling-XOR 与 LBA8/LBA12 加解密直接使用；LBA6 current `ReadSector6`/Windows current CheckLabel 正常分支不读取这个持久化副本，因此副本本身是 checksum-covered / semantic-ignore 的 write-owned metadata，而不是 current 输入参数 | 严格22份含独立SanDisk：22/22 `u32(+0x100)==CRC32(device_id)` 且全部非零；CI门禁 `lba6_crc_usb_id_pair_is_device_id_crc_and_doubled_guard` | 正式字段名、双平台 producer、值算法、同源运行时用途、LBA6 negative semantic consumer、整扇 checksum ownership 与真实盘均闭合；4B COMPLETE。未来 reader 可继续忽略该副本而从 device-id 重算 |
| LBA6 | 0x104–0x107 | COMPLETE | write-owned doubled CRC compatibility guard = 2 × CRC32(device_id) mod 2^32 | Linux ctor/`Init` 直接 `m_crcUsbID[1]=m_crcUsbID[0]*2`；Windows两套构造路径同样 `object+0x48=object+0x44<<1`；`BuildSector6` 连续复制8B | Windows `CheckLabel/sub_100152A0`、`cemsudisk`、`vrvaud_c` 都保留 `+0x100!=0 && +0x104==(+0x100<<1)` 的 historical consistency check，并映射到版本/系统标签不匹配错误；current build 的该值相关分支虽被恒真 `if(1)` 隔离，但正常 reader 仍明确 semantic-ignore 该副本，整扇 checksum 覆盖其物理值 | 严格22份：22/22 `u32(+0x104)==u32(+0x100).wrapping_mul(2)`；独立SanDisk同样吻合；同一CI门禁锁定 | producer、历史 consumer 语义、current negative semantic consumer、checksum ownership 与真实盘关系均闭合；该4B是 retained compatibility guard，不因 current 分支不可达而继续视为未知业务字段 |
| LBA6 | 0x108–0x187 | COMPLETE | UsbMainBSec static bootstrap/message template material | Windows/Linux BuildSector6 先复制官方 `UsbMainBSec`；本128B没有任何字段 overlay | Windows/Linux ReadSector6 均先校验覆盖前508B的 SAFE6 checksum；字段 parser 不读取本段 | 严格22份22/22等于官方模板，包含 `Invalid partition table` / `Error loading operating system` / `Missing operating system` legacy message material；CI含独立SanDisk锁定 | 128B fixed template material 的 producer、完整性 consumer、实盘闭合，COMPLETE |
| LBA6 | 0x188–0x1BF | COMPLETE | `m_usbLabel[64]` first 56B physical slot, including nonsemantic post-NUL backing | Linux DWARF fixes `UsbLabelParam/UsbWriteParam.m_usbLabel@+0x218`; `UsbWriteParam(UsbLabelParam&)@0x1C362` uses built-in `strcpy_s(...,64,...)`, and this copy-constructor does **not** memset the 0x299B destination object first. The built-in `strcpy_s@0x1B9B0` returns immediately after copying the first NUL, so destination bytes after NUL retain prior backing. Windows `sub_10013FD0` / Linux `BuildSector6` then fixed-copy the first 0x38=56B of that array to `out+0x188` | Linux `ReadSector6@0x1E84B..` constructs a C++ string from `decoded+0x188` and writes it back to `UsbLabelParam.m_usbLabel[64]`; Windows reader isomorphic. `BuildSector8` serializes the same logical value as ELABEL `Label=`; LBA6 checksum covers every physical byte of the 56B slot | committed originals all decode to the same business value `江苏电力!SAFE6`, yet the bytes after its NUL form **at least 3 distinct and all-nonzero backing profiles**; regression `lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries` locks this. LBA6 C-string and LBA8 `Label=` remain equal in every committed original | Entire 56B behavior is closed: prefix is the C-string, bytes after NUL are **writer-uninitialized backing** copied from the 64B source array, not hidden fields or fixed padding. Future nonzero backing is valid compatibility data; canonical provisioning may zero it deterministically and recompute checksum |
| LBA6 | 0x1C0–0x1C8 | COMPLETE | `m_usbGSerial` 的 profile-independent C-string prefix / terminator position | current Windows/Linux `BuildSector6` 都先清16B temp，再从 `UsbWriteParam.m_usbGSerial` 固定复制前15B；已知 short/long profile 到 byte8 为止始终仍属于字符串本体：short 为 `322CA28A\0`，long 为 `322CA28A-` | Windows/Linux `ReadSector6` 从 `+0x1C0` 按 C-string 解释；只有首个NUL之后才进入 backing | strict 22盘：16份 short=`322CA28A`、6份 long=`322CA28A-D7D144`；完整历史去重 census 另复核20个 front，前8B 20/20=`322CA28A`，byte8 仅出现 `00` 或 `2D('-')`。legacy MBR underlay surviving bytes从 short NUL 之后才开始 | 已知代际分叉不触及前9B的字段归属；9B从PARTIAL升COMPLETE |
| LBA6 | 0x1C9–0x1CE | COMPLETE | GSerial dynamic string-tail / caller-owned post-NUL backing | current Windows/Linux writer 对16B temp清零后，固定从 source `m_usbGSerial[0..14]` 复制15B；因此长 profile 时本6B可继续属于字符串正文，短 profile 时则只是 source NUL 后 backing。first-party Windows `BuildSector6` 虚拟执行把短 GSerial 的 source backing 人为设为 `A5×6`，盘面逐字节保留，证明 builder不赋予第二字段语义 | Windows/Linux reader从 `+0x1C0` 仅按 C-string 消费到首个NUL；短 profile 的本6B完全不参与业务解析，长 profile则作为同一 GSerial 字符串尾部被正常消费 | strict/current/legacy实盘同时覆盖 short/long；两份 legacy 中这些 post-NUL bytes 恰带旧 MBR 几何残值，只说明历史 source backing provenance。新增 official virtual fixture 证明任意非零 backing 可合法 round-trip 到盘面 | 动态边界、writer ownership、C-string consumer 与非零 backing 正向证据均闭合；legacy MBR残值不再被误当独立字段。6B从PARTIAL升COMPLETE |
| LBA6 | 0x1CF | COMPLETE | `m_usbGSerial[15]` dedicated zero terminator byte | Windows/Linux `BuildSector6` 都先把16B临时槽清零，只从 source `m_usbGSerial` 固定复制前15B，因此 byte15 不受 source post-NUL backing 影响，始终保留显式零 | Windows/Linux `ReadSector6` 从 `+0x1C0` 按 C-string 读取；当15B业务字符串占满前15B时该字节提供终止NUL，短串时仍只是固定槽尾零 | committed current/legacy fixtures全部为0；另对 `nopwd_tool/backup` 22份历史 front 用 SAFE6 checksum 过滤后 22/22 有效且 `+0x1CF=0` | producer ownership、C-string consumer、跨 current/legacy 历史实盘均无分叉；1B从PARTIAL升COMPLETE |
| LBA6 | 0x1D0 | COMPLETE | `BeiZhu` C-string 首字节 / 空串 NUL | current Windows/Linux writer从 BeiZhu 输入槽复制前15B到零化temp；reader从 `+0x1D0` 按 C-string 读回。无论 profile 是空串还是 GBK“普通”，首字节都仍属于字符串本体（空串时就是终止NUL） | Windows/Linux reader以 C-string 解释，不存在首字节的 underlay-only 分支 | strict 22盘：20份首字节=00（空），2份首字节=C6（GBK“普通”首字节）；旧 MBR underlay在“普通”NUL之后才暴露 | profile-independent 字符串边界闭合，1B升COMPLETE |
| LBA6 | 0x1D1–0x1DE | COMPLETE | BeiZhu dynamic string-tail / caller-owned post-NUL backing | current Windows/Linux writer同样只复制 source BeiZhu 前15B到零化temp：非空 profile 中首个NUL前属于同一 BeiZhu 字符串，空/短 profile 中其余字节只是 source backing。first-party Windows `BuildSector6` 虚拟执行把空 BeiZhu 的 source `+1..14` 人为填成 `5A×14`，输出逐字节保留 | Windows/Linux reader只按 C-string 消费到首个NUL，绝不解释 post-NUL backing；legacy“普通”profile的 MBR几何残值位于NUL之后，因此同样不进入业务语义 | 22盘含20空+2个GBK“普通”，并有8/22 post-NUL非零；两份 legacy backing 与旧 MBR snapshot 连续。official virtual fixture又证明任意 `5A×14` backing 合法存在而不改变空字符串语义 | 与 Label/Office 固定槽同类：正文与backing由首NUL动态分界，backing是caller-owned compatibility bytes而非隐藏字段。14B从PARTIAL升COMPLETE |
| LBA6 | 0x1DF | COMPLETE | `BeiZhu[15]` dedicated zero terminator byte | current Windows/Linux `BuildSector6` 对16B临时槽先清零、只复制 source BeiZhu 前15B；historical v19 也已补齐上游约束：`object+0x2620` 在全部 executable xref 中只有 `0x1000B219` 的 writer-caller 读取和 `0x1000CCEF` 的唯一写入，后者明确执行 `strcpy_s(dest=object+0x2620, cap=0x10, source=object+0x2478)`。随后 caller 先零化32B arg8，再以 cap=32 从这个 cap=16 object field复制，因此有效 producer 最多15B正文+NUL，byte15 必为0 | reader从 `+0x1D0` 按 C-string 消费；该字节是 current 与 v19 两代合法 producer 的固定安全终止NUL | committed current/legacy fixtures全部为0；扩展22份 checksum-valid 历史 front 同样22/22 `+0x1DF=0` | 上游 cap=16 机器码直接消除了“v19 cap=32 可能让正文占据 byte15”的歧义；跨 profile producer/consumer/实盘完整闭合 |
| LBA6 | 0x1E0–0x1ED | PARTIAL | v19 BeiZhu cap-32 C-string slot post-NUL template backing / legacy MBR entry3 snapshot fragment | current Windows/Linux `BuildSector6` 的较新 ABI 只显式拥有到 `+0x1DF`；historical v19.11.4.1 则把 `+0x1D0` 建模为 **capacity=32 的 BeiZhu C-string 槽**：`fcn.10006370@0x10006648..0x10006656` 对 `sector+0x1D0` 调 `strcpy_s(cap=0x20, caller arg8)`。其真实 caller `0x1000B16C..0x1000B226` 先把完整32B局部清零，再从 `object+0x2620` 以 `strcpy_s(cap=0x20)` 填入 BeiZhu；进一步枚举全部 executable `object+0x2620` xref 只得到 `0x1000B219`（此处读取）与 `0x1000CCEF`（唯一写入），唯一写入在 `0x1000CCE8..0x1000CCF8` 明确执行 `strcpy_s(dest=object+0x2620, cap=0x10, source=object+0x2478)`。所以 v19 的合法源字段最多15B正文+NUL，32B arg8 的 index15 必为NUL、index16..31保持先前零化；v19 `UsbMainBSec@0x101BA790` 的 `+0x1D0..+0x1F3` 也全部为0。因此该代 writer 对本14B明确只能生成0，而不是主动生成 MBR 字段。legacy Aigo+SanDisk 两盘则精确保留旧 MBR entry3 bytes[2..15]：start-CHS尾、`type=0x07`、end-CHS、start_lba、sector_count | v19 `ReadSector6@0x10006E07..0x10006E16` 从 decoded `+0x1D0` 调同一 `strcpy_s` 返回 caller arg8，capacity同为32，只消费到首个NUL。六个已恢复 caller 分三组：第一组 arg8 局部除初始化/两次 reader 传参外无后续引用；第二组成功路径解析的是另一 `esp+0x18` 字符串，不读取 arg8 的稳定 `esp+0x38` 输出槽；第三组 arg8 局部 `-0x64` 的唯一后续值使用是再经 `strcpy_s(cap=16)` 写入 object+0x140。因此 NUL 后 `+0x10..+0x1D` 没有 cmp/test/hash/branch/字段提取业务消费 | 20/22 strict历史为零；2/22 nonzero 且 type/start/count 与各自 LBA12 type4精确对应。`scripts/protocol/audit_legacy_lba6_mbr_underlay.py` 进一步固定 Aigo 完整金标与独立 SanDisk 原始 LBA6 fixture，逐字重放完整16B entry：两份 boot/CHS/type 前8B一致而 start_lba/sector_count 后8B随盘变化，真实免密 SanDisk 同一 underlay 则为16B全零；可选本机扫描1745个 PE 对公共前缀无命中。SanDisk三分区几何完全同步；Aigo rev_pmap 显示这里是更早的 stale MBR snapshot；`scripts/protocol/audit_v19_lba6_beizhu_slot.py` 固定 v19 DLL SHA 并重放上述模板、writer、reader 与 caller-use 约束 | consumer 边界现已闭合，旧“v19 overlay 不触及本14B”陈述已纠正；机器码证据继续**排除 v19.11.4.1 作为 nonzero snapshot producer**：pinned v19 writer/template 恰恰只能生成 post-NUL zero，无法生成两份真实 nonzero MBR underlay。`scripts/protocol/audit_netac_mbr_entry_boundary.py` 又直接排除当前捕获的 Netac 1.3.1.16 FormatExA MBR 初始化路径：它复制模板后以 `memset(output+0x1CE,0,0x30)` 清空 entries2-4，随后直接 patch 只落在 entry1；因此 nonzero `+0x1DE` entry3 必须来自更老 formatter 或另一条 EDP MBR copy/reuse 路径。exact legacy underlay producer/profile-selection 仍缺，故按严格 producer+consumer+physical 门槛继续 PARTIAL，禁止仅因 reader 不消费而升级 |
| LBA6 | 0x1EE–0x1EF | COMPLETE | zero compatibility tail / MBR entry4 unused prefix | current `UsbMainBSec` 对第4条 entry 的 status/start-head 为0且 current BuildSector6不覆盖；v19 虽把 `+0x1D0..+0x1EF` 作为 cap=32 BeiZhu目标槽，但其上游 `object+0x2620` 唯一写入是 cap=16 C-string，caller 又先零化完整32B arg8，因此 source NUL 后的 index30/31 必保持0；legacy MBR snapshot跨到 entry4 后实盘也为 `00 00` | ReadSector6只按 C-string 消费至NUL，对这2B无独立值相关业务读取 | committed current/legacy fixtures均为0；扩展 `nopwd_tool/backup` 22份全部 checksum-valid，22/22 `+0x1EE..+0x1EF=00 00` | current/v19 producer、legacy snapshot profile、negative consumer和全历史实证均无分叉；v19 cap=32 本身不再构成该2B边界歧义 |
| LBA6 | 0x1F0–0x1F3 | COMPLETE | write-only `!SAFE` label-generation metadata (`m_encrypt`) | DWARF 正式定位 `UsbWriteParam.m_encrypt@+0x258`；Windows `RegsiterUsb` 对注册字符串执行5字节 `!SAFE` 匹配，相等/不等分支在 `0x1003BA94/0x1003BAC7` 分别写1/0，随后立即传给 BuildSector6；Windows/Linux BuildSector6 都把该 bool 扩成 DWORD 写 `+0x1F0` | 读取侧正式 `UsbLabelParam` 结构没有 `m_encrypt` 成员；Linux ReadSector6 不返回它。Windows `CheckLabel/sub_100152A0` 校验前508B checksum 后显式解析其它字段，但不读取 `+0x1F0`；已扫 runtime 无值相关 consumer。因此消费语义是 checksum-covered / semantic-ignore，而非运行时加密开关 | committed strict originals 22/22=1；独立 SanDisk 原始 LBA6 同样=1；CI `lba6_m_encrypt_is_the_observed_write_only_safe_label_metadata` 锁定 | producer 的0/1规则、正式字段名、negative semantic consumer、checksum ownership 与真实盘均闭合。COMPLETE 不表示恒为1；非 `!SAFE` producer 可合法写0 |
| LBA6 | 0x1F4–0x1FB | COMPLETE | UsbMainBSec static zero tail before checksum | Windows/Linux BuildSector6 都由 `UsbMainBSec` 初始化，字段 overlay 最后只写到 `+0x1F3`，故8B保持模板零 | 两端 ReadSector6 的 checksum 覆盖到 `+0x1FB`；字段 parser无独立读取 | 严格22份22/22解密为8B零，官方模板同样为零；CI含独立SanDisk锁定 | explicit template-zero producer + checksum consumer +实盘，8B COMPLETE |
| LBA6 | 0x1FC–0x1FF | COMPLETE | SAFE6 checksum | writer 对前508B计算 checksum | reader/inspect 校验 | 22/22 校验通过 | 完成 |
| LBA7 | 0x000–0x0BF | COMPLETE | 3×64B packed legacy EDPF table | Windows `CUsbRegsiter::CreatePartitions` 以 packed `0x40` stride 构造最多3条 old entry；`edpediskctrl.dll::sub_100125B0` 将 runtime `0x60` entry 反向映射回 packed old table，`SavePartionSector/sub_10028580 -> sub_10010FC0` 负责整表写回。Flag、Version、PartionCount、PartionType、NeedDisturb、NeedEncrypt、StartSector、SectorSize、PartionSize、UserKeyCRC、FileKeyCRC 与 legacy wrapped8 的逐字段 producer 生命周期均已在下方/详细审计独立闭合 | `ReadPartionInfoExEx/sub_10010B40`、old→new converter、`NewCheckDisTurbUsb(*)`、登录/挂载/改密与 file-key CRC 链按字段消费；Version 与 entry1/2 NeedDisturb 已由 structural-preserve + negative-semantic-consumer 闭合，wrapped8 已由解包+CRC active consumer 闭合 | 22份 strict originals 全部按0x40 stride合法；另有独立真实免密 SanDisk 两-entry profile。Version、NeedDisturb positional profile、wrapped8 正向解包/CRC 等均有 committed 回归门禁 | 此行为**总括行**：192B 的逐字段证据已全部闭合，最终严格计数为192/192 COMPLETE；Linux natural `0x48` ABI只用于字段名/结构交叉，不得覆盖 Windows 物理 offset |
| LBA7 | 每条entry +0x004–+0x007 | COMPLETE | entry-local `Version` compatibility metadata | `CreatePartitions` 先清零3×0x40 packed old table，三条都没有 `+0x04` 覆盖写，因此 current producer 为0；Windows `0x40->0x60` 与 `0x60->0x40` converter 均逐条结构保留该DWORD | 两版 `vrvaud_c` 对完整0xC0 packed table 的行为 xref 均不读取三条 Version；协议代际由14B pass-info Version 决定。Linux 对应 old→new converter 也只结构搬运；`CDiskReader::GetTagPartitionInfo/DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0` 的真实消费字段分别集中在 Flag/PartionType/UserKeyCRC、StartSector、FileKeyCRC、wrapped key、EncryptMode，均不读取 entry Version | committed original fixtures + 独立真实免密 SanDisk 的全部有效 LBA7 entries 均 `Version=0`；扩展只读历史去重扫描仍无非零反例 | 不是 Reserved，也不是 PartionCount；闭合的是“formal ABI compatibility metadata，current writer=0、converter structural-preserve、runtime negative-semantic-consumer”的完整生命周期，3×4B=12B COMPLETE |
| LBA7 | entry0 +0x010–+0x013 | COMPLETE | entry0 `NeedDisturb` MBR scramble/descramble gate | Windows `CreatePartitions` 对 entry0 显式写入调用者传入的 `NeedDisturb=1`；old/new ABI converter 双向保留该DWORD | 两版 Windows `vrvaud_c` 的 `NewCheckDisTurbUsb(*)` 直接检查 packed entry0 `NeedDisturb@+0x10 != 0`；`SetProtect` 在该检查链后调用 `sub_1006ff80 -> sub_1006e580`，把 LBA0 `+0x1BE..+0x1FD` 的64B MBR表替换为静态 scramble 表；`UnsetProtect -> sub_1006ffd0 -> sub_1006e9b0` 则从 LBA2 读整扇恢复到 LBA0 并刷新磁盘属性 | 严格22份 original real-device：22/22 entry0 `NeedDisturb=1`；另有真实免密 SanDisk 两条-entry profile 同样 entry0=1 | 字段不是泛化“防篡改”位，而是驱动侧是否进入系统可见 MBR 分区表 scramble/descramble 流程的门控；静态扰动表只有一条 type=0x04、start_lba=66、sector_count=1 的占位 entry。entry1/entry2 的同名字段仍未找到独立 consumer |
| LBA7 | entry1/entry2 +0x010–+0x013 | COMPLETE | positional `NeedDisturb` compatibility metadata | current `CreatePartitions` 的调用参数固定为1：entry0/entry1 显式写1，entry2无覆盖写而继承整表清零0；Windows old/new converter 双向结构保留该DWORD | 两版 `vrvaud_c` 的行为读取只命中 entry0 `NeedDisturb`；entry1/entry2 没有条件分支或参数映射。Linux 对应 old→new converter保留字段，但 `CDiskReader` 文件系统检查链不读 NeedDisturb | committed original fixtures 全量门禁按**位置**锁定三-entry `(1,1,0)` 与两-entry `(1,1)`；独立真实免密 SanDisk 的 type4 位于 entry1 且值为1，证明该字段不是 `PartionType -> NeedDisturb` 恒等式；扩展历史扫描没有第三种 positional profile | 8B 闭合为 current writer positional compatibility profile + structural-preserve + cross-platform negative semantic consumer；COMPLETE 不把 entry1/2 解释成 entry0 的 MBR 扰动行为，也不禁止未来其它 writer profile |
| LBA7 | 每条entry +0x038–+0x03F | COMPLETE | 8B legacy wrapped file-key | Windows `sub_10028DB0` 以 `fold32(password)` 对两个32位 half 做对称 XOR 包装；`sub_100125B0` 映射回 old 0x40 entry；`SavePartionSector/sub_10028580 -> sub_10010FC0` 写 LBA7 | `sub_10026050` 对 v0x0064 固定解包8B，随后以 `sub_10038840` 计算 CRC32 并比较同 entry `FileKeyCRC(+0x34)`；改密后反向重包 | 22份 original real-device 中全部28条非零 type2/type4 legacy entry 独立复算 28/28 PASS；默认 `fold32("0000aaaa")=0x91919191` | **LBA7 v0x0064 packed legacy file-key wrapping** 已闭合；FileKeyCRC 4B此前已经计入 COMPLETE，本轮仅新增3×8B=24B，禁止重复计数 |
| LBA7 | 0x0CA | COMPLETE | pass-info `bNoUsbChkPasSafe` | current Windows `CreatePartitions/sub_1003DB50` 明确从制标请求写 `tail+0x0A`；`WriteNormalULabel/sub_10046E80` 又把 `UsbWriteParam+0x7EC` 传入该请求字段 | 独立 Linux 官方 `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接执行 `cmp byte [pass+0x0A],1; setne showparam+0x03`，随后 `CreateSafe6TmpPolicyFile@0x407D50` 将结果纳入 CRC/加密 SAFE6 policy；独立 `EdpEDiskBack::Safe6PolicyFile::GetSafe6Policy@0x4100B0` 与 `linuxedpedisk::Safe6PolicyFile::GetSafe6Policy@0x41EAA0` 解密并恢复该 policy/runtime 参数 | 严格22份原始盘：18×0、4×1；22/22 LBA7/LBA12 同盘取值一致；CI `pass_info_no_usb_safe_flag_varies_and_matches_between_lba7_and_lba12` 锁定0/1双值与跨扇区一致性 | 字段行为闭合到“值等于1时将 SAFE6 show-policy byte +3 清零，否则置1”，不是仅 opaque round-trip；producer、真实行为 consumer、跨独立客户端传递和实盘双值证据齐全 |
| LBA7 | 0x0CC–0x0CD | COMPLETE | dormant pass-info `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` compatibility bytes | Linux DWARF `edpdiskglobal.h:164/165` 明确给出两个独立 `BYTE` 字段，物理偏移 `+0x0C/+0x0D`；current `CreatePartitions/sub_1003DB50` 在 `0x1003DC16..0x1003DC26` 显式清零完整14B pass-info，后续 store 只到 `+0x0A`，因此 current producer 为0/0 | 四个不同哈希/代际的 `EdpEDiskCtrl` reader 均把完整14B pass-info 结构复制到输出；已复核成功尾部只对 `Version(+0)`、Share retry `(+3)`、Encrypt retry `(+6)` 做 XOR/值处理，`+0x0C/+0x0D` 只 structural-preserve。Linux checker 同样保存完整14B但无这2B业务读取；两代 `vrvaud_c::BackupPromptInfo/BackupStartTime/BackupEndTime` 已证明是独立 policy/string/DWORD 链，与 pass-info 无数据流 | committed originals 的 LBA7/LBA12 两份副本逐盘0/0且一致；全目录去重扫描19个真实 LBA7 密文 profile（覆盖 pass-info v0x0064 与 v0x0206）仍19/19=0/0 | 闭合语义是“正式命名但在已覆盖实现中 dormant 的 compatibility bytes”：producer=0、reader structural-preserve/negative-semantic-consumer、跨代/跨LBA实盘一致。COMPLETE 不声称其历史设计单位是小时/天；未来非零 profile 必须保留并扩展，不得机械清零 |
| LBA7 | 0x0CE–0x1FF | COMPLETE | packed old-table post-table writer-zero region | Windows `edpediskctrl.dll::sub_10010FC0` 先以 `sub_1004D110(...,0,0xFFF)` 明确 memset staging，随后只复制 `0xC0` packed table + `0x0E` pass-info，再对完整512B rolling并写 LBA7；`sub_1004D110` 机器码已复核为 memset 等价实现 | Windows `ReadPartionInfoExEx/sub_10010B40` 解密完整512B，但成功后只复制 `0xC0` table 和 `0x0E` pass-info，完全不返回/解释 `0x0CE..0x1FF`；Linux natural-ABI builder也独立采用“整块清零→写结构→整扇rolling”的同原则，但其表尾在0xE6，只作原则佐证、不用于覆盖Windows物理offset | 严格22份原始生成参考（21 non-converted backup + 独立SanDisk）逐盘解密：22/22 `0x0CE..0x1FF == zero[306]`；CI门禁 `lba7_post_table_plaintext_is_zero_through_sector_end` 锁定 committed original subset | 306B 的 producer零来源、negative consumer、物理边界和原盘均闭合；这里的 COMPLETE 表示 writer-owned zero region，不是靠“样本碰巧全零”推断 |
| LBA8 | 0x000–0x003 | COMPLETE | LLGB magic | Windows/Linux `BuildSector8` | reader 先检查 LLGB | 22/22 | 完成 |
| LBA8 | 0x004–0x007 | COMPLETE | logical length | writer=`0x80+strlen(ELABEL)` | decoder决定动态加密前缀 | 22/22吻合 | 完成 |
| LBA8 | 0x008–0x00B | COMPLETE | ToolVersion[4] | Windows `sub_100148d0` 与 Linux `BuildSector8@diskfile.cpp:805` 都写固定字节 `01 00 00 01` | `ReadSector8(UsbLabelParam&)` 的语义 parser 不读取该版本戳；`ReadSector8(BYTE*)` 仅把完整解密扇区原样导出 | 22/22原始盘=`01 00 00 01`；CI原始夹具锁定 | 4B writer、reader行为、实盘一致，无已知 profile 分叉 |
| LBA8 | 0x00C–0x00F | COMPLETE | Labversion | Windows/Linux writer 都固定写 `0x00000222` | 语义 parser 跳过该 DWORD；raw reader 仅原样导出 | 22/22原始盘=`0x222`；CI原始夹具锁定 | 4B 标签版本戳闭合 |
| LBA8 | 0x010–0x013 | COMPLETE | writeTime | Windows writer 调 `GetTickCount()`；Linux `CLabelManage::GetTickCount@0x1FBAA` 用 `clock_gettime(CLOCK_MONOTONIC)` 转为毫秒并截为32位 | 两个官方 reader 都不把该值用于标签解析/准入；raw reader 只导出原值 | 22/22原始盘均非零且跨标签变化；CI夹具保持多值反例 | 不是墙钟时间，而是制标时单调时钟毫秒计数（32位回绕） |
| LBA8 | 0x014–0x017 | COMPLETE | HDSerialInfo / mirrored host-hardinfo identity DWORD | `tagEdpUsbLableInfo.HDSerialInfo@+0x14`。current Windows/Linux BuildSector8 从零初始化 header 得到0；另取得并哈希核验 2020 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`），其可达 ELABEL writer `sub_10007DF0` 先调 `UsbTools.dll` ordinal4，结果为0才 fallback ordinal3，并把返回DWORD写到临时LLGB结构 `+0x14`。本机 `UsbTools.dll` 导出表已精确证明 ordinal4=`EDP_DiskNumber`、ordinal3=`EDP_DeviceNumber`；两者分别是跳到 `DeviceNumber.dll` ordinal3/ordinal1 的纯 thunk。`EDP_DiskNumber` 对 PhysicalDrive ATA serial 规范化后做 reflected CRC32；`EDP_DeviceNumber` 复用同一 serial stream，再按接受顺序追加 `MACAddress<i>=<12位大写无分隔MAC>\r\n`，至少一条时追加 `MACCount=<N>\r\n`（排除非零条件不满足及描述同时命中 VIRTUAL+VMWARE 的虚拟适配器），随后对 `serial_blob` 与 `mac_blob` 拼接 做同一 CRC32 | 注册侧 semantic reader不读取该DWORD；current runtime `sub_10016260` 与2020 `EdpEDiskCtrl.dll` 都结构保存它但未发现值相关行为读点。新增跨扇区证据：LBA4 restore node 的 `MyHardinfo` 在22份 strict originals 中逐盘与本DWORD完全相等 | current profile全0；legacy profile全非零并按捕获/硬件环境成组；**22/22 `LBA4.MyHardinfo == LBA8.HDSerialInfo`**，且非零集合精确包含 `A017AD78/A68BAE08/8B4613F5/2AB0E33C`。2020 writer证明存在“宿主磁盘/主机身份 CRC32 -> HDSerialInfo” producer family | 字段级生命周期闭合为跨LBA镜像的 host-hardinfo identity DWORD：v19 official writer 对 LBA4/LBA8 分别独立调用同一 `EDP_DiskNumber`/fallback `EDP_DeviceNumber`，22/22 strict physical mirror一致，非零值按宿主环境成组且 `A68BAE08` 跨 Lexar/Aigo 不同目标U盘出现，runtime consumer无值相关分支。相邻 UsbOnlyInfo 的 generation 分叉继续单独记账，不再阻塞本4B，故升 COMPLETE |
| LBA8 | 0x018–0x01D | COMPLETE | `MacInfo[6]` reserved/unused MAC slot | Linux DWARF 正式定义 `MacInfo unsigned char[6]@+0x18`；Windows/Linux BuildSector8 都先清零完整 header，Linux 再把仍为0的 DWORD+WORD写到+0x18，current producer明确为6B零 | Windows `sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 均从 ElabOffset 解析 ELABEL，不读取 MacInfo；raw reader仅 opaque 导出，不赋予业务语义 | 严格22份原始盘跨 current/legacy identity **22/22均为6B零**；CI `lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty` 现对全部 profile 锁定 MacInfo=0 | 官方字段边界、显式零 producer、negative semantic consumer 与跨代实盘均闭合，无已知 profile 分叉，6B COMPLETE |
| LBA8 | 0x01E–0x02D | COMPLETE | `UsbOnlyInfo[0..15]` optional 16-char compatibility identity text | current Windows `RegsiterUsb -> sub_100148d0` 明确以 main onlyid 执行 `sprintf("%08x%08x", onlyid,0)`；Linux BuildSector8同构。2020 `sub_10007DF0` 使用相同16字符模板，但第二DWORD来自 `EDP_DiskNumber`/fallback `EDP_DeviceNumber`，因此 active producer profile 至少包含 current `main-onlyid + 0` 与 transitional `main-onlyid + host-hardinfo`；strict legacy physical profile则为该槽 absent/zero | registration semantic reader完全跳过该槽；current runtime `sub_10016260` 与2020 `EdpEDiskCtrl.dll` 只结构保存，未发现值相关行为读点；Linux `ReadSector8(UsbLabelParam&)` 从 ElabOffset 解析 ELABEL，不用 UsbOnlyInfo 做准入或业务决策 | committed originals 中 current identity精确等于 `format("%08x%08x", main_onlyid_bits,0)`，strict legacy为16B全零；2020 official producer又证明第二个8字符组可合法承载非零 host-hardinfo | 字段级生命周期按 optional compatibility slot 闭合：两种 active writer、strict-legacy absent-zero physical profile、跨代 negative semantic consumer均已明确。COMPLETE 不声称 strict legacy 曾执行 v19 格式化，也不把 absent profile反推成某个未取得的 exact EXE；未来未知非零格式必须按新profile保留 |
| LBA8 | 0x02E–0x03D | COMPLETE | `UsbOnlyInfo` fixed C-string terminator + zero suffix | current Windows/Linux producer均生成**恰好16字符** `%08x%08x`，目标32B header槽来自完整零初始化，故 byte16 是终止NUL、其后15B保持零；2020官方 `sub_10007DF0` 同样先清零完整临时标签结构，再对32B槽执行同一16字符格式化，因此 suffix同为 `00[16]`。strict legacy profile整槽 absent/zero，自然保持相同后16B | registration semantic reader完全跳过 UsbOnlyInfo；2020/current runtime最多结构复制该槽，没有 suffix 值相关 consumer；另一本机 v3.6.12.28 runtime 甚至只复制 slot首DWORD，直接跳过这16B suffix | committed strict originals 22/22 解密后 `+0x2E..+0x3D==zero[16]`，跨 current/legacy identity无反例；现有回归再显式锁定 universal suffix-zero | 16B不存在已知profile分叉，且 current/2020 producer零来源、legacy absent profile、跨代negative/structural consumer与实盘均闭合；未来若出现非零suffix必须新增profile，不得机械清零 |
| LBA8 | 0x03E–0x03F | COMPLETE | ElabOffset | `BuildSector8@diskfile.cpp:805` 写 `0x0080`；官方结构 `tagEdpUsbLableInfo.ElabOffset@edpdiskglobal.h:413` | `ReadSector8@diskfile.cpp:1102` 读取 WORD 并用 `decoded+ElabOffset` 构造 ELABEL 字符串 | 22/22原始盘=0x80，且22/22都指向 `<ELABEL>`；CI真实夹具锁定 | 2B 寻址语义、producer、consumer、实盘全部闭合 |
| LBA8 | 0x040–0x07F | COMPLETE | Reserverd[64] | Windows `sub_100148d0` 与 Linux `BuildSector8` 都先零初始化整个 header，再把未被其它赋值覆盖的 64B 原样复制到该区 | `ReadSector8(UsbLabelParam&)` 直接越过该区定位 `ElabOffset` 指向的 ELABEL；raw reader 仅原样导出，不赋予业务语义 | 22/22原始盘解密后64B全零；CI原始夹具锁定 | 官方结构名、零初始化 producer、negative consumer 和实盘全部闭合为 reserved-zero 区 |
| LBA8 | 0x080–0x1FF | COMPLETE | **LBA8 dynamic ELABEL + encrypted backing + preserved tail** | Windows `sub_100148d0` 与 Linux `BuildSector8@0x1D602` 独立同构：都只把 17-key ELABEL+NUL 写到 `+0x80`，按 `(logical_len / 16 + 1) * 16` 对现有 LBA8 **原地**加密；两函数都不清 caller output。Windows `RegsiterUsb` 在此之前已先读完整13扇区到 `var_500`，再把旧 `LBA8` 指针直接传入 writer，因此 ELABEL NUL 后到 `encrypted_len` 的字节属于既有 backing、会随前缀一起加密，`encrypted_len..0x1FF` 则完全 preserve-existing | Windows 注册侧 `cemsusbregsiter.dll::sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 独立只回填同一 7-key 集合：`registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit`。但运行时 `out_raw_data/EdpEDiskCtrl.dll::sub_10016260` 是更完整的 consumer：`runtime EdpEDiskCtrl reader parses all 17 ELABEL keys`，将 `GLab/Indus/Orgcd/Org/Unit/Dept/User/Alarm/Autonum/Label/Rmark/VOL0/1/2/VOLC0/1/2` 全部写入 `tagEdpUsbLableInfo` 对应槽。NUL 后块内 backing 和块外 tail 不赋予字段语义；inspect 的 nonzero in-block backing、nonzero physical tail、16B-aligned extra-block 三个回归锁定兼容读取边界 | 严格22份原始盘：`logical_end=0x148..0x183`、encrypted prefix=`0x150..0x190`，22/22 正文均为同一17-key顺序且十个当前 compatibility 键为空；committed original 门禁同时覆盖多个动态长度，并验证当前实盘 `ELABEL NUL 后到 encrypted_len 的字节属于既有 backing` 的观测值目前为零。所有观测物理 tail 也为零，但两者的零值都不是协议要求 | 384B 的动态状态机已逐类闭合：正文/终止NUL由writer拥有；块内剩余字节为 encrypted preserved backing；块外为 unencrypted preserved tail。7个核心键有注册侧 Windows/Linux 双 consumer，运行时 EdpEDiskCtrl 又对全部17键提供完整结构 consumer；实盘无已知 body/profile 分叉。因此整段384B升 COMPLETE；这不要求 backing/tail 恒零，未来非零值必须按边界原样保留 |
| LBA9 | 0x000–0x003 | COMPLETE | EETU magic | `CUsbRegsiter::SetTempUse` 构造 `EETU`；`WriteTempUseInfo` 可运行时回写 | `ReadTempUseInfo` 必须校验 EETU magic | 20个非零LBA9原始样本 | 完成 |
| LBA9 | 0x004–0x00B | COMPLETE | ullBTime | Windows `SetTempUse` 从开始时间字符串解析为64位值；空/短字符串保持0 | Linux `CheckTempUse` 与 `time(NULL)` 比较；非零且 now < ullBTime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 开始时间下界语义闭合，0表示不启用该下界 |
| LBA9 | 0x00C–0x013 | COMPLETE | ullETime | Windows `SetTempUse` 从结束时间字符串解析为64位值；空/短字符串保持0 | `CheckTempUse` 与 `time(NULL)` 比较；非零且 now > ullETime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 结束时间上界语义闭合，0表示不启用该上界 |
| LBA9 | 0x014–0x017 | COMPLETE | useCount | `BusManageImp::WriteNormalULabel` 普通模式从请求 `+0x947` 取次数；特殊 OutManage-off 模式明确写 `0xFFFFFFFF`；`CUsbRegsiter::SetTempUse` 再将 request+0x40 原样写 EETU+0x14 | Linux `CheckTempUse`：`0xFFFFFFFF` 不递减/不回写；0=次数耗尽；其它正值减1并 `WriteTempUseInfo` 回写 | 20/20原始EETU=0xFFFFFFFF；真实CI夹具回归 | 4B 次数控制及无限次数哨兵完全闭合 |
| LBA9 | 0x018–0x07D | COMPLETE | **EETU reverse[0..101] writer-uninitialized opaque backing** | Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 正式定义 `reverse[104]@+0x18`。Windows `CUsbRegsiter::SetTempUse` 先清零 EETU magic 后124B，再固定 `memcpy(EETU+0x18, request+0x44, 0x66)`。继续回溯 `BusManageImp::WriteNormalULabel/sub_100A99F0` 原始机器码确认：SetTempUse 请求 `&var_BD4` 只写 `begin[32]`、`end[32]`、`useCount@+0x40`；此前 `sub_1009BAD0` 的 `ECX=&var_9EC` 只清 `ebp-0x9EC..-0x56`，并不覆盖位于 `ebp-0xBD4` 的请求对象，因此 `request+0x44..+0xA9` 102B 是明确的 writer-uninitialized caller backing，而非 reserved-zero | 当前 Windows runtime `ReadTempUseInfo/sub_10013490` 解密并把完整0x80 EETU缓存到 `CEdpDiskControl+0x1076`；行为代码 `GetTempUseInfo/CheckTmpUse` 只直接读取 `ullBTime/ullETime/useCount`（显式对象引用截止 `+0x108A`，reverse 从 `+0x108E` 开始无独立读点）。登录递减次数后 `WriteTempUseInfo/sub_10013770` 将完整0x80重新加密写回，所以 **runtime preserves the full 0x80 EETU while only consuming time/useCount**。Linux `CheckTempUse` 同样不读取 reverse | 20/20原始非零LBA9的 EETU `reverse[104]` 全零；committed original 门禁 `real_eetu_temp_use_limits_match_the_official_unlimited_profile` 锁定该观察，扩展历史只读扫描也未发现非零 reverse | 前102B的“值不稳定/当前恰零”本身不是未知字段：正式边界、writer-uninitialized来源、runtime透明保存/negative semantic consumer与原始实盘均闭合，按与 LBA6/LBA8 backing 相同口径升 COMPLETE。未来若出现非零 reverse 必须原样保留，不得清零或赋予隐藏字段语义 |
| LBA9 | 0x07E–0x07F | COMPLETE | reverse[102..103] zero tail | `SetTempUse` 对 EETU +0x04..+0x7F 先整体清零，随后从 +0x18 只覆盖0x66B，即最后覆盖到 +0x7D；因此 +0x7E/+0x7F 在所有 current writer 路径都保留显式零初始化 | Linux `CheckTempUse` 只读取 ullBTime/ullETime/useCount，对 reverse[104] 完全无业务读取；运行时回写只修改 useCount 并保留其余字节 | 20/20原始EETU均为 `00 00`；CI门禁 `lba9_eetu_final_two_reverse_bytes_are_writer_zero_padding` | 2B 满足 explicit-zero producer + negative consumer + real-device evidence，可严格升 COMPLETE；不得把前102B一起升级 |
| LBA9 | 0x100–0x103 | COMPLETE | SAPF magic | 旧writer恢复模板 | `UDiskLabelRepair::Repair0Sector` | 14样本 | 完成 |
| LBA9 | 0x104–0x113 | COMPLETE | MBR恢复entry | writer保存16B entry | repair直接写回 LBA0 0x1BE | 14/14 | 完成 |
| LBA9 | 0x114–0x11F | COMPLETE | **profile-overlap region：SAPF unowned trailing backing / long-User continuation** | SAPF profile 下历史 writer 虽未定位，但该12B没有独立字段 store 证据；真实 decoded tail 呈现零、`0xFFFFFFFE` 与多组 `0x77xxxxxx` 等典型 backing 形态。current long-User profile 则由 Windows/Linux/vrvaud 三套 `BuildSector6` 明确把 `User[28..NUL]` 连续写入 LBA9+0x100，最大155B User 的 first-party virtual writer 正例覆盖整个 `+0x100..+0x17F`，因此本12B在该 profile 下是确定的 User continuation payload | SAPF `sub_10008550` 虽 structural-copy 解码完整32B，但只校验 magic；上层双SAPF一致性只比较 decoded `+0x04/+0x08/+0x0C/+0x10`，`Repair0Sector/sub_10008620` 又只把这四个DWORD写回 LBA0 `+0x1BE..+0x1CD`，对 `+0x14..+0x1F` 12B零读取/零写回。long-User profile 则由 `ReadSector6` 固定从 LBA9+0x100 取0x80B continuation回填 User | 14份真实SAPF中 decoded trailing 12B 至少5种 profile，CI同时要求 zero/nonzero 两类都存在，排除“协议固定零”；最大155B official virtual long-User fixture又给出同一物理12B的 active payload 正例并完整 round-trip | 两种已知 profile 均已闭合：SAPF下是**unowned trailing backing + negative semantic consumer**，long-User下是 active continuation。历史 SAPF 最初 backing 来源不再是业务语义 blocker；兼容实现必须按 profile 解释，禁止把SAPF tail强制清零或把long-User字节当SAPF字段 |
| LBA9 | 0x120–0x17F | COMPLETE | long-User continuation remainder / otherwise-preserved backing | 三套 current writer 的机器码已逐一核对为同构：Windows `0x1001417D..0x100141B1`、Linux `0x1CDA3..0x1CDE2`、vrvaud `0x10119082..0x101190B5` 都计算 `out + 3*sector_size + 0x100`，写入 `User[28..NUL]`，长度=`strlen(User)-27`；User固定数组容量0x9C允许最大155B C-string，因此最大写长恰为128B并止于+0x17F。短User路径不触碰该区，保留既有 backing | Windows `ReadSector6/sub_100152A0` marker 分支固定读取 LBA9+0x100 的0x80B到 User continuation；Linux reader同构。SAPF consumer明确止于+0x11F，EETU/EPPE运行时也不消费+0x120..+0x17F | first-party Windows `BuildSector6` 隔离执行：LBA9预填0xCC，最大155B User 后 `+0x100..+0x17F` 被精确写满“127B剩余字符+NUL”，`+0x000..0x0FF` 与 `+0x180..` 仍保持0xCC，证明真实写边界；同一 wire 再由 official `ReadSector6` 动态执行完整 checksum/rolling/marker path，输出155B原串完全一致。CI fixture锁定。physical originals虽无长User，但14/14 SAPF盘与独立SanDisk在该区为零的 preserve profile仍保留 | 96B 已有 first-party writer→wire→consumer 正向闭环，并证明 otherwise-preserve 生命周期；virtual fixture与physical census继续分栏，因此本段升 COMPLETE |
| LBA9 | 0x180–0x183 | COMPLETE | EPPE magic | `SetPassInfoEx` | `ReadPassExInfo` | 6样本 | 完成 |
| LBA9 | 0x184–0x187 | COMPLETE | minimum password length | writer限制6..19 | `ReadMinPassLenInfo` 返回该DWORD | 6/6=8 | 完成 |
| LBA9 | 0x188–0x1FF | COMPLETE | **EPPE writer-owned zero tail** | PE机器码 `SetPassInfoEx/sub_1003ADD0`：先校验输入DWORD为6..19，再对 EPPE `+0x04..+0x7F` 124B整体清零，写 magic，随后明确 `EPPE+0x04=*arg0`；因此 `+0x08..+0x7F` 120B 在 current writer 中为显式零 | 正式注册侧 `CUsbRegsiter::GetPassInfoEx` 解密并校验 EPPE 后只执行 `*out = *(decoded+0x04)`；独立 `modfilesyscheck::ReadMinPassLenInfo` 同样只消费 magic/+0x04。两套 `EdpDiskCtrl` 虽保留可搬运完整0x80B的 `ReadPassExInfo/GetPassExInfo` compatibility helper，但其外层对象由唯一两个 DLL export（Create/Release）创建，current factory vtable `0x1008021c` 不包含该 helper，反编译交叉引用也仅见实现/相邻 thunk，未形成 current 产品语义消费路径 | 严格22份中6份EPPE；6/6 minPassLen=8 且解密后120B全零；CI门禁 `real_eppe_samples_keep_the_current_writer_zero_tail` | 120B 的 current producer、两个独立 semantic reader 的 negative consumer、当前公开接口边界与原始实盘均闭合，按 writer-owned zero region 升 COMPLETE。COMPLETE 不授权清洗未知历史非零 profile：兼容读取若未来遇到非零 tail 应保留/报告，而不是据此臆造业务字段 |
| LBA9 | 0x080–0x0FF | PARTIAL | long-Dept continuation slot（current join=60 / CEMS2.0 legacy join=59） | Windows `BuildSector6/sub_10013FD0`、Linux `BuildSector6@0x1CAAC` 与 `vrvaud_c::sub_10118ED0` 三套 current producer一致：Dept长度>=64时在 LBA6+0写 `0x40245E2A + Dept前60B`，再把 `Dept[60..NUL]` 写入 LBA9+0x80；若未触发长Dept则该builder不覆盖此区。对本机 VRV 二进制按 `0x40245E2A` 做全量常量指纹后，producer 命中仅见 current `cemsusbregsiter.dll` / `vrvaud_c.dll`，且两者都复制60B；历史 v19.11.4.1 直接 LBA6 writer `fcn.10006370` 则完全没有 marker 分支 | current Windows `ReadSector6/sub_100152A0` 先从 marker 后复制完整 `0x3C=60B` inline prefix，再检查 `prefix[59]`：0时 continuation 接 Dept+59，非零时接 Dept+60。CEMS2.0 `cems/Edp/fileophook.dll` `fcn.10022f80@0x10022F80` 的 marker 分支在复制60B inline 后，无条件把0x80 continuation 写到目标 `+0x3B`；x64 build同构，PDB路径明确落在 `\\SVNRoot\\vrvrsms2.0\\Cems2.0\\trunk\\modCems\\Bin\\FileOpHook*.pdb`。因此这里闭合的是 **CEMS2.0 join59 reader only**，不能反推同代 writer | 严格22盘恰有8盘 marker+continuation：4盘 join=60、continuation含NUL共17B；4盘 join=59、continuation含NUL共18B；两组均重建为相同76B合法GBK Dept。CI `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 锁定 inline[59]!=0 -> 60、inline[59]==0 -> 59 与两种真实profile | current producer、current自描述reader、CEMS2.0固定join59 reader及实盘均闭合；**all locally available marker writers use 60**，所以 exact blocker 进一步收缩为尚未取得的 CEMS2.0 同代 legacy join59 writer/profile-selection。128B继续PARTIAL |
| LBA10 | 0x000–0x003 | COMPLETE | EESI magic | `SetEdpEdiskSetInfo` 强制写 `EESI` magic，并只加密/覆盖前0x80B | `GetEdpEdiskSetInfo` 解密前0x80B并首先校验 `EESI` | purpose-specific Netac pre-write physical capture SHA-256 `3c7e795b...` 在 `CRC32(device_id)=0x5088EE37` 下解出 `EESI`；19+1 general census 则保持 absent-zero profile | producer、reader、正向/缺省两类真实物理 profile 均闭合；purpose-specific capture 不参与 general census 计数 |
| LBA10 | 0x004–0x007 | COMPLETE | **UsbSuspensionWnd lifecycle/control flag** | 两套独立 `EdpEDisk.exe`（SHA-256 `cfa13177...` / `dc71c300...`）启动时都先将完整0x80B EESI缓冲清零并显式写 `+0x04=1` 后调用 vtable `+0x20 GetEdpEdiskSetInfo`；同两套程序的卷标设置对话框 `IDOK` handler zero-initializes the full 0x80B EESI payload，只填 `+0x08/+0x18` 两个卷标，再经 vtable `+0x24 SetEdpEdiskSetInfo` 保存，因此该 producer 路径明确写 `+0x04=0`；底层 setter 只强制 magic，其余DWORD原样落盘 | 两套程序均加载 `UsbSuspensionWnd.dll` 的 `Show/Destroy/SetParentWnd`：读回 EESI 后 `+0x04==0` 路径调用 `Destroy`；自动登录成功后 `+0x04!=0` 且 suspension-window helper 已初始化时，进入刷新/`Show` 链；`UserLogin` 自身不把该DWORD当卷标开关 | provenance-audited Netac pre-write capture 解出 `+0x04=1`；general census 提供 absent-zero profile | 4B边界、0/1官方 producer、值相关 consumer 与正向物理值全部闭合 |
| LBA10 | 0x008–0x017 | COMPLETE | Share/type2 volume label | `SetEdpEdiskSetInfo` 原样复制调用者结构前0x80并加密写入；默认 reader 初始化为GBK“交换区” | `CEdpDiskControl::UserLogin` 将该槽赋给本地 string；type2分支传给 `SetVolumeLabelA` | provenance-audited Netac pre-write capture 解出固定16B槽 `bdbbbbbbc7f8...` = GBK“交换区” | 16B边界、writer、业务 consumer 与正向真实盘值闭合 |
| LBA10 | 0x018–0x027 | COMPLETE | Encrypt/type4 volume label | 同上；默认 reader 初始化为GBK“保密区” | `UserLogin` type4分支传给 `SetVolumeLabelA` | provenance-audited Netac pre-write capture 解出固定16B槽 `b1a3c3dcc7f8...` = GBK“保密区” | 16B边界、writer、业务 consumer 与正向真实盘值闭合 |
| LBA10 | 0x028–0x07F | COMPLETE | EESI caller-owned compatibility extension | 两个独立 EESI `EdpEDiskCtrl` build 的 Get/Set 证明完整0x80B structural round-trip；current official UI profile对这88B写零 | current callers不读取这88B；更老两代无EESI接口 | provenance-audited Netac EESI-enabled capture与独立旧 SanDisk 正例均为88B全零 | 生命周期边界已闭合：caller可round-trip扩展字节，当前官方caller写零且业务consumer不解释；COMPLETE不表示未来扩展必须恒零 |
| LBA10 | 0x080–0x1FF | COMPLETE | cross-generation unowned preserve/ignore physical tail | 两个独立 EESI build的setter只覆盖前0x80B并原样回写后0x180B；旧两代没有该tail producer | getter只解密/返回前0x80B，所有已审consumer均忽略后384B | 仓库现行20份唯一金标（19加密+1真实免密）均为384B零；官方writer的preserve行为独立闭合 | COMPLETE表示该384B不属于EESI payload且必须preserve，不表示协议要求恒零 |
| LBA11 | 0x000–0x003 | COMPLETE | DRKB magic | `CDataSecrity::RandBuffer256` 先写 DRKB | `ReadSector11` 首先校验 DRKB | 22/22 | 完成 |
| LBA11 | 0x004–0x0FF | COMPLETE | random252 | `RandBuffer256`: `srand(time(NULL)); rand()%255` 共252B | `DataEncrypt/DataDecrypt` 将整个 DRKB块纳入 CRC32 密钥输入 | 22/22；均无0xFF；7 CI夹具回归 | 每字节都是密钥扰动材料，来源和消费闭合 |
| LBA11 | 0x100–0x103 | COMPLETE | PDKB magic（解密后） | `BuildSector11` 构造 PDKB plaintext | `ReadSector11` 解密后必须校验 PDKB | 22/22 | 完成 |
| LBA11 | 0x104–0x1FF | COMPLETE | 加密的 UID + zero fill | 正常注册 writer：`cemsusbregsiter.dll::sub_10014720` 以 `DISK_GEOMETRY_EX.DiskSize` 为 `ullSize`；repair writer：`UDiskLabelRepair.dll::CLabelRepair::Repair -> sub_10008950(ReWrite11Sector) -> sub_10003A40`，其 `disk_info+0x30/+0x34` 由 `IOCTL_DISK_GET_DRIVE_GEOMETRY(0x70000)` 返回的 `DISK_GEOMETRY` 经 `sub_10019EF0` 64-bit multiply 计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector` 后生成 LBA11 | 正常 consumer：Windows/Linux `ReadSector11` 以 exact DiskSize 解密；repair consumer：`CLabelRepair::Repair -> sub_10008820 -> sub_10003BD0` 用同一 CHS `disk_info+0x30/+0x34` 校验 LBA11，失败才进入 ReWrite11Sector | 严格22份：21/22 exact DiskSize，1/22 Aigo U335 rev_pmap 为 CHS；另有同一 Aigo rev_pmap 的独立真实 exact-size LBA11 捕获，证明 profile 取决于 writer 路径而非硬件；22/22 解密后 UID 正确且 UID 后全零 | 两种已观测 wire profile 的 producer、consumer、容量算法和实盘均闭合；因此后半252B升级 COMPLETE |
| LBA12 | 0x000–0x11F | COMPLETE | 3×96B EDPF | Windows/Linux writer；current `CEMSUsbRegsiter.dll::CreatePartitions` 的 mode1/2/3 均已在隔离 Unicorn 虚拟盘 harness 中原生执行，最终 wrapped-key callsite 分别命中 `0x1003ED77/0x1003ED44/0x1003EDA7` 各1次 | 登录/挂载/兼容链大量消费；mode1=A7F0/A6B0、mode2=SM4-ECB、mode3=AES-128-ECB，统一以 `MD5(effective_password)` 包装16B file-key并以 FileKeyCRC 校验 | 22盘 real-device 仍只有mode2；另有三份 **official-binary virtual writer** 512B正向fixture，三种mode在同一输入下解出同一16B file-key并命中同一 FileKeyCRC | 主表所有字段生命周期均闭合；虚拟writer fixture明确不计入real-device census，但它是官方一方二进制直接生成的wire sample，不是edpcli合成输出 |
| LBA12 | 每条entry +0x004–+0x007 | COMPLETE | packed entry-local `Version` compatibility metadata | current `CreatePartitions` 对3×96B整表先 `memset(0,0x120)`，后续没有任何 `+0x04` 覆盖写，故 current producer 三条均为0 | Windows 主运行时对96B entry 的登录/改密/挂载消费集中在 PartionType、NeedEncrypt、StartSector、PartionSize、UserKeyCRC、FileKeyCRC、wrapped key、EncryptMode；未发现 Version 分支。Linux packed `LayoutParsedata/Volume::GetPartitionHeader/PartitionHeader` 只结构缓存整条 entry，协议版本由14B pass-info Version 决定，不读取 entry-local Version | committed original fixtures 22/22×3 entry 均 Version=0；全树22份完整历史备份用当前 edpcli 解密器复算仍 `Version=(0,0,0)` 22/22 | 正式 ABI 字段，但在已覆盖 producer/runtime 中是 dormant compatibility metadata；3×4B=12B COMPLETE。不能再把 `+0x08 PartionCount` 误标为 Version，也不禁止未来新 writer profile 写非零 |
| LBA12 | 0x010–0x013 | COMPLETE | entry0.NeedDisturb compatibility gate | `CUsbRegsiter::CreatePartitions` 写入 entry0；Linux `edpdiskglobal.h:82` 定义字段 | `vrvaud_c::NewCheckDisTurbUsb(*)` fallback 在 `Format.cpp:0x3CE/0x380` 直接以该 DWORD 非零判 success | 22/22原始盘=1；20个entry0 type1、2个type2；7 CI夹具锁定 | 完成的是 entry0 兼容门控行为；其它 entry 的 NeedDisturb 不随之升级 |
| LBA12 | entry1/entry2 +0x010–+0x013 | COMPLETE | positional `NeedDisturb` compatibility metadata | current `CreatePartitions` 的 current profile 由整表零初始化后显式形成 `(1,1,0)`：entry0/entry1=1，entry2保留0；字段随96B entry结构保存 | Windows 行为 consumer 只命中 entry0 fallback gate；UserLogin/挂载参数链不读取 entry1/entry2 NeedDisturb。Linux packed runtime把 NeedDisturb/NeedEncrypt 一起缓存进 `PartitionHeader`，但后续加解密/文件系统路径无 NeedDisturb 值相关读取 | committed original fixtures 22/22 为 positional `(1,1,0)`；全树22份完整历史备份经当前 edpcli 正式解码仍22/22相同 | 8B 闭合为 current positional compatibility profile + structural-preserve + cross-platform negative semantic consumer；COMPLETE 不把 entry1/2 解释成 entry0 的 MBR 扰动行为 |
| LBA12 | 每条entry +0x038–+0x047 | COMPLETE | wrapped file-key material | current writer 的三条可达分支已经闭合：mode1 `sub_10001190`=A7F0、mode2 `sub_100036E0/sub_10011010`=SM4-ECB、mode3 `sub_1000FC10`=AES-128-ECB；key均来自 `MD5(effective_password)`，mode byte写入 `+0x58` | Windows `UserLogin/sub_10028AB0` 对1/2/3分别解包并统一做 FileKeyCRC；mode3 CRC失败还有按mode1重试的历史兼容；Linux当前checker明确消费mode1/2 | 22盘44条加密real-device entry全部mode2；隔离执行官方 `CreatePartitions` 又得到mode1/2/3三份确定性512B LBA12正向fixture：密码 `ProofPass1!` 三种wrapped16分别经 A6B0、独立标准SM4-ECB、独立标准AES-128-ECB恢复同一 `147196f5a2ec7912edf13f75d766cb42`，三者CRC均=`0xFF4C1D36`且等于盘内 FileKeyCRC；CI锁定 | **first-party runtime positive-wire closure**：没有把虚拟fixture冒充physical capture；但官方writer原生执行、UI→crypt→mode可达链、独立reader round-trip与既有real-device mode2共同消除了该16B的语义不确定性，因此3×16B从PARTIAL升COMPLETE |
| LBA12 | 每条entry +0x048–+0x057 | COMPLETE | `EncryptFileKey32[16]` cross-generation compatibility slot | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`；current packed writer 后续只写 wrapped16 `+0x38..47` 与 mode `+0x58`，所以该16B保持显式零。旧72B `tagEdpPartionInfo` **根本没有**该槽；Linux checker 的 old→new `GetPartionFromOld` 也只把旧8B key搬到 natural `+0x40`，不填 natural `+0x50 EncryptFileKey32[16]` | 104B checker DWARF正式命名 `EncryptFileKey32[16]@+0x50`，但 `DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0` 都不读取它；packed `libedpedisk.so` 会在按值构造时结构缓存完整96B，但严格按 `PartitionHeader` 符号边界审计，映射到对象 `+0x88/+0x90` 的两个QWORD只在构造器写入，后续没有值相关读取；正对照 wrapped-key 起点 object `+0x78` 被 SMS4/AES128/OldEdp decrypt 实际消费。Windows UserLogin/改密同样只消费 `+0x38..47/+0x58` | 严格22份原始盘全部现存 EDPF entry 共66条，`+0x48..57` **66/66全零**；CI `lba12_encrypt_file_key32_compatibility_slots_are_zero_in_original_entries` 锁定 | **LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure**：旧ABI无槽、新ABI正式保留名字、current producer显式零、跨Windows/Linux只结构搬运不参与算法、原始实盘全零。COMPLETE 表示“兼容槽生命周期/无当前业务语义”闭合，不把它误称 Reserved，也不禁止未来其它ABI结构性携带非零值 |
| LBA12 | 每条entry +0x059–+0x05F | COMPLETE | packed Reserved[7] | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`，后续只写至 +0x58；Linux DWARF正式字段名 `Reserved[7]` | Windows UserLogin/改密只消费 wrapped16 与 +0x58；Linux decrypt/改密同样不消费 Reserved | 22盘66/66 entry全零；CI原始夹具锁定 | **LBA12 packed Reserved[7] producer/negative-consumer closure**；与前面的 `EncryptFileKey32[16]` compatibility slot 分开建模 |
| LBA12 | 0x12A | COMPLETE | pass-info `bNoUsbChkPasSafe` | 与 LBA7 同一 current CreatePartitions 请求输入，LBA12 builder 保存同一 pass-info 字节 | `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接比较该字段并生成 SAFE6 show-policy byte +3；policy 经 `CreateSafe6TmpPolicyFile` 加密后被 `EdpEDiskBack` 与 `linuxedpedisk` 两套 `Safe6PolicyFile::GetSafe6Policy` 恢复 | 严格22份18×0+4×1，且22/22与同盘 LBA7 +0x0A相同；CI锁定双值/一致性 | 与 LBA7 同一逻辑字段、同一 producer/consumer 链，1B COMPLETE |
| LBA12 | 0x12C–0x12D | COMPLETE | dormant pass-info `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` compatibility bytes | 与 LBA7 共用同一个 current pass-info producer：完整14B先清零，写 LBA12 前只把 Version 切换为 `0x0206`，两个 period BYTE 保持0/0；DWARF 正式字段定义同样适用 | Windows old/current reader 对完整14B结构复制但不消费最后2B；Linux checker保存整个 pass-info，但实际解密/文件系统检查不读取这两个字段；独立 `vrvaud_c` backup policy 链已排除 | committed fixtures 逐盘与 LBA7 两字节一致且均0；历史去重 profile 跨 v0x0064/v0x0206 未见非零 | 与 LBA7 同一 dormant compatibility-field 生命周期闭合；2B 升 COMPLETE，不推导未实现的时间单位 |
| LBA12 | 0x12E–0x16F | COMPLETE | post-table zero initialized padding | writer整块零初始化且不覆写 | 主reader不消费该区 | 22/22解密为零 | producer+negative consumer+实盘闭合 |
| LBA12 | 0x170–0x1FF | COMPLETE | post-table zero initialized padding / continuous-cipher tail | Windows current `sub_10014F30` 分配 `sector_size+1` 后整块清零，v0x206 只复制 `0x120+0x0E=0x12E` 结构字节，随后加密整扇；Linux `BuildSector12` 同样先把 `sector_size+1` 全零再只复制表/表尾并整扇加密 | Windows `sub_100160B0` 固定解密0x200B，但只复制 `0x120+0x0E` 返回；Linux主reader同样只解释表/表尾，不消费 post-table 区 | 严格22份 + 独立SanDisk 解密后 `0x12E..0x1FF` 全零；既有连续密文门禁同时证明 `0x170..` 不是RAW尾 | 与 `0x12E..0x16F` 同属一个 post-table zero padding 区；此前 PARTIAL 行是主表 stale 状态，总进度表早已把这144B计入 LBA12 的393B COMPLETE，因此本次只纠账、不重复增加总数 |
<!-- FIELD_LEDGER_END -->

### 4.0 LBA3：EDP 只保留的外部制造/MP 扇区

这一扇区此前只因为 21/22 全零、1 份带 `this is mp mark` 而记为 UNKNOWN。
本轮没有沿用旧文档结论，而是重新从官方写链、官方 reader 集合和 22 份原始盘三条线核对。

Windows 当前注册入口
`CUsbRegsiter::RegsiterUsb / sub_1003b560 @ usbregsiter.cpp:0x915..0xA04`
先调用 `ReadSectorData(..., count=0x0D)` 把 LBA0–12 整段读入 staging buffer。
SAFE6 分支随后明确重建 LBA4、LBA6、LBA8、LBA11，并由分区/EDPF helper
处理其它协议扇区；整个函数没有 LBA3 builder。最后仍以同一个 staging buffer
调用 `WriteSectorData(..., count=0x0D)`。所以对 LBA3 而言，官方 EDP 注册 writer
不是“生成零扇区”，而是：

```text
read existing LBA3
    -> no EDP mutation
    -> write the same LBA3 bytes back with the 13-sector batch
```

Linux 当前 `libcemsfilesyscheck.so` 提供
`BuildSector0/4/6/7/8/11/12`、`BuildSector0/1/2_Gpt` 以及
`ReadSector4/6/8/11/12`，独立不存在 `BuildSector3` / `ReadSector3`。
Windows 当前注册、登录和修复组件也未发现 LBA3 payload 解析路径。
因此 current EDP 已经给出 **preserve + 不解析** 的明确协议边界。为验证该边界并非
current-only，本轮又对 historical v19.11.4.1 做了固定 SHA-256 的全 DLL seek 审计：
31 个 `SetFilePointer` 调用点中，可恢复为 `sector_size × N` 的固定倍率集合精确为
`{1,2,4,6,7,8,12}`，不存在 `N=3`；SAFE6 `virtual_56@0x1000CC50` 也不直接调用
generic absolute-seek wrapper。可重放脚本为 `scripts/protocol/audit_v19_lba3_preserve.py`。
另有 2021 `UDiskLabelRepair.dll` 的 `RepairSafe6Label/RewriteSafe6BakLabel` 只在
LBA4-LBA12 与尾部镜像间复制9扇区，同样没有把 LBA3 纳入 repair payload。

22 份原始生成参考重新逐字节统计：

- 21/22：LBA3 512B 全零；
- 1/22：Kingston DataTraveler 3.0 非零；
- 该唯一非零扇区并不只是尾部字符串：
  - `+0x001 = 0x01`；
  - `+0x020..+0x027 = b5 7e 9c 45 00 80 00 14`；
  - `+0x1F0..+0x1FF = "this is mp mark\\0"`；
  - 其它字节为零；
- 同 VID/PID 的另一份 Kingston 原始盘 LBA3 仍为全零。

本轮又把范围扩到 `nopwd_tool/backup` 与 `utils/backup` 两个目录的60份 `.bin`
历史/真实快照做只读 census；该扩展集合包含历史/转换状态，只用于 profile 发现，
不改变上述22份 strict generation reference 的计数。非零 LBA3 只有3份，并形成
**两个不同的 MP payload profile**：2026-08-03 两份逐字节相同的 Kingston 快照
使用 `+0x020..027=a8 82 a4 22 00 20 02 16`；strict 2026-09-03 Kingston 使用
`+0x020..027=b5 7e 9c 45 00 80 00 14`。两类都保持 `+0x001=01` 和
`+0x1F0..1FF="this is mp mark\\0"`，而同型号其它快照还存在整扇全零 profile。

新增历史 profile 的原始 LBA3 SHA-256 为
`a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e`；仓库夹具
`tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex` 逐字节锁定。
回归 `lba3_mp_marker_has_multiple_real_historical_payload_profiles` 明确要求两个
marker profile 的中间8B不同，防止把任一单盘的值错升成全局固定模板。

因此不能把 `this is mp mark` 单独建模成 EDP 字段，也不能把21个零样本解释成
“协议规定全零”，更不能把 `+0x020..+0x027` 建模成单一固定常量。相反，跨 current
Windows、Linux、v19.11.4.1 与 2021 repair 的证据共同限定了 **EDP 自己对这512B没有
payload ownership：只保留、不解释、不重建**。这与 LBA4 unowned backing、LBA5 opaque
preserve 的 COMPLETE 口径一致，因此 LBA3 512B 现按
`manufacturer-owned opaque MP metadata / EDP preserve-only sector` 升为 COMPLETE。

这里的 COMPLETE 只关闭 **EDP LBA0-LBA12 协议语义**；Phison exact host serializer、
controller/firmware 私有字段定义仍是 manufacturer provenance 开放问题。未来遇到任何未知
非零 LBA3，兼容实现都必须逐字节保留，不能因为当前多数样本为零而清洗。

### 4.1 LBA8 ElabOffset：2B 完整闭环

官方结构来自 Linux DWARF：

`tagEdpUsbLableInfo @ edpdiskglobal.h:403`

```text
+0x00 Flag
+0x04 cbSize
+0x08 ToolVersion[4]
+0x0C Labversion
+0x10 writeTime
+0x14 HDSerialInfo
+0x18 MacInfo[6]
+0x1E UsbOnlyInfo[32]
+0x3E ElabOffset   // WORD
+0x40 Reserverd[64]
+0x80 UsbLabel / ELABEL storage begins
```

producer：

`CLabelManage::BuildSector8 @ diskfile.cpp:805`

```text
EightSecInfo = zero_initialized()
EightSecInfo.Flag = "LLGB"
EightSecInfo.ElabOffset = 0x80
...
copy ELABEL string to byte[0x80]
```

consumer：

`CLabelManage::ReadSector8 @ diskfile.cpp:1102`

```text
decrypt(sector8)
if header.Flag != "LLGB":
    return error

offset = u16(header + 0x3E)
elabel = C_string(header + offset)
parse ELABEL key/value pairs
```

原始实盘验证：

- 22/22：`ElabOffset == 0x0080`；
- 22/22：`decoded[ElabOffset..]` 以 `<ELABEL>` 开始；
- CI 真实夹具新增同一断言。

因此 `LBA8 +0x3E..+0x3F` 2B 从 PARTIAL 升级 COMPLETE。
其它头字段即便已有官方名称，也不会因为与它相邻而自动升级。

#### LBA8 static version/writeTime/reserved header

本轮继续对同一个 `tagEdpUsbLableInfo` 逐字段追踪，而不是把整个 header 一次性
标成“已知”。Windows `sub_100148d0` 与 Linux
`CLabelManage::BuildSector8@0x1D602` 的实际写序列一致：

```text
+0x08 ToolVersion[4] = 01 00 00 01
+0x0C Labversion     = 0x00000222
+0x10 writeTime      = monotonic milliseconds, low 32 bits
+0x40 Reserverd[64]  = zero-initialized
```

`writeTime` 的来源已独立闭合：

- Windows `sub_10016610` 只有一次 `GetTickCount()` 调用并直接返回；
- Linux `CLabelManage::GetTickCount@0x1FBAA` 调
  `clock_gettime(CLOCK_MONOTONIC)`，计算
  `tv_sec * 1000 + tv_nsec / 1_000_000`，返回低 32 位。

所以它不是 Unix 时间戳，也不是制盘日期，而是**制标进程所在系统自启动后的
单调时钟毫秒值**，发生 32 位回绕是协议允许的自然结果。

consumer 侧也重新核对：

- `ReadSector8(char*, UsbLabelParam&)` 解密后只检查 `LLGB`，
  读取 `+0x3E ElabOffset`，随后从该 offset 解析 ELABEL；
  不读取 ToolVersion、Labversion、writeTime 或 Reserved；
- `ReadSector8(char*, BYTE*)` 在检查 LLGB 后只是把完整 512B 解密结果
  `memcpy` 给调用者，并返回 `+0x04 cbSize`，同样不解释这些字段。

22 份原始参考重新解密统计：

- ToolVersion：22/22 = `01 00 00 01`；
- Labversion：22/22 = `0x222`；
- writeTime：22/22 非零，且跨独立标签存在多值；
- Reserved[64]：22/22 全零。

因此上述 76B 可从 PARTIAL 升 COMPLETE。相邻的
`HDSerialInfo/MacInfo/UsbOnlyInfo` **没有跟着升级**：当前 writer 虽可解释
其当前写法，但 22 盘已经出现历史 profile 差异，尤其
`HDSerialInfo` 非零组和 `UsbOnlyInfo` 空/16位十六进制串并存；旧 producer
尚未闭合。

### 4.2 LBA10 两个 16B 字段：交换区/保密区卷标

旧分析曾把这两个槽解释成“数值/时间戳候选”。重新验证后该解释应废弃。

当前 Windows reader `GetEdpEdiskSetInfo -> sub_1000f930` 在输出结构初始化时：

```text
out + 0x04 = 1
out + 0x08 = "交换区"   // GBK
out + 0x18 = "保密区"   // GBK
```

若 LBA10 存在有效 EESI，则 reader 只解密前 `0x80`，并用实盘结构覆盖这份默认值。

writer `SetEdpEdiskSetInfo -> sub_1000fc70`：

```text
input->magic = "EESI"
plain80 = input[0x00..0x7F]
cipher80 = A6B0(plain80, CRC32(device_id))
read sector10
replace sector10[0x00..0x7F] = cipher80
write sector10
```

因此 `+0x08/+0x18` 的 producer 来源就是 API 调用者提供的两个固定 16B
文本字段，而不是 writer 内部再派生的数据。

更关键的是 `CEdpDiskControl::UserLogin` 的实际汇编数据流：

```text
GetEdpEdiskSetInfo(&info)

shareLabel   = string(info + 0x08)
encryptLabel = string(info + 0x18)

if current_partition.type == 2:
    SetVolumeLabelA(drive, shareLabel.c_str())

if current_partition.type == 4:
    SetVolumeLabelA(drive, encryptLabel.c_str())
```

当前 build 中汇编可直接看到：

- `info+0x08 -> std::string @ ebp-0x74`；
- type2 分支将 `ebp-0x74.c_str()` 传给 `SetVolumeLabelA`；
- `info+0x18 -> std::string @ ebp-0x54`；
- type4 分支将 `ebp-0x54.c_str()` 传给 `SetVolumeLabelA`。

另一版 `out_raw_data/EdpEDiskCtrl.dll` 也存在同构 reader/writer 与
type2/type4 `SetVolumeLabelA` 路径，排除单版本偶然行为。

> **2026-09-21 current-gold 纠偏：** 本小节下方保留的 EESI 正例属于历史研究集合，
> 不是第1.1节现行金标。按现行去重规则重放后，19份唯一严格加密原盘 + 1份真实免密盘
> 的 LBA10 **20/20 全零**。因此 EESI 的静态 producer/consumer 边界仍可引用，
> 但 `LBA10+0x000..0x07F` 必须保持 PARTIAL，直到取得符合来源规则的真实启用样本。

历史研究集合当时的只读验证（仅作推导记录）：

- 21/22：LBA10 全零；
- 1/22：第三来源独立 SanDisk 原始加密盘存在有效 EESI；
- 该样本：
  - `+0x08..0x17 = "交换区" + NUL/zero fill`；
  - `+0x18..0x27 = "保密区" + NUL/zero fill`；
  - `+0x28..0x7F = 0`，但此事实仍不能把后续区域升级为 padding。

补充历史只读证据：`/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`
记录 `device_id="disk&ven_netac&prod_onlydisk&rev_0000"`，整份6656B快照
SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`。
该捕获的 LBA6 `crcUsbID`、LBA7/LBA12 EDPF 都与同一 device_id 自洽；LBA10
独立解密再次得到 `EESI/+0x04=1/交换区/保密区`，且 `+0x28..0x7F` 88B全零。
仓库测试夹具 `netac_onlydisk_20260804_lba10_head.hex` 锁定其前0x80密文。
由于这份历史快照尚未完成与主22份参考同等级的原始生成来源链审计，它只作为
额外真实 profile 证据，**不**扩大22份严格生成参考计数，也不据此把88B升级 COMPLETE。

仓库新增原始证据夹具：

`tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba10.bin`

SHA-256：
`240d04e7c97d300c5081f793d72850d49acbf5408bc0d8cf32de8eef7a5e8f02`

`+0x04..0x07` 本轮继续追到 `EdpEDisk.exe` 的真实外部调用者后已经闭合。
此前“未找到外部 vtable Get/Set caller”的结论需要废弃：

- `out_raw_data/EdpEDisk.exe`（SHA-256
  `cfa1317775801381b6ca51f13857d1e52506ff48ac4df94d7b9f91f742d3e4a1`）与
  `VRV/cems/ydcc/edpedisk.exe`（SHA-256
  `dc71c30041c4fe9fab277737116216502e9f6a610630a1a70b125c59441e1fd1`）
  在 `CEdpDiskDlg::OnInitDialog` 都先清零对象内 `0x80` 字节 EESI 缓冲，
  显式写 `buffer+0x04=1`，再经接口 vtable `+0x20` 调
  `GetEdpEdiskSetInfo(buffer)`；
- 同两套程序的卷标设置对话框 `IDOK` handler 则先清零完整 `0x80B` EESI，
  只把两个编辑框写入 `+0x08/+0x18`，然后经 vtable `+0x24` 调
  `SetEdpEdiskSetInfo`。因此这条官方 producer 明确把 `+0x04` 写成0，
  同时也明确把 `+0x28..0x7F` 写成0；
- 两套程序都加载 `UsbSuspensionWnd.dll` 并解析 `Show`、`Destroy`、
  `SetParentWnd`。重新读回 EESI 后，`+0x04==0` 的路径会调用 `Destroy`；
  自动登录成功后，`+0x04!=0` 且 helper 初始化成功时会进入
  suspension-window 的刷新/`Show` 链；
- 因而 `+0x04` 不是卷标文本开关，也不是固定常量。它是一个有明确0/1
  producer 和值相关 consumer 的 **UsbSuspensionWnd lifecycle/control flag**。
  这里按可观察行为命名，不宣称恢复了原厂 C++ 成员名。

历史集合中唯一启用 EESI 的 SanDisk 样本该 DWORD=1；其余21份没有 EESI。
该结论曾在旧口径下用于升级状态；**现行严格账本已经撤销这一升级**，因为这个正例
不属于第1.1节允许的金标来源。

因此以下项目是**旧口径阶段性状态，不是 current strict 状态**：

- `LBA10 +0x04..0x07` 4B → COMPLETE；
- `LBA10 +0x08..0x17` 16B → COMPLETE；
- `LBA10 +0x18..0x27` 16B → COMPLETE；
- `+0x28..0x7F` 88B → COMPLETE：闭合为 **EESI caller-owned compatibility
  extension**。底层 Get/Set 负责完整 structural round-trip；两套 official UI
  caller 当前写零且业务 consumer 不解释；未来非零扩展必须原样保留，不得机械清零；
- `+0x80..0x1FF` 384B → COMPLETE：按后续跨代审计已闭合为
  cross-generation unowned preserve/ignore physical tail，不能再按 padding 分析。

current strict 状态见第3节主账本：前0x80B统一为 PARTIAL；只有
`+0x80..0x1FF` 的 preserve/ignore lifecycle 保持 COMPLETE。

### 4.3 LBA9 EETU：时间窗口 + 使用次数 20B 完整闭环

旧分析只观察到 `+0x14 = FFFFFFFF`，一度把它当成未知 flag/固定值候选。
重新从 DWARF、Windows producer、Linux consumer 和原始实盘四条线交叉后，
这段已经可以精确命名。

Linux DWARF 官方结构：

`tagEdpEDiskTmpUse @ edpdiskglobal.h:481`

```text
size = 0x80
+0x00 flag       int
+0x04 ullBTime   uint64_t
+0x0C ullETime   uint64_t
+0x14 useCount   uint32_t
+0x18 reverse    char[104]
```

#### Producer：Windows `CUsbRegsiter::SetTempUse`

当前 Windows 二进制的真实机器码（不是反编译器猜测）显示：

```text
request:
    +0x00 begin-time string
    +0x20 end-time string
    +0x40 use count
    +0x44 extra/reverse material

EETU = zero[0x80]
EETU.flag = "EETU"

if begin-time string is present:
    EETU.ullBTime = parse_time(begin)
else:
    EETU.ullBTime = 0

if end-time string is present:
    EETU.ullETime = parse_time(end)
else:
    EETU.ullETime = 0

EETU.useCount = request.useCount
copy(EETU.reverse, request+0x44, 0x66)

encrypt EETU[0x00..0x7F] with device-id CRC key
read-modify-write LBA9 first 0x80
```

日志中保留原源码位置：

- `CUsbRegsiter::SetTempUse`：`usbregsiter.cpp:0x6DF`；
- 时间解析日志：`usbregsiter.cpp:0x702`；
- 写入结束：`usbregsiter.cpp:0x70F`；
- 实际 LBA9 读写 helper 路径：`usbregsiter.cpp:0x735..0x743`。

#### useCount 的上游来源

`BusManageImp::WriteNormalULabel` 的机器码把临时使用本地请求结构的
`+0x40` 明确初始化为：

```text
if OutManage switch is off:
    tempUse.useCount = 0xFFFFFFFF
    begin = default-empty
    end   = default-empty
else:
    tempUse.useCount = request + 0x947
    begin = request + 0x907
    end   = request + 0x927

SetTempUse(&tempUse)
```

另一条 `BusManageImp::GenloginCfg` 路径同样使用
`0xFFFFFFFF` 与正常请求次数二选一，进一步排除偶然常量。

#### Consumer：Linux `CheckTempUse`

Linux 运行时直接消费同一结构：

```text
info = ReadTempUseInfo()
now = time(NULL)

if info.useCount == 0xFFFFFFFF:
    # 无限次数，不递减，也不回写
elif info.useCount == 0:
    return COUNT_EXHAUSTED
else:
    info.useCount -= 1
    WriteTempUseInfo(info)

if info.ullBTime != 0 or info.ullETime != 0:
    if info.ullBTime != 0 and now < info.ullBTime:
        return OUTSIDE_TIME_WINDOW
    if info.ullETime != 0 and now > info.ullETime:
        return OUTSIDE_TIME_WINDOW

return OK
```

因此：

- `ullBTime` 是临时使用开始时间下界；
- `ullETime` 是临时使用结束时间上界；
- 两者都为0时不启用时间限制；
- `useCount=0xFFFFFFFF` 是**无限次数哨兵**；
- `useCount=0` 表示次数耗尽；
- 其它正值每次成功检查都会减1并写回。

#### 22份原始实盘验证

全量只读复核：

- 22份原始参考中，20份存在 EETU，2份 LBA9 该区全零；
- 20/20 EETU：
  - `ullBTime = 0`；
  - `ullETime = 0`；
  - `useCount = 0xFFFFFFFF`；
  - `reverse[104]` 当前实盘均为0。

CI 中的原始完整夹具也有多份 EETU，回归会逐盘解密并固定前三项。
`reverse[104]` 需要进一步拆开，不能再整体归一：

- `SetTempUse` 先把 magic 之后的 `0x7C` 字节全部清零；
- 随后 `copy(EETU.reverse, request+0x44, 0x66)` 只覆盖 reverse 的前102B，
  即 LBA9 `+0x18..+0x7D`；
- 最后2B `+0x7E..+0x7F` 从未被覆盖，因此始终保留 writer 的显式零初始化；
- 对 `BusManageImp::WriteNormalULabel` 的真实机器码做栈区扫描后，上游临时请求
  只显式写 begin/end/useCount；在调用 `SetTempUse` 前没有对
  `tempUse+0x44..+0xA9` 这102B做整体初始化。故前102B即使当前20/20为零，
  也不能解释成协议固定零 padding。后续进一步恢复 `sub_1009BAD0` 的真实 thiscall
  参数后，已确认该 memset 清的是 `&var_9EC` 另一对象，完全不覆盖 `&var_BD4`
  SetTempUse 请求，因此这102B的 producer 已从“疑似未初始化”闭合为明确的
  writer-uninitialized backing。

Linux `CheckTempUse` 对整个 reverse[104] 都不读取，只消费时间窗和 useCount；
后续 Windows runtime 审计又确认 `ReadTempUseInfo` 整0x80缓存、
`GetTempUseInfo/CheckTmpUse` 的显式读点只到 useCount，`WriteTempUseInfo` 再整0x80
透明写回。因此当前最终结论为：

- `reverse[0..101] / LBA9 +0x18..+0x7D`：**writer-uninitialized opaque backing**，
  producer + transparent-preserve/negative semantic consumer + real-device profile 已闭合，
  后续升级 COMPLETE；
- `reverse[102..103] / LBA9 +0x7E..+0x7F`：
  **explicit zero-init producer + negative consumer + 20/20 real-device zero**
  三条证据闭合，升级 COMPLETE。

本轮因此从 PARTIAL 升级：

- `+0x04..0x0B`：8B；
- `+0x0C..0x13`：8B；
- `+0x14..0x17`：4B；
- `+0x7E..0x7F`：2B；
- 此处记录的是最初阶段累计 **22B COMPLETE**；后续 reverse 前102B补齐 producer/runtime
  透明保存证据后，EETU 本段又新增102B COMPLETE，最终计数以主账本为准。

### 4.4 LBA5：512B 整区是 opaque write-protection probe scratch sector

旧分析文档曾把 LBA5 称作“写保护探测牺牲扇区”。这次没有沿用旧结论，
而是从当前/另一版运行时、当前注册 writer 和 22 份原始实盘重新验证。

#### Consumer：两版 EdpDiskCtrl 同构

当前版本：

`/VRV/cems/ydcc/edpediskctrl.dll.m::sub_10012490`

另一版：

`/out_raw_data/EdpEDiskCtrl.dll.m::sub_10015270`

两者逻辑完全同构：

```text
seek((metadata_base + 5) * sector_size)
read(one_sector, scratch)

if read succeeded:
    seek((metadata_base + 5) * sector_size)
    ok = write(one_sector, scratch)   // 写回刚刚读到的完全相同字节
    if !ok && GetLastError() == ERROR_WRITE_PROTECT /* 0x13 */:
        return WRITE_PROTECTED

return NOT_WRITE_PROTECTED
```

当前 `CEdpDiskControl::UserLogin` 的调用语境进一步确认该返回值用途：

```text
if ProbeSector5WriteProtection():
    global_read_only = 1
    log("UDisk Write-protected and can only read-only use!")
else:
    global_read_only = 0
```

也就是说 consumer 只关心“能否把**同一批字节**写回”，从未解析 LBA5
任何 offset、magic、flag 或 checksum。

#### Producer：注册 writer 保留既有 LBA5

`CUsbRegsiter::RegsiterUsb` 的当前 writer 流程是：

```text
buffer = zero[13 * sector_size]
ReadSectorData(existing LBA0..12, buffer)

rebuild known sectors:
    LBA4  <- BuildSector4
    LBA6  <- BuildSector6
    LBA7  <- CreatePartitions / BuildSector7 path
    LBA8  <- BuildSector8
    LBA11 <- BuildSector11
    LBA12 <- CreatePartitions / BuildSector12 path
    ...

# no builder targets base + 5 * sector_size

WriteSectorData(buffer, count=13)
```

额外审计：

- `CreatePartitions` 的真实目标是 `base + 7*sector_size` 与
  `base + 12*sector_size`；
- `BakupUsbSec` 只把当前 metadata 复制到磁盘尾部备份区，不修改内存 LBA5；
- SAFE1 builder 使用独立临时缓冲区，不存在 `base+5` 写入；
- 对当前与另一版 `EdpDiskCtrl` 的 raw-sector 引用搜索，`base+5`
  都只出现于上述“读后原样写回”probe。

因此官方 writer 对 LBA5 的规则不是“必须写零”，而是**preserve existing bytes**。

#### 22份原始实盘

只读全量复核：

- 22/22：LBA5 512B 全零；
- 22/22：整扇 SHA-256 都是
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；
- 但这只证明当前原始参考的实际内容，不把“零”提升为协议固定值。

因此本账本把整个 LBA5 的 **512B** 标为 COMPLETE，含义非常具体：

> LBA5 没有字段级 payload；整扇内容对协议是 opaque bytes，注册流程原样保留，
> 运行时仅把它作为可安全执行“读→同字节写回”的写保护探测 scratch 区。

如果未来发现非零原始 LBA5，这个结论不会失效：只要 writer 仍 preserve、
probe 仍原样写回，非零内容同样符合协议。CI 对当前原始夹具的“全零”断言只用于
防止样本集被悄悄替换，不把全零编码成生成规则。

### 4.5 LBA1/LBA2：GPT profile 的 first-party 动态正向证据

旧文档把 LBA1/LBA2 简写成“保留/全零”。重新验证后，这个描述不完整。

Linux 官方库直接保留三套 GPT builder：

```text
CLabelManage::BuildSector0_Gpt @ diskfile.cpp:1445
CLabelManage::BuildSector1_Gpt @ diskfile.cpp:1458
CLabelManage::BuildSector2_Gpt @ diskfile.cpp:1493
```

DWARF 恢复出的核心结构：

```text
GPT_Header       size = 0x200 (512B)
  +0x00 signature[8]       "EFI PART"
  +0x08 version
  +0x0C headersize
  +0x10 headercrc32
  +0x14 reserve
  +0x18 header_lba
  +0x20 backup_lba
  +0x28 pation_first_lba
  +0x30 pation_last_lba
  +0x38 guid[16]
  +0x48 pation_table_first
  +0x50 pation_table_entries
  +0x54 pation_table_size
  +0x58 pation_table_crc
  +0x5C notuse[420]

GPT_Partition    size = 0x80 (128B)
  +0x00 pationtype[16]
  +0x10 pationid[16]
  +0x20 pation_start
  +0x28 pation_end
  +0x30 pation_attr
  +0x38 pation_name[72]
```

`BuildSector1_Gpt` 的机器码明确执行：

```text
header = gpt_header template
header.backup_lba = total_lba - 1
header.pation_last_lba = backup_lba - 0x21
header.pation_table_crc = CRC32(partition_table_bytes)

tmp = header[0x00..0x5B]
tmp.headercrc32 = 0
header.headercrc32 = CRC32(tmp)

copy full 512B header to LBA1 output
```

`BuildSector2_Gpt` 则按128B GPT entry模板写 type GUID、partition GUID、
起止 LBA 等字段。

#### Windows consumer 重新验证

`CUsbRegsiter::IsAllowRegisterCommonLabel -> sub_1002ab70`：

```text
inspect LBA0 protective MBR
if first partition start == 1 and size == 0xFFFFFFFF:
    lba1 = metadata + sector_size
    require lba1[0:8] == "EFI PART"
    require u64(lba1+0x18) == 1
    return GPT
```

随后 GPT 分支把 `metadata + 2*sector_size`，也就是 LBA2，交给
`sub_1002b2f0`。该 parser 的双层循环为：

```text
for sector in 0..sector_count:
    base = lba2 + sector * sector_size
    for entry in 0..4:
        p = base + entry * 0x80
        if type_guid is nonzero/recognized:
            consume pation_start @ +0x20
            consume pation_end   @ +0x28
            consume pation_attr  @ +0x30
```

因此 LBA1/LBA2 不是“永远没用的保留零扇区”，而是存在正式 GPT profile。

#### physical 实盘限制与 official-binary virtual writer

22份当前原始 SAFE6 参考复核：

- LBA1：22/22 整扇全零；
- LBA2：22/22 整扇全零；
- 两者 SHA-256 均为零扇区
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`。

这只能说明当前 physical 参考盘没有启用 GPT metadata profile，不能反证官方 GPT builder。
本轮进一步在**完全隔离、不接触物理 raw device** 的 Unicorn x86-64 环境中直接执行
Linux first-party `libcemsfilesyscheck.so` 内的三个官方 builder；builder 本体与内部
`calculate_crc32` 均原生执行，只对 libc `memset/memcpy` 做 ABI 边界替代：

```text
BuildSector0_Gpt @ 0x1FD30
BuildSector2_Gpt @ 0x1FFF6
BuildSector1_Gpt @ 0x1FDAA
```

使用 2GiB / 512B sector、固定 partition GUID
`00112233445566778899aabbccddeeff`，先由 `BuildSector2_Gpt` 生成 entry0，再由
`BuildSector1_Gpt` 对完整16KiB partition array 计算 CRC 并生成 LBA1。输出为：

- LBA1：`EFI PART`、Version=`0x00010000`、HeaderSize=92、CurrentLBA=1；
- BackupLBA=4194303、FirstUsable=34、LastUsable=4194270；
- Disk GUID=`a2a0d0ebe5b9334487c068b6b72699c7`；
- partition array LBA=2、entries=128、entry-size=128；
- 独立 IEEE CRC32 复算：header=`0xA4B46C72`、array=`0xD32CFEA7`，均与 official writer 输出一致；
- LBA1 `+0x5C..+0x1FF` 全部来自512B官方模板的显式零区；
- LBA2 entry0：Basic Data type GUID、caller-supplied partition GUID、start=63、
  end=4194270、attr=0、name[72]=0。

随后把同一34扇 staging image 直接喂给 current Windows
`CEMSUsbRegsiter.dll::IsAllowRegisterCommonLabel/sub_1002AB70`，该官方 consumer
原生执行后返回 **2 = GPT**。所以 LBA1 已具备 cross-platform
first-party writer→wire→consumer 正向闭环。

这里仍保留一个严格区分：`BuildSector2_Gpt` 每次只负责**完整写一个128B active entry**，
它不清理其它 entry slot；所以 staging area 的 entries1..127 零值仍不能冒充 official
producer 常量。但后续沿 consumer 继续追踪后，unused entry 的 residual 已经按**不依赖具体值的
unowned storage**闭合：current Windows `sub_1002B2F0` 和 Linux
`AnalyzeGptPartitionTable@0xFB36` 都先比较16B TypeGUID，只有匹配受支持非零GUID后才读取
start/end/attr 等 residual；Windows official parser 对 `TypeGUID=0 + residual=0xA5` 的
动态 probe 又证明336B residual 运行时读取次数为0。

因此最终状态为：

- LBA1：**512B COMPLETE**；
- LBA2 `0x000..0x07F` entry0：**128B COMPLETE**；
- LBA2 entries1..3：16B TypeGUID 由 one-partition creator + unused discriminator 闭合，
  其余112B/entry由 first-party negative-consumer 闭合为 **unused-entry unowned residual**；
- LBA2：**512B COMPLETE**。

fixtures `official_virtual_gpt_lba1.hex` / `official_virtual_gpt_lba2.hex` 与 CI
`official_virtual_gpt_builder_emits_valid_lba1_and_entry0` 固定 active writer 结构与 CRC；
`strict_progress_has_no_partial_detail_rows_for_fully_complete_lbas` 则防止以后把 LBA2 residual
重新误记为 producer-owned zero 或 PARTIAL。

## 5. LBA11 完整 producer / consumer 追踪

### 5.1 Producer：Linux 官方实现

二进制：

`libcemsfilesyscheck.so`

DWARF 原源码：

- `/mnt/git/cross_platform/src/global/src/diskfile.cpp:783`
  - `CLabelManage::BuildSector11(const char *pVid, const char *pPid, ULONGLONG ullSize, char *buffer)`
- `/mnt/git/cross_platform/src/global/inc/datasecrity.h:25`
  - `CDataSecrity::RandBuffer256`
- `datasecrity.h:41`
  - `CDataSecrity::DataEncrypt`

函数地址：

- `BuildSector11 = 0x1D350..0x1D601`
- `RandBuffer256 = 0x20204..0x202D3`
- `DataEncrypt = 0x202D4..0x20474`

已恢复伪代码：

```text
BuildSector11(pVid, pPid, ullSize, out512):
    drkb = byte[256]
    len = 0x104
    RandBuffer256(drkb, len)
        # 实际返回 len = 0x100
        # drkb[0:4] = "DRKB"
        # srand(time(NULL))
        # for i in 4..255:
        #     drkb[i] = rand() % 255

    pdkb_plain = zero[256]
    pdkb_plain[0:4] = "PDKB"
    memcpy(pdkb_plain+4, m_strUID.c_str(), m_strUID.length())

    key_crc = CRC32(
        drkb[0:256]
        || pVid[0:4]
        || pPid[0:4]
        || little_endian_u64(ullSize)
    )

    pdkb_cipher = Encrypt(key_crc_le32, pdkb_plain[0:256])

    out512[0x000:0x100] = drkb
    out512[0x100:0x200] = pdkb_cipher
```

`CLabelManage` DWARF：

- `m_strUID @ +0x10`；
- 构造函数参数官方名 `pUID`；
- `CDiskReader::InitDisk` 将 `m_strUid @ +0x248` 传给
  `CLabelManage::Init(..., pUID, ...)`。

### 5.2 Consumer：Linux 官方实现

DWARF 原源码：

`/mnt/git/cross_platform/src/global/src/diskfile.cpp:1168`

函数：

`CLabelManage::ReadSector11 = 0x1F220..0x1F424`

伪代码：

```text
ReadSector11(pVid, pPid, ullSize, sector512, outUid):
    drkb = sector512[0x000:0x100]
    cipher = sector512[0x100:0x200]

    if drkb.magic != "DRKB":
        return ERR_DRKB

    key_crc = CRC32(
        drkb
        || pVid[0:4]
        || pPid[0:4]
        || little_endian_u64(ullSize)
    )

    plain = Decrypt(key_crc_le32, cipher)

    if plain.magic != "PDKB":
        return ERR_PDKB

    outUid = C_string(plain + 4)
    return OK
```

### 5.3 Windows 独立 producer / consumer

Windows `cemsusbregsiter.dll` 存在与 Linux 完全独立、但公式一致的实现：

| 角色 | 函数 | 地址 | 反编译位置 |
|---|---|---:|---:|
| LBA11 builder | `sub_10014720` | `0x10014720` | `cemsusbregsiter.dll.m` 约 L78343 |
| DRKB/random producer | `sub_10002B90` | `0x10002B90` | 约 L82081 |
| KDF/encrypt | `sub_10002C30` | `0x10002C30` | 约 L82112 |
| LBA11 reader | `sub_10015F00` | `0x10015F00` | 约 L93391 |

关键证据：

- `sub_10002B90` 固定写 `0x424B5244 == "DRKB"`，随后循环生成
  `rand()%0xFF`；
- `sub_10014720` 构造 `0x424B4450 == "PDKB"`，并把 UID C-string
  写到明文 `+0x04`；
- `sub_10002C30` 把
  `DRKB256 || VID4 || PID4 || size8` 作为 CRC32 输入，再用 CRC
  的 4B little-endian 作为后半 256B 加密 key；
- `sub_10015F00` 先检查 `DRKB`，按同一 KDF 解密后再检查 `PDKB`。

伪代码：

```text
BuildSector11_Windows(vid4, pid4, disk_size, uid):
    rnd[0:4] = "DRKB"
    for i in 4..255:
        rnd[i] = rand() % 255

    plain = zero[256]
    plain[0:4] = "PDKB"
    copy_c_string(plain+4, uid)

    crc = CRC32(rnd || vid4 || pid4 || LE64(disk_size))
    out[0:256] = rnd
    out[256:512] = A7F0(plain, LE32(crc), 0)
```

### 5.4 Windows 当前 writer 的容量来源

`cemsusbregsiter.dll.m::sub_10019780`（约 L83945）直接读取物理盘容量：

```text
GetPhysicalDiskSize(disk_number):
    path = "\\\\.\\PHYSICALDRIVE%d"
    h = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE)
    geometry = DeviceIoControl(
        h,
        IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,   // 0x700A0
        out_size = 0x28
    )
    return geometry.DiskSize                // output +0x18, uint64
```

调用链已连通：

```text
sub_100186C0 enumerate USB interface
    -> sub_10018C40 resolve disk number
    -> sub_10019780(PHYSICALDRIVE#, &DiskSize)
    -> disk-info local +0xB0/+0xB4
    -> sub_10017EB0 copies disk-info object
    -> RegsiterUsb/sub_1003B560
    -> sub_10018480 copies disk-info to local
    -> local var_A8/var_A4
    -> sub_10014720(..., size8, ...)
```

因此**当前 Windows writer 的 LBA11 KDF 容量输入就是物理
`DISK_GEOMETRY_EX.DiskSize`**，不是 CHS 推导值。

此前唯一缺口是 Aigo U335 `rev_pmap / onlyid=1987718388` 为什么使用 CHS 容量。
本轮已从独立官方 repair 组件闭合这条路径：

```text
UDiskLabelRepair.dll::CLabelRepair::Repair
  -> sub_10008820                 # Check LBA11
       -> sub_10003BD0            # 用 disk_info+0x30/+0x34 解密 PDKB
  -> if check fails:
       sub_10008950               # ReWrite11Sector
         -> sub_10003A40          # 重新生成 DRKB/PDKB LBA11
```

`disk_info+0x30/+0x34` 的来源为：

```text
sub_10002A20
  -> sub_10002FF0
       DeviceIoControl(IOCTL_DISK_GET_DRIVE_GEOMETRY = 0x70000,
                       out_size = 0x18)
  -> DISK_GEOMETRY:
       Cylinders            @ +0x08/+0x0C (u64 in enclosing object)
       TracksPerCylinder    @ +0x14
       SectorsPerTrack      @ +0x18
       BytesPerSector       @ +0x1C
  -> sub_10019EF0 64-bit multiply helper
  -> capacity = Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector
  -> disk_info+0x30/+0x34
```

`sub_10019EF0` 的机器码已复核为标准 64-bit multiply helper：返回 `EDX:EAX`，
不是业务函数。对常见 255/63/512 geometry，这正是此前实盘复算得到的 CHS-floor
容量。也就是说，**CHS 不是注册 writer 的隐式分支，而是 repair writer/reader 的
明确容量来源**。

### 5.5 22份原始实盘验证

只读重算结果：

- 21/21 原始完整备份均成功恢复 PDKB + 正确 device_id；
- 独立 SanDisk 原始加密盘成功恢复
  `disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00`；
- 总计 **22/22**；
- 21/22 使用 `DiskSize`；
- 1/22（Aigo U335 `rev_pmap`）使用 CHS 取整容量；
- 22/22 第一半扇都是 `DRKB + 252B`，随机区没有出现 `0xFF`，符合 `rand()%255`。

同一 Aigo U335 `rev_pmap` 的辅助真实采集同时存在 CHS 与 exact DiskSize 两种
LBA11，进一步证明差异来自 writer path，而不是硬件身份本身。至此两种已观测
profile 都具备 producer、consumer、容量算法和真实盘验证，因此：

- LBA11 `0x000..0x0FF`：COMPLETE；
- `0x100..0x103`：COMPLETE；
- `0x104..0x1FF`：由 PARTIAL 升级 COMPLETE；
- **LBA11 整扇 512B = 100% COMPLETE**。

## 6. LBA7 / LBA12 EDPF 字段 producer-consumer 图

字段名称来自 Linux DWARF，但这里存在一个必须显式区分的 ABI 分叉：

`/mnt/git/.../global/inc/edpdiskglobal.h:76`

```text
Linux libcemsfilesyscheck.so natural tagEdpPartionInfo (sizeof=0x48)
+0x00 Flag
+0x04 Version
+0x08 PartionCount
+0x0C PartionType
+0x10 NeedDisturb
+0x14 NeedEncrypt
+0x18 StartSector        (u64)
+0x20 SectorSize         (u64)
+0x28 PartionSize        (u64)
+0x30 UserKeyCRC
+0x34..+0x37 alignment hole
+0x38 FileKeyCRC
+0x40 EncryptFileKey     (u64)
```

但 **Windows 真实物理 LBA7 不是这个 72B natural ABI**，而是去掉对齐洞后的
64B packed ABI：

```text
Windows physical LBA7 packed entry (stride=0x40)
+0x00 Flag
+0x04 Version
+0x08 PartionCount
+0x0C PartionType
+0x10 NeedDisturb
+0x14 NeedEncrypt
+0x18 StartSector        (u64)
+0x20 SectorSize         (u64)
+0x28 PartionSize        (u64)
+0x30 UserKeyCRC
+0x34 FileKeyCRC
+0x38 EncryptFileKey     (8B legacy wrapped key)
```

**LBA7 packed 64-byte ABI versus Linux natural 72-byte ABI** 已由三条独立证据锁定：

- `libcemsfilesyscheck.so::BuildSector7@0x1DCDA` 的 natural build 复制
  `0xD8 = 3*0x48`，其 pass-info 位于 `+0xD8`；
- Windows `cemsusbregsiter.dll::sub_10016490` 明确按 `0x40` 读取旧表并逐字段
  扩展到 `0x60` runtime table；`edpediskctrl.dll::sub_100125B0` 做反向
  `0x60 -> 0x40` 映射；
- 22份 original real-device：按 `0x40` stride，21/22 为3条 EDPF、1份已知
  中间态为2条，且 `+0xC0` 的14B pass-info 22/22 可恢复合法版本；
  按 `0x48` stride 则 22/22 都无法得到三条连续 EDPF，`+0xD8` 也无一得到合法
  pass-info。CI 原始夹具另外锁死 `0x40` 三条 entry 与 `+0xC0` 表尾。

这里的“22份”继续特指**原始生成协议参考集**。另有
`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4`：
它是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的设备快照，
不是 edpcli 自生成/免密转换产物。本轮把它作为独立第23份真实行为/profile
样本纳入复核，但不拿它替代22份原始生成参考。该盘 LBA7 明确为：

```text
entry0: Version=0, PartionCount=2, PartionType=2, NeedDisturb=1, NeedEncrypt=1
entry1: Version=0, PartionCount=2, PartionType=4, NeedDisturb=1, NeedEncrypt=1
pass-info Version=0x0064
```

因此真实产品确实存在两条 entry 的 LBA7 profile；此前22份参考中的 Netac
`onlyid=949028302 @ 17:24:33` 仍因同 onlyid 前后只有 LBA7 被改动而作为
该扇区的局部实验态降权，但不能再把“LBA7=2”本身视为实验态特征。
此外，`no_password_disk4/info/disk4_info.json` 中旧解析结果
`"ver": 2` 是把 `PartionCount@+0x08` 错标成 Version；按物理 packed ABI
重新解码后两个 entry 的 `Version@+0x04` 都为0。仓库
`protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex` 回归门禁
专门拦截这类字段错位。该新增样本不改变当前 COMPLETE/PARTIAL 字节计数。

Linux producer/reader 原源码位置：

- `CLabelManage::BuildSector7` → `diskfile.cpp:895`；
- `CLabelManage::BuildSector12` → `diskfile.cpp:921`；
- `CLabelManage::ReadSector12` → `diskfile.cpp:1195`。

Windows 当前 producer：

- `cemsusbregsiter.dll::CUsbRegsiter::CreatePartitions`
  （反编译 `sub_1003db50`，本地约 L120709）；
- 日志保留的原源码位置：
  `usbregsiter.cpp:0xE13..0xE15`；
- 该函数按 `0x60` stride 构造 packed LBA12 entry。

核心伪代码：

```text
for each partition:
    entry.Flag          = "EDPF"
    entry.PartionCount  = total_count
    entry.PartionType   = type
    entry.NeedDisturb   = profile value
    entry.NeedEncrypt   = profile value
    entry.StartSector   = calculated_start
    entry.SectorSize    = sector_size
    entry.PartionSize   = calculated_bytes
    entry.UserKeyCRC    = CRC32(password)
    entry.FileKeyCRC    = CRC32(file_key)
    entry.wrappedKey    = wrap(file_key, password, mode)
    entry.EncryptMode   = mode
```

### 6.0 entry Version 与 entry1/entry2 NeedDisturb：compatibility metadata 生命周期闭合

本轮直接验证 Windows 官方 PE 机器码：

- \`cemsusbregsiter.dll::sub_10016490\` 按 \`0x40 -> 0x60\` 遍历三条 old entry，
  明确复制 \`Version@+0x04\` 与 \`NeedDisturb@+0x10\`；
- \`edpediskctrl.dll::sub_100125B0\` 的 \`0x60 -> 0x40\` 反向转换也逐条复制二者；
- 因此它们确实属于物理 old ABI 字段，而不是 padding。

当前 writer \`CUsbRegsiter::CreatePartitions/sub_1003DB50\` 的真实赋值序列是：

\`\`\`text
memset(old_table, 0, 3 * 0x40)

# entry Version
entry0.Version = 0        # 三条都没有 +0x04 覆盖写
entry1.Version = 0
entry2.Version = 0

# NeedDisturb
caller passes need_disturb = 1
entry0.NeedDisturb = need_disturb
entry1.NeedDisturb = need_disturb
entry2.NeedDisturb = 0    # 没有覆盖写，保留 memset
\`\`\`

这与原始三分区实盘的 \`1/1/0\` 完全一致，也解释了为何不能把 NeedDisturb
理解成 \`PartionType\` 的函数：新增真实免密 SanDisk 的两条表中，
type4 位于 entry1，因此其 NeedDisturb=1；标准三分区 type4 位于 entry2，
NeedDisturb=0。回归测试
\`lba7_need_disturb_is_not_a_partition_type_invariant\`
固定这一 profile/位置差异。

consumer / structural-preserve 继续向下追踪后的边界：

- 两版官方 \`vrvaud_c\` 都把完整 packed old table 读入缓冲；
- \`NewCheckDisTurbUsb\` / \`NewCheckDisTurbUsbEx\` 实际只检查
  entry0 \`NeedDisturb@+0x10\`；
- 本轮把两版全局 table 的 ABI 又按机器码/地址重新锁定：ydcc build
  `0x1020BF40`、Win10 build `0x10172520` 都先 memset **0xC0**，随后承接
  3×0x40 packed old table；两版 `sub_*C5D0` 都以 `(i << 6)+base+0x0C`
  遍历 PartionType，明确 stride=0x40，不是0x60 runtime；
- 对这两个0xC0全局区做完整静态 xref：两版都只有
  `entry0 NeedDisturb@base+0x10` 的行为读取；entry1/entry2 `+0x10`
  和三条 `Version@+0x04` 均无直接 xref。动态循环也只读取 `PartionType@+0x0C`；
- Linux \`CLabelManage::GetPartionFromOld\` 对 Version/NeedDisturb 只是 ABI 搬运；
- Linux \`PartitionHeader\` 体系会携带整条 new entry；进一步对
  \`CDiskReader::GetTagPartitionInfo\`、\`DecryptFileKey\`、
  \`CheckFileKeyCrc\`、\`ReadFileSysSector0\`、
  \`DecryptFileSysSector0\` 的真实机器码逐一复核后，消费字段集中在
  Flag/PartionType/UserKeyCRC、StartSector、FileKeyCRC、wrapped key、EncryptMode，
  均不读取 entry \`Version@+0x04\` 或 \`NeedDisturb@+0x10\`；
- 旧 Windows \`EdpEDiskCtrl\` 读取 old LBA7 后，实际协议代际仍由
  14B pass-info Version 决定，不依赖 entry \`Version@+0x04\`。

实盘方面，committed original fixtures 的全部有效 entry 与新增真实免密 SanDisk
两条 entry 均为 \`Version@+0x04=0\`；扩展只读历史扫描也没有发现非零 Version。
NeedDisturb 则稳定按**位置 profile**出现：

- 三条 entry：\`(1,1,0)\`；
- 两条 entry：\`(1,1)\`。

尤其 SanDisk 的 type4 位于 entry1 且 NeedDisturb=1，而标准三分区 type4 位于
entry2 且 NeedDisturb=0，因此它不能被解释成 `PartionType -> NeedDisturb`
恒等映射。回归门禁现在对所有 committed original fixtures 逐条锁定
Version=0 与上述 positional NeedDisturb profile，而不是只抽两个代表盘。

这里的 COMPLETE 语义必须与 entry0 的真实 MBR 行为分开：

- **3×Version 共12B**：formal ABI compatibility metadata。current writer 由整表
  zero-init 产生0；Windows old/new converter 双向 structural-preserve；
  Windows 两个独立 build 与 Linux 文件系统检查链均不把它作为版本选择或行为条件，
  真正协议代际由 pass-info Version 决定；
- **entry1/entry2 NeedDisturb 共8B**：current writer 的 positional
  compatibility metadata。entry1继承调用者参数1，entry2继承 zero-init 0；
  converter structural-preserve；已审 Windows/Linux 运行时没有 entry1/2
  行为读取。只有 entry0 的同名 DWORD 另有 MBR scramble/descramble 行为 consumer。

因此这20B现在满足与 `EncryptFileKey32` compatibility slot 相同的严格闭环模型：
**明确 producer/profile + 双向/跨ABI structural preserve + cross-platform
negative semantic consumer + 多真实 profile 实盘门禁**。它们不是 Reserved，
也不能被清洗为统一常量；未来若发现其它 writer profile，应扩展 profile 而不是
推翻“当前运行时只结构保留、不赋予独立业务语义”的闭环。

本轮先新增 **20B PARTIAL -> COMPLETE**；随后 pass-info
\`+0x0C/+0x0D\` 两个 BackupPromptPeriod compatibility BYTE 也由独立跨版本
reader/producer 审计闭合，见下节，因此 LBA7 最终达到 **512/512 COMPLETE**。

#### LBA7/LBA12 BackupPromptPeriod：正式但 dormant 的 compatibility bytes

Linux DWARF 把这两个字节直接钉到 \`edpdiskglobal.h:164/165\`：

- \`ShareBackuppromptPeriod\`：\`BYTE @ +0x0C\`；
- \`EncryptBackuppromptPeriod\`：\`BYTE @ +0x0D\`。

它们不是 padding/bitfield。current Windows writer
\`CreatePartitions/sub_1003DB50\` 先把完整14B pass-info 显式清零，随后有效
store 最远只到 \`+0x0A\`，因此两字节的 current producer 是明确的0/0。
生成 LBA12 时只把同一 pass-info 的 Version 改为 \`0x0206\`，两字节不变。

reader 侧继续跨四个不同哈希的 \`EdpEDiskCtrl.dll\` 版本复核：

- current ydcc 与 out_raw 两版都把完整14B pass-info structural-copy 到输出；
  随后只对 \`Version(+0)\`、Share retry \`(+3)\`、Encrypt retry \`(+6)\`
  做 XOR/字段处理；
- 两个更老 build 同样输出完整14B；Win10 build 的成功尾部可直接看到
  3×DWORD + 最后1×WORD 的完整结构搬运，最后 WORD 正是 \`+0x0C/+0x0D\`，
  随后仍只处理 \`+0/+3/+6\`；
- Linux \`libcemsfilesyscheck.so\` 保存完整 pass-info，但实际文件系统检查链
  不读取这两个 BYTE。

另外，两代 \`vrvaud_c\` 虽有
\`BackupPromptInfo/BackupStartTime/BackupEndTime\`，机器码已经证明它们从
策略字符串解析到独立 string/DWORD globals；其磁盘 old-table 全局严格是
\`0xC0 = 3×0x40\` packed entries，不包含后续14B pass-info。该链与
pass-info 没有数据流，不能凭名字相似合并。

实盘方面：

- committed originals 的 LBA7/LBA12 两份副本逐盘均0/0且完全一致；
- 全目录只读去重扫描19个真实 LBA7 密文 profile，覆盖 pass-info
  \`v0x0064\` 与 \`v0x0206\`，仍是19/19=0/0；
- 没有任何非零历史 profile。

因此这2B/每份表的 COMPLETE 含义不是“period 的单位已证明为小时或天”，而是：
**正式命名的 backup-prompt-period compatibility fields 在所有已覆盖产品实现中
处于 dormant 状态；writer 明确写0，reader只结构保存/返回而无值相关语义消费。**
未来如果发现非零旧 profile，必须 preserve/report 并扩展 profile，不能按 current
规则机械清零，也不能把未知非零值强行解释成当前不存在的时间单位。

### 6.1 entry0 NeedDisturb：4B 已完整闭合

Producer：

- Windows `CreatePartitions` 生成 packed entry；
- Linux 官方字段名为 `NeedDisturb @ +0x10`。

Consumer：

- `vrvaud_c.m::NewCheckDisTurbUsb`，原日志位置
  `Format.cpp:0x3CE`；
- `ReadPartionInfoExNew` 成功后：

```text
if (packed_entry0.NeedDisturb != 0):
    *is_new_tag_disk = 1
    success = 1
```

- `NewCheckDisTurbUsbEx` 在 `Format.cpp:0x380` 有同一判断；
- Windows 10 另一套 `vrvaud_c` 也独立存在同构判断。

原始实盘只读复核：

- 22/22：`entry0.NeedDisturb == 1`；
- 20/22：entry0 type=1；
- 2/22：entry0 type=2；
- 22/22 pass-info version=`0x0206`。

因此这 **4B** 可升级 COMPLETE。注意完成的是“旧兼容识别路径的非零门控”
这一具体行为，不是对字段名作“扰码/防篡改”等词义扩张；entry1/entry2
仍需分别追 consumer，不能因为同名字段而自动升级。

### 6.2 LBA7 v0x0064 +0x38..+0x3F：8B legacy file-key wrapping 完整闭合

Windows 运行时 `edpediskctrl.dll::sub_10026050` 是旧表 key 的直接 consumer 和
改密 producer。对目标 entry，它依据 pass-info/version 决定 key 长度：

```text
key_len = 8
if pass_info.Version == 0x0206:
    key_len = 16

plain_key = unwrap(password, entry.EncryptMode, entry.wrapped_key)
if CRC32_bare(plain_key[0:key_len]) != entry.FileKeyCRC:
    reject
```

对 v0x0064 legacy table，转换后的 `EncryptMode=0`。`sub_10028AB0` 的 mode0
分支调用 `sub_10011290 + sub_10011450`。继续拆机器码可得：

- `sub_10011290(password)`：按 little-endian 4B chunk 求和；不足4B的尾部
  以零补齐后再加，结果按 u32 回绕；
- `sub_1005CEF0` 是 unsigned 64-bit right-shift helper；
- `sub_1004EF80` 是 unsigned 64-bit multiply helper；
- 化简编译器 helper 后，`sub_10011450` 对两个32位 half 的实际变换为：

```text
K = fold32(password)
plain_lo = wrapped_lo XOR K
plain_hi = wrapped_hi XOR K
```

逆向写回 `sub_10028DB0` 使用同一对称变换：

```text
wrapped_lo = plain_lo XOR K
wrapped_hi = plain_hi XOR K
```

默认密码的两个独立值：

```text
CRC32_bare("0000aaaa") = 0x0429735D
fold32("0000aaaa")     = 0x91919191
```

持久化链：

```text
CEdpDiskControl::ChangePwd / sub_100269A0
  -> sub_10026050
       -> sub_10028DB0          # 生成新的 wrapped8
       -> runtime entry +0x34   # FileKeyCRC
       -> runtime entry +0x38   # wrapped8
       -> if version == 0x64:
            sub_100125B0        # 0x60 runtime -> 0x40 packed
  -> CEdpDiskControl::SavePartionSector / sub_10028580
       -> version == 0x64:
            sub_10010FC0
              memcpy old_table[0xC0]
              append pass-info[0x0E]
              rolling-XOR whole LBA7
              WriteFile(sector 7)
```

22份 original real-device 的只读独立复算覆盖全部非零 legacy 加密 entry：

- 共 **28条** type2/type4 entry 同时具有非零 `FileKeyCRC + wrapped8`；
- 28/28 的 `UserKeyCRC == 0x0429735D`；
- 每条分别执行 `wrapped_lo/hi XOR 0x91919191` 得到8B file-key；
- **28/28** 都满足
  `CRC32_bare(unwrapped_file_key8) == entry.FileKeyCRC`；
- current-style 对应字段为零，与 newer profile 分界一致。

因此每条 packed entry 的 `+0x38..+0x3F` 8B 可由 PARTIAL 升为 COMPLETE，
三个 entry 共新增 **24B COMPLETE**。`+0x34..+0x37 FileKeyCRC` 早已因 CRC
consumer 链计入 COMPLETE，本轮不重复增加4B/entry。

CI 回归分别锁定物理 LBA7 必须使用0x40 packed stride，以及 committed original
fixtures 的默认密码 legacy wrapped8 必须能按上述算法解包并通过 FileKeyCRC。

### 6.2.1 LBA7 `0x0CE..0x1FF`：306B post-table zero region 完整闭合

Windows 主物理 old-table writer `edpediskctrl.dll::sub_10010FC0` 已回到实际实现复核：

```text
staging[0..0xFFF] = 0                         # sub_1004D110 == memset
staging[0x000..0x0BF] = packed_table[0xC0]
staging[0x0C0..0x0CD] = pass_info[0x0E]
rolling_xor(staging[0x000..0x1FF])
WriteFile(LBA7, 0x200)
```

`sub_1004D110` 不是根据命名猜测：其机器码按 `arg2` 长度逐字节/`rep stosd`
填充 `arg1`，语义就是 memset。`sub_10010FC0` 在任何 table/tail copy 之前把
staging 清零，并且最后一个结构写入恰好结束在 `0x0CE`，所以 plaintext
`0x0CE..0x1FF` 的306B有明确的 **writer-zero producer**。

对应 Windows consumer `ReadPartionInfoExEx/sub_10010B40`：

1. 从 LBA7 读取完整 sector；
2. 对前512B执行完整 rolling 解码；
3. magic 合法后只向调用者复制 `decoded[0x000..0x0BF]` 的0xC0 table；
4. 再复制 `decoded[0x0C0..0x0CD]` 的0x0E pass-info；
5. **从不返回或解释 `decoded[0x0CE..0x1FF]`。**

Linux `BuildSector7@0x1DCDA` 也独立执行“约2KB staging 先清零→复制 natural
3×0x48 table +0x0E tail→对完整512B rolling”。因为 Linux natural ABI 的尾端是
`0x0E6`，这条证据只用于确认同源 builder 的未用区域零初始化原则，**不能**把
Linux `0x0E6` 偏移混成 Windows packed `0x0CE` 的物理边界。

严格22份 original generation reference set 再逐盘独立复算：

- 22/22 LBA7 均按各自 `CRC32(device_id)` 派生 rolling key 恢复 `EDPF`；
- **22/22 `decoded[0x0CE..0x200] == zero[306]`**；
- 独立 SanDisk original 同样为完整306B零，无 legacy 非零反例。

新增 CI 门禁 `lba7_post_table_plaintext_is_zero_through_sector_end`。因此这306B
满足字段/区域边界、官方 producer、negative consumer、原始实盘四证合一，
由 **UNKNOWN -> COMPLETE**。LBA7 严格状态随之变为：

```text
512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100.0%
```

post-table 审计当时新增306B COMPLETE；后续又闭合 pass-info
`bNoUsbChkPasSafe(+0x0A)` 1B。本轮再按 compatibility metadata 生命周期闭合
3×Version 共12B与 entry1/entry2 NeedDisturb 共8B，最后把两个
BackupPromptPeriod BYTE 闭合为 dormant compatibility fields；LBA7 至此整扇完成。

### 6.3 LBA12 +0x38..+0x47：历史阶段记录（当前 mode1/2/3 语义均已 COMPLETE）

> 本节保留当时仅闭合 mode2 的推导过程；**当前结论以前方 canonical 字段账本为准**。后续 first-party `CreatePartitions` 隔离执行已经补齐 mode1/mode3 positive wire 与独立 unwrap/CRC，因此整字段现为语义 COMPLETE；mode1/mode3 仍在 `profile_coverage.tsv` 标记为 `MISSING_PHYSICAL`。

当时对 packed entry 的 16B wrapped file-key 做了重新独立审计，
不再沿用更早脚本结论。

Windows producer：

```text
CreatePartitions
  -> generate file_key16
  -> entry.FileKeyCRC = CRC32_bare(file_key16)
  -> entry.UserKeyCRC = CRC32_bare(original_password)
  -> if original_password == "0000aaaa":
         sub_10040400(password)
         # 替换为隐藏10B字符串
  -> md5 = MD5(effective_password)
  -> if EncryptMode == 2:
         if GLOBAL/oldSM4 == "1":
             alternate mode2 implementation
         else:
             standard SM4-ECB(file_key16, md5)
  -> entry.wrapped16 = ciphertext
```

其中 **LBA12 v0x0206 hidden default-password file-key wrapping** 的
默认密码替换过程已经从 Windows PE 机器码重新恢复：

```text
sub_10040400:
  seed32      = 468b46088b4e048bd02bd13bd37f2183
                c1098d3c003bf97f028bf98b065750e8
  key1[i]     = seed[i] XOR seed[i+16]
  key2[i]     = seed[i] + seed[i+16]  (u8 wrapping)
  table96     = A6B0_DECRYPT(obfuscated_table96, key1)
  index16     = A6B0_DECRYPT(obfuscated_index16, key2)
  password[i] = table96[index16[i]], i=0..9
```

独立重算结果：

```text
key1 = 8782cb348b75fdf4d2a028b0d528716b
key2 = 0794d3448b89fd0ad2b6cac6d9d6716b
index[0..10] = 38 17 51 4d 0c 05 26 16 2d 0d
effective_password = "LtSWi[2f)j"
MD5(effective_password) = 548b072cba7f104d88a446556cc3c432
```

这个字符串不是从旧文档复制，而是本轮直接由当前 DLL 的机器码常量和
已验证 A6B0 算法重算得到。用相反的 A7F0 方向时 index 会越界，
也提供了负向验证。

Windows `sub_100036e0` 所走的 mode2 算法可由以下常量/轮函数确认
为标准 SM4：

- FK =
  `A3B1BAC6 56AA3350 677D9197 B27022DC`；
- CK 从 `00070E15 1C232A31 383F464D ...` 顺序展开；
- S-box 与标准 SM4 S-box 逐字节一致；
- key schedule T' 使用 rot13/rot23；
- round T 使用 rot2/rot10/rot18/rot24；
- 总计32轮。

Linux consumer：

```text
CDiskReader::DecryptFileKey
  -> default input "0000aaaa"
  -> version == 0x0206 && password == default:
         GetIniString(password)
  -> AlgorithmSpace::fileKey_Decrypt
       mode2:
         MD5(effective_password)
         MC_KKSMS4::DecryptBuffer(wrapped16)
  -> CDiskReader::CheckFileKeyCrc
       CRC32(file_key16) == entry.FileKeyCRC
```

22份 original real-device reference set 的只读重算：

- 共 66 条 packed entry；
- 44 条 type2/type4 为 EncryptMode=2；
- 其中 43 条 `UserKeyCRC=0x0429735D`；
- 使用 OpenSSL 3.6.3 标准 SM4-ECB +
  `MD5("LtSWi[2f)j")` 独立解包：**43/43** FileKeyCRC 校验通过；
- 唯一非默认 type2 的同盘 type4 仍是默认密码；
  从 type4 独立解出共享 file-key
  `eadd58009f9abe0625a1f1f779d4c98b`，
  CRC=`F7EEA980`，同时精确匹配 type2/type4 保存的 FileKeyCRC；
- 两条 wrapped16 不同，排除“直接复制同一 wrapped material”。

所以当前实盘使用的
`v0x0206 + mode2 + oldSM4!="1"` 分支已经闭环。

随后继续追完 **LBA12 alternate wrapping-mode algorithm map** 后，
可以把“其它算法分支未知”这一缺口进一步消掉：

| EncryptMode | Windows writer | Windows reader | Linux当前build | 当前结论 |
|---:|---|---|---|---|
| 1 | `sub_10001190`: A7F0, key=MD5(password) | `sub_100384E0`: A6B0 inverse | `fileKey_Decrypt case1 -> Decrypt` | 算法双向闭合；22盘正向样本0 |
| 2 | `sub_100036E0` 或 `sub_10011010`: 标准SM4-ECB | `sub_10028AB0 case2`: 标准SM4 decrypt | `MC_KKSMS4::DecryptBuffer` | 44条真实entry；已闭合 |
| 3 | `sub_1000FC10`: AES-128-ECB | `sub_1002F670`: AES-128 inverse；CRC失败后强制mode1重试 | 当前 `fileKey_Decrypt` 无case3 | 算法/兼容行为闭合；22盘正向样本0 |

继续向上追 writer 可达性后，已经能排除“mode1/mode3 只是死代码”的解释：

- `CUsbRegsiter` 构造函数在 `0x10038EB4` 把 `this+0x6EC` 默认置为 `2`；
- 唯一 setter `sub_1003B4E0(arg)` 直接覆盖 `this+0x6EC`；
- `WriteNormalULabel` 对请求字节 `arg+0x7E8` 做正式映射：值1调用 setter(1)，值2调用 setter(3)，其它值调用 setter(2)；
- `CreatePartitions` 三处 wrapped16 writer 都读取同一个 `this+0x6EC`：2走SM4、1走A7F0、3走AES-128，并把其低字节原样写到每条 packed entry 的 `EncryptMode@+0x58`；
- `usbtoolBusManage.dll::UsbtoolBusMgrInter::LabelInfo::Print` 把 `LabelInfo+0x7E8` 明确打印为 `crypt=%d`，因此底层请求字段已经有上层业务名 `crypt`；
- 制标 UI 的 `tabAlgorithmComboBox` 实际加入 `SMS4`、`AES`、`AES_CROSS` 三项并默认 index=0；`currentIndex()` 在 `0x467270..0x467282` 直接写入策略对象 `normalDetail+0x44`，日志字段名为 `normalDetail.algorithm`。

此前保留的 UI 桥接缺口现已由同一 `cemssafeudisklabeltool.exe` 直接闭合：
`tabAlgorithmComboBox::currentIndex()` 写 `normalDetail+0x44`；`sub_42E8E0` 随后执行
`LabelInfo.crypt(+0x7E8) = normalDetail.algorithm(+0x44)`，反向 `sub_42EC90` 又执行
`normalDetail.algorithm(+0x44) = LabelInfo.crypt(+0x7E8)`。组合框初始化汇编 `0x47780F..0x47788C`
构造 `AES_CROSS/AES/SMS4` 后按栈顶实参顺序向 QStringList 实际 append
`SMS4 -> AES -> AES_CROSS`，并 `setCurrentIndex(0)`。因此数据流精确为：

- `SMS4(index0) -> crypt0 -> EncryptMode=2`；
- `AES(index1) -> crypt1 -> EncryptMode=1`；
- `AES_CROSS(index2) -> crypt2 -> EncryptMode=3`。

UI、normalDetail、LabelInfo 与 CreatePartitions 的三档 mode 分派现已成为连续可验证数据流。

mode1 的关键固定关系：

```text
key = MD5(effective_password)
A7F0 key material = key XOR "EDPSECDISK200709"
wrapped16 = A7F0(file_key16, key, counter=0)

reader:
file_key16 = A6B0(wrapped16, key, counter=0)
CRC32(file_key16) == FileKeyCRC
```

mode3 则是：

```text
key = MD5(effective_password)
wrapped16 = AES-128-ECB-ENC(file_key16, key)

reader:
file_key16 = AES-128-ECB-DEC(wrapped16, key)
CRC32(file_key16) == FileKeyCRC
```

Windows `UserLogin` 还明确实现 mode3 历史 fallback：第一次 mode3 解包
CRC失败后，以 mode1 重解同一 wrapped16；第二次 CRC 成功则接受。
对应日志分别为 `EncryptMode == eEncryptAESOPENSSL` 与
`dwKeyCrcOld == m_epiNewInfos[nIndex].FileKeyCRC`。

`oldSM4=="1"` 也不再视为独立 wire profile。该分支
`sub_10011010` 与默认 `sub_100036E0` 均逐常量/轮函数对应标准SM4：
同一 S-box、FK、CK、32轮和同一 block 输入输出语义；Windows mode2
reader 只有一个标准SM4解包分支且完全不读取 `oldSM4` 配置。
所以它只是实现选择，不改变盘面格式。

因此 `+0x38..+0x47` 整体仍记 **PARTIAL**，但剩余原因已收缩为：
**22份原始盘没有 mode1/mode3 的正向样本**。当前44条加密entry全部mode2；
UI→crypt→EncryptMode 的桥接和数值映射已经闭合。严格完成度统计仍不增加
16B×3，避免用静态算法闭合替代真实盘证据。

扩展历史语料又改成**完全不依赖 device-id 文件名或 sidecar**的只读 census：
对 `/Users/zhangyuxi/Desktop/u_disk` 下3896个至少13扇区、至多1GiB的候选，先以固定
LBA6 rolling key 解出 `m_crcUsbID[0]@+0x100`；该 DWORD 就是
`CRC32(device_id)`，可直接作为 LBA12 A6B0 key。只计解密后 entry0=`EDPF`、
entry count=1..3 且所有0x60-stride entry magic有效者。最终得到58份有效捕获：
49份 mode tuple=`[0,2,2]`，9份=`[2,2]`，**mode1/mode3仍为0**。
该扩展集合含历史/转换状态，只作为 profile 搜索负证据，不并入22份 strict originals，
也不提高完成度。

### 6.3 LBA12 +0x48..+0x5F：扩展 key-material 与 Reserved[7] 必须分开

两个 Linux 组件使用不同 ABI：`libedpedisk.so` runtime 的
`tagNewEdpPartionInfo`/header size 明确为 `0x60=96B`；
`libcemsfilesyscheck.so` 的 DWARF 则是 `sizeof=0x68=104B`，
其中 `+0x50 EncryptFileKey32[16]`、`+0x60 EncryptMode`、
`+0x61 Reserved[7]`。同名 C 结构不能按偏移直接混用。

对 Windows 96B packed runtime：

- `CreatePartitions / sub_1003DB50` 首先执行
  `memset(var_1364, 0, 0x120)`，一次清零完整3×96B entry；
- 后续 wrapped16 只写 `+0x38..+0x47`，EncryptMode 写 `+0x58`；
- `UserLogin` 从每条 entry `+0x38` 只复制16B，并从 `+0x58` 读取 mode；
- `sub_10026050` 改密码同样只读写16B `+0x38..+0x47`；
- Linux `libedpedisk.so::SetPartitionNewPass @ 0x52520` 对
  version=0x0206 也只更新16B `+0x38`。

22份 original real-device reference set 的66条 EDPF entry 重新统计：
`+0x59..+0x5F Reserved[7]` 为 **66/66 全零**。
这7B同时具备正式字段名、writer零来源、negative consumer 和实盘闭环，
因此三个 entry 共 **21B PARTIAL -> COMPLETE**。

相邻 `+0x48..+0x57` 后续又完成了单独的跨代闭环，不能再停留在
“存在字段名，所以用途未知”的阶段：

- 旧 `tagEdpPartionInfo` 是 **72B ABI**，只到 `EncryptFileKey@+0x40`，
  **old 72-byte ABI has no EncryptFileKey32 slot**；
- 104B checker ABI 才正式增加 `EncryptFileKey32[16]@+0x50`；
  `GetPartionFromOld` 把旧72B entry 转成104B时不填该数组；
- `CDiskReader::DecryptFileKey` 只读 natural `+0x40..+0x4F` 的16B主 wrapped key
  和 `+0x60 EncryptMode`；`CheckFileKeyCrc`、`ReadFileSysSector0`、
  `DecryptFileSysSector0` 同样没有 `+0x50` 值相关读取；
- packed 主挂载库 `libedpedisk.so` 构造 `PartitionHeader` 时会按值缓存完整96B，
  所以 packed `+0x48..+0x57` 会结构性进入对象 `+0x88/+0x90`。但按
  `PartitionHeader` 符号边界逐函数审计，这两个对象QWORD除构造写入外没有任何
  算法读取；正对照 packed wrapped-key `+0x38..+0x47` 映射到 object `+0x78/+0x80`，
  其中 `+0x78` 被 SMS4/AES128/OldEdp 三套解密路径实际取址并按16B消费；
- Windows current `CreatePartitions` 先把3×96B整表清零，后续不覆写 packed
  `+0x48..+0x57`；Windows UserLogin/改密也只使用主 wrapped16 与 EncryptMode；
- 22份 original real-device reference set 的66条现存 entry 中，该16B **66/66全零**。

因此这不是 Reserved[7] 的一部分，也不是当前第二把活动 file key；它是正式保留名
`EncryptFileKey32[16]` 的 **cross-generation compatibility slot**。旧ABI无槽，
新ABI保留并可随结构搬运，但当前已知算法链不消费；current packed producer显式零。
这满足本项目对“结构缓存但无语义消费”区域的 COMPLETE 标准，三个 entry 共
**48B PARTIAL -> COMPLETE**。未来若其它独立ABI出现非零该槽，兼容实现应结构保留，
不能因为当前66/66为零而强制清零。

**LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure**
与 `Reserved[7]` 继续作为两个独立门禁，避免再次混成“23B zero padding”。

## 7. 代码与测试门禁

### 7.1 CI 实盘子集

`tests/provision_protocol_audit.rs` 必须持续验证仓库中的真实原盘夹具：

- 只统计非免密夹具；
- LBA11 必须是 DRKB；
- random252 不得出现 writer 不可能产生的 `0xFF`；
- 使用 ASCII VID/PID；
- DiskSize/CHS 至少有一种必须解出 PDKB；
- PDKB+4 必须与备份 device_id 一致；
- 数字 little-endian VID/PID 必须不能误解成功。

### 7.2 文档契约测试

`tests/protocol_documentation_contract.rs` 负责拦截文档口径回退：

1. LBA0–12 必须各自合计 512B；
2. 总字节必须 6656B；
3. COMPLETE 总量不得低于当前基线；
4. UNKNOWN 不得高于当前基线；
5. 每一条 COMPLETE 字段必须填写 producer、consumer、实盘验证；
6. 主文档必须保留官方制盘工具链；
7. 主文档必须明确禁止把“样本全零/只有字段名/能生成”当完成。

## 8. 后续提升顺序

按 2026-09-21 审计意见执行，优先级固定为：

1. **先保持可复现基线为硬门禁**：任何 COMPLETE 必须能从
   `audit/protocol/byte_ledger.tsv` 回指 producer / consumer / physical evidence；
   virtual execution 与 physical capture 永不混写。
2. **集中追历史制标链，同时覆盖 LBA0 / LBA4 / LBA6 / LBA8 / LBA9 legacy 分叉**：
   目标指纹固定为 join59、非零 `HSerialCRC[5]`、`UsbOnlyInfo=0`、动态 MBR
   template。`CEMSUsbRegsiter.dll 19.11.4.1` 已明确只能解释其中一部分，且 paired
   caller 会给 HSerialCRC 零态；其 LBA6 writer 也从零模板起步，禁止再拿它解释
   strict legacy 非零 HSerialCRC / MBR snapshot。
3. **LBA3 独立作为制造协议主线**：先锁定 strict Kingston `0951:1666`、
   `62008590336B` 那一只设备的 controller / chip F/W / ID_BLK，再追制造 buffer 到
   host-visible LBA3 的真实映射和 firmware consumer。`audit/protocol/lba3_identity.md`
   记录了为什么相同 VID/PID/product/capacity 仍无法区分 PS2307 与 PS2309。
4. **LBA10 明确按样本依赖处理**：现行20份唯一指定金标全部为零，不再通过继续静态
   反编译 EESI 来提升前0x80B；只有新增符合第1.1节来源规则的真实启用样本并完成
   解密/字段/消费验证后才允许升级。
5. **最终逐字节交付**：`scripts/protocol/query_byte_ledger.py` 已提供物理偏移、LBA
   偏移、raw byte、状态、profile 和证据索引的查询骨架；后续只在实际 decoder
   被接入后输出 decrypted byte/offset，禁止把 raw byte 冒充解密视图。

## 9. 操作安全边界

本审计阶段：

- 可以读取本地反编译文件、二进制、历史原始备份；
- 可以修改 edpcli 代码、测试、文档；
- 可以生成内存/文件中的模拟 LBA0–12；
- **禁止对真实物理 raw USB 执行写入**，除非用户再次明确授权。

## 10. 验证历程附录

> 本附录由原 `PROVISION_PROTOCOL_AUDIT_2026-09-19.md` 迁移而来。
> 保留每次逆向如何得到结论、哪些假设被反证、哪些代码/机器码/实盘相互印证。
> 其中出现的阶段性计数或已被后续证据推翻的结论，仅作为**验证历史**，不得覆盖前文
> `STRICT_PROGRESS` 与字段账本。

日期：2026-09-19
范围：旧审计集合共 23 份前部快照，但其中混入免密/实验态，不能再统称“23 份真实原盘”。当前生成协议参考集改为 22 份只读样本：`nopwd_tool/backup` 中 21 份非免密完整备份 + 1 份独立 SanDisk 原始加密盘；仓库保留 7 份裁剪后的原始 LBA0–12 协议夹具。全程只读，不对物理 raw disk 写入。

### 结论

1. onlyid 不是 device_id / VID / PID / 容量的确定函数；Windows 官方注册路径已经找到其生成源。
   - 同一 Netac 0dd8:2005、相同容量、相同 device_id 的真实样本存在多个不同 onlyid。
   - 样本同时存在大于 i32::MAX 的十进制文本和负数文本。
   - `cemsusbregsiter.dll` 的 `RegsiterUsb -> sub_1003d960` 明确执行 `CoCreateGuid()`，随后对 GUID 原始 16B 调用协议同款 `CRC32_bare`，结果写入对象字段 `+0x698`；`sub_10014550` 再把该 32-bit 值格式化进 LBA4 的 `$$$...$$$`。
   - 因此 Provision 可以自动生成 onlyid：`random GUID 16B -> CRC32_bare -> u32 bit pattern`。为克隆/重建已有标签身份，仍保留显式 onlyid 输入。

2. LBA12 0x170..0x200 的 144B 不是 donor 随机保留区。
   - 对全部已提交真实备份逐字验证：
     tail == a7f0_full(144B zero, CRC32(device_id), initial_counter=0x170)。
   - 因此新盘可由目标 device_id 纯生成该区域，不复制 donor。

3. 当前提交样本中的空白扇区策略已经有真实样本证据，但“当前样本全零”不等于协议上永远保留。
   - LBA1、2、5：当前 22 份生成协议参考样本全部为全零。
   - LBA3：strict 22份中21份为全零、唯一非零样本带 Kingston 制造标记
     `this is mp mark`；扩展历史备份又发现第二种 marker profile，二者
     `+0x020..027` 不同，同型号也存在全零快照，因此不能把任一非零 profile
     当成固定模板。
   - LBA10：21/22 全零；唯一非零样本就是独立 SanDisk 原始加密盘，前 0x80 经 A6B0 解密后为 `EESI`，后 0x180 物理全零。因此 LBA10 是可选设置扇区，不应继续命名为“保留扇区”。
   - 官方 `RegsiterUsb` 主路径中的 `0x0d` 已确认是 sector count=13；从 LBA0 开始连续读写，协议范围因此严格为 **LBA0–LBA12**。当前 backup / inspect / Provision / restore 都统一使用这 13 个扇区。

4. LBA8 的 GLAB canonical 值在所有可解码样本中一致：
   322CA28A-D7D1448B-DCE2CED9。
   - User / Dept 是动态业务字段。
   - Autonum 在历史样本存在不同世代，因此作为 profile 字段处理，不从 donor 复制。

5. LBA9 在历史原盘存在 EETU/SAPF 与全零两种合法形态；现有 apply 的最终免密形态会清零 LBA9。
   - 新盘 Provision 的第一阶段目标与现有免密产品形态一致，canonical profile 采用全零 LBA9。
   - 不尝试复制厂商/旧版本 SAPF 的未知附加字段。

6. LBA6、LBA7、LBA8、LBA11、LBA12 已有 decoder 可作为基础验证器，但本轮审计发现现有 decoder 有数处结构解释错误，必须修正后才能作为逐字节 oracle。
   - LBA6 checksum、device CRC；
   - LBA7 rolling-XOR/EDPF；
   - LBA8 LLGB/User/Dept；
   - LBA11 PDKB + 目标 device_id；
   - LBA12 A6B0/EDPF。

### 逐字节复核新增结论（2026-09-19）

以下结论不是直接采信 `u_disk` 文档，而是先用历史真实前部镜像重新独立复算，再把关键变体裁剪为当前仓库中的 LBA0–12 协议夹具；`u_disk` 只作为候选结论和反编译入口。

**证据口径更新（2026-09-19）：`nopwd_tool/backup` 中由 edpcli/旧工具执行免密转换后产生的快照只能用于产品回归，禁止作为“原始加密标签如何生成”的证据。其中 Aigo U335 `onlyid=2071754312 @ 12:09:32` 已由 MBR/LBA6/LBA7/LBA12 内容确认是转换后的免密状态，因此当前“原始生成协议”参考集仍使用其余 21 份非转换完整备份，再补入独立 SanDisk 原始加密盘，共 22 份。另一方面，`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4` 是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的原始设备快照，不是 edpcli 自生成/转换盘；它作为独立的第 23 份**真实设备行为/profile 证据**纳入 LBA7 等观测，但不替代 22 份“原始生成参考”去证明加密制盘 writer 语义。已知局部实验态仍按 LBA 单独降权。仓库 `tests/provision_protocol_audit.rs` 对转换盘继续显式排除，并把该真实免密盘的 LBA7 单独放在 `protocol_evidence` 下，不让它进入原始生成参考循环。**

#### LBA3：EDP preserve-existing，厂商 MP 语义仍未闭合

重新追当前 Windows `CUsbRegsiter::RegsiterUsb/sub_1003b560`：

- 入口先 `ReadSectorData(..., count=0x0D)` 读取现有 LBA0–12；
- SAFE6 注册分支明确调用 LBA4/LBA6/LBA8/LBA11 builders，并继续构造协议分区信息；
- 没有 LBA3 builder，也没有对 `buffer + 3 * sector_size` 的 payload 解析/重建；
- 最终仍以同一 staging buffer 执行
  `WriteSectorData(..., count=0x0D)`。

所以当前官方 EDP writer 对 LBA3 的语义是 **preserve existing bytes**，不是
“强制生成 512B zero”。Linux `libcemsfilesyscheck.so` 的独立符号/实现集同样只有
`BuildSector0/4/6/7/8/11/12`、GPT 0/1/2，以及
`ReadSector4/6/8/11/12`，不存在 `BuildSector3` / `ReadSector3`。
对 Windows 当前注册、登录、修复组件的字符串/调用路径复核也没有找到
LBA3 payload consumer。这只能证明 EDP 当前组件**不解释**该扇区，不能替代
厂商量产工具或控制器固件的 consumer 证据。

22份原始生成参考逐字节复核：

- 21/22 整扇全零；
- 唯一非零盘为 Kingston DataTraveler 3.0；
- 该盘的真实非零结构是：
  `+0x001=01`、
  `+0x020..027=b5 7e 9c 45 00 80 00 14`、
  `+0x1F0..1FF="this is mp mark\\0"`；
- 同 VID/PID 的另一 Kingston 原盘整扇全零。

随后对 `nopwd_tool/backup` 与 `utils/backup` 两个历史备份目录共60份 `.bin`
做只读扩展扫描；该扩展集合包含历史/转换状态，只用于 profile census，不改变22份
strict original generation reference 的计数。非零 LBA3 只有3份，并精确归成两类：

- 2026-08-03 两份 Kingston 快照逐字节相同：`+0x001=01`、
  `+0x020..027=a8 82 a4 22 00 20 02 16`、
  `+0x1F0..1FF="this is mp mark\\0"`；
- strict 2026-09-03 Kingston：`+0x001=01`、
  `+0x020..027=b5 7e 9c 45 00 80 00 14`、尾 marker 相同；
- 同型号其它快照还存在整扇 zero profile。

本轮继续追制造端来源后，`"this is mp mark"` 已不再只是无法归属的 ASCII
尾标。USBDev.ru 的 Phison FW.BIN / firmware 资料明确使用同一
`this is mp mark` 作为 Phison firmware/MP marker；Falcon Sandbox 对真实
`MPALL_F1_9000_v372_0B.exe` 的静态结果又同时包含该 marker、
`C:\\PhisonLog` 和多个 Phison controller 型号串。因此 LBA3 的制造端家族
可以收敛为 **Phison MP/FW manufacturing metadata**，不再只写“未知厂商 MP”。

继续取得并离线逆向 FlashBoot 的 MPALL v3.72.0B 原包后，Phison 制造链又向前推进，
但同时否证了一个过强假设。`MPALL_F1_9000_v372_0B.exe` SHA-256=
`96614750c61e0ad6b05d19e74848c1679f6318dd21de6faee46c92fb05152142`，
与公开 Hybrid Analysis 样本完全一致。其机器码直接证明：

- `CBaseController::WriteF2Mark@0x00581B00` 把 `object+0x1C00C` 交给低层
  `fcn.004203D0`；后者构造 `06 06 01 ...` vendor command 并写 **0x200B**；
- 随后 `fcn.004202C0` 以 `06 05 ... "INFO"` 读取 response；
- `WriteF2Mark` 对读回 buffer 起始处与 `object+0x1C00C` 做完整 **0x200B `memcmp`**。

因此可以确认 MPALL 存在真实的 **512B F2 INFO write/readback path**。但进一步
逐偏移审计证明，不能把这512B直接当成物理 LBA3：

- MPALL 的正式 INFO 校验路径要求 staging 开头为 `12 01 00 02`；真实 Kingston
  非零 LBA3 则从 `00 01 00 00` 开始，而且整扇均不存在 `12 01 00 02`；
- `GetInfo.exe` 在打印 `Get_Info_Page INFO` 后先清 `0x4D1EC0` 的0x210B，再把
  `0x4D1EC0` 与字符串 `INFO` 交给设备读取函数。相邻 `0x4D1CB0 / 0x4D1EC0 /
  0x4D20D0` 恰为连续三个0x210B response work buffer，对应 Version/INFO/RD 路径；
- GetInfo 的正式制造字段也位于完全不同的位置：`SampleMark` 由对象字段控制，
  写到 raw INFO response `+0xD9/+0xDA` 为 `12 56` 或 `00 00`；`MPF1F2` 则按
  controller generation 从 INFO `+0x93` 或 `+0xDF` 的 bitfield 解析；这些偏移
  均与 LBA3 `+0x001/+0x020..027` 不对应；
- MPALL 包内两份 `FW*.BIN` 与两份 `BN*.BIN` 的 `"this is mp mark"` 都在各自
  **最后512B的 `+0x000`**，而实体 LBA3 marker 在 **`+0x1F0`**；
- 两种真实 LBA3 的8B材料 `a882a42200200216` / `b57e9c4500800014` 以及各自
  4B半段，在已取得的 MPALL EXE、FW/BN BIN 中均无直接常量命中。

所以本轮真正闭合的是：**Phison F2-mark/F2-INFO 制造生态确实存在独立512B
写入/回读协议，但“F2 INFO staging == LBA3”已被结构反证。** `WriteF2Mark`
只能继续作为厂商家族/邻近制造路径证据，不能作为 LBA3 exact producer 记账。

controller 型号仍不能锁死。本地 strict 非零盘为 Kingston DataTraveler 3.0
`VID=0951/PID=1666`、121110528 个 512B sector，即 `62008590336B`；
Psychson 历史真实记录中同一 VID/PID/model/物理容量至少同时出现：
**PS2307 + MPALL v3.34.07** 与 **PS2309 + MPALL v5.35.35**。
因此这些外部标识只支持 Phison family 归属，不能证明本地盘必为某一个
PS22xx controller；后续应优先差分这两个精确量产代际的 F2 INFO 路径。

因此至少存在 **两个非零 manufacturer/MP profile + 一个 zero profile**；尾 marker
可以作为 MP payload 家族锚点，但中间8B绝不是固定常量。新增
`tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex`，其512B
原始扇区 SHA-256 为
`a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e`，并以
`lba3_mp_marker_has_multiple_real_historical_payload_profiles` 对两个真实 marker
profile 做差异门禁。

因此 LBA3 不能继续作为“完全不知道边界”的 UNKNOWN，也不能把尾部 ASCII
误建模成一个独立 EDP 字段，更不能把某一份 `+0x020..027` 当成固定模板。整扇
512B 继续保持 PARTIAL：
**EDP preserve/ignore 边界与 Phison 制造家族已闭合，MPALL/GetInfo 又提供了
独立 F2 INFO 路径及其与 LBA3 不同构的直接反证；但 LBA3 自身 exact producer、
`+0x001/+0x020..027/+0x1F0` 的生成规则/字段定义和 controller firmware consumer
仍缺失，所以严格规则下仍是 0B 可计 COMPLETE。**

#### LBA4：必须区分历史 raw-zero gap 与 current SAFE6 full rolling

- `0x00..0x17`：明文 `$$$<onlyid>$$$` 及填充。
- `0x18..0x46`：有效 rolling-XOR 密文区，必须无条件按 16 位 word 解码；密文字节自然等于 0 也不能跳过。
- `0x47..0x1fb`：真实盘存在 raw-zero gap 与 rolling-encrypted-zero 两种物理表示；当前22份生成参考都没有在该437B恢复出非零业务 payload。
- `0x1fc..0x1ff`：尾部 rolling-XOR 锚点，解密后为 `LLGB`。
- 新的“按区段处理”规则对当前 22/22 参考样本全部恢复：
  - `0x39..0x3c == "LLGB"`；
  - `0x1fc..0x1ff == "LLGB"`；
  - `u32@0x18 == onlyid ^ 0x88888888`。
- 旧 `inspect.rs` 的“raw 单字节为 0 就恢复成 0”规则会把真实样本 `onlyid=949028302` 的 `LLGB` 错解为 `\0LGB`；本轮已改为只在整个 `0x47..0x1fb` 未写区全零时保留该区物理零，修复后当前 22/22 参考样本均恢复双 `LLGB` 锚点。

##### LBA4 `0x47..0x1FB`：437B UNKNOWN -> PARTIAL -> COMPLETE

本轮重新对齐 Windows PE current producer、Linux DWARF producer/reader 与22份原始盘。Windows current
`CEMSUsbRegsiter.dll::sub_10014550` 与 Linux
`CLabelManage::BuildSector4@diskfile.cpp:741` 都只把0x2F restore node写到
`+0x18..+0x46`；non-null node 分支随后把 rolling XOR 继续覆盖到
`+0x1FF`，因此 `+0x47..+0x1FB` 只是随同 backing 一起被变换，并没有独立字段 producer。

Linux `ReadSector4@diskfile.cpp:957` 会对 `+0x18..+0x1FF` 执行同一 rolling XOR，
但最终只把 `decoded+0x18` 起0x2F复制给 restore-node 输出并校验
`OnlyIdXor8`；不会向调用者返回或解释 `+0x47..+0x1FB`。

严格22份原始生成参考重新统计：

- 18/22（17份 backup + 独立 SanDisk）为物理 raw-zero gap；
- 4/22 为几乎全非零 rolling 形态；
- 4/4 rolling 形态按 onlyid key 解码后437B全零；
- raw-zero 形态按区域规则保持后同样是 semantic zero。

因此这437B先具备了物理边界、current full producer变换范围、reader negative
semantic consumer 和真实双 profile 验证，从 UNKNOWN 降为 **PARTIAL**。随后继续追
first-party runtime 后，确认此前“必须找到 raw-zero 最初 producer 才能闭合”的前提本身
过强：这437B根本不是一个要求固定初始化值的业务字段，而是 **unowned backing / representation carrier**。

隔离 Unicorn 直接执行 current Windows `BuildSector4/sub_10014550`，把
`+0x47..+0x1FB` 预填为任意非零 `0xA5`：

- non-null restore-node 分支：437B raw bytes 全部进入 rolling 变换；独立按 onlyid key
  反滚后 **437/437 精确恢复原始 `0xA5`**；函数本体没有在该区写任何业务 payload；
- `arg0==NULL` 分支：同一437B **437/437 原样保持 `0xA5`**，函数完全不碰该区；
- full 分支仍由函数自身重写尾部 `+0x1FC..+0x1FF=LLGB` 后再 rolling；NULL 分支连尾锚点
  也不主动创建，进一步证明两条路径的区别是“transform existing representation”与“preserve existing representation”。

同一 full-rolling nonzero fixture 再交给 official Windows
`ReadSector4/sub_10015090` 动态执行；仅替换 MSVC `std::string/atoi` 运行库边界，rolling、
0x2F memcpy 与 `OnlyIdXor8` 校验均原生。reader 返回0，恢复
`onlyid=1625940067`、`OnllyID2Nd=main`、`LLGB`、Version=1；返回对象仍只有0x2F
restore node，437B backing 不进入 API 输出。

这与 LBA5 的 opaque-preserve 口径完全一致：**COMPLETE 描述的是字节的生命周期/所有权，
不是宣称当前样本中的零值是协议常量。** raw-zero 历史最初是谁写入已经不再是业务语义
blocker；对未知非零 backing，兼容实现必须 preserve 或按 full branch 可逆 rolling，禁止清洗。

因此 `+0x47..+0x1FB` 共437B从 PARTIAL 升 **COMPLETE**。新增
`official_virtual_lba4_full_nonzero_backing.hex` / `official_virtual_lba4_null_nonzero_backing.hex`
与回归 `official_virtual_lba4_backing_is_unowned_and_representation_only` 固定任意非零正例。

强化门禁 `lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes`：
committed real fixtures 必须同时覆盖 raw-zero 与 rolling-encrypted-zero 两种物理表示，
并断言两类样本的 semantic gap 都为 zero[437]。

继续按 identity node 形态与物理表示做交叉审计后发现一个关键反例：raw-zero **并不等于
legacy generation**。严格原始 Kingston `onlyid=1625940067 @ 2026-08-27 17:30:24`
同时满足 current identity 条件 `OnllyID2Nd==main onlyid && HSerialCRC[5]==0`，但其
`+0x47..+0x1FB` 仍为 raw-zero；同设备 `17:28:57` 只读快照的 LBA0-13 与
`17:30:24` 逐字节完全一致。仓库新增 LBA4 单扇区证据
`tests/fixtures/protocol_evidence/kingston_20260827_current_identity_raw_zero_lba4.bin`，
SHA-256=`85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec`，以及
回归 `lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile`。

因此 short/full representation 的真实选择条件仍不能由第二ID/HSerial代际形态反推；
但在 backing 已闭合为 unowned representation carrier 后，这个选择条件只影响 **wire
representation**，不再影响437B的业务语义完成度。它仍值得追踪用于历史复现，却不再是
这437B的 COMPLETE blocker。

##### current SAFE6 分支纠偏：Provision 现有 short canonical 不是官方 current writer

本轮回机器码确认 `sub_100880D0` 是标准 strcmp 语义：返回0表示字符串相等。
`CUsbRegsiter::RegsiterUsb` 对 `SAFE6` 相等分支会构造0x2F restore node并调用
`sub_10014550(&restore_node, main_onlyid, LBA4)`；也就是说**官方 current SAFE6
注册路径传 non-null restore node，执行 full rolling loop**。

审计开始时仓库 `src/provision/generate.rs::build_lba4` 仍在 active node 和 trailing
LLGB 之间强制 `LBA4[0x47..0x1FB]=0`，并把这种 raw-zero short form 称作
“current Windows writer profile”。这与 current 官方 SAFE6 producer 不一致，因而被
本轮列为 blocker。下节完成 server-flag producer/reader/22盘闭环后，该 blocker 已
实际修复：Provision 现使用 full rolling + post-XOR flags；历史 raw-zero 实盘只保留
兼容读取，不能再作为新盘 canonical。

##### LBA4 `+0x45/+0x46`：post-XOR family 已扩展到 v19；inspect 不再按 identity 猜表示

Linux DWARF 将 restore node 最后2B正式命名为：

- `node+0x2D = bDataToServer`；
- `node+0x2E = bConnetServer`。

current Windows/Linux 与 historical Windows v19.11.4.1 已独立闭合相同的 post-XOR
wire rule：

- Windows `sub_10014550`：完整 rolling loop 后执行
  `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 做完整 0xF4-word rolling，随后
  `0x1D302..0x1D329` 同样 post-XOR 写回这2B；
- v19.11.4.1 `fcn.10006090`：rolling 后于 `0x10006249..0x10006257` 把
  `node+0x2D/+0x2E` 写到物理 `+0x45/+0x46`。其 SAFE6
  `virtual_56@0x1000CC50` 可从 object/request 复制 legacy HSerial material 到 node，
  因此 post-XOR 表示并不要求 `OnllyID2Nd==main && HSerial==0`。

reader 也已逐指令复核：

- Windows `sub_10015090` 统一 rolling 后复制 0x2F node，只校验
  `OnlyIdXor8`，不恢复两字节；
- Linux `ReadSector4@0x1E048, diskfile.cpp:956` 在
  `0x1E18C..0x1E1D8` 做同一 rolling，再 memcpy 0x2F node，仍只校验
  `OnlyIdXor8`，同样没有补偿 post-XOR store。

所以这里存在真实的 producer/reader 非对称，但应限制在 **已证明的 post-XOR writer
family**，不能限制在某种 identity shape。对这类 writer，物理 raw flag 就是
producer node flag；generic rolling 后得到的是官方 reader transformed byte。另一些
older physical samples 与 ordinary rolling representation 相容，但 exact writer 仍缺。

旧22份原始生成参考的 census 仍保留为历史观测，不再作为 representation classifier：

- 22/22 physical bytes 与 generic rolling 输出不相等；
- 6/22 current-identity：同时满足
  `OnllyID2Nd==main onlyid && HSerialCRC[5]==0`，physical=`00 00`，generic
  为6组不同非零值；这与当前 Windows `RegsiterUsb` 对 node 整体清零、只赋值到
  `+0x2C`、再由 BuildSector4 post-XOR 写回两个0字节完全吻合；
- 14/22 legacy-identity：`OnllyID2Nd!=main onlyid && HSerialCRC[5]!=0`，physical 非零，
  generic=`00 00`；
- 2/22 legacy-identity（Aigo rev_pmap + SanDisk）：
  physical 非零，generic=`0B 00`。

真实免密 SanDisk 进一步给出 identity classifier 的直接反例：second=`0x4A32BA39`、
HSerial非零，wire=`00 00`，current official reader=`D4 D9`。仓库
`scripts/protocol/probe_lba4_reader.py` 固定 DLL SHA-256
`122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb` 与 gold SHA-256
`d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258`，隔离执行
`ReadSector4@RVA 0x15090`，实际覆盖到 `0x15295`、无异常，得到相同结果。onlyid 的
K0=`0xBFED`，两个 flag 位置的 rolling key byte 分别=`D4/D9`，因此 wire-zero 足以
机械地产生 reader `D4 D9`，但不能反推 producer 业务值。

实现已经同步修正：

- `src/inspect.rs`：先 rolling 解码并处理历史 raw-zero gap，`decoded` 始终保持
  official ReadSector4 view；不再根据 second/HSerial 覆盖 flags。两个字段同时显示
  `reader=` 与 `wire=`，producer-side 明确标记为需要 writer provenance；
- `src/provision/generate.rs`：current SAFE6 改为完整
  `+0x18..+0x1FF` full rolling，之后执行相同的2B post-XOR覆盖；
- `src/provision/validate.rs`：不再接受“物理 gap 全零”作为 current canonical，
  而是精确重建 official full-rolling wire image 并比较；
- 历史 raw-zero short form 继续由 inspect 兼容读取，不被删除。

上层 consumer 继续向下追后得到更严格的负边界：Windows
`ReadRestorInfo/sub_10041290` 自身不修正这2B；`ActiveNormalUDev -> sub_1003CEB0`
只消费 restore node 的身份材料；`GetUpLoadInformation/sub_10039C30` 会把 node
作为 API 输出的一部分带出去，但本 DLL 内不读取 `+0x2D/+0x2E`；Linux
`libcemsfilesyscheck.so` 除 BuildSector4 的两次 post-XOR store 外，也没有其它
对这两个字段的直接访问。

继续把两个 BYTE 拆开后：

- `bDataToServer @ +0x45` 的 legacy official-reader 逻辑视图确有非零 `0B` profile。
  本轮按主文档 caller-owned 门槛补齐：v19 official writer 原生注入 `node+0x2D=0B`
  后只改变 wire `+0x45` 一个字节，证明 writer 是透明序列化边界；strict Aigo
  `onlyid=1987718388` 的 wire `64 7A` 又按正式 rolling 精确还原为 logical
  `0B 00`（key bytes=`6F/7A`）。结合 Windows/Linux/2021 reader 的完整node
  structural-preserve 以及 restore/upload 路径无该BYTE值相关分支，本字节按
  **caller-owned compatibility metadata** 升为 COMPLETE。更早 ordinary-rolling
  manufacturing EXE 仍未知，但只影响 representation provenance；
- `bConnetServer @ +0x46`：current Windows/Linux 与 v19.11.4.1 SAFE6 node constructor
  都由 zero-init 保持该 byte=0；historical rolling-form encrypted originals 的正式 reader
  view 同样为0。更关键的是 `scripts/protocol/probe_lba4_v19_writer.py` 保留真实免密
  SanDisk 的 second=`0x4A32BA39` 与 nonzero HSerial，只把 node flags 设为`00 00`，在
  memory-backed I/O 中原生执行 v19 `fcn.10006090`，最终输出与真实 LBA4 **512/512
  完全一致**、SHA-256=`c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8`，
  且无 emulator exception。故 authentic reader=`D9` 已正向闭合为 post-XOR representation
  artifact，而不是 nonzero producer flag。再结合 2021 historical repair 的整9扇区 raw-copy
  行为与跨代 negative semantic consumer，本字节重新闭合为 **COMPLETE**。COMPLETE 不代表
  reader 必为0，也不把 v19 虚拟重建冒充该物理盘 exact manufacturing provenance。

##### LBA4 `0x18..0x46` 官方结构与第二 ID

Linux DWARF 恢复出 `edpdiskglobal.h::tagEdpPartionRestorInfoNode`，总长 `0x2F`，正好对应 LBA4 `0x18..0x46`：

- `+0x00` / LBA4 `+0x18`：`OnlyIdXor8`，读写两端均闭合为 `onlyid ^ 0x88888888`；
- `+0x04` / LBA4 `+0x1C`：`OnllyID2Nd`；
- `+0x08..+0x1B` / LBA4 `+0x20..+0x33`：`HSerialCRC[5]`；
- `+0x1C` / LBA4 `+0x34`：`SingleUsbFlg`；
- `+0x1D..+0x20` / LBA4 `+0x35..+0x38`：`MyHardinfo`；
- `+0x21..+0x24` / LBA4 `+0x39..+0x3C`：`NewLabFlag`，当前样本为 `LLGB`；
- `+0x25..+0x28` / LBA4 `+0x3D..+0x40`：`Version`；
- `+0x29..+0x2C` / LBA4 `+0x41..+0x44`：四个 sector 字段；
- `+0x2D/+0x2E` / LBA4 `+0x45/+0x46`：两个 server flag。

Linux DWARF 同时恢复 `diskfile.h::UsbLabelParam`：`+0x278..+0x28B = HDOnlySerial[5]`，`+0x28C..+0x2AB = szOnlyID[32]`。该结构嵌在 Windows `CUsbRegsiter object+0x2E0`，所以此前把 `object+0x55C` 直接叫作 `onlyID2Nd` 是错误的；`object+0x55C` 实际只是 `HDOnlySerial[1]`。

当前 Windows 新建分支的真实写链是：`object+0x698 -> node.OnllyID2Nd`，而 `object+0x698` 已闭合为 `CoCreateGuid -> GUID raw 16B -> CRC32_bare -> main onlyid`。同时 `object+0x558..0x568 -> HSerialCRC[5]`。因此当前 writer profile 的第二 ID 等于本次新生成的 main onlyid。

继续向 runtime/服务器备份路径追后，`OnllyID2Nd` 的行为语义已不再只是字段名推断。
当前 `CEMSUsbRegsiter.dll` 形成了完整的 **backup encrypt -> server/storage -> activation decrypt**
闭环：

- `GetUpLoadInformation/sub_10039C30` 读取 restore node 后，先从盘面解出 LLGB 标签和
  v0x0202/v0x0206 EDPF 分区信息，拼成 upload/backup blob；最终调用
  `sub_10001190(..., key=restore_node+0x04, key_len=4)` 对整份 blob 加密；
- `ActiveNormalUDev/sub_100399A0` 先 `ReadRestorInfo` 得到同一0x2F node，再进入
  `sub_1003CEB0`；后者直接把 `arg0+0x04`（即 `OnllyID2Nd`）作为4B key seed
  传给 `sub_100012D0`，原地解密外部传入的 activation blob；
- 解密后的第一DWORD必须是 `LLGB`。成功后 `sub_10041480` 把 LLGB 标签内容写回
  LBA8；随后按尾部 Version `0x0206/0x0202` 拆出 0x120/0xC0 分区表，调用
  `sub_100414F0` 重建 LBA12；
- `sub_10001190` 与 `sub_100012D0` 的 key derivation 完全同构：对16个 key byte，
  `key16[i] = key4[i mod 4] XOR base16[i]`，机器码中的 base16 精确为 ASCII
  **`EDPSECDISK200709`**。随后两者调用同一 key schedule，块变换分别走
  `sub_100028A0` 与 `sub_10002A00`，构成 encrypt/decrypt 对。

因此 `OnllyID2Nd` 应按行为命名为 **backup/activation encryption key seed**；“第二ID”
只是历史结构名。current writer 把 main onlyid 直接复用为该 seed；legacy profile 则保存
独立 seed。对 `nopwd_tool/backup` 可由文件名提供 main-onlyid 的22份历史捕获重算后，
15份 legacy 中 **15/15 second key 都不等于本设备组或全语料任何 main-onlyid**；相同
main-onlyid 的重复捕获又保持 second key 稳定，排除“上一次 main-onlyid”解释。committed
三份 legacy fixture 进一步锁定：NETAC_A=`44D9CE02`、NETAC_B=`028EFFD3`、
LEXAR=`7647B1EF`。

此前这里只缺 legacy producer；本轮已由官方历史二进制补齐。

取得并哈希核验的归档 `CEMSUsbRegsiter.dll` v19.11.4.1，
MD5=`783d01f19e998a514834bc5e5f4249ad`。该版本仍有可达 SAFE6 注册路径
`ISUdiskRegsiterObj::virtual_56@0x1000CC50`。其机器码在识别 `"SAFE6"` 后：

```text
0x1000D109  memset(node, 0, 0x2F)
0x1000D11D  call fcn.100058E0
0x1000D122  node+0x04 = EAX          ; OnllyID2Nd
```

`fcn.100058E0@0x100058E0` 又可完整恢复：

```text
GUID g;
CoCreateGuid(&g);
init_crc_table();
crc = 0;
for byte in raw_bytes(g)[0..16]:
    crc = (crc >> 8) ^ table[(crc_low8 ^ byte) & 0xFF];
return crc;
```

也就是说，旧 SAFE6 writer 的第二 key 不是 main onlyid、device-id、MBR signature
或宿主硬件号派生，而是**单独生成的一枚 GUID 的16个原始字节经协议同款
CRC32_bare 得到的32-bit随机 key**。current writer 则取消第二次随机生成，直接把
main onlyid（它本身也是 `CoCreateGuid -> CRC32_bare`）复用到 `OnllyID2Nd`。

这解释了此前所有真实 profile：

- current：`OnllyID2Nd == main onlyid`；
- legacy：`OnllyID2Nd` 与 main onlyid 独立；
- 同一已制盘标签的重复备份中 second key 保持稳定，因为随机只发生在制标写入时，
  后续只是持久化读取；
- committed NETAC_A/NETAC_B/LEXAR 的 exact second key 继续锁定为
  `44D9CE02/028EFFD3/7647B1EF`；
- 回归进一步证明这些 legacy key 既不等于 main/其它 main-onlyid，也不等于
  `CRC32(device_id)`、LBA0 disk signature 或 `MyHardinfo`，防止再次把随机
  backup key 误归类为目标盘/宿主机身份。

结合已经闭合的 backup encrypt / activation decrypt 双向 consumer，
`OnllyID2Nd` 的代际差异现在只剩“seed 来源不同”，其业务语义没有分叉。
因此 LBA4 `+0x1C..+0x1F` **4B 从 PARTIAL 升 COMPLETE**。

这一结论本轮又回到 **PE 机器码**重新核验，避免依赖 Hex-Rays 风格 `.m`
伪代码的局部漏语句：

- `RegsiterUsb@0x1003BBBB` 先把 0x2F-byte restore node 清零；
- `0x1003BBD1` 读取 `object+0x698`；
- `0x1003BBD7` 写入 `ebp-0x3C`，按该 node 的栈基址精确对应
  **`node+0x04 OnllyID2Nd`**；
- `0x1003BBF0..0x1003BC79` 依次读取
  `object+0x558/+55C/+560/+564/+568`，并写到
  **`node+0x08/+0x0C/+0x10/+0x14/+0x18`**，即五个
  `HSerialCRC DWORD`；
- 随后 `sub_10014550(node, object+0x698, LBA4)` 再写
  `node+0x00 = parsed_main_onlyid ^ 0x88888888`，复制完整 0x2F node，
  并执行 rolling-XOR。

对应的 `.m` 反编译文本漏掉了 `node+0x04 = object+0x698` 这一条；
因此后续审计以这里的原始机器码为准。新增回归明确锁定
**LBA4 current writer machine-code node layout**，避免再次被反编译变量布局误导。

继续向上追 current API 还得到一个重要边界：

- `WriteNormalULabel -> sub_10047690` 会把上层请求写入嵌入的
  `UsbLabelParam`；
- 它覆盖的最后几个相关块到 `UsbLabelParam+0x268` 为止；
- Linux DWARF 已知 `HDOnlySerial[5]` 在
  `UsbLabelParam+0x278..+0x28B`；
- current `sub_10047690` **没有任何对 +0x278 这20B的写入**。

所以当前注册接口并不存在“把某个新算出的 5×DWORD HSerial 填进去”的步骤；
current-style HSerial 为零来自当前对象初始化/未提供输入这一 profile。
这比“current writer 恰好写零”更精确，但仍不能解释旧盘的非零五元组。

当前 22 份参考样本分三组：

- 6/22：`OnllyID2Nd == main onlyid` 且 `HSerialCRC[5] == 0`，完全吻合当前 Windows writer；
- 14/22：`HSerialCRC[5]` 固定为 `00001D29, 0000007B, 000004DD, 00000079, 0000007C`，跨 Aigo/Lexar/Netac 等厂商复用，但 `OnllyID2Nd` 随标签实例变化；
- 2/22：另一组高熵 `HSerialCRC[5]`，Aigo/SanDisk 之间部分成员重合。

本轮按严格“21份非转换 backup + 独立 SanDisk 原始加密盘”的 22份生成参考集
重新做交叉统计，得到一个此前没有单独写死的强约束：

- **6/22**：`OnllyID2Nd == main_onlyid`，同时 `HSerialCRC[5] == 0`；
- **16/22**：`OnllyID2Nd != main_onlyid`，同时 `HSerialCRC[5] != 0`；
- 两个条件在当前22份原始生成参考上是 **22/22 双向等价**，没有交叉反例。

这里特别排除了 `nopwd_tool/backup` 中未带 `_nopwd_` 名称、但内容已经确认
属于免密转换态的 Aigo `onlyid=2071754312 @ 20260828_120932`，并补回独立
SanDisk 原始加密盘；如果直接扫 backup 目录会得到错误的 7/15 计数。

这进一步说明 `OnllyID2Nd` 与 `HSerialCRC[5]` 至少属于同一代
restore-node profile，而不是两个可独立任意组合的字段；但“相关”仍不等于
已经找到旧 producer。

本轮又把 current producer 向结构构造层前推：

- Linux DWARF 明确给出
  `UsbLabelParam.HDOnlySerial[5] @ +0x278..+0x28B`；
- Linux `UsbLabelParam::UsbLabelParam()` 在
  `diskfile.cpp:553..556` 对完整 `0x2AC` 结构执行清零；
- Linux `UsbWriteParam::UsbWriteParam(UsbLabelParam&)`
  在 `diskfile.cpp:558..568` 逐项复制单位、部门、姓名、GSerial、Label、
  AutoID、备注等字段，但**没有复制 +0x278 的 HDOnlySerial[5]**；
- Linux `CLabelManage::BuildSector4@diskfile.cpp:741..` 接收的是已经构造好的
  `tagEdpPartionRestorInfoNode*`，只负责把 0x2F-byte node 写入 LBA4、
  写尾部 LLGB 并执行 rolling-XOR；函数内部**完全不生成 HSerialCRC**。

Windows 与之完全同构：

- `CUsbRegsiter` 内嵌 `UsbLabelParam` 基址是 `this+0x2E0`；
  因而 `HDOnlySerial@+0x278` 精确对应 `this+0x558..+0x568`；
- `sub_10047690(request, this+0x2E0)` 填充 current `UsbLabelParam` 时
  同样跳过 HDOnlySerial 20B；
- `RegsiterUsb` 进入正式制标前还调用
  `sub_100139F0(this+0x2E0 -> temp UsbLabelParam)`，该拷贝函数也逐字段复制，
  **再次跳过 +0x278..+0x28B**；
- 最终 LBA4 writer 才从 `this+0x558..+0x568` 读取 5×DWORD 写入
  restore node。也就是说 current 路径从默认构造、请求转换、临时拷贝到
  最终序列化，均没有任何“计算 HSerial”的步骤。

因此 current profile 的 `HSerialCRC[5]=0` 已经不是单点观察，而是
**跨 Windows/Linux 两套官方 producer 的零来源闭合**。但是旧16份非零 profile
使用的“第二 ID + 5×DWORD HSerial”上游注入接口/算法仍未在现存官方构建中找到；
当前收集的 `usbtoolbusmanage.dll` 上层 `BusManageImp::WriteNormalULabel`
也未出现主机硬盘序列/DeviceNumber 到这20B的连接。

所以本轮仍不把 `HSerialCRC[5]` 升为 COMPLETE：完成的是 current-zero producer
和 old/current profile 边界，而不是 legacy 非零 producer + consumer。
新增门禁 `lba4_current_writer_profile_never_carries_legacy_hserial_material`
在仓库提交的7份完整原盘子集中固定“current mirror -> zero /
legacy non-mirror -> nonzero”的关系；22份完整统计继续由本机原始证据集审计。

同一轮 22份原始盘还重新统计了 restore node 后半：

- `SingleUsbFlg @ LBA4+0x34`：22/22 = 0；
- `NewLabFlag @ +0x39..0x3C`：22/22 = `LLGB`；
- `Version @ +0x3D..0x40`：22/22 = 1；
- 四个 sector byte `+0x41..0x44`：22/22 = `08 04 0C 01`；
- `MyHardinfo @ +0x35..0x38` 与两个 server flag `+0x45/+0x46`
  则明显随 profile 变化。

本轮继续把 `MyHardinfo` 与其它已经恢复的身份字段做逐盘交叉，得到一个新的
强约束：**strict 22份原始参考逐盘均满足
`LBA4.MyHardinfo == LBA8.HDSerialInfo`**。这不是“多数相同”，而是22/22精确相等：

- current profile：两处同时为0；
- legacy profile 的非零集合为
  `A017AD78 / A68BAE08 / 8B4613F5 / 2AB0E33C`，两扇区逐盘完全镜像；
- 同一 Netac `0dd8:2005` 的多个不同 onlyid 都稳定为 `A017AD78`；
- 该DWORD逐盘既不等于 `CRC32(device_id)`，也不等于 MBR disk signature，排除
  “U盘自身ID/MBR值的简单副本”解释。

current producer 也进一步闭合：`RegsiterUsb@0x1003BBBB` 对完整0x2F restore node
执行 `memset(0)`；随后写 OnllyID2Nd、HSerialCRC、SingleUsbFlg、LLGB、Version、
sector tuple，却**没有任何对 node+0x1D..0x20 的覆盖**，因此 current
`MyHardinfo=0` 是正式 writer 行为，不是样本巧合。

这把字段角色从 opaque profile DWORD 收敛为 **LBA8.HDSerialInfo 的 mirrored
host-hardinfo compatibility copy**。但严格 COMPLETE 门槛仍未跨过：2020
HDSerialInfo producer family 已知，而 strict legacy 盘对应的更早 producer / 将同值写入
LBA4 MyHardinfo 的直接 copy 点仍未定位，所以本轮只增加语义证据，不增加 COMPLETE 字节数。
新增回归 `lba4_myhardinfo_mirrors_lba8_hdserialinfo_in_original_profiles` 锁定零/非零双profile。

current Windows 机器码确实在 node 清零后显式写入前四项固定值：
`SingleUsbFlg=0`、`NewLabFlag=LLGB`、`Version=1`、
`08 04 0C 01`。当前 Windows/Linux `ReadSector4` 路径都会把完整
0x2F node 在 rolling decode 后结构性返回，只对 `OnlyIdXor8` 做强校验，
而对这四组固定字段没有值相关分支。按当前仓库已经统一用于 write-only / compatibility
metadata 的 COMPLETE 标准，这种**structural-preserve + semantic-ignore** 是明确的
consumer 行为，而不是“consumer未知”。因此把这17B重新拆分：

- `SingleUsbFlg@+0x34` 1B：COMPLETE；
- `MyHardinfo@+0x35..0x38` 4B：仍 PARTIAL，因值分 profile 且 producer/选择条件未闭合；
- `NewLabFlag@+0x39..0x3C` 4B：COMPLETE；
- `Version@+0x3D..0x40` 4B：COMPLETE；
- sector tuple `+0x41..0x44` 4B：COMPLETE。

上述固定 metadata 共13B均有回归覆盖，但其中 `NewLabFlag` 4B 在旧36B LBA4基线中已经作为第二个 `LLGB` 锚点计入，因此本轮严格进度只净新增9B，禁止重复累计。新增回归把这些字段从代表盘扩展到全部 committed original fixtures；22份严格原始集
仍维持 Single=0 / LLGB / Version=1 / `08 04 0C 01` 无反例。这里闭合的是
**fixed restore-node compatibility metadata lifecycle**，不是宣称这些值永远不能在
未来协议版本中变化。

特别是 6份 `HSerialCRC=0 && OnllyID2Nd=main_onlyid` 的 current-style
实盘，其 server flags 仍分别出现非零变化，说明“current-style HSerial”
不能进一步推导整个 restore node 都是当前 DLL 的同一静态 profile。

这直接否定“`HSerialCRC[5]` 必然是当前 U 盘自身唯一序列”的强解释：

- 已提交 Lexar 与 Netac 真实夹具的 device_id、VID 均不同，但解出的
  `HSerialCRC[5]` 逐字节完全相同且非零；审计测试显式锁住这个反例；
- 22 份全量样本还呈现明显 profile 聚类：
  - 上述 14 份固定 HSerial 组全部同时表现为 LBA9 `EETU+SAPF`；
  - 当前 writer 形态的 6 份 `HSerialCRC=0 && OnllyID2Nd=main onlyid`
    全部同时表现为 LBA9 `EETU+EPPE`；
  - 另外 2 份高熵 HSerial 样本同时是 LBA9 全零、LBA6 扩展区非零。
  这只能记为**格式/注册环境 profile 的相关性**，不能反推因果关系。

本轮又把 `bDataToServer@+0x45` 的非零 reader profile 与这组代际指纹精确对齐：
旧备份集中唯一 `reader=0B 00` 的样本就是 Aigo U335
`rev_pmap / onlyid=1987718388`。独立按正式 rolling 算法解码得到
`HSerialCRC[5]=B5FF9C55/B39DAB28/7E8F5D4A/9AC9605A/7AC6D637`、
`MyHardinfo=8B4613F5`，physical flags=`64 7A`；同一份盘又是 LBA9 整扇全零、
LBA6 扩展区非零，并且是严格参考中的唯一 CHS MBR profile。也就是说已观察到的
`bDataToServer reader=0B` 不属于 14份固定 `1D29/7B/4DD/79/7C + SAPF` 主流 legacy
代，而与**高熵 HSerial / zero-LBA9 / CHS** 的更老 profile 同现。这进一步缩窄了
missing writer 的代际范围，但在取得那个 writer 前，仍禁止把 `0B` 自行解释成某个
业务枚举值。

同一物理 Aigo U335 / 同一 `onlyid=1987718388` 还有一份 2026-09-16 免密转换后快照。
与 2026-08-27 原始加密备份逐扇区比较时，LBA0/LBA6/LBA7/LBA11/LBA12 均发生变化，
但 **LBA4 512/512 byte 完全一致**（两份 LBA4 SHA-256 均为
`aad70723b3c1...`）。因此 `reader=0B 00`、高熵 HSerial 和 `MyHardinfo=8B4613F5`
都明确早于免密转换并被转换路径原样保留；不能再把这个 flag profile 解释为 nopwd 工具的
派生结果。回归 `lba4_old_server_flag_profile_survives_nopwd_conversion_bit_exact` 固定该纵向证据。

Linux DWARF 还把该 node 的静态使用面收窄：`LPEDP_PARTION_RESTORINFO_NODE`
在 `libcemsfilesyscheck.so` 这个编译单元中只作为 `BuildSector4@diskfile.cpp:740` 与
`ReadSector4@diskfile.cpp:956` 的参数出现；`ReadSector4` rolling 后只整体复制 0x2F node，
随后唯一字段级判断是 `OnlyIdXor8`。所以该组件没有 `bDataToServer/bConnetServer` 的
值相关业务消费者。这个 negative consumer 证据缩小了 `+0x045` 的缺口，但旧 `0B`
producer 与其它上层组件是否消费它仍未取得，故状态继续 PARTIAL。


本轮又对 committed Lexar + Netac A/B/C 四份固定-HSerial 原盘做跨字段负相关审计，
把两个容易误连的候选关系明确排除：

- 四盘的 `HSerialCRC[5]` 20B **逐字节完全相同**，均为
  `1D29 / 7B / 4DD / 79 / 7C`；
- 但 Lexar 的 `MyHardinfo/LBA8.HDSerialInfo=2AB0E33C`，Netac 三盘则稳定为
  `A017AD78`，因此这20B不是 host-hardinfo DWORD 的展开/分片；
- 同一批固定-HSerial 盘的 SAPF decoded tail `+0x114..+0x11F` 同时覆盖
  **全零、稳定非零和同一 Netac 后续变化**三种 backing 形态；特别是 Netac A/B
  tail相同，而 Netac C tail 已变化，但 HSerial 仍完全不变。

新增回归
`lba4_fixed_hserial_is_independent_from_hardinfo_and_sapf_backing`
把这组四盘反例锁死。因此旧 `HSerialCRC[5]` 不能再解释为
`MyHardinfo/HDSerialInfo` 的展开，也不能解释为 SAPF 32B backing/tail 的缓存；
其旧 producer 输入仍应继续沿独立的主机/注册环境身份链追踪。

主机身份候选链也进一步做了排错：

- Linux `libbusManage.so::UserInfo::GetHDiskSerialZ()` 会读取注册主机硬盘序列，
  因而“主机身份参与旧 profile”仍是合理候选；
- Windows `vrvaud_c` 的确存在 `EDPUToolClientInfo/HDSerialCRC`，其来源已闭合到
  `DeviceNumber.dll::EDP_DiskNumber()`，失败时回退 `EDP_DeviceNumber()`；
- `EDP_DiskNumber()` 会枚举 `PhysicalDrive0..3`，并内含标准
  CRC32 多项式 `0xEDB88320`，但公开结果最终只是**单个 32-bit DWORD**；
- 当前反编译语料中 `DeviceNumber.dll` 只在 `vrvaud_c` 身份/策略链出现，
  未在 `cemsusbregsiter/usbtoolbusmanage` 注册写链发现连接；
- `cemsudisk` 的 `HDSerialNumber` 位于 `CallBackLog::BuildLog` 审计字段采集，
  同样不是标签写链；
- 对候选二进制做 20B 精确扫描，也没有发现
  `1D29/7B/4DD/79/7C` 或高熵 HSerial 数组的静态常量。

本轮又把 2020 `busManage.dll` 中 `"ReadUsbHserialsInfo failed"` 的调用点按
**接口 ABI** 追到底，得到比“没有发现 DeviceNumber 到 20B 的连接”更强的直接负证据：

- 字符串 `0x1002D05C` 的唯一代码 xref 落在
  `fcn.10010730@0x10010730` 的 `0x100108CC`。实际调用位于
  `0x10010891..0x100108C6`：caller 先准备三个输出区，第一项由
  `DWORD 0 + memset(后续 0x2B)` 组成连续 **0x2F-byte restore node**，第二、第三项
  各为独立 4B DWORD，然后通过 CEMSUsbRegsiter 接口 `vtable+0x2C` 调用；返回0才打印
  `ReadUsbHserialsInfo failed`。这不是 import，也不是本地 helper，而是历史接口的虚方法槽。
- v19.11.4.1 `CEMSUsbRegsiter.dll` 的 `ISUdiskRegsiterObj` vtable 已精确定位到
  `0x1019DB54`；因此 `+0x2C` 正好是
  `ISUdiskRegsiterObj::virtual_44@0x100054A0`。该方法把**第一个输出指针**依次交给
  `fcn.100072C0 / fcn.1000DDA0 / fcn.1000DB90` 三个主/备 reader。主 reader
  `fcn.100072C0` seek 到 `4*BytesPerSector`，完成 `$$$`/onlyid 校验与 rolling decode 后，
  以 `16 + 16 + 8 + 4 + 2 + 1` 字节的连续 stores 精确回填 0x2F-byte node；其中
  `node+0x08..+0x1B` 的 20B HSerial 是**从盘面 restore node 解码回来**，不是此函数运行时生成。
- 只有 restore node reader 成功后，`virtual_44@0x100055A2..0x100055BD` 才分别调用
  `UsbTools.dll` ordinal3=`EDP_DeviceNumber` 与 ordinal4=`EDP_DiskNumber`，并把两个
  返回值分别写入**第二、第三个 4B 输出指针**。因此旧 ABI 明确是
  `restore-node[0x2F] + DeviceNumber DWORD + DiskNumber DWORD` 三条独立输出通道，
  而不是“DeviceNumber 直接填 HSerial[5]”。
- 2020 `busManage` 成功后没有消费那两个 host-identity DWORD；它只把第一项
  0x2F-byte node 传给同一接口的 `vtable+0x44`。在 v19.11.4.1 vtable 中该槽精确映射
  `ISUdiskRegsiterObj::virtual_68@0x1000E650`；restore 路径从传入 node 只取
  `node+0x04 OnllyID2Nd` 作为恢复密钥，不读取 `node+0x08..+0x1B` 的 HSerial 值。
- current `usbtoolbusmanage.dll` 又提供代际对照：`ActiveNormalUDev` helper
  `fcn.100AA7E0` 已改为通过新接口 `vtable+0x14` 直接调用 `RestoreRegsiterUsb`，二进制中
  不再存在旧 `ReadUsbHserialsInfo` 三输出调用形态，确认上述 `+0x2C/+0x44` 是历史 ABI，
  不是对 current 接口的误配。

同一套 v19.11.4.1 writer 还把 **HSerial 的历史写入 transport** 闭合到了 LBA4：

- `ISUdiskRegsiterObj::virtual_8@0x1000B9C0` 将
  `request+0x150/+0x154/+0x158/+0x15C/+0x160` 五个 DWORD 原样写到
  `object+0x2488/+0x248C/+0x2490/+0x2494/+0x2498`。
- SAFE6 `virtual_56` 构造 0x2F restore node 时逐项读取上述对象槽并放入
  `node+0x08..+0x1B`，随后调用 `fcn.10006090`。后者以 `4*sectorSize`
  seek 到 **LBA4**，把传入 node 编码进 `$$$...$$$` 扇区并写回。因此历史传输链为
  `request+0x150..+0x160 -> object+0x2488..+0x2498 -> node+0x08..+0x1B -> LBA4`。
- 2020 `BusManageImp::ActiveNormalUDev` 对这份 0x184-byte request 先整体
  `memset(0)`，只填 `request+0xD0` 起的 0x40-byte 字符串区；它不覆盖
  `+0x150..+0x160`，所以这条已取得 caller 路径**确定**向五个 HSerial DWORD 输入全零。
- 继续穷举整个 2020 `busManage.dll` 后，这个负结论已从单一路径扩展到**全部已恢复
  request constructor**：模块中只有 `fcn.10010020`、`fcn.100103B0`、
  `fcn.10010580`、`fcn.10010730` 四处先对 0x184-byte request 完整 `memset(0)`；四个
  函数逐指令扫描均没有访问 `request+0x150/+0x154/+0x158/+0x15C/+0x160`。由于
  request 基址固定为 `EBP-0x190`，五个 HSerial 槽精确落在
  `EBP-0x40/-0x3C/-0x38/-0x34/-0x30`，可以排除把其它栈局部变量误认成字段访问。
  因而**这个 2020 BusManage 模块中不存在另一条隐藏构造路径会填入 strict legacy 的
  非零 HSerial 五元组**；缺失 producer 必须是更早或不同的 caller。
- `fcn.10008800` 从 `4*sectorSize` 连续读 `9*sectorSize`，即 LBA4-LBA12，
  再原样写到 **`disk_end-0x80000`**。该地址与 `fcn.1000DB90` 的第三 reader
  完全一致，所以这一候选是 LBA4-LBA12 的 512 KiB 盘尾镜像头，不是另一套 HSerial
  生成算法。

以上闭合的是“caller 输入槽 -> 对象 -> restore node -> LBA4/盘尾镜像”的持久化链；
strict legacy 的非零五 DWORD 在进入 request 之前如何生成仍未闭合。

因此当前不能把 `DeviceNumber/HDSerialCRC` 单 DWORD 与 LBA4
`HSerialCRC[5]` 直接等同；**旧 `ReadUsbHserialsInfo` ABI 已直接排除这种等价关系**。
这里仍不能进一步断言“更早 writer 从未使用 DeviceNumber/硬件身份材料”：未知的更早 caller
仍可能存在某种 `host material -> 5×DWORD` 转换。由于 strict legacy 非零
`request+0x150..+0x160` 的真正赋值点/算法仍未找到，LBA4 这20B继续保持 PARTIAL，
不得据此增加 COMPLETE 字节数。

##### Provision LBA4 canonical 修正（历史阶段；已被本轮 SAFE6 full-rolling 证据部分推翻）

本轮发现 Provision 生成器曾把不同 profile 混在一起：

- `+0x18..+0x1F` 被错误当作 8B 随机 `lba4_nonce`；
- `+0x20..+0x33` 被硬编码成 14/22 样本中的旧 profile
  `1D29,7B,4DD,79,7C`；
- `sparse_rolling_encrypt` 还把“明文为 0”错误解释成“物理字节保持 0”，
  与已验证的有效 node 区连续 rolling-XOR 冲突。

这一阶段曾把新盘 canonical 调整为：

- `OnlyIdXor8 = main_onlyid ^ 0x88888888`；
- `OnllyID2Nd = main_onlyid`；
- `HSerialCRC[5] = 0`；
- `0x18..0x46` 整个有效 node 连续 rolling-XOR，即使明文字节为 0 也加密；
- short-form `0x47..0x1FB` 作为整段“未写区”保持物理零；
- `0x1FC..0x1FF` 使用同一条继续推进的 rolling key schedule 写入 LLGB 锚点。

`ProvisionEntropy.lba4_nonce` 与 `sparse_rolling_encrypt` 已删除；这一历史阶段的
validator 还曾逐字节检查完整47B node和 short-form 物理零区。

**本轮最新证据已经证明上面的“short-form = current Windows writer”结论不成立。**
current SAFE6 分支会传 non-null restore node 并执行 full rolling loop。
紧接着的 `+0x45/+0x46` 审计已把 Windows/Linux producer、ReadSector4 非对称行为
和22份原始盘全部重新闭合，随后代码已经切换为真正的 current SAFE6 full
representation；历史 raw-zero form 只作为兼容读取 profile 保留。本段保留作为
“错误 short canonical 是如何被证据推翻”的历史记录，不再代表当前实现状态。

#### LBA6：`0x1C0..0x1EF` 必须拆开

Windows `sub_10013fd0` 与 Linux `CLabelManage::BuildSector6(UsbWriteParam&, char*)` 两套独立 writer 对齐：

- `0x1C0..0x1CF <- m_usbGSerial[0..14] + NUL`；
- `0x1D0..0x1DF <- BeiZhu[0..14] + NUL`；
- `0x1F0..0x1F3 <- m_encrypt`（低字节布尔值扩成 DWORD）；
- `0x1E0..0x1EF` 当前 writer 没有显式覆盖；本轮已证明两份旧 profile
  的非零内容不是独立扩展字段，而是**旧 MBR partition-table underlay 的残片**。

进一步核对 Windows 静态 `UsbMainBSec` 模板后，旧的
`0x1CA=128480` 解释可以撤销：

- 模板 `+0x1BE..+0x1CD` 本身就是标准 16B MBR partition entry：
  `status=0, type=0x07, start_lba=63, sector_count=128480`；
- 模板 `+0x1FE..+0x1FF = 55 AA`，并含标准 MBR boot code / 错误文本；
- 因而模板 `+0x1CA` 恰好只是这个 MBR entry 的 `sector_count` 字段位置；
- 但 BuildSector6 随后会把 `0x1C0..0x1CF` 整个 16B 区覆盖成
  `m_usbGSerial[0..14] + NUL`。所以最终盘面上的 `+0x1CA`
  已不再是模板 MBR 字段，而只是 **GSerial 固定槽内第 10..13 字节**。

22 份原始参考样本进一步直接否定“`+0x1CA` 是独立状态 DWORD”：

- 14/22：`u32@+0x1CA = 128480`；
- 2/22：`u32@+0x1CA = 20417`；
- 6/22：`u32@+0x1CA = 0x34314437`，小端字节就是字符串
  `"7D14"`，来自 `"322CA28A-D7D144"` 的中间四个 ASCII 字节。

因此同一 offset 同时出现“模板几何值 / 另一几何值 / ASCII 文本”不是协议多态，
而是**错误地把字符串槽中的四个字节当整数解释**。

当前 reader 也支持这个结论：

- Linux `ReadSector6` 把 `sector+0x1C0` 当 C 字符串，用于 GSerial 前缀匹配；
- `sector+0x1D0` 直接按字符串读回 `UsbLabelParam.BeiZhu`；
- 没有把 `+0x1CA` 作为整数读取，也没有发现 `0x1E0..0x1EF`
  的当前业务消费者。

Windows 新注册路径还解释了为何 NUL 后会出现看似“有规律”的尾字节：
`RegsiterUsb` 的本地写参数对象没有先整体清零，`sub_100139f0`
使用 strcpy_s 风格函数只复制到 NUL，而 BuildSector6 随后固定复制 15B
GSerial / 15B BeiZhu。current `BuildSector6` 自身则会先把临时16B缓冲清零，
再从输入槽固定取前15B并覆盖16B，因此 current profile 的 NUL 后物理值取决于
上游输入槽的 backing bytes，而不是目标扇区 underlay。

旧 profile 的**值来源**必须单独看：两份 legacy 实盘的 post-NUL 字节不是随机残值，
而是与后续 `+0x1E0` 连成一份结构完整的旧 MBR partition table。但继续用 first-party
Windows `BuildSector6` 做非零 backing 正向实验后，槽本身的语义已经可以统一：在短
`GSerial="322CA28A"` 的 source NUL 后人为预填 `A5×6`，以及空 BeiZhu source NUL 后预填
`5A×14`，official builder 都把这些 bytes 原样复制到 LBA6；两个 slot 的 byte15 则仍由
零化temp固定为NUL。Windows/Linux reader又都只消费首个NUL前的C-string。因此
`+0x1C9..+0x1CE` 与 `+0x1D1..+0x1DE` 的**协议角色**是动态的“字符串正文或 caller-owned
post-NUL backing”，而不是第二业务字段。legacy MBR几何只是该 backing 的一种历史 provenance；
它仍值得保留取证，但不再阻塞这20B的字段语义闭合。新增
`official_virtual_slot_backing_lba6.hex` 与回归
`official_virtual_lba6_fixed_string_slots_preserve_nonsemantic_source_backing` 锁定任意非零
backing也必须原样保留。

全量原始样本当前分布：

- GSerial C 字符串：
  - 16/22 为 `"322CA28A"`；
  - 6/22 为 `"322CA28A-D7D144"`；
  - 16份短字符串样本 **16/16 在首个 NUL 后仍有非零字节**；
  - 在可解析 LLGB 的样本中都与 LBA8 GLab 前缀一致；
- BeiZhu C 字符串：
  - 20/22 为空；
  - 2/22 为 GBK `"普通"`；
  - 全部22份中有 **8/22 在首个 NUL 后仍有非零 backing bytes**；
- `0x1E0..0x1EF`：20/22 为模板零；2/22（Aigo U335 旧形态 +
  SanDisk 原始盘）保留 legacy MBR partition-table fragment；
- `u32@0x1F0`：22/22 均为 1；官方 writer 字段名是 `m_encrypt`，
  不能再标成“注册标志”。本轮进一步回到 Windows PE 原始机器码闭合其
  current producer：`RegsiterUsb` 的临时 `UsbWriteParam` 基址为 `ebp-0x3F4`，
  因而 `m_encrypt@+0x258` 精确映射到 `ebp-0x19C`；5字节比较目标
  `0x100C9A90` 为 ASCII `!SAFE`，相等分支 `0x1003BA94` 写1、非相等分支
  `0x1003BAC7` 写0，随后 `0x1003BAF5` 立即调用 `BuildSector6` 写到
  `LBA6+0x1F0..0x1F3`。因此“为什么 current 样本为1”的 producer 来源已闭合。
  继续核对 consumer 后，Linux DWARF 正式 `UsbLabelParam` 结构根本没有
  `m_encrypt` 成员；Linux `ReadSector6` 不返回该字段。Windows
  `CheckLabel/sub_100152A0` 在整段 rolling-XOR 解码和前508B checksum 验证后，
  显式读取 Dept/User/GSerial/Label 等字段，却没有 `+0x1F0` 的值相关访问；
  已扫 runtime 组件同样未见行为 consumer。独立 SanDisk 原始 LBA6 也保持值1。
  因此这4B由 PARTIAL 升为 **COMPLETE**，保守语义为
  **write-only `!SAFE` label-generation metadata**：writer 保存当次匹配结果，
  checksum 覆盖其物理字节，但读取侧不把它作为运行时加密开关。

本轮还重新从 Linux 官方二进制本身核对当前模板，而不是沿用旧文档：
`nm -S -C` 定位 `UsbMainBSec@0x22BB40,size=0x1000`，`.data`
原始字节显示模板相对 `+0x1E0..0x1EF`（VMA `0x22BD20..0x22BD2F`）
确为 16B 零；`BuildSector6` 从该模板起步且不再覆盖这一段。
这只闭合了 **current profile 的零来源**，不能解释两份旧 profile。

进一步取得的历史 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5
`783d01f19e998a514834bc5e5f4249ad`）把时间边界又向前推进了一代。SAFE6
上层 `fcn.1000B110` 把 Dept/User/Label/GSerial 等注册参数汇总后调用
`fcn.10006370`；后者对目标扇区执行 `rep movsd, ECX=0x80`，从静态
`0x101BA790` 精确复制 512B 模板，再写入各业务槽，随后对前 `0x1FC`
字节计算同款 checksum，并以 `6 * sector_size` 定位后 `WriteFile` 一扇区。
直接读取该历史模板可见 `+0x1E0..+0x1ED` 为14B零，而 `fcn.10006370`
的全部字段 overlay 也没有触及这一区间。因此 **2019-11 的正式 writer
仍生成 zero profile**，不能产生 Aigo/SanDisk 两份 nonzero MBR snapshot。
这把 exact producer 的搜索范围继续收缩到更早的 CEMS2.0/legacy writer；
在取得那个同代 producer 前，这14B仍不得升级 COMPLETE。

join59 一侧也进一步把“兼容 reader”与“同代 writer”拆开。CEMS2.0
`cems/Edp/fileophook.dll`（PDB 路径包含
`vrvrsms2.0\\Cems2.0\\trunk`）的 `fcn.10022f80@0x10022F80`
在 `0x100232ED` 比较 `0x40245E2A` marker；命中后先复制
`0x3C=60B` inline prefix，再把 LBA9 `+0x80` continuation 写到输出
prefix 基址 `+0x3B`，所以 Dept[59] 会被 continuation 首字节覆盖。其 x64
同源 build 也保持同一 `+0x3B` 接缝。这是可直接复核的
**CEMS2.0 join59 reader only**，并不能证明 producer 也以59B切分。

为避免把“旧 reader ABI”反推成 writer，本轮又对本机 VRV 树中所有包含
`0x40245E2A` 常量的二进制做了指纹筛选。实际 `mov marker` 的 producer
只落在 current `cemsusbregsiter.dll` 与 `vrvaud_c.dll`，两者都明确复制
`0x3C=60B`；其余命中均为 `cmp marker` 的 reader。换言之，
**all locally available marker writers use 60**。结合 v19.11.4.1
`fcn.10006370` 本身根本没有 long-Dept marker，现有证据只能证明
“CEMS2.0 reader 固定 join59 + current writers 固定 join60”，还不能闭合
产生 Lexar join59 实盘的 exact-generation writer/profile-selection。因此
LBA6 `+0x03F` 与 LBA9 `+0x080..0x0FF` 继续保持 PARTIAL。

本轮又把 current `cemsudisk.dll::sub_101015e0` 的兼容选择条件收敛到单个
wire 字节，而不是继续把它笼统记成“代际模式”。固定 SHA-256
`32e88065725ccb9bc50e24c244f5686bf1d38335f58737f454a8f4ca2c892fd1`
后，机器码 `0x10101910..0x1010191B` 先把 marker 后 **15 DWORD = 60B**
inline Dept prefix 复制到输出；紧接着 `0x1010191D` 读取源局部
`[ebp-0x6D]`。该地址正好是从 `[ebp-0xA8]` 开始的60B中第60个字节，
即 **serialized Dept[59] 本身**。随后：

- Dept[59] == 0：`0x10101934` 把128B LBA9 continuation 写到输出
  `+0x7B`，因此 continuation 覆盖 Dept[59]，形成 join59；
- Dept[59] != 0：`0x1010194F` 改写到 `+0x7C`，从 Dept[60] 继续，
  形成 join60。

同一审计同时锁定 current `CEMSUsbRegsiter::BuildSector6@0x10013FD0` 的
反向边界：`0x1001402D` 只有在 `strlen(Dept)>=64` 时进入 long-marker
分支，`0x10014077` 固定复制60B inline prefix，`0x100140C4` 又固定从
`Dept+0x7C = Dept[60]` 取 continuation 写到 LBA9+0x80。因此 current writer
不可能自行生成“long Dept 但 Dept[59]=NUL”的 join59 wire。
`scripts/protocol/audit_join59_selector.py` 固定两份 DLL SHA 并重放这些
指令约束。这个结果闭合了 **consumer/profile-selection rule**，但没有提供
缺失的 historical producer，所以严格字节状态不变。

两份旧 profile 的实际解密字节是：

```text
Aigo U335:
  +0x1D0  c6 d5 cd a8 00 ff 00 50 00 00 1c 58 7d 0e 00 00
  +0x1E0  c1 ff 07 ef ff ff 1c a8 7d 0e e3 f4 27 00 00 00

SanDisk:
  +0x1D0  c6 d5 cd a8 00 ff 00 50 00 00 b2 3a 05 0e 00 00
  +0x1E0  c1 ff 07 ef ff ff b2 8a 05 0e 77 3c 4c 00 00 00
```

其中 `c6 d5 cd a8` 是 GBK“普通”，随后立即 NUL。重新按标准 MBR
`4 × 16B partition entry @ 0x1BE..0x1FD` 对齐后，旧布局可以精确解释：

```text
entry1 = 0x1BE..0x1CD
entry2 = 0x1CE..0x1DD
entry3 = 0x1DE..0x1ED
entry4 = 0x1EE..0x1FD

GSerial 0x1C0..0x1CF
  -> 覆盖 entry1 bytes[2..15] + entry2 bytes[0..1]

BeiZhu  0x1D0..0x1DF
  -> 覆盖 entry2 bytes[2..15] + entry3 bytes[0..1]

m_encrypt 0x1F0..0x1F3
  -> 再覆盖 entry4 bytes[2..5]
```

因此 `+0x1E0..0x1ED` 正好是**第3条 MBR entry 丢掉前2B后的连续14B**：

- `+0x1E0..1E1 = start CHS 的后2B = C1 FF`；
- `+0x1E2 = partition type = 0x07`；
- `+0x1E3..1E5 = end CHS = EF FF FF`；
- `+0x1E6..1E9 = start_lba`；
- `+0x1EA..1ED = sector_count`；
- `+0x1EE..1EF` 已进入第4条 MBR entry，两个旧样本均为 `00 00`。

CHS 也不是“看起来像 MBR”的弱匹配。两块独立 nonzero 实盘的完整 entry3
前8B 都是 `00 00 C1 FF 07 EF FF FF`；按传统 MBR CHS 编码解码：

- start CHS = `C=1023, H=0, S=1`；
- end CHS = `C=1023, H=239, S=63`。

即两端都处于传统 CHS 柱面上限的饱和值，并呈现 240-head/63-sector 风格。
配合下述 `start_lba/sector_count` 与 LBA12 type4 的 2/2 精确相等，说明
`+0x1E0..+0x1ED` 的 14B 已全部可以解释为 **旧 MBR entry3 的结构字段**，
而不是未知私有字段。严格状态仍保持 PARTIAL 的唯一原因是尚未取得把这份
旧 MBR snapshot 带入 SAFE6 backing 的 exact producer/profile-selection。

两块独立真实盘与 LBA12 做交叉后：

```text
Aigo U335:
  LBA6 MBR entry3 start_lba    = 243116060
  LBA6 MBR entry3 sector_count = 2618595
  LBA12 type4 StartSector      = 243116060
  LBA12 type4 PartionSize/512  = 2618595

SanDisk:
  LBA6 MBR entry3 start_lba    = 235244210
  LBA6 MBR entry3 sector_count = 4996215
  LBA12 type4 StartSector      = 235244210
  LBA12 type4 PartionSize/512  = 4996215
```

两份都是 **type/start/count 全匹配**。而严格22份参考进一步给出关键反例：

- 2/22 legacy fragment 非零，2/2 都与 LBA12 type4 几何精确一致；
- 20/22 `+0x1E0..1EF` 全零；
- 这20份的 LBA12 **仍然20/20存在 type4**。

所以这不是“只要存在 type4 就必须写入”的冗余副本，而是**历史 writer/profile
才保留的 MBR-layout 快照/underlay**。

GSerial/BeiZhu 的 legacy post-NUL 尾也继续支持同一解释：

- 两盘 GSerial 都是 `"322CA28A\0"`，其后的 `u32@+0x1CA=20417`
  落在 entry1 的 `sector_count` 位置；
- 两盘 BeiZhu 都是 GBK `"普通\0"`，NUL 后从 `+0x1D5` 继续出现
  entry2 end-CHS 尾、`start_lba@+0x1D6` 与 `sector_count@+0x1DA`；
- SanDisk 的 entry1/entry2 surviving geometry 与 LBA12 type1/type2 精确一致；
- Aigo 旧 MBR 为连续边界 `entry1 count=20417 -> entry2 start=20480`，
  而 LBA12 新表为 `20418 -> 20481`，呈现明确 ±1 版本差异；type4 边界仍一致。
  这进一步说明旧 fragment 不是由当前 LBA12 简单复制出来，而是独立的旧布局。

producer/consumer 边界也重新核过，并补上 v19.11.4.1 的精确 ABI：

- current Windows `sub_10013FD0` 与 Linux `BuildSector6` 都从静态
  `UsbMainBSec` 复制整扇；current writer 在写 GSerial/BeiZhu 时，对16B临时缓冲
  先清零，再复制输入槽前15B并固定覆盖16B；
- historical v19.11.4.1 `BuildSector6/fcn.10006370` 也先复制完整512B
  `UsbMainBSec@0x101BA790`，但其 BeiZhu 物理槽是更宽的 cap=32 ABI：
  `0x10006648..0x10006656` 对 `sector+0x1D0` 调
  `strcpy_s(cap=0x20, caller arg8)`。这意味着旧文档“v19 overlay 不触及
  +0x1E0..+0x1ED”是错误的；本14B 位于 v19 BeiZhu 槽的**容量范围**内；
- 这仍然不是 raw 32B copy。共享 helper `0x101473FE` 在复制首个 NUL 后立即停止。
  已恢复 writer caller `0x1000B16C..0x1000B226` 先把 arg8 的32B局部完整清零，
  再从 `object+0x2620` 以 `strcpy_s(cap=32)` 写入 BeiZhu；同时 pinned v19
  `UsbMainBSec +0x1D0..+0x1F3` 也全部为0。因此短 BeiZhu 下 v19 只能让
  `+0x1E0..+0x1ED` 保持0，不能生成两份物理 nonzero MBR underlay；
- historical v19 `ReadSector6@0x10006E07..0x10006E16` 从 decoded
  `+0x1D0` 通过同一 `strcpy_s(cap=32)` 返回 caller arg8，只消费到首NUL。
  六个 caller 分三组：第一组目标局部除初始化/两次 reader 传参外无引用；第二组
  成功路径读取另一 `esp+0x18` 字符串，不读取目标稳定 `esp+0x38` 槽；第三组
  目标 `-0x64` 的唯一后续值使用是再以 `strcpy_s(cap=16)` 写入对象。
  因而 post-NUL `+0x10..+0x1D` 没有 cmp/test/hash/branch/字段提取业务消费；
- Windows 当前 `UsbMainBSec@0x100E7220` 只发现读取 xref，没有运行时写入；
  Linux `UsbMainBSec@0x22BB40` 同样只在 current builder 被读取；
- current `BuildSector0`/Netac/hardware MBR builder 都只构造单条普通分区，
  不能生成这里的三分区 legacy layout；
- `UDiskLabelRepair.dll` 的 `CLabelRepair::CheckSafe6LabelExist`
  直接从 LBA12 解析 type1/2/4，`Repair0Sector/ReCreate0Sector` 从 sector9/
  backup sector 恢复或重建 LBA0；目前没有发现它直接读取 LBA6 fragment。

因此“legacy opaque extension”这一旧命名仍应撤销。consumer 侧现已闭合到
**C-string prefix only / post-NUL semantic-ignore**，但 `+0x1E0..+0x1ED`
仍不能升 COMPLETE：两份 nonzero 物理值确实是旧 MBR entry3 fragment，而 pinned
v19/current writer 都只能给出 zero profile。生成该 dynamic legacy MBR underlay 的
exact earlier writer/profile-selection 仍缺，故继续保持 PARTIAL。

新增回归门禁：

- `lba6_legacy_beizhu_post_nul_bytes_continue_into_mbr_type4_fragment`；
- `lba6_authentic_sandisk_legacy_mbr_type4_fragment_matches_lba12`；
- `lba6_legacy_mbr_fragment_is_profile_specific_even_when_lba12_type4_exists`。

第三条门禁明确防止未来把“LBA12 有 type4”错误实现成“新盘必须回填 LBA6 MBR”。

**LBA6 C-string slots have profile-dependent post-NUL backing bytes**：基于上述 producer、
consumer 和22份原始盘反例，本轮纠正之前的严格账本：
`+0x1C0..0x1CF` 与 `+0x1D0..0x1DF` 从 COMPLETE 回退为 PARTIAL。
这是证据标准收紧后的纠错，不是协议理解退步；两段的 C-string 业务语义仍然成立。

因此旧免密转换器里的 `0x1CA=128480`、`0x1D4..0x1EC=0`
只能保留为**历史兼容 patch recipe**，不能再进入新盘 Provision 的协议模型。
新盘 canonical profile 现在按当前 writer 边界确定性生成：
`GSerial="322CA28A" + NUL + zero tail`、空 BeiZhu、`0x1E0..0x1EF=0`、
`m_encrypt=1`；不模拟 writer 的未初始化尾字节。

#### LBA6 `m_autoid@0x70` / `m_UsbOffice@0x80`：post-NUL backing 来源闭合

Linux DWARF/机器码继续把这条链闭合到整个固定槽的存储行为：

- `UsbLabelParam.m_autoid @ +0x258`；
- `UsbWriteParam.m_autoid @ +0x259`；
- `UsbWriteParam.m_UsbOffice char[64] @ +0x198`；
- `UsbWriteParam(UsbLabelParam&) @ 0x1C362` 分别通过
  `strcpy_s(...,16,...)` / `strcpy_s(...,64,...)` 写这两个数组；
- 该二进制自带的 `strcpy_s(char*, unsigned long, char const*) @ 0x1B9B0`
  逐字节复制，遇到 NUL 后立即返回，**不会清 destination 剩余 capacity**；
- 这个 copy-constructor 入口也没有先对 0x299B `UsbWriteParam` 整体 memset，
  所以目标数组首个 NUL 后会保留对象原有 backing；
- `BuildSector6@diskfile.cpp:672` 固定 `memcpy 16B` 到 LBA6 `0x70..0x7F`；
- 同一 BuildSector6 固定 `memcpy 64B` 到 LBA6 `0x80..0xBF`；
- `ReadSector6@diskfile.cpp:1005` 再通过 `strcpy_s(...,16,...)` 把
  LBA6 `+0x70` 读回 `UsbLabelParam.m_autoid`；
- Office 同样通过 `strcpy_s(...,64,decoded+0x80)` 读回；
- `BuildSector8` 把同一 `m_autoid` 序列化为 ELABEL `Autonum=`；
- LBA6 SAFE6 checksum 覆盖 `+0x000..+0x1FB`，所以 post-NUL backing
  虽无业务字段语义，仍属于完整性保护的物理存储内容。

全 22 份原始参考只读复核：

- 22/22 的 LBA6 `+0x70` C 字符串与 LBA8 `Autonum=` 完全一致；
- 分布为 `YD000001` 14、空串 6、`1` 2；
- 第一个 NUL 后经常非零；committed originals 中**同一个空 autoid**
  至少出现2种不同且非零的 post-NUL backing；
- Office 同样存在空/非空值，且**同一个空 Office**至少出现3种不同且非零的
  post-NUL backing。

这直接排除了“隐藏字段”与“固定零 padding”解释。尾字节值不稳定，但不稳定的
**生成原因与消费规则已经闭合**：copy-constructor 不预清对象 + 自带 strcpy_s
不清剩余 capacity + BuildSector6 固定宽度 memcpy，形成
writer-uninitialized backing；ReadSector6 只解释首个 NUL 前的 C-string。

因此严格 COMPLETE 可以覆盖这种“值不确定、行为确定”的存储语义：

- `LBA6 +0x70..+0x7F m_autoid[16]`：16B PARTIAL -> **COMPLETE**；
- `LBA6 +0x80..+0xBF m_UsbOffice[64]`：64B PARTIAL -> **COMPLETE**。

这不要求 Provision 模拟未初始化内存泄漏；new-disk canonical 可在 NUL 后使用
确定性零 backing，并重算 checksum。兼容读取必须继续接受真实旧盘任意 backing。

#### LBA6 `m_usbLabel@0x188`：同一字符串三种非零 backing，56B 整槽闭合

继续沿同一个 `UsbWriteParam(UsbLabelParam&)@0x1C362` 下钻后，Label 与
autoid/Office 实际共享同一类 producer bug：

- Linux DWARF 给出 `UsbLabelParam.m_usbLabel[64]@+0x218` 与
  `UsbWriteParam.m_usbLabel[64]@+0x218`；
- copy-constructor 调用
  `strcpy_s(dst+0x218, 64, src+0x218)`；
- 该构造器入口没有先 memset 整个 0x299B `UsbWriteParam`；
- 自带 `strcpy_s@0x1B9B0` 在复制第一个 NUL 后立即返回，不清剩余 capacity；
- Windows `sub_10013FD0` 与 Linux `BuildSector6@0x1CAAC`
  都只把该64B数组的前 `0x38=56B` 固定复制到 LBA6 `+0x188..+0x1BF`；
- Linux `ReadSector6@0x1E84B..` 从 `decoded+0x188` 构造 C++ string，
  Windows reader 同构；`BuildSector8` 又把同一个逻辑字符串写成
  ELABEL `Label=`；
- SAFE6 checksum 覆盖 LBA6 前508B，所以 NUL 后 backing 虽无业务字段语义，
  仍是受完整性保护的真实物理字节。

实盘给出了比“存在非零尾”更强的反例。committed original fixtures 中，
业务值全部是同一个 `江苏电力!SAFE6`，但首个 NUL 后的41B backing 至少出现
**3种不同 profile，而且3种都含非零字节**。如果这些字节属于隐藏字段或固定 padding，
同一业务字符串不应自然产生这种多 profile；该分布与“不清目标对象 + strcpy_s 到 NUL
即停 + 固定56B memcpy”的官方 producer 逐项吻合。

回归 `lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries`
现同时要求：

- LBA6 Label C-string == LBA8 `Label=`；
- Label 槽存在 post-NUL 非零 backing；
- 同一个 `江苏电力!SAFE6` 至少保留3种不同且非零的 post-NUL backing。

因此 `LBA6 +0x188..+0x1BF` 56B 从 PARTIAL 升 **COMPLETE**。这里的 COMPLETE
表示“业务字符串 + writer-uninitialized backing”的逐字节行为闭合，不意味着尾部必须
复刻旧内存垃圾；canonical 新盘仍可在 NUL 后写确定性零并重算 checksum，reader 必须接受
旧盘的任意 backing。到这一中间审计阶段，LBA6 更新为
**356 COMPLETE / 156 PARTIAL / 0 UNKNOWN = 69.5%**。

#### LBA6 Dept 64B：已知 join59/join60 分叉严格只剩 `+0x3F` 1B

Dept 主槽继续按字节下钻后，不能再把整64B因为 legacy join59 producer 未知而一起留在
PARTIAL。Linux `UsbWriteParam(UsbLabelParam&)@0x1C362` 对 department 的实际代码是：

`strcpy_s(dst+0x40, 0xBC, src+0x40)`

这与 autoid/Office/Label 使用同一个自带 `strcpy_s@0x1B9B0`，并且 copy-constructor
入口同样不先 memset 0x299B `UsbWriteParam`。因此 short Dept 的物理槽语义已经与其它
fixed C-string slot 一致：首个 NUL 前是业务字符串，NUL 后是
writer-uninitialized backing。Linux `BuildSector6@0x1CAAC` 又明确分两条：

- `strlen(dept) <= 63`：直接把 `UsbWriteParam+0x40` 的完整64B memcpy 到 LBA6；
- `strlen(dept) > 63`：先清64B临时槽，写 marker `0x40245E2A`，再复制
  Dept 前60B 到 marker 后，并把 Dept[60..NUL] 写入 LBA9+0x80。

因此 current long profile 中：

- `+0x00..03 = 2A 5E 24 40`；
- `+0x04..+0x3E = Dept[0..58]`；
- `+0x3F = Dept[59]`。

现有 strict-original current Kingston join60 与 strict-original legacy Lexar join59
给出一个非常精确的历史边界：两者重建后的完整 Dept 都是同一76B字符串，解密后的
LBA6 Dept 槽 **前63B逐字节完全相同**，唯一差异是最后1B：

- current join60：`+0x3F = 0xA8`，即 Dept[59]；
- legacy join59：`+0x3F = 0x00`，LBA9 continuation 从 Dept[59] 开始；
- reader 已有明确 compatibility 分支：inline[59] 非零时 join=60，为0时 join=59。

新回归在
`lba9_dept_continuation_preserves_both_official_reader_join_profiles`
中固定“前63B相同、只末1B分叉”；另新增
`lba6_short_dept_slot_is_c_string_plus_uninitialized_backing`，要求至少3份原始
short-Dept fixture 的 LBA6 C-string 与 LBA8 `Dept=` 相同，并且
`+0x00..+0x3E` 内必须保留真实 post-NUL 非零 backing 反例。

据此可以严格拆分：

- `LBA6 +0x000..+0x03E` 63B：**COMPLETE**。short profile 的
  C-string/backing producer、long profile 的 marker+Dept[0..58] producer、
  reader、以及 current/legacy 原盘均闭合，且已知代际分叉不触及本段；
- `LBA6 +0x03F` 1B：继续 **PARTIAL**。current writer/reader 已知，
  但 legacy join59 为什么把 Dept[59] 改为NUL、由哪个旧 producer/选择条件产生，
  尚未定位。

这不是把未知 legacy producer“平均摊掉”，而是把它隔离到唯一真实分叉字节。
LBA6 在当时更新为 **431 COMPLETE / 81 PARTIAL / 0 UNKNOWN = 84.2%**；后文
long-User first-party runtime 闭环后，Owner/User 32B 又从 PARTIAL 升 COMPLETE，
最终以文末严格总表为准。

#### LBA6 原352B UNKNOWN 已全部拆清：固定字段槽 + UsbMainBSec 静态模板

本轮从 Windows/Linux 官方 BuildSector6 与 ReadSector6 的真实机器码重新恢复
物理布局，而不是继续把已命名字段之间的空洞记作 UNKNOWN。

Linux DWARF 明确给出 UsbLabelParam(size=0x2AC) 与
UsbWriteParam(size=0x299)。与本轮相关的成员为：

- department @ +0x40；
- owner @ +0xFC；
- Office[64] @ +0x198；
- GSerial[64] @ +0x1D8；
- Label[64] @ +0x218；
- autoid[16] @ +0x258/+0x259；
- BeiZhu[16] @ +0x268/+0x289。

Windows sub_10013FD0 首先从 UsbMainBSec@0x100E7220 复制整扇到 LBA6；
Linux BuildSector6@0x1CAAC 同样先从 UsbMainBSec@0x22BB40 复制 sector_size。
随后字段 overlay 只覆盖
+0x000..03F、+0x050..06F、+0x070..07F、+0x080..0BF、
+0x100..107、+0x188..1BF、+0x1C0..1CF、+0x1D0..1DF、
+0x1F0..1F3。

因此旧352B UNKNOWN 可严格拆成两类。

第一类是此前账本漏掉的136B fixed storage slots：

- +0x060..06F：原账本只记录 User 前16B，实际 Owner/User 主槽为完整32B
  +0x050..06F；
- +0x080..0BF：m_UsbOffice[64]；
- +0x188..1BF：m_usbLabel 的56B物理主槽。

官方 reader 与之对称：Owner 普通路径按 C 字符串从 decoded+0x50 恢复，
长值使用 0x40245E2A marker 并从 LBA9+0x100 continuation 重组；
Office 从 decoded+0x80 读回 64B C-string；Label 从 decoded+0x188
构造字符串后写回 m_usbLabel[64]。

实盘证明三种固定槽都有首个 NUL 后非零 backing。进一步追 producer 后，
Office 与 Label 的 backing 来源先行闭合；Owner/User 随后也由 current Windows
first-party `BuildSector6` 最大155B正向运行、`ReadSector6` 动态 round-trip 以及
`sub_100139F0 -> strcpy_s@0x10097F4F` 的短值 backing 生命周期闭合，已升级 COMPLETE。CI
`lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries`
继续锁定 LBA6 Owner C-string == LBA8 User、LBA6 Label C-string == LBA8 Label，
并同时锁定“同一空 Office 至少3种 backing”与“同一
`江苏电力!SAFE6` 至少3种不同且非零 backing”两组反例。新增
`official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 则锁定
最大 long-User wire profile。因此旧136B现在 **Owner 32B + Office 64B + Label 56B
全部 COMPLETE**。

第二类是216B writer-owned static UsbMainBSec material：

- +0x040..04F：16B；
- +0x0C0..0FF：64B；
- +0x108..187：128B；
- +0x1F4..1FB：8B。

这216B满足严格 COMPLETE，而不是因为样本碰巧相同：

1. Windows/Linux BuildSector6 都先复制官方 UsbMainBSec，字段 overlay
   不触及这四段；
2. Windows sub_100152A0 与 Linux ReadSector6@0x1E2CC 都在任何字段解析前
   对 raw +0x000..1FB 整体计算 SAFE6 checksum，不匹配直接拒绝，因此这216B
   是明确完整性输入；
3. 正确从 ELF .data 的 UsbMainBSec@0x22BB40 抽取后，四段与 Windows
   template 以及实盘逐字节一致；
4. 21份 non-converted 完整 backup + 独立 SanDisk 共22份原始盘全部一致。

+0x108..187 还包含 legacy MBR 的 Invalid partition table /
Error loading operating system / Missing operating system message material；
+0x1F4..1FB 是显式模板零。CI 新增
lba6_static_usb_main_bsec_holes_are_exact_and_checksum_protected，
锁定 committed originals + 独立 SanDisk。

此前 LBA6 从 4 COMPLETE / 156 PARTIAL / 352 UNKNOWN 更新为
220 COMPLETE / 292 PARTIAL / 0 UNKNOWN；随后闭合 autoid 16B + Office 64B
达到 300/212；随后闭合 Label 56B，在继续拆 Dept 之前的中间计数为
**356 COMPLETE / 156 PARTIAL / 0 UNKNOWN**。

全 LBA0–12 的 UNKNOWN 首次降为0B。这只表示每个物理字节至少已有明确区域/
存储行为边界；仍有大量 PARTIAL 尚未满足最终业务语义闭环，不能把
“无 UNKNOWN”写成“协议已经全部完成”。

同时，Dept/Owner overflow 路径获得跨扇区纠偏：BuildSector6 的 out 指针
指向 LBA6，因此 out + 3*sector_size + 0x80/0x100 实际落在
LBA9+0x80/+0x100。这给此前 LBA9 Dept/backing 历史 profile 提供了官方
producer 解释，也证明 LBA6/LBA9 不是互不相关的独立扇区。

#### LBA6 `+0x100..0x107`：官方 `m_crcUsbID[2]` 与 doubled guard 已恢复

此前这里仅记为“device-id CRC材料”，第二 DWORD 没有正式含义。本轮直接用
Linux DWARF、Linux/Windows producer、Windows legacy check 与严格22份实盘把结构恢复到：

```text
CLabelManage:
  +0x24  DWORD m_crcUsbID[0] = CRC32(device_id)
  +0x28  DWORD m_crcUsbID[1] = m_crcUsbID[0] * 2 mod 2^32

LBA6:
  +0x100..0x103 = m_crcUsbID[0]
  +0x104..0x107 = m_crcUsbID[1]
```

Linux 官方 DWARF 在 `diskfile.h:204` 明确给出：

```text
CLabelManage +0x24 : unsigned int m_crcUsbID[2]
```

producer 不是推测，而是两条 Linux 初始化路径都逐指令一致：

- `CLabelManage::CLabelManage(...) @ 0x1C538`：
  - `memset(this+0x24, 0, 8)`；
  - `CRC32(0, m_strUID.c_str(), m_strUID.length()) -> this+0x24`；
  - `this+0x28 = this+0x24 * 2`；
- `CLabelManage::Init(...) @ 0x1C76E` 重复同一套 `CRC32 -> doubled DWORD` 算法；
- `BuildSector6@diskfile.cpp:710` 执行 `memcpy(out+0x100, this+0x24, 8)`。

Windows current producer 与 Linux 独立对齐：

- `sub_10013D20` 清零 `object+0x44..+0x4B`；
- `object+0x44 = CRC32(device-id string)`；
- `object+0x48 = object+0x44 << 1`；
- `sub_10013B80` 的另一条构造路径完全同构；
- `BuildSector6/sub_10013FD0` 执行 `memcpy(out+0x100, object+0x44, 8)`。

第一 DWORD 还是整个标签族的实际密钥源，而不只是“写在 LBA6 里的编号”：

- Linux `BuildSector7` 读取 `this+0x24`，折叠高低16位生成 old-table rolling-XOR key；
- `EncryptSector8Data` 直接把 `this+0x24` 作为4B key；
- `BuildSector12`/`ReadSector12` 也以 `this+0x24` 作为4B加解密 key；
- `ReadSector8` 两个入口同样从 `this+0x24` 取 key。

因此 `m_crcUsbID[0]` 可以闭合为**由 device-id 派生的主标签加解密/rolling key material**。
这里仍然区分“运行时成员的用途”与“LBA6 持久化副本的 consumer”：Linux
`ReadSector6` 当前不读取 `+0x100/+0x104`，但在统一 COMPLETE 标准下，
这已经构成该 write-owned 副本的 **negative semantic consumer**，而不是新的语义缺口。
LBA6 前508B checksum 又覆盖这8B，所以物理副本仍属于明确的完整性保护范围。

第二 DWORD 的历史用途也找到了。Windows current
`CheckLabel/sub_100152A0` 的尾部分支保留：

```text
if u32(lba6+0x100) != 0 &&
   u32(lba6+0x104) == (u32(lba6+0x100) << 1):
    result = 13
else:
    result = 11
```

Linux DWARF 的官方错误枚举给出：

```text
11 = ERROR_USBVERSIONNOMATCH
13 = ERROR_SYSLABELMISTMATCH
```

同一倍增判断还复制在 `cemsudisk` 与 `vrvaud_c` 的 SAFE6 parser 中，说明
`m_crcUsbID[1]` 的确是历史上的 **doubled CRC consistency guard**，不是随机派生值。

但本轮同时回到 PE 机器码复核可达性，避免把反编译伪代码误当 current consumer。
`cemsusbregsiter.dll` 在该判断前实际是：

```text
mov edx, 1
test edx, edx
je   legacy_crc_pair_check
```

因此 current binary 永远走 `if(1)` 的正常解析分支，CRC pair check 不可达；
`cemsudisk` 与 `vrvaud_c` 同一份逻辑也都保留为
`if (1) { ... } else { crc pair check }`。全仓精确搜索没有第四处可达的
`+0x104 == +0x100*2` 检查。Windows `modfilesyscheck.dll` 则只读取/解析
LBA12，不读取 LBA6。

另一个看似相关的 `vrvaud_c object+0x100/+0x104` 可达分支已排除：
`CUDiskRoamControl::SetPolicy` 明确把它们分别设置为 `StartPolicy` 状态与
`RoamSwitch`，只是对象偏移碰巧相同，与 SAFE6 无关。

严格22份原始参考重新独立计算：

- 22/22 `u32(LBA6+0x100) == CRC32(device_id)`；
- 22/22 第一 DWORD 非零；
- 22/22 `u32(LBA6+0x104) == u32(LBA6+0x100) * 2 mod 2^32`；
- 独立 SanDisk 原始 LBA6 也满足两式。

新增回归门禁：

- `lba6_crc_usb_id_pair_is_device_id_crc_and_doubled_guard`。

按当前仓库已经用于 `m_encrypt`、compatibility slot 等字段的统一标准，
这8B现在可以完成闭合：

- `+0x100..103`：COMPLETE，write-owned identity/key metadata。正式字段名、
  双平台 producer、CRC32(device_id) 算法、同源运行时 key 用途、LBA6
  negative semantic consumer、checksum ownership 与22/22实盘均齐全；
- `+0x104..107`：COMPLETE，retained doubled-CRC compatibility guard。正式
  producer 与历史 doubled-guard consumer 都已存在；current build 虽不走
  historical branch，但正常 reader 明确 semantic-ignore 该副本，checksum 仍覆盖；
  22/22实盘严格满足倍增关系。

这不是把不可达死代码当 current consumer；COMPLETE 的对象是**盘面字段的完整生命周期**：
current writer 继续写、current reader可忽略、historical reader曾按 doubled guard 消费。
  但 current 三个同源实现中的该判断均不可达。

禁止后续仅凭死代码或“同源成员用于其它扇区加密”把这8B升级 COMPLETE。

#### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

- 当前 22/22 参考样本均满足：
  - 解密后 `0x00..0x03 == "LLGB"`；
  - `u32@+0x04 = 0x80 + strlen(ELABEL)`，**不包含结尾 NUL**；
  - 实际 A6B0/A7F0 长度为
    `((u32@+0x04 / 16) + 1) * 16`，即始终覆盖装有结尾 NUL 的下一块；
  - 当前22份原始参考在该动态加密长度之后都观测为物理零；但这只是样本事实，
    **不是 writer 的 zero-padding 语义**。Windows/Linux BuildSector8 都只覆盖并
    加密动态前缀，不会清零输出扇区剩余 tail；current 注册路径又以预读的既有
    LBA0–12 缓冲为 backing，因此 `encrypted_len..` 的正式行为是
    **preserve existing physical bytes**；
- 真实样本的有效长度覆盖 `0x148 / 0x154 / 0x16b / 0x17a / 0x17c / 0x17e / 0x181 / 0x183` 等多种值，实际加密前缀可为 0x150、0x160、0x170、0x180、0x190。
- 独立 SanDisk 原始加密盘此前曾被误判为“非 LLGB”：根因是使用了
  `disk&ven_sandisk&prod_ultra&rev_1.00` 这一另一份免密/历史样本的短 device_id。
  该原盘自己的 LBA7 只有在
  `disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00` 下才能恢复 EDPF；
  同一权威 device_id 解 LBA8 后得到标准 `LLGB`，`+0x04=0x15E`。
  因此当前原始参考集是 **22/22 LLGB**，没有证据把该 SanDisk 归入 EKTF。
- 因此旧固定 368B (`0x170`) decoder 会：
  - 对短标签多解无意义块；
  - 对长标签截断真实 `VOL/VOLC` 字段。
- 本轮又重新按严格22份原始生成参考逐盘复算：
  - `logical_end` 范围为 **0x148..0x183**；
  - 实际 encrypted prefix 范围为 **0x150..0x190**；
  - 22/22 的物理 tail 当前为零，但 producer 明确允许已有非零 backing 被保留。
  为防止实现重新把 tail 当作 LLGB 密文或固定零区，
  `tests/inspect.rs::lba8_preserves_nonzero_bytes_after_the_dynamic_encrypted_prefix`
  构造非零物理 tail，要求 inspect 只解密动态前缀并原样保留后部字节。
  同时新增
  `lba8_decrypts_one_extra_block_when_logical_length_is_16_byte_aligned`：
  旧 inspect 使用普通 `round_up_16(logical_len)`，当 logical_len 恰好16B对齐时
  会少解一整块；现已按官方 writer 的
  `(logical_len / 16 + 1) * 16` 修正，明确把 ELABEL 结尾 NUL 所在的额外块纳入
  A6B0 解密，并继续保持其后物理 backing 不动。
  因而旧账本按当前样本最大正文位置切出的 **102B UNKNOWN** 已撤销：
  `+0x80..+0x1FF` 必须作为一个动态的
  **ELABEL / encrypted-block padding / preserved-tail** 区整体记为 PARTIAL。
  LBA8 当时严格状态同步纠正为 **86 COMPLETE / 426 PARTIAL / 0 UNKNOWN**；
  后续再把 header `MacInfo[6]` 独立闭合后，当前为
  **92 COMPLETE / 420 PARTIAL / 0 UNKNOWN**。
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在当前 22/22 参考样本逐字节一致，是跨扇区动态字段，不是可固定 profile 常量。

##### LBA8 current UsbOnlyInfo：main onlyid 的十六进制 wire 表达

本轮继续把 `+0x14..+0x3D` 的 current producer 从“格式看起来像 onlyid”
追到 Windows 实际调用栈，避免用样本相关性代替 producer 证据。

Windows `RegsiterUsb` 在调用 `sub_100148d0(BuildSector8)` 前的真实机器码为：

```text
1003BD3E  object+0x698 -> EDX
1003BD4A  push EDX                         ; main onlyid
1003BD51  ESI = object+0x2E0              ; UsbLabelParam
1003BD57  sub ESP,0x2AC
1003BD5D  ECX=0xAB
1003BD64  rep movsd                        ; copy full 0x2AC UsbLabelParam by value
1003BD7B  push LBA8_output
1003BD82  ECX = CLabelManage
1003BD88  call sub_100148d0
```

进入 `sub_100148d0` 后，位于 by-value `UsbLabelParam` 之后的尾随 DWORD
精确落在 `[ebp+0x2B8]`：

```text
1001491B  mov edx,[ebp+0x2B8]
10014921  push edx
10014922  push "%08x%08x"
1001492E  call sprintf
...
10014A86..10014A96
          strcpy_s(LBA8+0x1E, 0x20, formatted)
```

因此 current Windows producer 可以严格写成：

```text
UsbOnlyInfo = sprintf("%08x%08x", main_onlyid_bits, 0)
```

Linux `CLabelManage::BuildSector8(char*, UsbLabelParam, unsigned int)` 也以同一
`"%08x%08x"` 模板消费第三个 u32 参数，形成跨平台 writer 对照。

严格 22 份原始生成参考重新按已经独立闭合的 LBA4 identity profile 分组：

- **6/6 current identity**：
  - `OnllyID2Nd == main onlyid`；
  - `HSerialCRC[5] == 0`；
  - LBA8 `HDSerialInfo == 0`；
  - `MacInfo[6] == 0`；
  - `UsbOnlyInfo == format("%08x%08x", main_onlyid_bits, 0)`；
- **16/16 legacy identity**：
  - `UsbOnlyInfo[32]` 为空/全零；
  - `HDSerialInfo` 保留历史非零 profile；
  - 独立 SanDisk 原始盘也落在该 legacy 组，没有 current 规则误命中。

CI 新增
`lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
锁定 current/legacy 双 profile，防止以后把二者错误归一。

这里随后进一步按正式结构边界拆分，而不是继续把42B绑在一起：

- `HDSerialInfo@+0x14..+0x17`：current=0，但16份 legacy 存在非零 profile，
  旧 producer 未定位，继续PARTIAL；
- `MacInfo[6]@+0x18..+0x1D`：Linux DWARF正式命名，Windows/Linux writer
  都从全零 header 初始化得到6B零；semantic reader只从 ElabOffset 进入 ELABEL，
  不读取 MacInfo；严格22份跨 current/legacy **22/22均为6B零**，没有已知
  profile 分叉。因此这6B满足 producer + negative consumer + real-device
  严格标准，升级 **COMPLETE**；
- `UsbOnlyInfo[32]@+0x1E..+0x3D`：current producer 已闭合，
  但16份 legacy 均为空且旧 producer/最终历史 consumer 未闭合，继续PARTIAL。

CI 的
`lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
现在对所有 committed original profile 统一断言 `MacInfo[6]==zero[6]`，
不再只在 current 分支检查。

动态头本轮继续从 producer/consumer 重新核对，新增闭合 76B：

- `+0x08..0x0B ToolVersion[4]`
  - Windows `sub_100148d0` 和 Linux `BuildSector8@0x1D602`
    都固定写 `01 00 00 01`；
  - 22/22 原始盘一致；
- `+0x0C..0x0F Labversion`
  - 两端 writer 均固定写 `0x222`；
  - 22/22 原始盘一致；
- `+0x10..0x13 writeTime`
  - Windows `sub_10016610` 直接返回 `GetTickCount()`；
  - Linux `CLabelManage::GetTickCount@0x1FBAA` 用
    `CLOCK_MONOTONIC` 计算毫秒并返回低32位；
  - 22/22 原始盘均非零并存在跨标签多值，因此不能再误称墙钟时间戳；
- `+0x40..0x7F Reserverd[64]`
  - 两端 writer 都先零初始化完整临时 header，该64B没有后续赋值；
  - 22/22 原始盘解密后全零。

consumer 同样闭合：

- `ReadSector8(char*, UsbLabelParam&)` 只检查 magic、读取
  `ElabOffset` 并解析 ELABEL，跳过上述四段；
- `ReadSector8(char*, BYTE*)` 只在 LLGB 校验通过后原样复制整份
  512B 解密结果，并返回 `cbSize`，不解释这些字段。

因此这 76B 具备 producer + consumer 行为 + 22盘原始证据，可以升级 COMPLETE。
但 `+0x14..0x3D` 仍明确保持 PARTIAL：当前 writer 的
`HDSerialInfo/MacInfo/UsbOnlyInfo` 写法无法解释所有历史盘，不能被相邻闭合字段带着升级。

Windows `sub_100148d0` 与 Linux
`CLabelManage::BuildSector8(char*, UsbLabelParam, unsigned int)`
还独立给出同一份 17-key ELABEL writer 模板：

`<ELABEL>GLab=%s||Indus=%s||Orgcd=%s||Org=%s||Unit=%s||Dept=%s||User=%s||Alarm=%s||Autonum=%s||Label=%s||Rmark=%s||VOL0=%s||VOL1=%s||VOL2=%s||VOLC0=%s||VOLC1=%s||VOLC2=%s||`

Linux writer 与 `UsbLabelParam` DWARF 对齐后，当前 profile 的赋值来源是：

- GLab <- `m_usbGSerial`
- Dept <- `m_usbdepartment`
- User <- `m_usbowner`
- Autonum <- `m_autoid`
- Label <- `m_usbLabel`
- Rmark <- `BeiZhu`
- Indus / Orgcd / Org / Unit / Alarm / VOL0/1/2 / VOLC0/1/2
  在该 writer 中保持初始空值。

22 份原始参考样本按**原始字节先切 `||`、再逐 value 做 GBK**
重新统计后，17 个 key 在 22/22 中全部存在：

- GLab：22/22 = `322CA28A-D7D1448B-DCE2CED9`
- Label：22/22 = `江苏电力!SAFE6`
- Indus / Orgcd / Org / Unit / Alarm / VOL0/1/2 / VOLC0/1/2：22/22 空
- Autonum：`YD000001` 14 份、空 6 份、`1` 2 份
- Rmark：空 20 份、`普通` 2 份
- Dept / User 为业务动态值。

其中 4 份较长盐城 Dept 的原始值最后停在单个 GBK lead byte `0xBD`，
紧接 ASCII `||User=`。这证明 parser 必须**先按原始 ASCII delimiter
切字段，再分别解码字段值**；若先整段 GBK 解码，`0xBD 0x7C`
会吞掉第一个 `|`，破坏 User 字段边界。edpcli 已增加该实盘形态的回归测试。

Provision 也已按官方 writer 修正：

- 不再限制“LBA8 body <= 240B / 固定加密 0x170”；
- canonical ELABEL 在扇区容量范围内动态生成；
- `+0x04` 不包含 NUL；
- 若 `+0x04` 恰为 16B 整数倍，仍额外加密一个块以容纳结尾 NUL。

##### LBA8 `+0x080..0x1FF`：动态 ELABEL / encrypted backing / preserved tail 完整闭合

继续回到两套官方 writer 的机器码后，原先“encrypted block padding”这一命名必须纠正。
Windows `cemsusbregsiter.dll::sub_100148d0` 与 Linux
`CLabelManage::BuildSector8@0x1D602` 的行为完全同构：

1. writer 自己只清零局部工作区，不清 caller 提供的 512B LBA8 output；
2. 把 17-key ELABEL 连同终止 NUL 写到 `ElabOffset=0x80`；
3. 以 `(logical_len / 16 + 1) * 16` 计算 `encrypted_len`；
4. 直接对 caller output 的 `0..encrypted_len` 原地加密；
5. `encrypted_len..0x1FF` 完全不触碰。

Windows 主注册链又给出 backing 来源的独立闭环：`RegsiterUsb` 先用
`ReadSectorData(..., count=0x0D)` 将旧 LBA0–12 读入 `var_500`，随后直接把
`var_500 + 8*sector_size` 传给 `sub_100148d0`。因此：

- ELABEL 正文与终止 NUL 是 writer-owned；
- **ELABEL NUL 后到 encrypted_len 的字节属于既有 backing**，只是因为位于动态加密前缀内而被一起变换，不是协议 zero padding；
- `encrypted_len..0x1FF` 是未加密的 preserve-existing physical tail。

consumer 需要分两层看。注册/制标侧 Windows `cemsusbregsiter.dll::sub_10015820` 与
Linux `ReadSector8(UsbLabelParam&)` 都只执行同一组7键回填：

`registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit`

Linux reader 分别回填 `UsbLabelParam.m_usbLabel/m_usbGSerial/m_usbdepartment/`
`m_usbowner/m_autoid/BeiZhu/m_usbUnit`。但继续审计运行时 DLL 后发现此前“其它十键无
consumer”的结论过强：`out_raw_data/EdpEDiskCtrl.dll::sub_10016260` 是完整的 runtime
reader，**runtime EdpEDiskCtrl reader parses all 17 ELABEL keys**，并将
`GLab/Indus/Orgcd/Org/Unit/Dept/User/Alarm/Autonum/Label/Rmark/VOL0/VOL1/VOL2/`
`VOLC0/VOLC1/VOLC2` 全部解析到 `tagEdpUsbLableInfo` 的固定槽。因此十个当前空值键
应称 compatibility wire slots，而不能称为“全产品 negative consumer”。

实盘侧，严格22份原始参考的正文 22/22 都保持相同17-key顺序；十个 current compatibility key 22/22 均为空。committed originals 新增
`lba8_real_elabel_keeps_all_wire_keys_and_current_compat_slots_empty`，锁定多种
`logical_len`、完整17-key顺序、十个 compatibility 槽空值以及当前块内 backing 的零观测。
另新增 `lba8_preserves_nonzero_backing_inside_the_last_encrypted_block` 合成回归，明确证明
inspect 对 NUL 后但仍位于 encrypted prefix 内的非零 backing 会正确解密保留；此前已有
`lba8_preserves_nonzero_bytes_after_the_dynamic_encrypted_prefix` 与
`lba8_decrypts_one_extra_block_when_logical_length_is_16_byte_aligned` 分别锁定块外 tail
preserve 和 16B 对齐额外一块。

因此 `+0x080..0x1FF` 的384B不再存在未闭合的存储/消费状态：同一 offset 可随
`logical_len/encrypted_len` 动态属于“正文+NUL / encrypted preserved backing / unencrypted
preserved tail”，但三类边界、producer ownership、consumer行为和原始实盘 profile 都已闭合。
本段从 PARTIAL 升 **COMPLETE**。这里的 COMPLETE 绝不意味着 backing/tail 必须为零；
未来遇到非零旧 profile 必须按动态边界保留。LBA8 于是从 `92/420` 提升到
**476 COMPLETE / 36 PARTIAL / 0 UNKNOWN = 93.0%**。

继续追运行时 reader 还补上了剩余 header 的结构传递链：`sub_10016260` 不仅解析17键，
还明确复制 `HDSerialInfo@+0x14` 和 `UsbOnlyInfo[32]@+0x1E` 到输出
`tagEdpUsbLableInfo`；其隐藏输出指针位于调用帧 `+0x120`。上层
`sub_100167c0` 的第6参数就是该输出指针，`CEdpDiskControl::UpdateLabelInfo` 传入
`this+0x2D0`，所以两字段最终缓存于对象 `+0x2E4` 与 `+0x2EE..+0x30D`。目前对这些
精确对象偏移尚未找到值相关行为读点；因此这只能补强“正式runtime结构consumer”，不能替代
legacy producer/派生公式与最终行为语义。剩余36B仍保持PARTIAL。

继续向旧版本追溯后，`HDSerialInfo` 的官方 producer family 已从“完全未知”推进到可执行算法，
但还不足以跨过 strict COMPLETE 门槛。新增证据如下：

- 从金山公开 DLL 档案取得 2020 `CEMSUsbRegsiter.dll` v19.11.4.1，MD5
  `783d01f19e998a514834bc5e5f4249ad`。其 ELABEL 模板在 `sub_10007DF0` 有真实 xref，
  不是链接残留；函数先清零 0xD54 临时标签结构，再调用 `UsbTools.dll` ordinal4，若结果为0
  才 fallback ordinal3，并把结果DWORD写入 LLGB header `+0x14 HDSerialInfo`。
- 同一 writer 随后直接在 `+0x1E UsbOnlyInfo[32]` 上执行
  `wsprintf("%08x%08x", caller_dword, hd_serial_info)`；因此 2020 这一代明确存在
  **HDSerialInfo非零 + UsbOnlyInfo第二DWORD复写同一值** 的过渡 profile。
- 本机 `UsbTools.dll` 的导出表把 **ordinal4 精确命名为 `EDP_DiskNumber`、ordinal3
  精确命名为 `EDP_DeviceNumber`**；两个导出又分别是纯 thunk，跳到
  `DeviceNumber.dll` 的 ordinal3 / ordinal1。这里必须区分两层 DLL 的 ordinal：
  `UsbTools` 是4/3，`DeviceNumber` 自己是3/1，旧文档把“语义映射”和“ordinal编号”
  混成一句的写法已纠正。两代 `EDP_DiskNumber` 的机器码同构：
  枚举 `PhysicalDrive0..3`，通过 `SMART_RCV_DRIVE_DATA(0x7C088)` 下发 ATA
  `IDENTIFY DEVICE(0xEC)`，取 words10..19 的20B Serial Number；每16-bit word交换字节、
  裁剪首尾ASCII空格，读取失败或20B全零 serial 跳过，其余按物理盘序号**无分隔拼接**。
  最终以标准 reflected CRC32 polynomial `0xEDB88320`、initial=0 对完整拼接字节串求值。
  因而已知 ordinal4 主路径可写成：

```text
EDP_DiskNumber = CRC32(serial_PhysicalDrive0 || serial_PhysicalDrive1 || ...)
```

  这里每个 `serial` 都是上述 ATA 规范化后的 C-string，失败/全零盘不参与。
- ordinal3 fallback `EDP_DeviceNumber` 也已继续闭合到可执行公式。2008
  `DeviceNumber.dll::EDP_DeviceNumber@0x10011E00` 使用同一个 stringstream 和同一个
  CRC32 helper：每个有效物理盘的规范化 ATA serial 仍按盘号无分隔写入主 stream；
  随后调用 `fcn_10014120(out, 1)` 获取 MAC 身份串，并通过
  `0x10010E70 = ostream << std::string` 把整串原样追加。mode=1 的跳表只走 MAC
  分支，不序列化 IP：adapter description 先转大写，VMware virtual adapter 由同时命中
  `VIRTUAL` 与 `VMWARE` 的路径排除，MAC 全零也跳过；接受的6B MAC 用 formatter
  style=4 输出为12位**大写、无冒号、无连字符**十六进制。序列化格式为：

```text
MACAddress0=AABBCCDDEEFF\r\n
MACAddress1=001122334455\r\n
...
MACCount=N\r\n
```

  index 从0递增；没有有效 MAC 时该 helper 返回空串。因此 fallback 可写成：

```text
EDP_DeviceNumber = CRC32(
    normalized_ATA_serials_without_delimiters ||
    MACAddress_lines || "MACCount=" || decimal(N) || "\r\n"
)
```

  CRC 仍是 reflected poly `0xEDB88320`、initial=0。`0x10010C60` 已由实现行为锁为
  `ostream << const char*`，`0x10010E70` 为 `ostream << std::string`，而整数 index/count
  通过 `0x10011250` 写入并以 `ret 4` 消费参数；所以这不是根据字符串常量猜出的模板，
  而是已沿调用栈闭合的真实序列化顺序。
- 另取得并核验 2020 `EdpEDiskCtrl.dll` v3.6.10.18，MD5
  `95a06e0d466ba40a7d5c0e6a409e2114`。其 `ReadOrgInfoSector@0x1000C870`
  已经完整复制 `HDSerialInfo@+0x14` 与32B `UsbOnlyInfo@+0x1E`，反证“旧 reader 只有4B
  UsbOnlyInfo”的早期猜测。该 DLL 不导入 `DeviceNumber.dll`；围绕 `this+0x1728` 的后续
  direct-member 审计只消费 `+0x00..+0x0C` 一带版本/标志。全DLL唯一
  `this+0x173C` 命中位于 `0x10015CA7`，用途是把该地址作为0x104B路径缓冲区覆写、规范化并
  `CreateFileA`，而不是读取原来的 HDSerialInfo DWORD。其它已确认结构搬运也止于 `+0x0D`，
  没有把 `+0x14` 间接搬去做比较。
- committed-original 回归 `lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
  现额外锁定 legacy 分支 `HDSerialInfo != 0`；current 仍锁定 `HDSerialInfo == 0`。

因此当前最严谨的代际模型至少是三段：

1. strict legacy originals：`HDSerialInfo != 0`，`UsbOnlyInfo[32] == 0`；
2. 2020 official transitional writer：`HDSerialInfo = EDP_DiskNumber`（0时fallback
   `EDP_DeviceNumber`），`UsbOnlyInfo` 第二DWORD复制同一值；
3. current writer：`HDSerialInfo = 0`，`UsbOnlyInfo = main_onlyid || 0` 的16字符十六进制文本。

这批证据闭合了一个真实非零 producer family、`EDP_DiskNumber` 的主算法以及 2020 runtime 的
negative value-consumer 行为，但**尚未找到能生成 strict legacy 第1种组合的更早 exact writer**，
ordinal3 fallback `EDP_DeviceNumber` 的完整输入公式现已补齐；但**尚未找到能生成 strict
legacy 第1种组合的更早 exact writer / profile selection**。不过继续把 `UsbOnlyInfo[32]`
按真正发生代际分叉的位置拆开后，后16B可以独立闭合：current Windows/Linux 和2020
transitional writer 都只生成固定16字符 `%08x%08x`，且目标header/临时结构预先整体清零，
因此 `+0x2E` 必为C-string终止NUL、`+0x2F..+0x3D` 15B保持零；strict legacy整槽absent/zero，
同一16B自然也是零。registration semantic reader跳过整槽，2020/current runtime只结构保存；
另一本机 v3.6.12.28 `EdpEDiskCtrl` 分支甚至只复制 `+0x1E` 首DWORD，直接跳过后28B，继续
证明 suffix不是行为字段。committed strict originals 新增统一门禁锁定22/22 suffix=`zero[16]`。

因此 LBA8 进一步拆为：

- `HDSerialInfo@+0x14..+0x17`：4B PARTIAL；
- `UsbOnlyInfo text@+0x1E..+0x2D`：16B PARTIAL，三代profile真实分叉仍需追；
- `UsbOnlyInfo suffix@+0x2E..+0x3D`：**16B COMPLETE**，语义为固定终止NUL+zero suffix。

LBA8 由 **476 COMPLETE / 36 PARTIAL** 提升到 **492 COMPLETE / 20 PARTIAL**。

#### LBA11：`DRKB + random252`，VID/PID 是 4 字符 ASCII

- `0x000..0x003 == "DRKB"`：当前 22/22。
- `0x004..0x0ff`：252B 运行时随机材料。
- Linux 官方 DWARF 已恢复 producer / consumer 的原始源码位置：
  - `CLabelManage::BuildSector11`：
    `/mnt/git/cross_platform/src/global/src/diskfile.cpp:783`，
    本地 `libcemsfilesyscheck.so@0x1D350`；
  - `CLabelManage::ReadSector11`：
    `diskfile.cpp:1168`，本地 `@0x1F220`；
  - `CDataSecrity::RandBuffer256`：
    `/mnt/git/cross_platform/src/global/src/datasecrity.cpp:14`，
    本地 `@0x20204`；
  - `CDataSecrity::DataEncrypt`：
    `datasecrity.cpp:34`，本地 `@0x202D4`；
  - `CDataSecrity::DataDecrypt`：
    `datasecrity.cpp:61`，本地 `@0x20476`；
  - `CLabelManage` 构造器：
    `diskfile.cpp:576`，本地 `@0x1C538`；构造参数 `pUID`
    被保存为成员 `m_strUID`。
- `RandBuffer256` 的实际生成算法已闭合：
  `srand(time(NULL))`，先写 `DRKB`，随后对 `i=4..255`
  写 `rand()%255`。因此前 256B 不再是“未知随机 blob”，而是
  **4B 协议 magic + 252B 明确 PRNG 输出**。
- `ReadSector11` 会先检查这 4B `DRKB`，随后把完整前 256B
  原样作为后半区 key 派生输入。因此 random252 的**生成来源和消费用途都闭合**。
- CRC 输入为：

  `DRKB || random252 || VID_ascii4 || PID_ascii4 || size_le64`

- VID/PID 按备份文件中的四位十六进制 ASCII 文本参与 CRC；将 VID/PID 当作数值 little-endian，当前 22 份参考样本均不能解出 PDKB。
- 后半 `0x100..0x1ff` 用该 CRC 的 4B little-endian 作为 A6B0 key；解密后当前 22/22 均为：

  `PDKB || device_id || 0x00 || zero_padding`

- Linux `BuildSector11` 明确先写 `PDKB`，再把
  `CLabelManage::m_strUID` 复制到 `+0x04`；`m_strUID`
  来源于构造参数 `pUID`。对应 consumer 解密后检查 `PDKB`，
  再把 `+0x04` C 字符串赋给输出 `strDPBack`。
  当前 21 份完整原始备份已逐份复核：
  **21/21 PDKB 字符串精确等于各自 device_id**；独立 SanDisk
  已在此前 22 份总审计中独立闭合。
- Windows 侧存在第二套独立同构实现：
  - producer `cemsusbregsiter.dll::sub_10014720 @0x10014720`
    （反编译文件约 L78343）；
  - random producer `sub_10002B90 @0x10002B90`（约 L82081）；
  - KDF/encrypt `sub_10002C30 @0x10002C30`（约 L82112）；
  - consumer `sub_10015F00 @0x10015F00`（约 L93391）。
  它们分别检查/写入同样的 `DRKB/PDKB`，并使用同一
  `rand256 || VID4 || PID4 || size8 -> CRC32 -> cipher` 公式。
- 当前 Windows writer 的 `size8` 来源也已闭合到物理容量：
  `sub_10019780` 打开 `\\.\PHYSICALDRIVE%d`，调用
  `DeviceIoControl(..., 0x700A0, ..., 0x28)`，把返回
  `DISK_GEOMETRY_EX +0x18` 的 64-bit `DiskSize` 写进磁盘信息对象
  `+0xB0/+0xB4`；`RegsiterUsb` 复制同一字段并传给
  `sub_10014720`。
- 本轮把 current Windows 的这条 size 链进一步追到逐次对象复制，排除了
  “枚举得到 DiskSize 后又在中间层做 CHS 转换”的可能：
  1. `sub_100186C0` 枚举 USB 磁盘时在栈上构造 disk-info 对象
     `var_7E0`（机器码 `1001878C: lea -0x7E0(%ebp)`）；
  2. `sub_10019780(...,&var_814)` 成功后，机器码
     `10018940..10018952` 直接把这 64-bit 值写到
     `var_7E0+0xB0/+0xB4`（即 `ebp-0x730/-0x72C`）；
  3. `sub_10019270` 从枚举集合选中目标磁盘；
  4. `sub_100196D0` 机器码 `10019731..10019741` 把选中对象复制进
     `CUsbRegsiter+0x808` 包装对象内部的 `+0x10` disk-info 子对象；
  5. `RegsiterUsb` 调 `sub_10018480` 再把该 `+0x10` 子对象复制到
     栈上 `var_158`；`sub_10017D10/sub_10017940` 对
     `+0xB0/+0xB4` 是 DWORD 原样复制；
  6. `var_158` 基址为 `ebp-0x158`，所以
     `+0xB0/+0xB4 == ebp-0xA8/-0xA4`；机器码
     `1003BD0A..1003BD17` 正是把这两个 DWORD 原样 push 给
     `sub_10014720(BuildSector11)`。
  current writer 从 IOCTL 的 `DiskSize` 到 LBA11 KDF 输入之间没有任何
  `255*63*512` 取整。
- 历史兼容公式本身也已经从“疑似 CHS helper”升级为**官方命名证据**：
  Linux `libcemsfilesyscheck.so` 带 DWARF 的
  `CDisk::GetWindowsDiskSizeFromLinux(unsigned long long&) @0x186D0`
  对应 `DiskInterface.cpp:1196`；独立 `checkdiskback` 中同名函数
  `@0x413040` 使用同一公式。两者都把容量按
  `255*63*512 = 0x7D8200` 向下取整。Windows
  `sub_100184C0` 是同构实现。
- `cemsusbregsiter.dll::sub_100184C0` 在当前注册 build 中确实没有业务 caller，
  不能用它解释实盘；但本轮从独立官方 `UDiskLabelRepair.dll` 找到真正 active 的
  CHS writer/reader 链：`CLabelRepair::Repair -> sub_10008820(Check LBA11)`，
  校验失败后进入 `sub_10008950(ReWrite11Sector) -> sub_10003A40` 重建 LBA11。
- repair 的物理盘对象由 `sub_10002A20 -> sub_10002FF0` 初始化；后者优先调用
  `DeviceIoControl(IOCTL_DISK_GET_DRIVE_GEOMETRY=0x70000, out=0x18)`，得到标准
  `DISK_GEOMETRY`。`sub_10002A20` 再通过 `sub_10019EF0` 计算
  `Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector`，把64位结果写到
  `disk_info+0x30/+0x34`。`sub_10019EF0` 机器码已复核为标准64位乘法 helper。
- `sub_10008820 -> sub_10003BD0` 用同一 `disk_info+0x30/+0x34` 作为 LBA11 KDF
  容量输入；`sub_10008950 -> sub_10003A40` 重写时也传同一64位 CHS 容量。
  因此 repair 路径同时给出了 CHS profile 的 active consumer 与 producer。
- 严格22份原始生成参考仍是 **21/22 exact DiskSize、1/22 CHS-floor**；
  唯一 CHS 参考为 Aigo U335 `onlyid=1987718388`。但本轮新增两个反例把
  “CHS 由设备型号/profile 静态决定”的解释排除了：
  - 独立原始 SanDisk（HSerial high-entropy、LBA6 `+0x1E0..1EF` 非零）
    的 LBA11 仍只在 exact DiskSize 下恢复 `PDKB + device_id`；
  - 同一 Aigo U335、同一 `device_id/VID/PID/物理容量` 的辅助真实采集
    `LBA11.bin/-3/-4` 使用 CHS，而 `LBA11-2.bin` 使用 exact DiskSize。
    这批辅助文件不计入22份 generation reference，只用于证明
    `rev_pmap` 字样/硬件身份本身不能唯一决定 size profile。
- 因而 profile 选择现已闭合到**调用路径**而不是硬件属性：
  **正常注册 writer/reader = exact `DISK_GEOMETRY_EX.DiskSize`；repair
  writer/reader = traditional `DISK_GEOMETRY` CHS capacity。**
  同一 Aigo U335 rev_pmap 的 CHS/exact 双真实捕获正好对应这两条官方路径。
- 新增回归门禁：
  - `lba11_authentic_sandisk_legacy_profile_still_uses_exact_disk_size`；
  - `lba11_same_rev_pmap_device_has_both_chs_and_exact_size_writer_profiles`。
  第二条测试使用的 `LBA11-2.bin` 被明确标注为辅助行为证据，不进入22份
  original generation reference 统计。
- 严格完成统计因此更新为：
  - `0x000..0x0FF`：**256B 完成**；
  - `0x100..0x103`：**4B 完成**；
  - `0x104..0x1FF`：**252B 完成**；
  - **LBA11 = 512B COMPLETE / 0B PARTIAL / 0B UNKNOWN。**
- Provision entropy 已同步改成仅接受 `random252`；builder 自己写
  `DRKB`，调用方不再能够把前 4B 协议结构字节当成外部随机材料。

#### LBA12：整扇 512B 是一个连续 A6B0/A7F0 密文

- 对当前 22/22 参考样本直接执行 `a6b0_full(raw512, CRC32(device_id), counter=0)`：
  - `decoded[0..4] == "EDPF"`；
  - `decoded[0x170..0x200] == zero[144]`。
- 因此旧描述“只加密前 368B，后 144B RAW”错误。
- 过去观察到的 `raw[0x170..] == a7f0_full(zero144, key, initial_counter=0x170)`，正是“整扇连续加密”的自然结果，不是独立 tail 格式。
- `0x170` 的正确含义只是 **当前 LBA12 EDPF 主表/表尾区域的结束位置**，
  不是密码学边界。代码常量已从误导性的 `EDPF_ENC_LEN` 改为
  `EDPF_TABLE_LEN`。
- edpcli 当前实现已统一为：
  - inspect：整扇 512B 解密，展示时只在 `0x000..0x16F` 解析 EDPF 结构；
  - 旧盘 `convert_lba12`：整扇解密，只修改 EDPF entry，再整扇重加密；
  - Provision builder：构造 512B 明文后一次性整扇加密；
  - Provision validator：整扇解密，校验表区，并要求 canonical profile 的
    `0x170..0x1FF` 明文为零。
- 旧盘转换 golden 14/14 在改成整扇重加密后哈希完全不变。原因是
  `0x170..0x1FF` 明文未修改，且 A6B0 是确定性的连续 counter 模式；
  因而重新计算得到的 tail 密文与旧实现“直接拼回原密文 tail”逐字节相同。
  这说明实现修正没有改变既有产品输出，只消除了错误协议模型。

本轮又专门复核了 `0x12E..0x1FF` 的 producer/consumer 边界，用来解决
逐字节主表与总进度表之间的旧账本不一致：

- Windows current writer `sub_10014F30`：
  - 分配 `sector_size+1`；
  - 对整个缓冲执行 `memset(...,0,sector_size+1)`；
  - v0x206 只复制 `0x120` 的 3×96B packed entry，再复制 `0x0E` pass-info；
  - 即最后一次结构写入恰好结束于 `0x12E`；
  - 后续对整扇执行加密并复制回输出；
- Windows current reader `sub_100160B0`：
  - 固定对完整 `0x200` 字节解密；
  - magic/version 合法后只复制 `0x120` entry 区与 `0x0E` pass-info；
  - `0x12E..0x1FF` 没有结构读取或返回；
- Linux `CLabelManage::BuildSector12@diskfile.cpp:922` 独立给出同样的生成原则：
  先按 `sector_size+1` 分配并整块清零，之后只复制相应 ABI 的表和表尾，
  最终以 `m_nSectorSize` 对整个扇区加密；
- Linux ABI 的 v0x206 表本身是 104B×3 的扩展布局，结束位置不同，
  因此它只用于证明“整扇先零初始化、未写区域保持零并整扇加密”这一 writer
  原则；**不能**拿 Linux 的 `0x138+0x0E` 偏移反向覆盖 Windows 96B packed 主盘面；
- Windows packed 主盘面的 exact boundary 仍由 `sub_10014F30/sub_100160B0`
  锁定为 `0x12E`。

实盘门禁也从原先只看 `0x170..` 收紧为完整 post-table 区：

- committed original fixtures 全部满足解密后 `0x12E..0x1FF == zero[210]`；
- 独立 SanDisk original 也满足同一条件；
- 新门禁 `lba12_post_table_plaintext_is_zero_through_sector_end` 锁定完整210B；
- 原 `every_committed_lba12_tail_is_encrypted_zeroes_from_device_id` 继续单独锁
  `0x170..` 的连续 counter 密文性质，防止未来再次把它误改成 raw tail。

因此 `0x12E..0x1FF` 应统一记为 **210B COMPLETE post-table zero padding**。
此前字段主表把 `0x170..0x1FF` 仍写成 PARTIAL 是 stale 状态；但下方严格进度表
和 LBA12 的 `393B COMPLETE` 早已把这144B包含进去，所以本轮只是修正字段账本，
**不得再次把144B加到总 COMPLETE 数**。

#### onlyid：注册时随机 GUID 的 CRC32，不是硬件 ID

- Windows 官方注册链：

  `CoCreateGuid -> GUID raw 16B -> CRC32_bare -> object+0x698 -> LBA4 $$$onlyid$$$`

- 该路径没有把 VID/PID、device_id、容量或 USB serial 混入 onlyid 生成。
- 这与真实样本“相同硬件参数存在不同 onlyid”一致。
- GUID 本身是注册实例随机量；onlyid 只是其 32-bit CRC 压缩结果，不能从 onlyid 唯一恢复原 GUID。
- 跨平台 Provision 不需要依赖 Windows `CoCreateGuid` API，只需要 16B 高质量随机熵并复用相同 CRC32 算法。

#### EDPF 14B 表尾：完整字段名已恢复，不是 terminator

- LBA7 表尾起点 `0xC0`，LBA12 表尾起点 `0x120`。
- 盘上存储前会对 `byte0 / byte3 / byte6` 各 `^ 0x88`；读端执行相反操作恢复结构。
- Linux DWARF 恢复出的官方 `tagEdpPartionPassInfo` 恰好为 14B：
  - `+0x00..01 Version`
  - `+0x02 IsForceChgPassShr`
  - `+0x03 MaxAllowSPErrPasTime`
  - `+0x04 CURSPErrPasTime`
  - `+0x05 IsForceChgPassEnc`
  - `+0x06 MaxAllowECErrPasTime`
  - `+0x07 CURECErrPasTime`
  - `+0x08 bNoPassSetFlg`
  - `+0x09 bNoPassNoChkIPFlg`
  - `+0x0A bNoUsbChkPasSafe`
  - `+0x0B bResetFileKey`
  - `+0x0C ShareBackuppromptPeriod`
  - `+0x0D EncryptBackuppromptPeriod`
- 已闭合行为：
  - `Version`：LBA7 样本出现 `0x0064/0x0206`，LBA12 主格式为 `0x0206`；
  - 两组 `MaxAllow*ErrPasTime/CUR*ErrPasTime`：密码错误计数上限/当前次数；错误时递增，成功后清零；
  - `IsForceChgPassShr/Enc`：Windows Login 对 type2/type4 分别检查；置 1 时阻止普通登录并进入强制改密码路径；
  - `bNoPassSetFlg`：Windows `sub_10008280` 把它作为“无密码/自动登录模式”条件；
  - `bNoPassNoChkIPFlg`：AutoLogin 根据其 0/1 选择是否执行 IP 检查，日志直接打印该字段名。
- 后续审计已闭合：
  - `bNoUsbChkPasSafe`：已追到 `checkdiskback::Update_EDPEDISKSHOWPARAM`
    的值相关策略行为；
  - `ShareBackuppromptPeriod`、`EncryptBackuppromptPeriod`：已闭合为
    **正式命名但在已覆盖实现中 dormant 的 compatibility BYTE**；current writer=0，
    多代 Windows/Linux reader 只 structural-preserve，不做值相关业务消费。
- 已闭合：
  - `bResetFileKey`：Windows `ChangePwd` 的强制改密分支会读取它；置位时重新生成 16B file-key 材料，否则保留并解包原 file-key。
- 当前 22 份参考样本的表尾形态仍只有 4 种；`+0x0B..0x0D` 在该样本集均为 0，
  但这只是观察事实，不能解释成协议恒零。

#### 官方前部写集：固定 13 sectors

- `RegsiterUsb` 分配/读取缓冲长度为 `sector_size * 0x0d`，并调用：
  - `sub_100136c0(..., count=0x0d)`：从起点读取 13 sectors；
  - `sub_10013810(..., count=0x0d)`：写回 13 sectors。
- `sub_100136c0/sub_10013810` 内部都明确以 `count * sector_size` 计算读写长度，因此 `0x0d` 是数量。
- 从 start LBA=0 开始，13 sectors 正好覆盖 **LBA0..LBA12**。
- `u_disk` 的官方 DLL 虚拟注册 trace 同样记录 `start=0, count=13`。
- 当前产品契约因此统一为：备份、inspect、生成、写入、读回、恢复全部只处理 LBA0–12，共 6656B。

#### EDPF：+0x08 是 PartionCount，不是 version

- LBA7（0x40 stride）和 LBA12（0x60 stride）均逐样本验证：`u32@entry0+0x08 == 实际连续 EDPF entry 数量`。
- 当前 22 份参考样本：LBA12 为 22/22 三条；LBA7 为 21 份三条、1 份两条。
- 在这 22 份“原始生成参考”内部，唯一 LBA7=2 的样本仍是 Netac `onlyid=949028302 @ 17:24:33`；与同 onlyid 的 17:23:49 / 17:24:20 对比，仅 LBA7 发生变化，其余 LBA0–12 一致，因此该扇区继续按局部实验/中间态降权。排除它后，原始 LBA7 参考是 21/21 三条。
- 新纳入的独立真实免密 SanDisk Ultra 则给出**第二个、且是真实在用的两条 entry profile**：entry0=type2、entry1=type4，二者 `NeedDisturb=1`、`NeedEncrypt=1`，`PartionCount=2`。这证明“两条 LBA7”本身不能再被描述成只可能是实验态；它只说明 Netac 那一份不能用于反推原始三分区 writer。
- 对该真实免密盘重新按 packed 0x40 ABI 逐字段解码时，两个 entry 的 `Version@+0x04` 都是 **0**；目录中旧 `disk4_info.json` 的 `"ver": 2` 来自历史解析器把 `PartionCount@+0x08` 错当成 version，现已由回归门禁明确拦截。
- 因此 `+0x08` 必须命名为 `partition_count` / `PartionCount`；表格式代际不能再从该字段推断。

#### LBA7 packed 64-byte ABI versus Linux natural 72-byte ABI

本轮重新从 Linux DWARF、Windows 转换器和22份 original real-device 三条线核对
LBA7，确认同名 `tagEdpPartionInfo` 存在不能混用的 ABI：

- `libcemsfilesyscheck.so` DWARF：`sizeof(tagEdpPartionInfo)=0x48`，
  `UserKeyCRC@+0x30`、`FileKeyCRC@+0x38`、`EncryptFileKey@+0x40`；
  `BuildSector7@0x1DCDA` 复制 `0xD8=3*0x48`，natural pass-info 在 `+0xD8`；
- Windows physical/runtime old table：stride 固定 `0x40`，去掉 natural ABI 中
  `+0x34..+0x37` 的对齐洞，因此物理布局为
  `UserKeyCRC@+0x30 / FileKeyCRC@+0x34 / wrapped8@+0x38`，表尾在 `+0xC0`；
- `cemsusbregsiter.dll::sub_10016490` 是官方 old->new converter：逐字段把
  `3*0x40` old entry 扩为 `3*0x60` runtime entry；目标先清零，所以
  runtime `EncryptMode@+0x58` 为0；
- `edpediskctrl.dll::sub_100125B0` 是反向 new->old converter，明确把 runtime
  `+0x34/+0x38/+0x3C` 写回 packed old `+0x34/+0x38/+0x3C`。

22份原盘按 device-id CRC 派生的 LBA7 rolling key 重新解密：

- packed `0x40`：21/22 为三条 EDPF，1份已知局部中间态为两条；
  `+0xC0` pass-info 22/22 都恢复合法 Version（21份0x0064、1份0x0206）；
- natural `0x48`：22/22 都无法得到三条连续 EDPF，`+0xD8` pass-info
  也 0/22 合法；
- 因此 Linux 72B natural ABI 只可用于字段名/源码来源参考，不能直接作为
  Windows 实盘 LBA7 物理 offset。

独立真实免密 SanDisk Ultra 也按同一 device-id CRC/rolling-XOR 重新解密，
得到 2×0x40 连续 EDPF、合法 `+0xC0` pass-info（Version=0x0064），并且
两个 entry 的 `Version@+0x04=0`。仓库新增
`tests/fixtures/protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex`
与定向测试固定这一事实。该样本增加的是“真实 profile 行为”覆盖，不改变
22份原始生成参考集的计数，也不单凭样本值把 Version/NeedDisturb 升级为 COMPLETE。

#### LBA7 entry Version / entry1+entry2 NeedDisturb：compatibility metadata 生命周期闭合

本轮直接回到官方机器码，而不是沿用旧结构猜测。

\`cemsusbregsiter.dll::sub_10016490\` 的 old->new 转换循环对三条 entry 逐条执行
\`0x40 -> 0x60\` 映射，并明确复制 \`old+0x04 -> new+0x04\`（Version）和
\`old+0x10 -> new+0x10\`（NeedDisturb）。\`edpediskctrl.dll::sub_100125B0\`
的反向 \`0x60 -> 0x40\` 转换同样逐条复制这两个 DWORD，因此二者都是实际 ABI
字段，而不是反编译器误识别的洞。

对当前官方 writer \`CUsbRegsiter::CreatePartitions/sub_1003DB50\` 再看机器码：

- 开头先 \`memset(old_table, 0, 0xC0)\`，一次清零完整的 \`3*0x40\` old table；
- 随后显式写 Flag/PartionCount/PartionType/NeedDisturb/NeedEncrypt/几何/CRC/key；
- 三条 entry 都没有任何 \`+0x04\` 覆盖写，所以当前 writer 的
  \`Version@+0x04=0\` 来自整表零初始化；
- 注册调用者对 CreatePartitions 的 NeedDisturb 参数固定传 \`1\`；
- entry0、entry1 都显式执行 \`NeedDisturb=1\`；
- entry2 没有对应覆盖写，因此继承整表清零值 \`0\`。

这解释了当前三分区 profile 的 \`1/1/0\`，但也证明它不是按 PartionType
定义的恒等规则。新纳入的真实免密 SanDisk 是两条 entry：
type2/entry0 NeedDisturb=1，type4/entry1 NeedDisturb=1；而标准三分区原盘的
type4/entry2 NeedDisturb=0。仓库新增
\`lba7_need_disturb_is_not_a_partition_type_invariant\` 门禁，禁止以后把
\`type4 -> 0\` 写死。

consumer 侧要区分“行为 consumer”与“compatibility structural consumer”：

- 两版 Windows \`vrvaud_c\` 的 \`ReadPartionInfoExNew\` 都会把完整 \`0xC0\`
  packed old table 读入运行时缓冲；
- \`NewCheckDisTurbUsb\` 与 \`NewCheckDisTurbUsbEx\` 只对
  entry0 \`NeedDisturb@+0x10\` 做非零门控；
- 本轮再次从两版全局缓冲的真实地址复核 stride：ydcc
  `0x1020BF40` 与 Win10 `0x10172520` 都只分配/清零 `0xC0`，而各自
  `sub_*C5D0` 用 `(i << 6)+base+0x0C` 枚举三条 PartionType，证明这里是
  **3×0x40 packed old table**；此前把该全局区描述成0x60 runtime table的口径撤销；
- 两版对完整 table 地址区间的静态 xref 结果一致：只有
  entry0 `NeedDisturb@base+0x10` 存在行为读取；entry1/entry2 NeedDisturb 与
  三条 `Version@+0x04` 都没有直接 xref，动态遍历也只读 PartionType；
- Linux \`CLabelManage::GetPartionFromOld\` 也只是把 Version/NeedDisturb
  从 old ABI 搬到 new ABI；
- Linux \`CLabelManage::GetPartionFromOld\` 对 Version/NeedDisturb 只做字段搬运；
  继续逐函数复核 \`CDiskReader::GetTagPartitionInfo\`、\`DecryptFileKey\`、
  \`CheckFileKeyCrc\`、\`ReadFileSysSector0\` 与
  \`DecryptFileSysSector0\` 后，真实行为读取集中在 Flag/PartionType/
  UserKeyCRC、StartSector、FileKeyCRC、wrapped key 与 EncryptMode；
  entry \`Version@+0x04\` / \`NeedDisturb@+0x10\` 均未参与这些检查链；

旧版 \`EdpEDiskCtrl.dll\` 也再次表明：旧 LBA7 被读出后，协议代际由
14B pass-info Version 决定/被上层固定为 \`0x64\`，并未发现 entry
\`Version@+0x04\` 用作版本选择。

实盘复核：committed original fixtures 的全部有效 EDPF entry 与新增真实免密
SanDisk 两条 entry 的 \`Version@+0x04\` 全部为0；扩展只读历史去重扫描同样
没有非零 Version。NeedDisturb 则稳定出现三-entry `(1,1,0)` 与两-entry `(1,1)`
两种 positional profile，没有第三种组合。

因此这里不把负 xref 误写成“Reserved”，而是按已经用于其它兼容槽的严格模型闭合：

- 3×Version 共12B：current writer zero-init；Windows old/new converter 双向保存；
  Windows 两版与 Linux 文件系统检查链都不把它作为版本选择条件；真正协议代际由
  14B pass-info Version 决定；
- entry1/entry2 NeedDisturb 共8B：current writer 分别形成 1/0 positional profile；
  converter 保存；Windows 两版只有 entry0 同名字段存在行为读取，Linux 对应检查链
  不读取 entry1/2。它们不能继承 entry0 的 MBR scramble 业务解释。

仓库门禁现对所有 committed original fixtures 逐 entry 锁定 Version=0 与
positional NeedDisturb profile，并由独立 SanDisk 的 type4@entry1=1 继续阻止
`PartionType -> NeedDisturb` 的错误恒等化。

所以这20B由 PARTIAL 升 COMPLETE，准确语义是
**formal ABI compatibility metadata：producer/profile 已知、converter structural-preserve、
cross-platform negative-semantic-consumer、真实 profile 已锁定**。COMPLETE 不要求
未来其它 writer profile 必须仍写0/1/0；遇到新 profile 应扩展兼容模型。

#### LBA7 v0x0064 packed legacy file-key wrapping

旧表 `+0x38..+0x3F` 8B wrapped key 本轮完成 producer-consumer 闭合。

直接 consumer 是 `edpediskctrl.dll::sub_10026050`：

1. v0x0064 固定 `key_len=8`；v0x0206 才切到16B；
2. 从 runtime entry `+0x38` 复制 wrapped material；
3. 调 `sub_10028AB0(password, ..., EncryptMode, ...)` 解包；
4. 对解出的8B 调 `sub_10038840`（CRC32_bare）；
5. 必须等于 entry `FileKeyCRC@+0x34`，否则拒绝。

legacy converter 产生的 `EncryptMode=0` 进入专门旧算法：

```text
K = fold32(password)
fold32: little-endian 4B chunk 求和，尾部不足4B补零，u32 wrapping
plain_lo = wrapped_lo XOR K
plain_hi = wrapped_hi XOR K
```

机器码辅助函数已独立拆清：`sub_1005CEF0` 是 unsigned 64-bit shift helper，
`sub_1004EF80` 是 unsigned 64-bit multiply helper；化简后
`sub_10011450` 就是上述两个 half 的 XOR 变换。逆向 producer
`sub_10028DB0` 使用同一对称公式。

默认口令独立重算：

```text
CRC32_bare("0000aaaa") = 0x0429735D
fold32("0000aaaa")     = 0x91919191
```

写回链同样闭合：

```text
CEdpDiskControl::ChangePwd / sub_100269A0
 -> sub_10026050
 -> sub_10028DB0
 -> sub_100125B0          # runtime 0x60 -> packed 0x40
 -> SavePartionSector / sub_10028580
 -> sub_10010FC0
      memcpy 0xC0 old table
      append 0x0E pass-info
      rolling-XOR
      WriteFile(LBA7)
```

22份 original real-device 全量正向复算：

- 共28条非零 type2/type4 legacy entry；
- 28/28 的 `UserKeyCRC=0x0429735D`；
- 用 `K=0x91919191` 对 wrapped8 两个 DWORD 分别 XOR；
- **28/28** 解出的8B file-key 都满足
  `CRC32_bare(file_key8) == entry.FileKeyCRC`。

因此三条 entry 的 wrapped8 共 **24B PARTIAL -> COMPLETE**。
`FileKeyCRC(+0x34..+0x37)` 此前已因 CRC consumer 链计入 COMPLETE，
本轮不重复把这4B/entry计数。新增回归门禁：

- `lba7_physical_entries_are_packed_64_not_linux_natural_72`；
- `lba7_v64_packed_legacy_file_key_wrap_matches_real_fixtures`。

#### LBA7 `+0x0CE..+0x1FF`：306B writer-zero post-table 区完整闭合

Windows `edpediskctrl.dll::sub_10010FC0` 是 packed LBA7 的直接写回 producer。
本轮继续回实际实现核对而不是沿用旧分析：

- `var_1020=0` 后调用 `sub_1004D110(&var_101F, 0, 0xFFF)`；
- `sub_1004D110` 的机器码已确认是 memset 等价实现（对齐后 `rep stosd`）；
- 随后只 `memcpy` 0xC0 packed old table 到 staging `+0x000`；
- 再复制0x0E pass-info到 `+0x0C0`；
- 最后对完整256个16-bit word，即512B做 rolling XOR并 `WriteFile` 到 LBA7。

因此 `+0x0CE..+0x1FF` 的306B不是“没人知道的尾巴”，而是显式 zero-init 后
从未被任何结构写覆盖的 writer-owned 区。

consumer 也闭合：Windows `ReadPartionInfoExEx/sub_10010B40` 会解码完整512B，
但 magic 成功后只复制0xC0 table和0x0E pass-info给调用者；`+0x0CE..+0x1FF`
完全不返回、不解析。Linux `BuildSector7@0x1DCDA` 独立体现相同的
“整块清零→结构写入→整扇rolling”原则，不过其 natural ABI 表尾在0xE6，
所以只作为原则交叉，不拿 Linux offset 覆盖 Windows packed 物理边界。

严格22份原始生成参考重新逐盘解密：**22/22 的 `+0x0CE..+0x1FF`
全部为 zero[306]**，独立 SanDisk original 也无反例。新增 CI 门禁：

- `lba7_post_table_plaintext_is_zero_through_sector_end`。

这306B因此满足区域边界 + official producer + negative consumer + 22盘原始证据，
可从 UNKNOWN **直接升级 COMPLETE**。复核总账时同时纠正此前漏记的
entry0 `NeedDisturb` 4B：该字段早已由 Windows producer、两版
`NewCheckDisTurbUsb(*)` active consumer 与22/22原盘闭合为 COMPLETE，但旧总数
没有加上这4B。因此 LBA7 严格状态应由实际的
`183 COMPLETE / 23 PARTIAL / 306 UNKNOWN` 先更新为
`489 COMPLETE / 23 PARTIAL / 0 UNKNOWN`；后续又闭合
`bNoUsbChkPasSafe(+0x0A)` 1B；本轮又闭合3条 entry Version（12B）与
entry1/entry2 NeedDisturb（8B），最后将 pass-info `+0x0C/+0x0D`
两个 BackupPromptPeriod BYTE 按 dormant compatibility-field 生命周期闭合。
因此 LBA7 当前为 **512 COMPLETE / 0 PARTIAL / 0 UNKNOWN**。

#### LBA12：主运行时盘面是 96B packed entry；不要与 104B 检查结构混用

对 LBA12 的“已知”采用更严格标准：**只知道 offset、长度或结构名，不等于知道字段语义**。
只有写端来源、读端消费、取值语义至少两项闭合，才计为“已知”；否则一律降为“部分已知”。

主运行时盘面 96B stride 已由两套独立代码闭合：

- Windows `cemsusbregsiter.dll::CreatePartitions` 构造 3 个 `0x60` entry，并把
  `0x120 = 3 * 0x60` 字节写入 LBA12 entry 区；
- Linux 挂载库 `libedpedisk.so::EdpDiskLayoutTagePartV2::LayoutParsedata`
  从物理扇区复制 **0x120B**，随后从物理 `+0x120` 读取 14B 表尾，并验证版本 0x206；
- `libedpedisk.so::Volume::GetPartitionHeader` 只按 96B entry 复制 `+0x00..+0x5f`，
  并直接读取 `entry+0x58` 的低 1B 做算法分派。

`libcemsfilesyscheck.so` 的 DWARF 还存在一个 **104B** `tagNewEdpPartionInfo`
（0x68），其 `BuildSector12(version=0x206)` 会处理 312B = 104×3，并使用
`+0x138` 表尾。这是文件系统检查/修复组件的扩展结构，不得反向覆盖主盘面的
96B packed layout。

#### LBA12 96B packed entry：当前字段置信度

| 偏移 | 长度 | 当前名称 | 当前置信度 | 证据/限制 |
|---|---:|---|---|---|
| +0x00 | 4 | Flag = `EDPF` | 已知 | 写端固定写入；读端判 magic |
| +0x04 | 4 | Version/entry-local compatibility metadata | **COMPLETE** | current 3×96B writer 整表零初始化后从不覆盖该DWORD；Windows/Linux packed runtime只结构携带，协议版本由14B pass-info Version决定；22份×3 entry与全树历史复算均为0 |
| +0x08 | 4 | PartionCount | 已知 | 写端来源 + 当前22/22均等于实际连续条目数 + Linux字段名 |
| +0x0C | 4 | PartionType | 已知 | 1=Boot / 2=Share / 4=Encrypt；Windows/Linux运行时均消费 |
| +0x10 | 4 | NeedDisturb | **COMPLETE（entry0行为 + entry1/2 compatibility）** | entry0 已由旧版 `NewCheckDisTurbUsb(*)` fallback 行为门控闭合；entry1/entry2 current positional profile 固定为1/0，Windows/Linux runtime只结构保留而无值相关消费；22份历史均 `(1,1,0)` |
| +0x14 | 4 | NeedEncrypt | 已知 | Windows InitDiskInfo/UserLogin 实际消费；0=unencrypted，1=启用透明加密 |
| +0x18 | 8 | StartSector | 已知 | 写端计算、挂载端使用 |
| +0x20 | 8 | SectorSize | 已知 | 实盘=512；布局/挂载使用 |
| +0x28 | 8 | PartionSize | 已知 | 写端计算、UserLogin/mount 参数实际消费 |
| +0x30 | 4 | UserKeyCRC | 已知 | 密码校验链消费；默认密码CRC已复算 |
| +0x34 | 4 | FileKeyCRC | 已知 | 解 wrapped key 后 CRC 校验；Windows UserLogin 明确比较 |
| +0x38 | 16 | wrapped file-key material | **COMPLETE** | mode1/2/3 的 UI→request→writer 可达链、官方 writer/consumer 算法均已闭合；22份原始实盘44条加密entry全部为mode2。新增隔离 Unicorn 虚拟盘证据直接执行官方 `CEMSUsbRegsiter.dll::CreatePartitions`：mode1/2/3 最终 entry0 wrapping callsite 分别命中 `0x1003ED77/0x1003ED44/0x1003EDA7` 各1次，三份512B LBA12 wire image由官方二进制原生生成；以 `ProofPass1!` 为输入，A6B0、标准SM4-ECB、标准AES-128-ECB 三套独立reader均恢复同一16B file-key `147196f5a2ec7912edf13f75d766cb42`，CRC均=`0xFF4C1D36`并等于 entry.FileKeyCRC。虚拟writer fixture明确不计入physical real-device census，但它不是edpcli公式合成数据，而是first-party executable runtime positive wire evidence |
| +0x48 | 16 | `EncryptFileKey32[16]` cross-generation compatibility slot | **COMPLETE** | old 72-byte ABI has no EncryptFileKey32 slot；104B checker ABI正式命名 natural `+0x50 EncryptFileKey32[16]`，但 old→new converter 不填，DecryptFileKey/CRC/filesystem decrypt 均不读；packed runtime只结构缓存该16B而无值相关读取，current Windows writer显式清零；22盘66/66 entry全零 |
| +0x58 | 1 | EncryptMode | 已知 | Windows writer/reader + Linux挂载分派；见下方支持矩阵 |
| +0x59 | 7 | Reserved[7] | **COMPLETE** | official DWARF 明确命名 Reserved[7]；Windows CreatePartitions 对 3×96B 整表先清零且只写到 +0x58；Windows/Linux runtime 登录/改密不消费该区；22盘 66/66 entry 为零 |

`EncryptMode` 的枚举由 Linux DWARF 直接给出：

- 0 = `eEncryptAES64`
- 1 = `eEncryptAES128`
- 2 = `eEncryptSMS4`
- 3 = `eEncryptAESOPENSSL`

但**枚举存在不等于每个组件都实际支持**：

- `libedpedisk.so::GetPartitionHeader` 主挂载路径：0→OldEdp，1→AES128，2→SMS4；
  其它值在该 build 不创建可用 header；
- `libcemsfilesyscheck.so::fileKey_Decrypt` 当前 build 只实现 mode=1/2；
- Windows writer 有 1/2/3 的 wrapped-key 写入分支；
- Windows `UserLogin` 对 mode=3 有“先按3解，CRC失败后按1重试”的兼容路径。

因此不能把“0/1/2/3”简单写成统一跨版本算法支持表。

`NeedEncrypt` 已由 Windows 运行时代码闭合：

- `InitDiskInfo` 在 Share entry 上读取 `entry+0x14` 并保存到运行时状态；
- `UserLogin` 在 Share 存在但该状态为 0 时直接记录
  `There are unencrypted!`；
- 所以该字段可定性为：0=该分区不启用透明加密，1=启用透明加密。

`NeedDisturb` 已补到“字段名 + 写端来源 + 正向兼容消费路径”，但这个结论有明确版本边界：

- Windows writer 直接写入 `CreatePartitions(arg2)`；
- 对已逆向的标准三分区创建分支，写端结果已经按 96B entry 基址重新核对：
  - Boot = 1；
  - Share = 1；
  - Encrypt = 0；
- 排除 edpcli 自生成的 `_nopwd_` 备份后，当前真实参考样本全部与该 writer profile 一致：
  type1=1、type2=1、type4=0；
- 这说明“真实参考集 + 当前 Windows writer”目前没有冲突，但仍不能把它升级成
  `PartionType -> NeedDisturb` 的协议恒等式；
- Windows `UserLogin` 真实机器码与 `EdpMountFile` 参数结构已经对齐：
  `NeedEncrypt`、StartSector、PartionSize、FileKey、FileKeyCRC、EncryptMode 会进入挂载参数，
  但 `NeedDisturb` 没有进入当前用户态→挂载库→驱动参数链；
- Windows 主 DLL 中 `entry+0x10` 的其它命中均是 96B entry 之间的结构复制；
  与之相对，`UserLogin` 对同一 entry 明确读取 `+0x0c/+0x14/+0x28/+0x30/+0x34/+0x38/+0x58`，
  未出现对 `+0x10` 的条件判断或参数映射；
- Linux `libedpedisk.so::EdpDiskLayoutTagePartV2::LayoutParsedata` 只把 3×96B entry
  整块复制进内存；`Volume::GetPartitionHeader` 再按值复制整条 96B entry；
  `PartitionHeader` 构造函数把 `NeedDisturb/NeedEncrypt` 这一 8B 保存到对象
  `+0x50..+0x57`。继续扫描 `PartitionHeader*` 方法后，未找到构造之后对对象
  `+0x50/+0x54` 的业务读取或分支；
- 旧版 `vrvaud_c.m::ReadPartionInfoExNew` 会将解出的 packed EDPF table
  复制到全局 `0x1020BF40`；同文件 `sub_1003c5d0` 用
  `base+0x0C+i*0x60` 读取 type 1/2/4，证明该全局区确实是 96B stride 表；
- 因此 `dword_1020BF50 = base+0x10` 精确落在 **entry0.NeedDisturb**。
  `NewCheckDisTurbUsb` 和 `NewCheckDisTurbUsbEx` 在
  `ReadPartionInfoExNew == EDP_SUCCESS` 后都执行：
  `if (entry0.+0x10 != 0) { out=1; success=1; }`；
- Windows 10 驱动包的另一套 `vrvaud_c.m` 独立出现同样布局：
  table base=`0x10172520`、type=`base+0x0C+i*0x60`、
  判断=`dword_10172530 = base+0x10`；
- 上层 `AllCheckModeUsb` 会把 `NewCheckDisTurbUsb(*)` 成功结果作为一条
  独立识别/处理分支继续执行，因此这不是仅复制后从不读取的死字段；
- 2026-06-29 Aigo U335 的真实客户端日志证明
  `NewCheckDisTurbUsb -> GetNewTagePartionInfo -> AllCheckModeUsb`
  是实际运行路径；但该次走的是新版 `GetNewTagePartionInfo` 成功分支，
  **不能**冒充 fallback `+0x10` 判断的动态实测。

因此能闭合的行为只到：
**entry0 +0x10 是旧版/兼容 NewCheckDisTurbUsb fallback 的 success 门控位。**
这仍不足以把字段名翻译为“扰码开关”“防篡改”“激活”“只读”等更具体功能。
新版主路径和 Linux 主挂载链仍可能只保留/透传该字段。

对 Provision 的直接约束是：canonical Share/entry0 的 `NeedDisturb`
必须保持非零；新增生成回归锁定 `entry0+0x10 == 1`。
这条消费者只读 entry0，不能据此推导 type4 的固定取值。

#### LBA12 pass-info：密码状态组进一步闭合

14B 表尾当前能确认：

- `+0x00..01`：版本；
- `+0x03`：Share 最大密码错误次数；
- `+0x04`：Share 当前错误次数；
- `+0x06`：Encrypt 最大密码错误次数；
- `+0x07`：Encrypt 当前错误次数。

Windows `ChangePwd/sub_10026050` 进一步证明：

- Share 改密成功时同时清 `tail+0x02` 与 `tail+0x04`；
- Encrypt 改密成功时同时清 `tail+0x05` 与 `tail+0x07`；
- Windows 登录入口在真正执行密码校验之前调用独立检查函数：
  type2 读取 `tail+0x02`，type4 读取 `tail+0x05`；对应字节非零时直接阻断普通登录。
  结合“改密成功清零”，`+0x02/+0x05` 的行为可闭合为 Share/Encrypt
  **强制改密状态标志**，不再只是“密码状态组成员”；
- `tail+0x0B` 也已出现真实消费者：仅在 version>=0x64、目标分区的强制改密标志非零、
  且 `tail+0x0B` 非零时，`ChangePwd` 才调用 `sub_10029e20` 生成新的 16B 材料；
  该生成函数以 `CoCreateGuid` 为源形成 16B 输出。随后代码重新计算 FileKeyCRC，
  再把该 16B 材料按新密码重新包装。若条件不成立，则走“解开原 wrapped key 并校验 CRC”
  的保留旧 file-key 路径。因此 `bResetFileKey` 可闭合为：
  **强制改密时是否同时重新生成 file-key 材料的门控**；
- `+0x0A bNoUsbChkPasSafe` 在当前 Windows 主 DLL 中被复制到对外结构的一个独立字节。
  机器码已确认该复制发生在
  `CEdpEDiskCtrlInterface::Init`：`m_PassInfo+0x0A -> Init输出+0x11`；
  `EdpEDisk.exe` 在初始化时把应用对象 `+0xA4` 作为该输出结构传入，因此该状态会被
  暴露到应用层；此前在 Windows 应用本体没有找到对对应 `app+0xB5` 的直接读取；
- 本轮把 `+0x0A` producer 再向上追了一层：当前
  `CUsbRegsiter::CreatePartitions/sub_1003DB50` 先把完整14B pass-info
  `memset(..., 0, 0x0E)`，随后机器码
  `1003E78A..1003E790` 明确执行
  `tail+0x0A = create_arg1+0x109`；而
  `WriteNormalULabel -> sub_10046E80` 又明确执行
  `create_arg1+0x109 = UsbWriteParam+0x7EC`。因此 `+0x0A`
  是注册/制标请求中的显式1B配置输入，不是未初始化噪声或尾部 padding；
- 同一个 current CreatePartitions 首次建表路径对 `tail+0x0C/+0x0D`
  没有任何覆盖写；二者直接继承 14B 全零初始化。**这一阶段**结合22份原始参考与
  新增真实免密 SanDisk 都为0，只能确认 current writer 的零来源，尚不足以升级；
  后续已补齐四代 Windows reader、Linux structural-preserve、独立 policy 排除和
  扩展历史 profile 证据，最终按 dormant compatibility-field 生命周期 COMPLETE，
  见本节后续更新；
- Linux `CDiskReader::ParseSector12` 把完整 14B pass-info 保存到
  `CDiskReader+0x210`。机器码全模块扫描可找到 `Version @+0x210`
  在 `DecryptFileKey` 中的显式读取，却没有找到
  `+0x21A/+0x21C/+0x21D`（分别对应 pass-info
  `+0x0A/+0x0C/+0x0D`）的直接业务读取。这只是
  **libcemsfilesyscheck.so 单模块**的负证据；其中 +0x0A 后续已在独立
  `checkdiskback` 找到真实 consumer，不能再写成“全产品不消费”；
- 当前 22 份原始参考样本中，`bNoUsbChkPasSafe(+0x0A)` 并非恒零：
  **18/22=0、4/22=1**；且每一份样本的 LBA7/LBA12 取值都逐字节一致。
  因此它明确是会随标签状态变化并跨两份表同步保存的真实字段，绝不能归为 padding；
- 本轮从此前未纳入主审计的独立官方 Linux 可执行文件
  `checkdiskback` 找到该字段的真实行为 consumer：
  `Update_EDPEDISKSHOWPARAM(checkdisk::_EDPEDISKSHOWPARAM*,
  tagEdpPartionPassInfo*) @ 0x406B70` 的首条逻辑就是
  `cmp byte [pass+0x0A],1; setne [showparam+0x03]`。
  这不是 memcpy/opaque round-trip，而是根据字段值生成布尔策略位：
  **bNoUsbChkPasSafe==1 -> showparam+3=0；其它值 -> showparam+3=1**。
  同函数邻接字节可由日志/赋值交叉定位：
  showparam+0=`bShowSharePartion`、+1=`bShowEncryptPartion`、
  +2=`bAutoLogin`、+4=`bForceChgPassShr`、+6=`bOtherDisk`；
- `CreateSafe6TmpPolicyFile@0x407D50` 在构造 SAFE6 临时策略时明确调用上述
  `Update_EDPEDISKSHOWPARAM`，随后把整个策略体纳入 CRC 并加密写出。
  消费端又有两套独立实现：
  `EdpEDiskBack::Safe6PolicyFile::GetSafe6Policy@0x4100B0` 与
  `linuxedpedisk::Safe6PolicyFile::GetSafe6Policy@0x41EAA0`，
  均解密/校验并恢复完整策略参数结构。前者 `BusService::Init` 还把7B
  show-parameter 尾组写入运行时对象，其中 showparam+3 落到
  `runtime+0x20041`；相邻日志明确打印 show-share/show-encrypt/auto-login/
  force-change-password 等策略字段；
- 因而 `bNoUsbChkPasSafe` 已满足本项目严格 COMPLETE 标准：
  **显式制标 producer + 值相关行为 consumer + 加密 policy 跨组件传递 +
  22份原始实盘0/1双值与 LBA7/LBA12 同步证据**。LBA7 `0x0CA` 与
  LBA12 `0x12A` 各1B均由 PARTIAL 升 COMPLETE；不需要把字段英文名进一步
  猜成未经证据支持的中文业务标签；
- `+0x0C/+0x0D` 当前 Windows 主 DLL、另一版 `out_raw_data/EdpEDiskCtrl.dll`、
  Linux `libcemsfilesyscheck.so`，以及本轮补扫的 Linux
  `EdpEDiskQt5/EdpEDiskBack/linuxedpedisk` 客户端路径均未找到直接消费者；
  Linux DWARF 只给出
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方字段名。
  当前 22/22 原始参考样本的两字节均为 0；
- 与 `+0x0B bResetFileKey` 类似，“真实样本全零”本身不能推出 padding。
  本段是较早阶段结论；后续通过 producer + 多代 structural-preserve +
  cross-platform negative-semantic-consumer 证明它们是**正式但 dormant 的
  compatibility fields**，不再要求虚构一个当前实现中不存在的 active period consumer；
- `+0x0A` 当前可记为“真实可变状态 + Windows Init 向外暴露”，但
  `bNoUsbChkPasSafe` 这个字段名本身仍不足以证明具体的密码安全绕过策略；
  在找到实际策略分支前不得把它翻译成“跳过安全检查”等确定行为。

本轮又补做了**跨版本 Windows consumer 审计**，结果进一步收紧而没有升 COMPLETE：

- current `edpediskctrl.dll` 的 pass-info 运行时基址可由已闭合字段反推出：
  `word_10092A20=Version(+0x00)`、`byte_10092A28=+0x08`、
  `byte_10092A29=+0x09`，因此剩余三字节精确映射为
  `0x10092A2A(+0x0A)`、`0x10092A2C(+0x0C)`、`0x10092A2D(+0x0D)`；
- `+0x0A` 在 current DLL 中只有一个直接 xref：
  `Init输出+0x11 = pass_info+0x0A`。解析接口对象 vtable 后确认，
  该代码属于 `CEdpEDiskCtrlInterface::Init`（vtable slot1），不是独立策略函数；
- current `edpedisk.exe` 在 `CEdpSecDiskAppApp::InitInstance` 中以
  `app+0xA4` 作为 Init 输出，因此 `+0x0A` 最终落在 `app+0xB5`；
  全文件扫描没有任何 `app+0xB5` 读取，`app+0xA4` 也只在这次 Init 调用中出现；
- 旧版 `/VRV/edp/EdpEDiskCtrl.dll` 可独立反推出
  `pass_info base=0x10063398`：`+0x08=0x100633A0`、
  `+0x09=A1`、`+0x0A=A2`、`+0x0C/+0x0D=word_100633A4`。
  该版本同样只把 `+0x0A` 复制到 Init 输出 `+0x11`，没有策略分支；
  `+0x0C/+0x0D` 只见成对清零，没有读取 xref；
- 旧 `edpedisk.exe` 同样只把 `app+0xA4` 传给 Init，后续没有消费
  `app+0xB5`。因此“仅向外暴露、当前宿主未消费”的边界至少跨两代 Windows
  实现成立，不是单个构建偶然遗漏；
- `vrvaud_c` 中的 `BackupPromptInfo/BackupStartTime/BackupEndTime`
  已追到备份 UI 与时间窗口逻辑，但没有任何数据流连接到 pass-info
  `+0x0C/+0x0D`，禁止仅凭名称相近把两者合并解释。

严格22份再次独立复算：

- `bNoUsbChkPasSafe(+0x0A)`：18份为0、4份为1；
- 4份非零均是真实 original reference，且均为 LBA7 version 0x0064；
- 22/22 的 `+0x0A` 在 LBA7/LBA12 两份副本中一致；
- `+0x0C/+0x0D` 仍是22/22全零。

这里的跨版本 Windows 审计最初只用于继续收紧 `+0x0C/+0x0D` 的边界；
`+0x0A` 已在后续独立 `checkdiskback::Update_EDPEDISKSHOWPARAM` + Safe6PolicyFile
链中找到值相关 consumer，并已按主账本升级 COMPLETE，不能再沿用本段较早阶段的
“缺最终策略 consumer”结论。对 `+0x0C/+0x0D` 的后续结论也已更新：它们不是
等待某个必然存在的 active consumer，而是 **dormant compatibility bytes**。

新增证据如下：

- Linux DWARF 直接把 `edpdiskglobal.h:164/165` 的两个成员定义为独立
  `BYTE`：`ShareBackuppromptPeriod@+0x0C` /
  `EncryptBackuppromptPeriod@+0x0D`，排除 padding/bitfield；
- 除 current ydcc 与 out_raw 两版外，又补审
  `VRV/cems/Edp/edpediskctrl.dll` 与
  `VRV/cems/Edp/edpdrivers_win10/EdpEDiskCtrl.dll` 两个不同哈希旧 build。
  四代 reader 的成功路径都把完整14B pass-info structural-copy 到输出；
  Win10 旧 build 可直接看到3×DWORD + 最后1×WORD的完整14B搬运，其中最后
  WORD 就是 `+0x0C/+0x0D`。随后字段处理仍只命中
  `Version(+0)`、Share retry `(+3)`、Encrypt retry `(+6)`；
- Linux checker 保存完整14B结构，但实际解密/文件系统检查路径不读取这两个 BYTE；
- 对两代 `vrvaud_c` 的 `BackupPromptInfo/BackupStartTime/BackupEndTime`
  继续追到机器码，确认它们从 policy 字符串解析到独立 string/DWORD globals，
  与14B pass-info 没有写回/映射数据流；
- 全二进制树 ASCII + UTF-16 精确搜索表明，
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方名称只存在于
  `libcemsfilesyscheck.so` 的 DWARF 类型信息，没有隐藏的第二套实现；
- 除 committed originals 逐盘 LBA7/LBA12 两副本0/0一致外，
  全目录去重扫描19个真实 LBA7 密文 profile（同时覆盖 pass-info
  `v0x0064` 与 `v0x0206`）仍19/19=0/0。

因此严格 COMPLETE 的语义不是“已经猜出 period 的历史设计单位”，而是：
**这两个正式字段在所有已覆盖 producer/consumer 代际中处于 dormant 状态；
current producer 明确写0，reader只结构保留/返回而无值相关语义消费。**
若未来出现非零旧 profile，应 preserve/report 并新增 profile，而不能按 current
实现机械清零或强行解释成小时/天。

对 current producer 的机器码边界又做了一次逐 store 复核：
`CUsbRegsiter::CreatePartitions/sub_1003DB50` 在 `0x1003DC16..0x1003DC26`
对完整14B pass-info 执行显式清零；后续赋值序列最远只在
`0x1003E78A..0x1003E790` 写到 `pass+0x0A`，没有任何 store 命中
`+0x0C/+0x0D`。因此这2B在 current LBA7 v0x0064 与随后仅改 Version 为
v0x0206 的 LBA12 路径中都是**明确 writer-owned zero**，不是“没有观察到赋值”或
未初始化 backing。两代 `vrvaud_c` 的 `BackupPromptInfo` 虽有0/非0行为分支，
但其磁盘 old-table 全局只承接 `0xC0 = 3×0x40` packed entries，pass-info tail
根本不在该全局内；当前也没有其它数据流把该 policy 项接到这2B。故名称相似不能
作为 consumer 证据。

Linux `PartitionHeader::SetPartitionNewPass` 同时给出负证据：

- 新密码只更新 `UserKeyCRC(+0x30)`；
- 新版 0x206 表只更新 `wrapped key(+0x38..+0x47)`；
- 不修改 `+0x48..+0x57`、`+0x58`、`+0x5c..+0x5f`。

因此这些区域不能解释成“密码修改状态缓存”。

#### LBA12 packed Reserved[7]：producer / negative-consumer / real-device 闭合

96B packed entry 的 `+0x59..+0x5F` 本轮从 PARTIAL 升为 COMPLETE，
不是因为“66/66 都是零”，而是四条证据同时闭合：

1. **官方字段定义**
   Linux DWARF 的 `tagNewEdpPartionInfo` 明确把 EncryptMode 后的 7B
   命名为 `Reserved[7]`。104B natural-aligned ABI 中它位于
   `+0x61..+0x67`；96B packed runtime 去掉对齐洞后对应
   `+0x59..+0x5F`。

2. **writer 零来源**
   Windows `CUsbRegsiter::CreatePartitions / sub_1003DB50` 开头执行
   `memset(var_1364, 0, 0x120)`，一次清零完整的
   **3×96B packed entries**。后续逐字段构造只写到
   `+0x58 EncryptMode`，没有对 `+0x59..+0x5F` 的覆盖。

3. **negative consumer**
   Windows `UserLogin` 只从 entry `+0x38` 复制16B wrapped key，
   从 `+0x58` 读取 EncryptMode；改密码路径 `sub_10026050`
   同样只读写 `+0x38..+0x47` 并读取 `+0x58`。
   Linux `CDiskReader::DecryptFileKey` 读取 EncryptMode 后不读取
   `Reserved[7]`；`libedpedisk.so::SetPartitionNewPass` 对 v0x206
   也只更新16B wrapped key。

4. **22盘实测**
   对 22份 original real-device reference set 解密后的全部
   **66条 EDPF entry** 重算：`+0x59..+0x5F` **66/66 全零**，
   无任何非零反例。

这里还顺带纠正一个 ABI 易错点：

- `libedpedisk.so::PartitionHeader` 构造函数明确记录
  `header_size=0x60`，其按值参数是 **96B packed ABI**；
- `libcemsfilesyscheck.so` DWARF 的同名
  `tagNewEdpPartionInfo` 是 **104B natural-aligned ABI**，
  `FileKeyCRC` 前存在4B对齐洞，并含 `EncryptFileKey32[16]`；
- 同名 C 结构在两个 Linux 组件中**不能按偏移直接混用**。

Reserved[7] 闭合后又继续独立追了相邻 packed `+0x48..+0x57`。最终不能把
`EncryptFileKey32[16]` 的正式名字本身当成“仍有隐藏算法”的证据：

- 旧 `tagEdpPartionInfo` 只有72B，`EncryptFileKey@+0x40` 后即结束；
  **old 72-byte ABI has no EncryptFileKey32 slot**；
- natural 104B `tagNewEdpPartionInfo` 才增加 `EncryptFileKey32[16]@+0x50`；
  `CLabelManage::GetPartionFromOld` 将旧72B entry迁移到104B时只复制旧 key 到
  natural `+0x40`，从不填 `+0x50`；
- checker 的 `CDiskReader::DecryptFileKey` 只取 natural `+0x40..+0x4F` 主16B key
  与 `+0x60 EncryptMode`，`CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0`
  同样没有 `+0x50` 值相关读取；
- packed `libedpedisk.so::PartitionHeader` 构造器确实按值缓存完整96B，因此
  packed `+0x48..+0x57` 会落到 object `+0x88/+0x90`。但按全部
  `PartitionHeader` 符号边界重扫，两个QWORD只在 ctor 写入，后续算法方法不读；
  正对照主 wrapped16 的 object `+0x78/+0x80` 中，`+0x78` 被
  SMS4/AES128/OldEdp decrypt 实际取址并按16B消费；
- Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`，后续只写主 wrapped16
  与 mode；Windows UserLogin/改密与 Linux packed mount/update 也都不消费扩展槽；
- 严格22份 original reference 的全部66条 entry 中，该16B **66/66全零**。

因此 packed `+0x48..+0x57` 的当前可观察行为已经闭合为正式命名但无当前算法消费的
**cross-generation compatibility slot**：旧ABI不存在，新ABI保留并可随结构搬运，
current packed producer显式零，运行时仅结构缓存。三个 entry 共48B从 PARTIAL 升
COMPLETE。COMPLETE 不表示所有未来ABI都必须写零；若发现独立非零 profile，应结构保留。

**LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure**
与 Reserved[7] 门禁独立存在，禁止把两者重新合并解释成 padding。

**LBA12 packed Reserved[7] producer/negative-consumer closure**
已加入回退门禁，防止以后再次把 `+0x48` 扩展槽与
`+0x59` reserved 混成一片“全零 padding”。

按上述严格口径，Windows/Linux 主运行时 96B packed LBA12 当前逐字节进度为：

- **完成 512B / 512B（100.0%）**
  - 三个 entry 中语义闭合字段：49B/entry，共 147B；
  - entry0 `NeedDisturb(+0x10)`：4B，旧兼容 consumer + 22/22 原始盘已闭合；
  - 三个 entry 的 `Version(+0x04)`：12B，current-zero producer + packed runtime structural-preserve/negative-semantic-consumer + 22×3实盘闭合；
  - entry1/entry2 `NeedDisturb(+0x10)`：8B，current positional 1/0 producer + cross-platform negative-semantic-consumer + 22盘实测闭合；
  - 三个 entry 的 `EncryptFileKey32[16]` compatibility slot：48B，current零producer + structural-cache/negative-semantic-consumer + 66/66实测闭合；
  - 三个 entry 的 `Reserved[7]`：21B，producer + negative consumer + 66/66 entry 实测闭合；
  - 三个 entry 的 wrapped key `+0x38..+0x47`：48B，mode2由44条real-device entry闭合；mode1/mode3由官方 `CreatePartitions` 隔离动态执行 + 独立算法解包 + FileKeyCRC round-trip 闭合；
  - 表尾已闭合字段：14B（含 `bNoUsbChkPasSafe` 与两个 dormant BackupPromptPeriod BYTE）；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为 post-table zero padding；
- **部分已知 0B / 512B（0%）**
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有未闭合字节。

这组数字只描述**主运行时 96B packed 格式**；不把 `libcemsfilesyscheck.so`
的 104B 扩展结构混入统计。

#### LBA12 wrapped material 的当前拆分

LBA12 entry `+0x30..+0x47`：

- `+0x30..+0x33`：`CRC32_bare(password)`；默认 `"0000aaaa" -> 0x0429735D`。
- `+0x34..+0x37`：`CRC32_bare(file_key)`。
- `+0x38..+0x47`：16B wrapped file-key material。
- 旧 23 份混合集曾得到的 wrapped-key 统计已撤销；本节只使用
  **22 份 original real-device reference set**。

本轮已经把 v0x0206 默认密码分支重新从官方 producer/consumer 和 22盘
独立闭合，形成 **LBA12 v0x0206 hidden default-password file-key wrapping**
证据链。

Windows producer（`CUsbRegsiter::CreatePartitions / sub_1003db50`）：

1. 在构造 packed 96B entry 前生成一份 16B file-key；
2. `entry+0x34 = CRC32_bare(file_key16)`；
3. `entry+0x30 = CRC32_bare(password_before_default_substitution)`；
4. 当 password 恰为 `"0000aaaa"` 时，先调用
   `sub_10040400(password)` 把它替换为隐藏 10B 字符串，再执行后续
   MD5 + file-key 包装；
5. EncryptMode=2 且配置 `GLOBAL/oldSM4 != "1"` 时走
   `sub_100036e0 -> sub_100031a0/sub_10003550`，该实现逐常量/轮函数
   对应标准 SM4（FK/CK、S-box、32轮、key-T' rot13/23、
   round-T rot2/10/18/24）；
6. 16B 包装结果写 `entry+0x38..+0x47`。

`sub_10040400` 的隐藏字符串本轮没有沿用旧脚本，而是从当前 Windows DLL
机器码重新提取：

- 32B seed：
  `468b46088b4e048bd02bd13bd37f2183c1098d3c003bf97f028bf98b065750e8`；
- 96B table 和 16B index 全部逐字节从该函数栈初始化恢复；
- 前16B xor 后16B 得第一轮 A6B0 key：
  `8782cb348b75fdf4d2a028b0d528716b`；
- 前16B wrapping-add 后16B 得第二轮 A6B0 key：
  `0794d3448b89fd0ad2b6cac6d9d6716b`；
- 解出的 index 前10B为：
  `38 17 51 4d 0c 05 26 16 2d 0d`；
- 最终从 decoded table 取值得到：
  **`LtSWi[2f)j`**；
- `MD5("LtSWi[2f)j") = 548b072cba7f104d88a446556cc3c432`。

Linux consumer 独立给出同一设计：

- `CDiskReader::DecryptFileKey @ 0xDF50` 先构造 `"0000aaaa"`；
- pass-info version=`0x0206` 且输入仍为默认密码时，调用
  `GetIniString(char*) @ 0xCCCC`；
- `GetIniString` 自身包含与 Windows 对应的 seed/table/index 混淆逻辑，
  最终覆盖输入前10B；
- `AlgorithmSpace::fileKey_Decrypt @ 0xC7B4` 对 mode2：
  `MD5(effective_password)` 后调用 `MC_KKSMS4::DecryptBuffer`；
- `CDiskReader::CheckFileKeyCrc @ 0xE170` 对解包出的 16B file-key
  重新 `CRC32`，必须等于 entry 保存的 `FileKeyCRC`。

22份原始盘重新解 LBA12 后共有 66 条 packed entry：

- 22 条 boot/type1 或旧 type2 entry 为 mode0、wrapped key 全零；
- 44 条 type2/type4 为 **EncryptMode=2**；
- 其中 43 条 `UserKeyCRC=0x0429735D`，即盘面密码标识仍对应
  原始默认字符串 `"0000aaaa"`；
- 另 1 条（Netac onlyid=3274129259 的 type2）为非默认
  `UserKeyCRC=0x438C9FFC`。

为避免旧分析脚本自证，本轮又使用系统 OpenSSL 3.6.3 的
**标准 SM4-ECB** 独立解 43 条默认 mode2 wrapped material：

- **43/43** 成功恢复 16B file-key；
- **43/43** 满足
  `CRC32_bare(unwrapped_file_key) == entry.FileKeyCRC`；
- 对唯一非默认 type2 的同一实盘，其 type4 仍使用默认密码；
  从 type4 独立恢复共享 file-key
  `eadd58009f9abe0625a1f1f779d4c98b`，
  得到 `CRC32=0xF7EEA980`，与 type2/type4 两条 entry 的
  FileKeyCRC 均精确一致；
- type2/type4 的 wrapped16 不同，证明“同一 file-key +
  不同 effective password 分别包装”的模型，而不是复制同一密文。

因此，对当前 22份原始盘实际使用的
`version=0x0206 + EncryptMode=2 + oldSM4!="1"` profile，
`+0x38..+0x47` 的 producer、consumer、默认密码替换规则和实盘结果
已经完整闭合。

本轮随后继续把其它算法分支追完，形成
**LBA12 alternate wrapping-mode algorithm map**。

##### mode2 的 `oldSM4` 开关不是新的盘面格式

Windows writer 的两个 mode2 实现分别是：

- `oldSM4=="1" -> sub_10011010`；
- 其它 -> `sub_100036e0`。

对 `sub_10011010` 再回到机器码/常量逐项核验：

- S-box @ `0x100C6548` 与标准 SM4 256B S-box 完全一致；
- FK 在内存中以小端 DWORD 保存，解释后仍是
  `A3B1BAC6 / 56AA3350 / 677D9197 / B27022DC`；
- CK 同样以小端 DWORD 保存，解释后从
  `00070E15 / 1C232A31 / 383F464D ...` 开始，完整对应标准32轮 CK；
- `sub_1000FD90` 是32轮 key expansion；
- `sub_10010B80` 是标准 round-T；
- `sub_1000FFA0` 正向使用 round keys；
- `sub_10010540` 反向使用同一 round keys。

因此 `sub_10011010` 与 `sub_100036e0` 都实现
**SM4-ECB(16B, MD5(effective_password))**，只是内部实现不同。
更重要的是 Windows `CEdpDiskControl::UserLogin -> sub_10028AB0`
在 mode2 只有一条标准 SM4 解包路径，而且完全不读取 `GLOBAL/oldSM4`。
所以该配置不能代表不同的 wire format；否则同一 reader 无法同时读取两种盘。
本账本将 `oldSM4` 定性为 **implementation switch, not wire-profile switch**。

##### EncryptMode=1：EDP A7F0/A6B0 16B wrapping

Windows writer：

```text
effective_password
  -> MD5 = 16B
  -> sub_10001190(file_key16, key=MD5)
  -> wrapped16
```

`sub_10001190` 的 key 初始化先执行：

```text
expanded_key_input[i] =
    MD5[i] XOR "EDPSECDISK200709"[i]
```

随后使用项目已独立恢复的 A7F0 正向块算法；16B file-key 只有一个 block，
counter 从0开始。

Windows consumer `sub_10028AB0 case 1`：

- 对输入 password 做 MD5；
- 调用 `sub_100384E0`；
- 其 key 初始化同样 XOR `"EDPSECDISK200709"`；
- 使用与 writer 相反的 A6B0 解包；
- `UserLogin` 随后 CRC32 16B 明文并比较 `FileKeyCRC`。

Linux `AlgorithmSpace::fileKey_Decrypt case 1` 也执行
`MD5(password) -> Decrypt(...)`，与同一 A6B0 family 对应。

因此 mode1 的 producer/consumer 算法已经闭合；缺口只剩当前
22份 original reference set 中**没有任何正向 mode1 packed entry**。

##### EncryptMode=3：标准 AES-128-ECB wrapping + Windows 历史 fallback

Windows writer `sub_1000FC10`：

- key = `MD5(effective_password)`；
- `sub_1000E0A0(key, 0x80, roundkeys)` 是 AES-128 key schedule；
- `sub_1000ECA0` 是标准 AES 正向 block transform；
- 16B file-key 恰好一个 block，因此没有 IV/链模式：
  **AES-128-ECB(file_key16, MD5(effective_password))**。

Windows consumer `sub_10028AB0 case 3`：

- 对 password 做 MD5；
- `sub_1002F670 -> sub_1002E7D0/sub_1002F090` 使用 AES-128
  inverse key schedule / inverse block transform；
- 解出16B后仍统一由 `UserLogin` 做 FileKeyCRC 校验。

而 `UserLogin` 对 mode3 还有明确的历史兼容分支：

1. 先按 entry 标记的 mode3 解包；
2. 若 FileKeyCRC 不匹配，日志输出
   `EncryptMode == eEncryptAESOPENSSL`；
3. 强制以 **mode1** 再解一次同一 wrapped16；
4. 若第二次 CRC 匹配则接受，日志
   `dwKeyCrcOld == m_epiNewInfos[nIndex].FileKeyCRC`。

这说明 mode3 标记历史上可能承载过 mode1-compatible ciphertext；
Windows reader 明确做了容错，而不是靠猜测。

Linux 当前 `libcemsfilesyscheck.so::fileKey_Decrypt` build 只显式实现
mode1/mode2，没有 mode3 分支。这是组件能力差异，不应把 mode3
误写成“Linux 同样支持”。

##### 严格完成状态

本轮继续向上追 `CreatePartitions` 的 EncryptMode 输入，确认 mode1/mode3
不是不可达兼容代码：

- `CUsbRegsiter` 构造函数 `0x10038EB4` 默认令 `this+0x6EC=2`；
- 唯一 setter `sub_1003B4E0(arg)` 直接覆盖该成员；
- `WriteNormalULabel` 读取请求 `arg+0x7E8`：值1→setter(1)，值2→setter(3)，其它→setter(2)；
- `CreatePartitions` 三组 entry writer 都按 `this+0x6EC` 分派 mode2/1/3，
  并把同一个 mode byte 写入 `entry+0x58 EncryptMode`；
- `UsbtoolBusMgrInter::LabelInfo::Print` 把 `LabelInfo+0x7E8`
  明确打印为 `crypt=%d`；
- 制标 UI 的 `tabAlgorithmComboBox` 初始化为 `SMS4/AES/AES_CROSS`
  三项，默认 index=0，`currentIndex()` 直接写
  `normalDetail+0x44`；策略日志把该字节命名为
  `normalDetail.algorithm`。

继续回到 `cemssafeudisklabeltool.exe` 的实际请求转换链后，UI 到底层 `crypt` 的
桥接也已经直接闭合，不再需要靠字段名相似推断：

- `WriteLabel` 从 `tabAlgorithmComboBox::currentIndex()` 直接写 `normalDetail+0x44`；
- `sub_42E8E0(normalDetail, LabelInfo)` 在 `0x42E8E0` 的转换链中明确执行
  `LabelInfo+0x7E8 = normalDetail+0x44`；反向 `sub_42EC90` 又执行
  `normalDetail+0x44 = LabelInfo+0x7E8`，证明这是同一个正式字段的双向映射；
- `QComboBox` 初始化汇编 `0x47780F..0x47788C` 先构造 `AES_CROSS/AES/SMS4`，
  但传给 `QStringList` 时按栈顶顺序实际依次 append `SMS4 -> AES -> AES_CROSS`，
  随后 `setCurrentIndex(0)`；因此 UI index 0/1/2 精确对应
  `crypt=0/1/2`；
- 结合 `WriteNormalULabel` 已确认的分派：`crypt=1 -> mode1`、
  `crypt=2 -> mode3`、其它（含默认0）`-> mode2`，最终得到
  **SMS4(index0) -> EncryptMode=2，AES(index1) -> mode1，
  AES_CROSS(index2) -> mode3**。

因此底层 `crypt`、UI `normalDetail.algorithm`、`LabelInfo+0x7E8` 与 writer
mode1/2/3 的可达性/数值映射现在全部闭合。

22份 original real-device reference set 中：

- 44条需要16B wrapped key 的 type2/type4 entry **全部 EncryptMode=2**；
- mode1 正向样本：**0**；
- mode3 正向样本：**0**。

为排除“旧备份里其实已有正例但因 device-id 缺失而被漏解”的可能，先前已对
`utils/backup` 与 `nopwd_tool/backup` 做过基于文件名/`.meta.json` 身份的复核。
本轮进一步取消这个依赖：对 `/Users/zhangyuxi/Desktop/u_disk` 下所有大小至少13扇区、
不超过1GiB的文件做只读 census，共扫描 **3896** 个候选。每个候选先用固定 LBA6
rolling key 解出 `LBA6+0x100 m_crcUsbID[0]`；这个 DWORD 本身就是
`CRC32(device_id)`，可直接作为 LBA12 A6B0 key，因此即使文件名和 sidecar 完全没有
device-id 也能独立尝试解 LBA12。只把解密后 entry0 magic=`EDPF`、entry count=1..3
且每条0x60-stride entry magic均有效的捕获纳入统计。结果为：

- **58** 份有效 EDPF 捕获；
- 49份 mode tuple=`[0,2,2]`；
- 9份 mode tuple=`[2,2]`；
- **mode1/mode3 命中仍为0**。

这批58份包含历史和转换状态，只用于扩大 physical real-device profile 搜索，不扩大22份
strict-original reference set；它仍然证明截至现有实盘语料，mode1/mode3 没有 physical
capture。随后新增的证据与这项 census 分开记录：

- 使用官方 current `CEMSUsbRegsiter.dll`，在不触碰任何物理 raw device 的 Unicorn
  虚拟盘 harness 中直接调用 `CUsbRegsiter::CreatePartitions/sub_1003DB50`；
- harness 只替换 WinAPI/SEH/配置/挂载后处理等环境边界，并固定 `CoCreateGuid` 作为
  deterministic entropy；**没有 stub** `sub_10001190`、`sub_100036E0`、
  `sub_10011010`、`sub_1000FC10` 或 `BuildSector12/sub_10014F30`；
- `this+0x6EC=1/2/3` 时，最终 entry0 wrapping callsite 分别只命中对应的
  mode1 `0x1003ED77`、mode2 `0x1003ED44`、mode3 `0x1003EDA7` 各1次；三次均
  `CreatePartitions ret=0`、无 emulator crash，且原生 `BuildSector12` 写出完整512B密文；
- 三份输出使用同一 device-id、同一密码 `ProofPass1!`、同一 deterministic GUID stream，
  解开外层 LBA12 后 `UserKeyCRC=0xE5A095A1`、`FileKeyCRC=0xFF4C1D36` 完全相同，
  只有 EncryptMode/wrapped16 按算法变化；
- mode1 以独立 A6B0 reader、mode2 以标准 SM4-ECB、mode3 以标准 AES-128-ECB 解包，
  三者都恢复同一 file-key
  `147196f5a2ec7912edf13f75d766cb42`，其 `CRC32_bare=0xFF4C1D36`；
- 三份512B输出已作为 `official_virtual_writer_mode{1,2,3}_lba12.hex` 固化，CI
  `lba12_official_virtual_writer_executes_mode1_mode2_mode3_wrapping_paths` 独立验证结构、
  三种解包与 FileKeyCRC。

这里没有把 virtual fixture 冒充 physical real-device capture；22份 strict originals 和扩大
census 的统计保持原样。但对“字节含义是否闭合”的标准而言，**first-party executable 自身
动态生成 wire bytes + 已闭合的 UI 可达链 + 独立 consumer round-trip** 已经消除了缺实盘
mode1/mode3 所代表的语义不确定性，而且比 edpcli 自己按逆向公式生成合成向量更强。因此
`+0x38..+0x47` 三条共48B从 PARTIAL 升 **COMPLETE**；`oldSM4` 仍只是 mode2 的
implementation switch，不形成第二 wire profile。

#### LBA9/LBA10 的非零形态

- LBA9 在当前 22 份参考样本中分为：
  - 14 份：EETU + SAPF；
  - 6 份：EETU + EPPE；
  - 2 份：全零。
- 20 份非零 LBA9 的 EETU 解密结果 **20/20 完全一致**：
  - `+0x00..+0x03 = "EETU"`；
  - `+0x04..+0x0B = ullBTime = 0`；
  - `+0x0C..+0x13 = ullETime = 0`；
  - `+0x14..+0x17 = useCount = FF FF FF FF`；
  - `+0x18..+0x7F reverse[104] = 0`。
- 时间窗/次数20B已经重新由官方 producer/consumer 闭合：
  - Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 给出正式字段名；
  - Windows `CUsbRegsiter::SetTempUse` 真实机器码将开始/结束时间字符串解析为
    两个64位时间值，并把请求 `+0x40` 原样写入 `useCount`；
  - `BusManageImp::WriteNormalULabel` 的机器码在特殊 OutManage-off 模式明确把
    临时使用请求次数写为 `0xFFFFFFFF`，普通模式则从业务请求 `+0x947` 取值；
  - Linux `CheckTempUse` 将 `useCount=0xFFFFFFFF` 当作无限次数哨兵：
    不递减、不回写；0表示次数耗尽；其它正值减1并回写；
  - `ullBTime/ullETime` 与 `time(NULL)` 比较，0表示对应时间边界不启用。
- `reverse[104]` 本轮继续拆到 writer 覆盖边界，并把前102B的真实 backing 来源追到上层调用栈：
  - `CUsbRegsiter::SetTempUse` 先对 EETU magic 后的 `0x7C` 字节整体
    `memset(0)`；
  - 随后只执行 `memcpy(EETU+0x18, request+0x44, 0x66)`，即覆盖
    reverse 前102B / LBA9 `+0x18..+0x7D`；
  - reverse 最后2B / LBA9 `+0x7E..+0x7F` 没有任何后续覆盖，因此保留
    明确的 writer 零初始化值；
  - 对 `BusManageImp::WriteNormalULabel/sub_100A99F0` 的真实机器码重新扫描：
    调用 `SetTempUse(&var_BD4)` 前只写 begin/end 字符串和
    `useCount@var_B94`；请求布局因此精确为 `begin[32] / end[32] /
    useCount@+0x40 / caller backing@+0x44`；
  - 该函数之前确实调用 `sub_1009BAD0()`，但机器码明确先执行
    `lea ecx,[ebp-0x9EC]`，而 `sub_1009BAD0` 只做 `memset(ecx,0,0x996)`；
    其清零范围为 `ebp-0x9EC..-0x56`，**不包含**位于 `ebp-0xBD4` 的
    SetTempUse 请求。因此 `request+0x44..+0xA9` 102B 不是遗漏的 reserved-zero，
    而是正式的 **writer-uninitialized opaque backing**；
  - Windows runtime `ReadTempUseInfo/sub_10013490` 会把完整0x80 EETU解密并缓存到
    `CEdpDiskControl+0x1076`；对该对象区的精确引用审计显示，
    `GetTempUseInfo/CheckTmpUse` 只读取 `ullBTime/ullETime/useCount`，显式字段引用
    截止 `+0x108A`，而 reverse 从 `+0x108E` 开始没有独立业务读点；
  - 登录成功需要扣减临时使用次数时，`UserLogin` 只执行
    `*(this+0x108A)--`，随后 `WriteTempUseInfo/sub_10013770` 将完整0x80 EETU重新
    加密写回，因此 **runtime preserves the full 0x80 EETU while only consuming
    time/useCount**；Linux `CheckTempUse` 也不读取 reverse；
  - committed originals 与扩展历史只读扫描当前仍未发现非零 reverse：严格非零
    LBA9 profile 的 EETU 均为 `reverse[104]=0`。这里的零只作为真实 profile 证据，
    不再被误写为协议固定值。

因此本轮把 **LBA9 +0x18..+0x7D 共102B** 从 PARTIAL 升 COMPLETE：正式
`reverse[104]` 边界、writer-uninitialized producer、runtime transparent-preserve /
negative semantic consumer 与原始实盘四项均已闭合；未来出现非零 backing 必须原样保留。
此前 `+0x7E..+0x7F` 2B 已由 SetTempUse 显式零初始化独立闭合，所以现在整个
`reverse[104]` 都是 COMPLETE，但前102B与末2B的 producer 语义不同，禁止重新合并为
“104B zero padding”。
- `EPPE` 位于 `0x180..0x1ff`，是独立 128B A6B0 区，counter 从 0 重新开始；当前 6/6 解密为 `EPPE 08 00 00 00` 后零填充。
- Windows `cemsusbregsiter.dll::SetPassInfoEx` 对输入 `+0x04` 明确限制为 6..19，
  构造 `EPPE` 后只覆盖 LBA9 `+0x180..+0x1ff`；Windows
  `edpediskctrl.dll::ReadPassExInfo` 与 `modfilesyscheck.dll::ReadMinPassLenInfo`
  均独立读取同一个 0x80B A6B0 区并检查 `EPPE`。后者把 `+0x04` 返回给调用者，
  因此该 DWORD 可闭合为**最小密码长度**；当前真实样本 6/6 均为 8。

本轮回到 PE 机器码补上反编译文本漏掉的一条关键赋值，并重新划分 EPPE 尾区：

```text
1003AF3B  mov eax,[arg0]        ; caller minPassLen
1003AF43  cmp eax,6
1003AF4C  cmp eax,0x13         ; 19
...
1003AF86  memset(EPPE+0x04,0,0x7C)
1003AF99  EPPE+0x00 = "EPPE"
1003AFA3  eax = [arg0]
1003AFA8  EPPE+0x04 = eax
```

因此 current producer 对 `EPPE+0x08..+0x7F` 的120B不是“碰巧为零”，
而是**显式 writer-zero tail**：

- 先把 magic 后的124B全部清零；
- 再只覆盖 `+0x04` 的最小密码长度；
- 内层 `sub_10042720` 对完整0x80B加密，并只替换 LBA9
  `+0x180..+0x1FF`。

严格22份原始参考中共有6份 EPPE，重新逐盘复算：

- 6/6 `minPassLen = 8`；
- 6/6 解密后 `+0x08..+0x7F == zero[120]`。

新增 CI 门禁：

- `real_eppe_samples_keep_the_current_writer_zero_tail`。

继续追 current 接口边界后，consumer 侧已经闭合到“只有 minPassLen 有语义”：

- 正式注册 API `CUsbRegsiter::GetPassInfoEx` 读取/解密完整0x80B、校验
  `EPPE` 后，**只执行 `*out = *(decoded+0x04)`**；tail 不向调用者暴露；
- 独立 `modfilesyscheck::ReadMinPassLenInfo` 同样只消费 magic 与 `+0x04`；
- 两套独立 `EdpDiskCtrl` 确实都保留了可搬运完整0x80B的
  `ReadPassExInfo -> CEdpDiskControl::GetPassExInfo` compatibility helper，
  但 DLL 对外只导出 `CreateEdpEDiskCtrlIntObj/ReleaseEdpEDiskCtrlIntObj`；
  current factory 返回对象的 vtable `0x1008021c` 只有15个方法，未包含
  `GetPassExInfo/sub_10022AF0`。两套反编译文本中该 helper 也只出现实现和
  相邻 thunk，没有形成 current 产品的值相关 tail consumer。

因此这里不是“因为没搜到字段名就猜 reserved”，而是已经具备明确 current
writer-owned zero producer + 两套 semantic reader negative consumer + current
public-interface boundary + 原始实盘四项证据。

结论：

- `LBA9 +0x188..+0x1FF` 120B：PARTIAL -> **COMPLETE**；
- COMPLETE 的含义是 **EPPE writer-owned zero tail** 在当前已知代际的存储/消费
  行为已经闭合，不表示未来历史 profile 必须为零；
- 兼容读取若遇到未知非零 tail，应保留/报告，不得以本结论为由主动清零。
- SAPF 位于 `+0x100..+0x11f`，整段按字节 `^0x88` 还原。其
  `+0x04..+0x13` 是一个完整 16B MBR partition entry，但**不是当前 LBA0 分区项的镜像**：
  当前 14 份 SAPF 样本中 14/14 均与当时 LBA0 `0x1be..0x1cd` 不同。
- `UDiskLabelRepair.dll::CLabelRepair::Repair` 在 LBA0 无效时先检查 LBA9 SAPF；
  `Repair0Sector(from sector 9)` 会把 SAPF 的四个 DWORD 直接写到新 MBR
  `0x1be/0x1c2/0x1c6/0x1ca`，随后写回 sector 0；若该路径失败才尝试尾部备份扇区。
  因此 SAPF 可闭合为 **LBA0 第一分区项的恢复模板/备份项**，不能再描述成“当前 MBR 副本”。

本轮继续把 SAPF 与 LBA9 中间区域按**真实物理边界**拆开，而不是把
`+0x080..+0x17F` 继续整体记 UNKNOWN。

##### current 注册/runtime 对 LBA9 中间256B的写边界：BuildSector6 有跨扇区 side effect

current `CUsbRegsiter::RegsiterUsb/sub_1003B560` 的机器码/反编译控制流明确：

1. 先从 metadata base 一次读取完整13扇 `LBA0..LBA12` 到工作缓冲；
2. 注册过程中显式调用 LBA4、LBA6、LBA8、LBA11 builder；
3. `sub_1003DB50` 只向 `sector_size*7` 与 `sector_size*12` 写入，
   即只重建 LBA7/LBA12；
4. `BakupUsbSec/sub_10040940` 只是把现有缓冲复制到尾部备份位置，
   不修改工作缓冲；
5. 最后把完整13扇工作缓冲写回。

此前据此写成“current 注册路径不拥有 LBA9”是不完整的。重新下钻
`BuildSector6` 本体后确认它会跨过自己的512B输出范围写 LBA9：

- Dept `strlen>=64`：
  - LBA6+0x000..0x03F 写 `0x40245E2A + Dept前60B`；
  - `out + 3*sector_size + 0x80` 即 LBA9+0x80 写
    `Dept[60..NUL]`，长度为 `strlen-59`，包含结尾NUL；
- User `strlen>=32`：
  - LBA6+0x050..0x06F 写 `0x40245E2A + User前28B`；
  - LBA9+0x100 写 `User[28..NUL]`，长度为 `strlen-27`，
    同样包含NUL。

Windows `sub_10013FD0` 机器码、Linux `BuildSector6@0x1CAAC` 和
`vrvaud_c::sub_10118ED0` 三套实现相互独立地给出同一布局。
因此 LBA9 中间256B不是纯 preserve 区，而是长 Dept/User continuation 与
SAPF/历史 backing 复用的多profile物理区。
运行时两个独立 setter 又进一步把所有权边界锁死：

- `SetTempUse`：read-modify-write LBA9，但只替换 `+0x000..+0x07F`；
- `SetPassInfoEx`：read-modify-write LBA9，但只替换 `+0x180..+0x1FF`。

对应 getter 也分别只读取首/尾0x80；这只能说明 TempUse/PassEx 运行时 API
不会触碰中间区，不能否定 BuildSector6 的跨扇区 continuation。

##### `+0x080..+0x0FF`：正式 long-Dept continuation，含 join=60 / join=59 双profile

严格参考中有8盘该128B非零，实际每盘只有16或17B非零。尝试以 device-id CRC
把它作为独立0x80 A6B0块解密，结果无任何可识别 magic/结构；而原始字节直接呈现
GBK文本特征。

最关键的跨扇区交叉：

- 4份具有完整76B ELABEL `Dept=` 的真实盘，
  LBA9 `+0x80` 的16B **逐字节等于 ELABEL Dept 的 `dept[60:]`**；
- 另4份 ELABEL Dept 在63B位置截断，并停在 GBK“建”的首字节 `BD`；
  LBA9 `+0x80` 以 `A8` 开头，随后正好是
  `湖输变电运检中心`，与同一完整部门字符串的后续字节吻合；
- 重新定位官方 BuildSector6/ReadSector6 后，这里已经不是“像 Dept backing”的推测：
  current producer 精确写 `Dept[60..NUL]` 到 LBA9+0x80；
- Linux/Windows/cemsudisk reader 都有相同兼容逻辑：
  - inline 第60字节非零：把 continuation 接到 Dept index60；
  - inline 第60字节为0：把 continuation 接到 Dept index59，
    专门修复历史 profile 的 off-by-one/GBK split。

继续追 reader 代际后，join59/60 的选择条件已经从“兼容分支”细化到机器码级。current
`CEMSUsbRegsiter.dll::ReadSector6/sub_100152A0@0x1001566F..0x100156D6` 的实际流程是：

1. marker 命中后，把 marker 后的 **0x3C=60B** 复制到局部 prefix；
2. 读取 `prefix[0x3B]`，也就是第60个 inline byte / Dept[59]；
3. 若该字节为0，LBA9+0x80 的0x80B continuation 写回 `Dept+0x3B`；
4. 若非0，则写回 `Dept+0x3C`。

所以这里没有隐藏 version/profile flag：wire image 自己用 inline 第60B是否为 NUL 决定接缝。

更重要的是，本机 `VRV/cems/Edp/fileophook.dll` 与 `fileophook64.dll` 给出了更早一代
reader。两者 PDB 分别落在
`\\SVNRoot\\vrvrsms2.0\\Cems2.0\\trunk\\modCems\\Bin\\FileOpHook.pdb`
和 `FileOpHook64.pdb`。旧32/64位机器码在 marker 分支中都先复制60B prefix，
随后**不做 prefix[59] 分支，直接把 continuation 写到 prefix-base+0x3B**，即固定
join=59。公开的2019 CEMS样本文件清单还把**同一产品目录路径**的
`cems\\edp\\fileophook*.dll` 与
`cems\\edp\\safeudisklabeltool\\cemsusbregsiter.dll` 放在同一软件包里；这只能作为
CEMS2.0/EDP 产品线共包证据，**不能**证明该2019归档中的 hook 与本机2022编译的
`FileVersion=1.0.0.11` 二进制哈希相同。

本轮继续排除了“这两份 reader DLL 自己还藏着 join59 raw writer”的可能路径：

- x86 `fileophook.dll` SHA-256 为
  `db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65`，PE 编译时间
  `2022-12-13 03:40:57`，fixed file version 为 `8.1.2211.2811`、字符串
  `FileVersion=1.0.0.11`。marker `0x40245E2A` 在该二进制只有一个命中，正是
  `fcn.10022F80@0x100232ED` 的 `cmp`。唯一有实际代码 xref 的
  `\\.\\PhysicalDrive%u` 路径进入 `fcn.10026280`，该函数在 `0x1002631B`
  以 `dwDesiredAccess=0x80000000 (GENERIC_READ)` 打开物理盘，随后只执行
  `SetFilePointer + ReadFile`。模块虽导入 `WriteFile`，但这些调用点没有连接到这一唯一
  live PhysicalDrive raw path，不能据此推导出 sector writer。
- x64 `fileophook64.dll` SHA-256 为
  `93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9`，PE 编译时间
  `2022-12-13 02:44:33`，版本同样为 `8.1.2211.2811 / 1.0.0.11`。该架构也只有一个
  marker 命中：`fcn.180026D70@0x18002724F` 的 `cmp`；其固定 join59 分支在
  `0x18002726C` 复制 `0x3C` bytes，再于 `0x180027277` 把 continuation 写到
  prefix `+0x3B`。唯一 `\\.\\PhysicalDrive%u` xref 则进入 `fcn.18002B880`，
  `0x18002B8FE` 同样使用 `GENERIC_READ`，最终只调用 `ReadFile`。
- 对整个 `/Users/zhangyuxi/Desktop` 与 `/private/tmp` 的 PE 文件按 marker 原始字节
  `2A 5E 24 40` 扫描，本轮只命中11个已知家族文件；额外出现的 2024
  `EdpEDiskCtrl.dll` 两个命中也都是兼容 reader 的 `cmp`，没有出现新的历史 marker writer。
  `VRV.zip` 中的 Edp FileOpHook 仍是同一 2022 尺寸/代际，ydcc BusManage/CEMSUsbRegsiter
  则是 current 2026 树，因此归档也未补出 earlier writer。

因此 legacy join59 现在可以严格描述为 **CEMS2.0 旧代 canonical reader ABI**，
而 current ydcc 通过 inline-NUL 自描述同时兼容59/60。仍缺的是同代
`safeudisklabeltool\\cemsusbregsiter.dll` 的原始 writer 字节；没有 writer 之前不能把
LBA6+0x3F 或 LBA9 continuation 误升 COMPLETE。

严格22份完整 census：

- 14/22 未触发 long-Dept marker；
- 8/22 触发 marker 且 LBA9+0x80 非零；
- 其中4/8为 current join=60：重建76B Dept，continuation含NUL共17B；
- 另4/8为 legacy join=59：LBA6 inline 在 GBK lead `BD` 后出现NUL，
  LBA9从 trail `A8` 开始；官方 reader 从 index59 覆盖后同样重建出
  完全相同的76B合法GBK Dept，continuation含NUL共18B。

CI 新增
`lba9_dept_continuation_preserves_both_official_reader_join_profiles`，
并提交一份严格原始 Lexar join59 的 LBA6/LBA9 最小证据夹具。

但严格 COMPLETE 仍差最后一环：已定位的 current Windows/Linux/vrvaud
三套 BuildSector6 都只生成 join=60，尚未找到4份 join=59 原盘对应的历史
producer。因此该128B继续 **PARTIAL**；blocker 已缩小为
“仅 legacy join59 producer 未定位”。

##### SAPF `+0x114..+0x11F`：32B decode 范围内的 profile-dependent backing

`UDiskLabelRepair::sub_10008550` 固定取 LBA9 `+0x100` 起 **0x20B**，
逐字节 `^0x88` 后再检查 `"SAPF"`。所以 SAPF 的物理解码范围明确是
`+0x100..+0x11F`，不是只到已闭合的 magic+16B恢复项。

14份真实 SAPF 对最后12B `+0x114..+0x11F` 重新解码后至少出现5种形态：

- 有全零 profile；
- 也有 `0xFFFFFFFE`；
- 多组 `0x77xxxxxx` 一类典型32位进程/栈 backing 值；
- 同一硬件/profile 可重复稳定出现同一组尾值。

这些12B不匹配同盘 LBA0 disk signature，也不匹配当前 MBR partition entry。
本轮继续把 repair consumer 精确到字段级：`sub_10008550` 会把整个32B SAPF
解码并返回；其上层双副本一致性检查只比较 decoded `+0x04/+0x08/+0x0C/+0x10`
对应的 MBR boot flag、partition type、start LBA、size，完全不比较 `+0x14..+0x1F`；
真正重建 LBA0 的 `sub_10008620` 也只把 SAPF `+0x04..+0x13` 这16B partition entry
写回 MBR `+0x1BE`。`vrvaud_c` 的两条快速路径更只解码/检查 `SAPF` magic 4B。
因此这12B已有明确的 **structural-copy / negative-semantic-consumer** 证据。继续取得
long-User first-party runtime 正例后，物理重叠的另一种 profile 也闭合：最大155B User 会由
official `BuildSector6` 把 continuation 连续写满 `+0x100..+0x17F`，official
`ReadSector6` 又完整重组原 User。也就是说 `+0x114..+0x11F` 的两种已知 profile 都已有
完整语义：

- SAPF profile：**unowned trailing backing**。真实值至少5种，repair/一致性代码不消费；
- long-User profile：**active continuation payload**。first-party writer→wire→reader 已闭合。

因此历史 SAPF 最初 producer 不再是这12B的业务语义 blocker；该区从 PARTIAL 升
**COMPLETE**。COMPLETE 绝不表示 SAPF tail 应归零，反而要求兼容实现保留任意 backing，
并按 profile 区分 long-User continuation。

##### SAPF 后 `+0x120..+0x17F`：long-User continuation / preserve 复用区

- SAPF reader只解码到 `+0x11F`；
- `vrvaud_c` 的快速检查只取 `+0x100` 的4B magic；
- `SetTempUse` / `SetPassInfoEx` 不会覆盖 `+0x120..+0x17F`；
- 但 BuildSector6 在 User>=32 时会从 LBA9+0x100 写
  `User[28..NUL]`，最大可以延伸覆盖整个 `+0x120..+0x17F`；
- 对应 ReadSector6 的 User marker 分支会从 LBA9+0x100 读完整0x80B，
  并支持 join=28 / legacy join=27 两种接缝；
- 14/14 SAPF真实盘及独立SanDisk当前都为零。

严格22盘没有 User>=32 的 physical 正向样本这一事实继续保留，但随后已用隔离虚拟盘直接
执行 official Windows `BuildSector6`/`ReadSector6` 获得最大155B User 的 first-party
runtime positive-wire round-trip；写边界精确止于 `+0x17F`。因此这96B已在后续审计中升
**COMPLETE**，不能再描述成“缺正向证据”。

新增门禁：

- `lba9_middle_profile_material_must_not_be_canonicalized_to_zero`：
  - committed真实夹具必须继续保留 `+0x80` 的非零 profile；
  - SAPF `+0x114..+0x11F` 必须同时保留 zero 与 nonzero 两类真实反例；
  - committed SAPF 夹具当前 `+0x120..+0x17F` 仍锁定为零观察，
    但文档明确零不是协议要求。

该阶段之后 LBA9 又继续闭合 long-User 与 SAPF trailing；最终以文末严格总表为准，当前为：

```text
384 COMPLETE / 128 PARTIAL / 0 UNKNOWN
```

其中 EETU reverse backing 已新增102B COMPLETE，随后 EPPE writer-owned zero tail
又新增120B COMPLETE；当前128B PARTIAL 集中在 Dept join59/join60 continuation。

> **附录口径警告：** 以下 LBA10 EESI 段落记录旧研究集合，旧集合曾把第三来源
> SanDisk EESI 作为正例。2026-09-21 已按第1.1节重新冻结 current gold；现行20份唯一
> 金标 LBA10 全零，因此前0x80B current strict 状态为 PARTIAL。下方静态代码路径和
> 历史 EESI 解码仍是有效背景证据，但其中任何“升级 COMPLETE”的阶段性文字均已
> 被第3节主账本覆盖。

##### LBA10 EESI：前 0x80B 的读写边界已闭合

Windows `edpediskctrl.dll` 同时给出读端和写端：

- `GetEdpEdiskSetInfo -> sub_1000f930`：定位 LBA10，读取整扇，但只对前 `0x80` 执行 A6B0 解密并检查 `0x49534545 == "EESI"`；成功后也只向调用者返回这 `0x80`。
- `SetEdpEdiskSetInfo -> sub_1000fc70`：强制写入 `EESI` magic，只加密输入结构前 `0x80`；随后先读取原扇区，只替换前 `0x80`，再把整 512B 写回。
- 因此 `0x80..0x1ff` **不是 EESI 自身的 padding**。当前 writer 明确保留这 384B 原字节；SanDisk 实盘该区恰好全零只能作为样本事实，不能推导协议恒零。

当前严格22份生成参考中的 EESI 实盘解密结果：

- `+0x00..0x03 = EESI`；
- `+0x04..0x07 = 1`；reader 和两套 `EdpEDisk.exe::OnInitDialog` 外部 caller
  都会先把该 DWORD 默认设为1，但卷标设置 `IDOK` producer 会把完整0x80B
  EESI清零后只写两个卷标，因此保存路径明确把该DWORD写成0；其值相关 consumer
  已追到 `UsbSuspensionWnd.dll` 生命周期控制，详见下节；
- `+0x08..0x17`：16B **Share/type2 卷标**；reader 默认字符串与实盘均为
  GBK“交换区”。`UserLogin` 把该槽赋给本地 `std::string`，在 type2 分支
  直接将其 `c_str()` 传给 `SetVolumeLabelA`；
- `+0x18..0x27`：16B **Encrypt/type4 卷标**；reader 默认字符串与实盘均为
  GBK“保密区”。`UserLogin` 在 type4 分支同样将该槽对应字符串传给
  `SetVolumeLabelA`；
- `+0x28..0x7f`：当前 SanDisk 实盘为零。虽然尚未发现字段消费者和正式字段名，
  但它们已经不能继续标 UNKNOWN：Get/Set 两端都把完整0x80B结构 round-trip，
  所以这88B明确属于 EESI API payload，只是业务语义未解释。

本轮又补到一份独立历史实盘 profile：

- `/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`，
  metadata 记录 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`，整份6656B
  SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`；
- LBA6 解码后的 `crcUsbID[0]=0x5088EE37` 与 `CRC32(device_id)` 精确一致，
  doubled guard、LBA7 packed EDPF、LBA12 packed EDPF 也都在同一 key 下自洽；
- LBA10 前0x80按 `CRC32(device_id)=0x5088EE37` 解密后再次得到
  `EESI`, `+0x04=1`, `+0x08="交换区"`, `+0x18="保密区"`，
  `+0x28..0x7F` 仍为88B全零；
- 回归门禁 `historical_netac_lba10_confirms_the_same_eesi_head_and_zero_uninterpreted_payload`
  固化其前0x80密文与解密结果。

这份 Netac 捕获明显不是 current canonical builder 的简单零模板（LBA6 含真实
legacy/backing profile），因此可作为额外真实设备/profile 佐证；但其“原始生成来源”
尚未达到主22份参考同等审计强度，所以本轮**不**把它并入严格生成参考计数，也不因为
两份 EESI 的88B都为零就把 `+0x28..0x7F` 升 COMPLETE。

本轮继续专门追 `+0x04`，先排除了一个很自然但错误的解释，再找到了真实行为链：

- current `CEdpDiskControl::UserLogin/sub_10022F50` 在栈上建立 EESI 输出结构，
  结构基址为 `ebp-0x334`，随后调用 `GetEdpEdiskSetInfo(&var_334)`；
- 因此 `+0x04` 精确对应 `ebp-0x330`，`+0x08` 对应 `var_32C`，
  `+0x18` 对应 `var_31C`；
- 登录函数后续明确读取 `var_32C/var_31C`，并把它们送入 type2/type4
  `SetVolumeLabelA` 路径，但**整个 UserLogin 没有任何 `ebp-0x330` 引用**；
- 所以 `+0x04` 不控制当前登录路径中“是否使用自定义交换区/保密区卷标”；
- `SetEdpEdiskSetInfo/sub_10022920 -> sub_1000FC70` 只是把调用者完整
  0x80 结构写入，除强制 magic=`EESI` 外不解释/改写 `+0x04`；
- 此前“接口 vtable slot8/slot9 没有外部 caller”的结论经继续反查后已经推翻。
  `out_raw_data/EdpEDisk.exe` 的 `CEdpDiskDlg::OnInitDialog` 明确在对象
  `+0x534` 建立0x80B EESI缓冲：先整块清零，写 `object+0x538=1`
  （即 EESI `+0x04=1`），构造 `+0x08/+0x18` 两个默认卷标，再通过
  `(*(**dword_4832bc + 0x20))(object+0x534)` 调用外部 getter；
- 同一程序的卷标设置对话框 `IDOK` handler `sub_4132B0` 则
  `memset(&var_9C,0,0x80)`，只填写 `var_94=+0x08` 与 `var_84=+0x18`，
  最后由 `sub_413280 -> vtable+0x24` 调 `SetEdpEdiskSetInfo`。因此这条官方
  producer 确定写出 `+0x04=0`；
- 独立 `VRV/cems/ydcc/edpedisk.exe` 完整复现同一模式：
  `CEdpDiskDlg::OnInitDialog` 写 `+0x04=1` 后 vtable+0x20 Get，设置对话框
  `sub_4152C0` 清零0x80后只填两卷标，`sub_415290 -> vtable+0x24` Set；
- 两套程序 SHA-256 分别为
  `cfa1317775801381b6ca51f13857d1e52506ff48ac4df94d7b9f91f742d3e4a1` 与
  `dc71c30041c4fe9fab277737116216502e9f6a610630a1a70b125c59441e1fd1`，
  不是同一文件副本；
- consumer 侧，两套程序均 `LoadLibraryA("UsbSuspensionWnd.dll")`，并解析
  `Show/Destroy/SetParentWnd`。读取 EESI 后，`+0x04==0` 分支调用该 helper 的
  `Destroy`；自动登录成功后，`+0x04!=0` 且 helper 初始化标志为真时进入
  刷新/`Show` 链。因此这是明确的值相关行为 consumer；
- 较旧 `/VRV/edp/EdpEDiskCtrl.dll` 与 SHA 不同的中间版本
  `/VRV/cems/Edp/edpediskctrl.dll` 都没有 `0x49534545(EESI)` 读写路径，
  说明该设置结构属于后续新增功能，不能借旧版行为反推 `+0x04`。

因此 `EESI+0x04` 的严格闭环已经成立：4B边界确定；官方 producer 同时存在
默认/启动路径写1与设置保存路径写0；官方 consumer 对0/非0执行不同的
`UsbSuspensionWnd` 生命周期行为；唯一启用 EESI 的原始 SanDisk 实盘值为1。
该DWORD由 PARTIAL 升为 **COMPLETE**，文档采用保守的行为命名
**UsbSuspensionWnd lifecycle/control flag**，不臆造未恢复的原始 C++ 成员名。

本轮进一步把 LBA10 后续区域从“未知”拆成两个不同的 PARTIAL 边界。

##### `+0x28..0x7F`：EESI caller-owned compatibility extension

current `edpediskctrl.dll` 与独立另一版
`/Users/zhangyuxi/Desktop/u_disk/out_raw_data/EdpEDiskCtrl.dll` 都满足：

```text
GetEdpEdiskSetInfo:
    read full LBA10
    decrypt exactly first 0x80
    verify "EESI"
    copy full 0x80 to caller

SetEdpEdiskSetInfo:
    force input.magic = "EESI"
    encrypt full input[0x00..0x7F]
    read existing full LBA10
    replace only existing[0x00..0x7F]
    write full LBA10 back
```

因此 `+0x28..0x7F` 的88B已有确定存储边界和双向 API 行为：
setter 可以保存调用者提供的这些字节，getter 会原样返回解密后的这些字节。
current `UserLogin` 对本地 EESI 输出结构只读取 `+0x08/+0x18` 两个卷标，
没有读取这88B。两套独立 `EdpEDisk.exe::OnInitDialog` 外部 getter caller 同样
只消费两个卷标；两套卷标设置 `IDOK` handler zero-initializes the full 0x80B EESI payload，
只填写两个卷标再经 vtable+0x24 Set，因此 current UI producer 对这88B的来源为零。

继续把调用面按“谁拥有这88B”而不是“必须猜出88B内部字段名”重新审计后，
这一区域可以严格闭合：

- 两个独立 EESI `EdpEDiskCtrl` build 都把 `+0x28..0x7F` 与其它
  EESI 字节一起完整 Get/Set，底层不生成、不解释、只做 structural round-trip；
- 两套独立 `EdpEDisk.exe` official caller 都在保存前把完整0x80B清零，
  只填写 `+0x08/+0x18` 两个卷标，因此 current official caller-owned
  profile 对这88B明确写零；
- current `UserLogin`、两套 `OnInitDialog` getter caller 以及已扫
  `UsbSuspensionWnd` 行为链均没有对这88B做值相关读取；
- 更老两个独立 `EdpEDiskCtrl` build 根本没有 EESI Get/Set，说明不能把
  88B误解释成继承自旧协议的隐藏活动字段；
- strict SanDisk EESI 与独立历史 Netac EESI 两个正向实盘 profile 的88B均为零，
  且已有回归锁定。

因此这88B从 PARTIAL 升为 **COMPLETE**，语义命名为
**EESI caller-owned compatibility extension**。这里的 COMPLETE 表示生命周期已经
闭合：API caller 可携带任意未来扩展字节，底层必须 round-trip；当前 official caller
写零且当前业务 consumer 不解释。它**不**表示未来值必须为零，也不把未知内部结构
冒充成 `Reserved[88]`。

##### `+0x80..0x1FF`：不属于 EESI 的 opaque preserved physical tail

两版 setter 都明确采用 read-modify-write：

- 先读取完整512B LBA10；
- 只覆盖前0x80B EESI密文；
- 后0x180B原样保留；
- 再写回完整扇区。

两版 getter 也都只解密/返回前0x80B，从不暴露后384B。
所以后384B不是“EESI padding”，而是 **EESI writer 不拥有、
只负责 preserve-existing 的共存物理尾区**。

继续做跨代代码 ownership 审计后，结论可以再提高一级：

- 两个独立 EESI build（`ydcc/edpediskctrl.dll` 与
  `out_raw_data/EdpEDiskCtrl.dll`）setter 都采用完全相同的
  read-modify-write：读取完整0x200B，只替换前0x80B，再原样写回后0x180B；
- 两个 getter 都只解密/返回前0x80B；
- 更老 `VRV/edp/EdpEDiskCtrl.dll` 与
  `VRV/cems/Edp/edpediskctrl.dll` 两个独立 build 连
  `EESI` magic / Get / Set 路径都不存在，因此同样没有该 tail 的 producer
  或 consumer ownership；
- 当前收集到的其它产品组件没有找到 LBA10 tail 的独立解析入口。

真实盘交叉验证也从22份扩展了一层：除 committed originals 与独立 SanDisk 外，
对本机历史语料用 **LBA6 crcUsbID guard + LBA12 解密后 EDPF magic** 双重过滤，
得到58份有效 EDP 前部快照；58/58 的 `LBA10+0x80..0x1FF` 均为零，
其中2份独立 EESI 正例也都是 tail384B 全零。

这里的 COMPLETE **绝不表示协议要求384B恒零**。恰恰相反，官方 setter 的
preserve-existing 行为说明：若未来遇到非零历史/共存 profile，兼容实现必须
原样保留，不能清零。

因此 LBA10 的严格账本从：

```text
36 COMPLETE / 4 PARTIAL / 472 UNKNOWN
```

先调整为：

```text
36 COMPLETE / 476 PARTIAL / 0 UNKNOWN
```

随后跨代 ownership + getter negative-consumer + 扩展实盘验证闭合后，
`+0x80..0x1FF` 384B 再从 PARTIAL 升为 **COMPLETE**。因此当前 LBA10 为：

```text
424 COMPLETE / 88 PARTIAL / 0 UNKNOWN
```

剩余88B只有 `+0x28..0x7F`；`+0x04..0x07` 已由两套独立官方 UI 的0/1
producer、`UsbSuspensionWnd` 值相关 consumer 与原始 SanDisk 实盘闭合为 COMPLETE。
后88B虽可完整 round-trip，且 current UI producer 明确写零，但仍缺字段划分、
非零 profile 与值相关 consumer。

`UserLogin` 的实际汇编还明确给出对象映射：`+0x08 -> ebp-0x74` 的
`std::string`，`+0x18 -> ebp-0x54` 的 `std::string`；type2/type4
分支分别以这两个对象调用 `SetVolumeLabelA`。另一版
`out_raw_data/EdpEDiskCtrl.dll` 也存在同构路径。因此两个16B字段的最终运行时
用途已经闭合为交换区/保密区卷标，不再只是“文本槽候选”。

#### LBA0 legacy MBR message-pointer bytes：+0x1B5..+0x1B7 闭合

本轮把原先笼统归为“446B bootstrap”的尾部重新逐字节拆开。
官方 Windows `UsbMainBSec @ 0x100E7220` 在：

```text
+0x1B5 = 0x2C
+0x1B6 = 0x44
+0x1B7 = 0x63
```

这三字节不是普通保留值，而是 legacy MBR bootstrap 的三个消息指针低字节。
消费链可直接由同一官方模板反汇编闭合：

1. 模板开头把源 `0x7C1B` 起的 `0x1E5` 字节复制到 `0x061B`，
   然后 `retf` 到复制后的代码执行；
2. 因此原扇区偏移 `X` 在运行时映射为绝对地址 `0x0600 + X`；
3. copied bootstrap 中三处：

```text
runtime 0x063A: mov al,[0x07B5]
runtime 0x0666: mov al,[0x07B6]
runtime 0x068F: mov al,[0x07B7]
```

随后固定 `AH=0x07`，所以三个值分别组成：

```text
0x072C -> 原模板 +0x12C -> "Invalid partition table"
0x0744 -> 原模板 +0x144 -> "Error loading operating system"
0x0763 -> 原模板 +0x163 -> "Missing operating system"
```

producer 侧也明确：`sub_10013FD0` / legacy full-MBR 路径会从
`0x100E7220` 整体复制 `UsbMainBSec`，因此这三字节随模板固定生成。
current 注册路径则会清零 `+0x000..+0x18F`，但不覆盖
`+0x190..+0x1BD`，所以旧模板尾部可能继续保留，即使 bootstrap body
已经被清掉。

22份 original real-device reference set 的逐盘统计只有两种状态：

- **14/22 = `2C 44 63`**；
- **8/22 = `00 00 00`**；
- 无第三种值。

CI 原始夹具同时保留两种 profile。零态表示该 legacy tail 不存在/已清空；
`2C 44 63` 态的 producer、consumer、目标字符串和实盘都已闭合。
因此 `+0x1B5..+0x1B7` 共 **3B PARTIAL -> COMPLETE**。

相邻区域不随之升级：

- `+0x1A0..+0x1A3`：legacy `BuildSector0/sub_10013F10` 明确写
  SectorSize；22盘有4份为512，其余为0。该字段后续已由可选 overlay 生命周期、
  跨组件 negative consumer 与历史 0/512 双 profile 进一步闭合，见后文独立小节；
- `+0x1B8..+0x1BB`：已经闭合为标准 MBR disk signature producer：
  `GetSystemTimePreciseAsFileTime`（fallback `GetSystemTimeAsFileTime`）
  -> FILETIME 转 Unix seconds -> low32 -> `CREATE_DISK_MBR.Signature`
  -> `IOCTL_DISK_CREATE_DISK`。22/22 非零、19种值，同一 onlyid 重复备份稳定。
  Windows drive-layout API 会把它作为 MBR Signature 报告，但当前已审 EDP
  `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 调用均未发现业务逻辑读取该值，因此按本项目
  “EDP consumer 也需闭合”的严格口径继续 PARTIAL；
- `+0x1BC..+0x1BD`：在这一阶段仅确认官方模板和22盘均为0，因此当时仍未升级；
  后续已继续补齐 current SAFE6/Linux writer ownership 边界、bootstrap/EDP negative
  consumer 与57份历史快照，最新结论见下方独立闭环小节。

#### LBA0 bootstrap profile：current 0x190 清零与 legacy UsbMainBSec 模板已分型

继续沿 current Windows 注册主链回溯后，LBA0 前400B已经不再只是“观察到有零态”。
`CUsbRegsiter::RegsiterUsb` 在各 sector builder 与分区构造完成后、最终
`WriteSectorData(..., count=0x0D)` 之前，**无条件**执行：

```text
memset(metadata + LBA0 + 0x000, 0, 0x190)
```

因此 current 注册 writer 对 `LBA0+0x000..+0x18F` 的正式输出就是 zero[400]。
独立的 `UDiskLabelRepair.dll::CLabelRepair::ReCreate0Sector` 新建路径也在
`sub_10003960` 中先清零同一0x190B，再清零0x40B MBR table 并重建分区项与
`55 AA`。这两条独立官方路径把“current zero bootstrap”从样本现象升级为
明确 producer 行为。

另一方面，`cemsusbregsiter.dll` 自身保留正式静态
`UsbMainBSec@0x100E7220`。对其前0x190B独立提取后的 SHA-256 为：

```text
4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed
```

21份非转换 `nopwd_tool/backup` 原始完整快照重新聚类后，前400B只有三类：

- 多份 legacy 原盘与上述 `UsbMainBSec` 前400B **逐字节完全一致**；
- 多份 current 原盘为完整 zero[400]；
- Aigo L8302 为单独的第三种 bootstrap profile（既非上述模板也非全零）。

仓库 curated 原始协议夹具已经同时覆盖第一、第二类，并新增
`lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix`
回归门禁。该门禁还固定当前已观测的尾部边界：

- `+0x190..+0x19F` 为零；
- `+0x1A0..+0x1A3` 仅见 SectorSize = 0 或 512；
- `+0x1A4..+0x1B4` 为零；
- `+0x1B5..+0x1B7` 是已闭合的 legacy message-pointer；
- `+0x1B8..+0x1BB` 是标准 MBR disk signature；
- `+0x1BC..+0x1BD` 为零。

此前这批证据**没有增加严格 COMPLETE 字节数**。原因是：current zero producer 虽已闭合，
标准 legacy template 的静态身份也已由多盘逐字节证实，但还没有定位到“哪一代官方
注册/格式化 writer 将这400B template 写入 LBA0”的历史 producer，也没有解释
Aigo L8302 第三种 bootstrap 的 producer/选择条件；尾部 SectorSize/reserved 的
业务 consumer 也未全部闭合。因此 `+0x000..+0x1B4` 继续整体保持 PARTIAL，
当时总计仍为 **3656B COMPLETE / 3000B PARTIAL**；后续对尾部逐段拆分后的最新统计见下文。

##### LBA0 `+0x190..+0x1B4` 再拆分：33B unowned compatibility region 闭合

继续按 SAFE6 实际调用链复核后，先纠正一个旧归因：Windows
`BuildSector0/sub_10013F10` **只在 SAFE1 分支**由 `RegsiterUsb` 调用；SAFE6 主链
不会调用它。SAFE6 在完成 LBA4/LBA6/LBA8/LBA11/EDPF 等构造后，最终只执行：

```text
memset(LBA0 + 0x000, 0, 0x190)
WriteSectorData(LBA0..12, count=13)
```

因此 `+0x190..+0x1BD` 在 current SAFE6 中不是新建 payload，而是既有盘面 backing。
Linux `CLabelManage::BuildSector0@0x1C8CC, diskfile.cpp:625` 独立证明同一边界：函数
只根据 `m_nSectorSize` 计算 MBR partition sector count、清/写 `+0x1BE` 的64B分区表，
完全不写 `+0x190..+0x1BD`。

把尾部按真实行为拆开后：

- `+0x190..+0x19F` 16B：legacy `UsbMainBSec` 固定为零；Aigo L8302 的
  `Netac_USB_API.dll::sub_10003880` 整扇模板也为零；current SAFE6/Linux builder
  都不拥有该区，只 preserve；
- `+0x1A0..+0x1A3` 4B：Windows SAFE1 `sub_10013F10` 明确写 sector size，
  legacy `UsbMainBSec` 为512；但 Linux BuildSector0 只使用 sector size 计算分区、
  不把它落到此槽，current SAFE6也只 preserve，因此这是独立 compatibility field；
- `+0x1A4..+0x1B4` 17B：与前16B相同，legacy/Netac producer均为零，current
  SAFE6/Linux writer均不拥有。

consumer 侧也逐段核对：`UsbMainBSec` 16-bit bootstrap 的明确尾部数据引用只有
`+0x1B5/+0x1B6/+0x1B7` 三个消息指针以及标准 MBR partition/signature；当前注册准入、
`UDiskLabelRepair::ReCreate0Sector/sub_10003960` 也不解析上述两段33B。repair 的新建
路径同样只清前0x190和`+0x1BE`分区表，证明这33B不属于 repair payload。

实盘方面，严格22份原始参考的两段33B均22/22全零；进一步只读扫描
`nopwd_tool/backup + utils/backup` 的57份完整历史快照仍为57/57全零。相邻
SectorSize 则明确出现双 profile：扩展57份为34×512、23×0，证明不能把整个尾部
机械叫做 zero padding。

因此本轮先升级真正闭合的两段：16B+17B = **33B PARTIAL -> COMPLETE**，语义为
**cross-profile unowned preserve / historical-zero compatibility region**。COMPLETE 不表示
未来必须为零；若发现非零未知 profile，兼容实现应原样 preserve。SectorSize 4B 因
SAFE6 历史0/512选择条件及值相关consumer仍缺，继续PARTIAL。

##### LBA0 `+0x1BC..+0x1BD`：2B standard MBR reserved / unowned compatibility word 闭合

这2B此前因为“模板为0 + 实盘全零”不足以满足严格口径而保持 PARTIAL。继续沿与
`+0x190..+0x19F/+0x1A4..+0x1B4` 相同的 ownership/consumer 方法复核后，证据补齐：

- current SAFE6 `RegsiterUsb` 只清 `+0x000..+0x18F`，随后从 `+0x1BE` 起重建分区表，
  因而 `+0x1BC..+0x1BD` 属于 pre-read backing，current writer 不拥有；
- Linux `CLabelManage::BuildSector0@diskfile.cpp:625` 同样只处理 `+0x1BE` 起的 MBR
  partition entries，不写该2B；
- legacy `UsbMainBSec` 与 Aigo/Netac 的完整 MBR 模板在这2B均为 `00 00`；
- 16-bit `UsbMainBSec` bootstrap 已确认的尾部直接引用集中在三个 message-pointer、
  partition table 与签名，不读取 `+0x1BC/+0x1BD`；current 注册/准入、
  `UDiskLabelRepair::ReCreate0Sector` 与已审 drive-layout 路径也不赋予这2B业务语义；
- 严格22份原始参考 22/22=`00 00`；扩展57份完整历史快照同样57/57=`00 00`，而
  相邻 `+0x1B8..+0x1BB` disk signature 在同一语料中明确多值，说明这里不是把整个
  MBR 尾部机械当成固定零。

因此 `+0x1BC..+0x1BD` 共 **2B PARTIAL -> COMPLETE**。其完成语义是
**standard MBR reserved / unowned compatibility word**：跨已知 writer profile 的 ownership、
negative semantic consumer 和实盘行为已经闭合。COMPLETE 不意味着未来盘面必须为零；
若遇到未知非零兼容值，edpcli 应保留而不是清洗。

##### LBA0 Aigo L8302 第三 profile：已定位 Netac Format 的完整 MBR producer

继续对第三类 bootstrap 做原始二进制反查后，Aigo L8302 不再是“来源未知的特殊
前缀”。严格原始样本
`disk26_491520000_vid3535_pid2000_disk&ven_aigo&prod_l8302_onlyid1911491440_20260903_120554.bin`
的 `LBA0+0x000..+0x18F` SHA-256 为：

```text
00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec
```

这400B 在随 CEMS 安装的 **8份独立 Netac/hardware 二进制**中逐字节出现，
包括：

- `hardware.dll`（模板 VA `0x1012C9E0`）；
- `Netac_USB_API.dll`（模板 VA `0x1014BA58`）；
- `Netac_USB_API64.dll`；
- `hardware1.dll / hardware1hd.dll`；
- `isoupdate/newusb20.dll`；
- Edp 目录下的对应 Netac 32/64 位库。

关键点不是“搜索到相同字符串”，而是 Windows PE 机器码给出了完整 writer：
`Netac_USB_API.dll::sub_10003880` 在 MBR 格式化的两个分支
`0x10003950 / 0x100039C0` 都执行：

```text
EDI = output_mbr
ESI = 0x1014BA58
ECX = 0x80
rep movsd                       # 精确复制 0x80 * 4 = 512B
memset(output_mbr + 0x1CE, 0, 0x30)
output_mbr[0x1BE] = 0x80
...                            # 重建 partition type/start/count
```

因此 `+0x000..+0x18F` 400B 在该 Format profile 下由嵌入模板**原样生成**；
分区表修改从 `+0x1BE` 附近开始，不会改动这400B。Aigo 实盘与 producer
模板逐字节相等，已经满足该第三 profile 的原始盘交叉验证。

同时又找到 legacy `UsbMainBSec` 的直接 LBA0 writer，而不再只知道它被
`BuildSector6` 当底模使用：`CUsbRegsiter::UnRegsiterUsb` 在
`sub_100419B0()` 判定分支中执行
`memcpy(temp, UsbMainBSec, sector_size) -> sub_10013E40 -> WriteSectorData(LBA0,1)`。
这证明 legacy 模板本身确实属于官方 LBA0 写回材料。

不过这批发现仍**不增加 COMPLETE**。严格缺口已经缩小为两个 profile-selection
问题：

1. 22份生成参考里的 legacy 注册盘，仍缺“旧注册/格式化版本为何在最终已注册状态
   保留 UsbMainBSec bootstrap”的历史选择链；当前找到的 `UnRegsiterUsb` writer
   不能自动等价成旧注册 producer。
2. Aigo 的 Netac Format 底层 producer 已闭合，但当前
   `cemssafeudisklabeltool -> usbtoolBusManage` 主链尚未找到静态调用
   `Format*_NetacAPI` 的上层选择点，不能宣称所有制盘都会先走该路径。

所以 LBA0 `+0x000..+0x1B4` 仍保持 PARTIAL，但“第三 profile producer 未知”
这一旧表述已经作废。

##### LBA0 Aigo/Netac 上层链继续收敛：`UsbFormat` 不是 Netac MBR Format 的直接调用点

继续从 `usbtoolbusmanage.dll::BusManageImp::WriteLabelImp` 的真实 SAFE_DEV
分支追踪后，先补齐了此前缺失的对象来源：

- `BusManageImp::Init` 动态加载 `SafeUsbRegsiterCems.dll`，解析
  `GetUsbTegsiterObj`，并把返回对象保存到内部 `+0x6798`；
- `WriteLabelImp` 对该对象依次调用虚表 `+0x10`、`+0x14`、`+0x20`；
- `+0x14` 精确对应
  `CCEMSSafeUsbRegsiter::UsbFormat@0x10004220`，失败后上层日志就是
  `UsbFormat failed`；
- `+0x20` 精确对应
  `CCEMSSafeUsbRegsiter::RegsiterSafeUsb@0x10004F20`。

但把 `UsbFormat` 本体继续反汇编后，必须修正“它就是 Netac Format
上层选择器”的初步假设。其前置 `+0x10` 方法会：

1. 先检查目标盘 `X:\\bin\\windows\\sectorManage.dll`，不存在时再检查
   `X:\\Costom\\sectorManage.dll`；
2. 在需要兼容处理的盘上加载 `X:\\Costom\\usb20dll.dll`；
3. 通过 `IF_OpenDevEx / IF_GET_Dev_Info` 读取设备信息命令 `0x52`；
4. 返回字节为 `0xA2` 时走“无需该预处理”的分支，否则上层才进入
   `CCEMSSafeUsbRegsiter::UsbFormat`。

`UsbFormat` 自身按对象 `+0x04` 的 office/normal 分支分别进入
`BackPassWordOffice` / `BackPassWord`，两条路径只动态解析
`IF_OpenDevEx / IF_CloseDev / IF_IIR_Manage` 并完成密码/IIR 兼容处理；
没有解析或调用 `IF_DiskFormat`。紧随其后的 `RegsiterSafeUsb` 也分成
已有 V2 label 的 read/modify/write 与 `SecUsbInterface.dll` 新建路径，
同样未出现 Netac `FormatExA_NetacAPI`。

另一方面，目标盘同套兼容库 `usb20dll.dll` 的导出
`_IF_DiskFormat@0x10003DA0` 已精确恢复：

```text
_IF_DiskFormat
  -> fcn.10003480(open/resolve target)
  -> NewUsb20.dll!FormatExA_NetacAPI(...)
```

也就是说，“CEMS 随盘兼容层确实提供 Netac Format API”已经由调用关系证明；
但在当前安装包全部顶层 EXE/DLL 中继续检查普通 import 和
`GetProcAddress("IF_DiskFormat")` 名称引用后，除 `usb20dll.dll`
自身的 export 名外仍未发现主制标链调用点。这个**负证据很重要**：
不能把 `WriteLabelImp -> CCEMSSafeUsbRegsiter::UsbFormat` 和
`usb20dll!IF_DiskFormat -> NewUsb20!FormatExA_NetacAPI` 两条链凭名称强行拼接。

因此 Aigo L8302 的 400B 模板 producer 仍然成立，但剩余问题进一步精确为：
**哪一个历史升级/量产/格式化入口真正调用 `IF_DiskFormat`，以及它以什么
设备/profile 条件选择该路径。** 该选择条件仍阻止 bootstrap 主体整体闭合；不过后续
逐字节交叉最终得到30B在三种已知 producer 中完全不受该选择影响，现已单独拆出闭合。

##### LBA0 前400B：三类 bootstrap profile 的30B逐字节不变量闭合

本轮不再把前400B强制当作一个不可拆分状态，而是直接对已知三种真实 producer
做逐字节交集：

1. current SAFE6：`RegsiterUsb` 对 `+0x000..+0x18F` 显式 `memset(0)`；
2. legacy：official `UsbMainBSec` 模板；
3. Aigo L8302：`Netac_USB_API.dll::sub_10003880` 的嵌入 MBR 模板，
   已由真实 Aigo 原盘逐字节验证。

三类模板前400B共同为0的物理字节只有30B。除连续的
`+0x17B..+0x18F` 21B 外，还有9个离散位置：
`+0x0E1/+0x0E8/+0x101/+0x103/+0x10B/+0x10D/+0x124/+0x143/+0x162`。
这些离散字节也逐条回到16-bit代码/字符串边界核实，不再因为“位于大块PARTIAL中”
而机械降级。

consumer 复核结果：

- legacy `UsbMainBSec` 的第三条错误消息
  `"Missing operating system"` 固定在 `+0x163..+0x17A`，因此
  **`+0x17B` 正是该 C-string 的 NUL 终止符**；既有 message-pointer 低字节
  `+0x1B7=0x63` 会把打印路径指到这条消息；
- legacy 终止符之后的 `+0x17C..+0x18F` 20B 没有代码/数据引用，是模板尾部零填充；
- Aigo/Netac MBR bootstrap 会先把完整512B从 `0x7C00` 搬到 `0x0600`
  （`mov cx,0x100; rep movsw`）再跳到 relocated code；三条错误消息位于原模板
  `+0x08B/+0x0A3/+0x0C2`，最后一条的 NUL 已在 `+0x0DA`。
  其执行路径没有任何引用落入 `+0x17B..+0x18F`，因此这21B在 Netac profile
  明确只是零填充；
- current SAFE6 不执行这套 bootstrap，而是直接清零同一区域。
- legacy 的7个代码散点也都有精确指令语义：
  `+0x0E1/+0x0E8/+0x124` 是三个 `mov dl,[bp+0]` 的零位移操作数；
  `+0x101/+0x103/+0x10B` 是三个 `push 0` 的零立即数；
  `+0x10D` 是 `push 0x7C00` 的低字节0。Aigo/Netac 在这些偏移已经是零填充，
  current SAFE6同样显式清零；
- legacy 前两条错误消息分别是
  `Invalid partition table@+0x12C..+0x142` 与
  `Error loading operating system@+0x144..+0x161`，所以
  `+0x143/+0x162` 分别是其NUL终止符；Aigo/Netac/current 在两处仍均为0。

实盘证据也补成三 profile 闭环：committed fixtures 同时覆盖 current-zero 与
legacy UsbMainBSec；另新增
`tests/fixtures/protocol_evidence/aigo_l8302_netac_lba0_prefix.hex`
锁定 Aigo L8302/Netac 的真实前400B。三类均满足
`LBA0[0x17B..0x190] == zero[21]`，且上述9个离散偏移也全部为0。回归
`lba0_bootstrap_body_closes_all_three_profile_invariant_zero_bytes`
同时锁定 legacy 指令操作数、三条错误消息终止符以及三 profile 物理零值。

因此：

- 7个 legacy 指令零操作数字节：COMPLETE；
- `+0x143/+0x162/+0x17B` 三个 legacy MBR 错误消息 NUL 终止符：
  COMPLETE；
- `+0x17C..+0x18F` 20B cross-profile fixed-zero bootstrap tail padding：
  COMPLETE；
- 其余370B bootstrap 代码/数据仍因两套非零模板与 historical profile-selection
  未闭合而保持 PARTIAL。

这次前400B共 **30B 净新增 COMPLETE**，LBA0 从112/400更新为
**142 COMPLETE / 370 PARTIAL**；profile-selection blocker 仍真实存在，但不再拖住
与选择无关的逐字节不变量。

##### LBA0 `+0x1B8..+0x1BB`：Windows MBR disk signature 4B 闭合

这4B此前已经有完整 producer 和真实盘变化规律，但因为没有找到 EDP 自身的
值相关业务 consumer，按旧的过严口径保留为 PARTIAL。本轮把“标准 Windows
MBR 字段的系统语义”和“EDP 是否附加解释”拆开后，证据链已经闭合。

producer 侧保持既有结论：

```text
CreateDiskMbr
  -> GetSystemTimePreciseAsFileTime
     (fallback GetSystemTimeAsFileTime)
  -> FILETIME 转 Unix seconds
  -> low32
  -> CREATE_DISK_MBR.Signature
  -> IOCTL_DISK_CREATE_DISK
```

Windows 正式结构定义又给出 consumer/字段语义：
`DRIVE_LAYOUT_INFORMATION_MBR.Signature` 就是用于唯一标识 MBR disk 的
drive signature，并由 `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 通过
`DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 返回。也就是说，这4B的 consumer
并不需要是 EDP 私有代码；它首先是 Windows 磁盘布局协议自身拥有的标准字段。

同时，本轮把当前 `CEMSUsbRegsiter.dll` 中唯一的
`IOCTL_DISK_GET_DRIVE_LAYOUT_EX (0x70050)` 调用继续追到
`fcn.10046320@0x100465A9`。调用成功后，代码只做：

```text
buffer + 0x04 : PartitionCount == 1
buffer + 0x00 : PartitionStyle == PARTITION_STYLE_MBR
```

而 `DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 位于该 header 的
`+0x08`，这条 EDP 路径没有读取它。于是“没有 EDP consumer”不再是证据缺口，
而是**negative-semantic-consumer 证据**：EDP 只关心布局类型和分区数量，不对
Windows disk signature 叠加第二层业务含义。

真实盘仍保持既有强验证：严格22份原始参考 22/22 非零、共有19个不同值；
同一 onlyid 的重复备份 signature 稳定，按 little-endian 解释又与历史初始化时间
一致。curated protocol fixtures 现再加门禁：每份真实 fixture 的 signature 必须
非零，并且 fixture 集合至少保留两个不同真实 signature，防止未来把该字段误清零
或退化成常量。

因此 `+0x1B8..+0x1BB` 共 **4B PARTIAL -> COMPLETE**。闭合语义是
**standard Windows MBR disk signature**：producer、Windows 标准 consumer、
EDP negative semantic consumer 和真实盘均已齐全。

##### LBA0 `+0x1A0..+0x1A3`：optional SectorSize compatibility overlay 4B 闭合

该 DWORD 过去一直因为 0/512 双态和缺少独立 reader 保留为 PARTIAL。本轮继续从
writer ownership、全组件访问点和历史 profile 的物理独立性三个方向交叉后，已经
可以把它从 bootstrap 主体中彻底拆出。

producer / lifecycle：

- Windows `CEMSUsbRegsiter.dll::BuildSector0/sub_10013F10@0x10013FB6` 在 SAFE1
  分支明确执行 `mov [sector0+0x1A0], m_nSectorSize`；
- legacy `UsbMainBSec` 模板该 DWORD 固定为512；
- Aigo/Netac `sub_10003880` 使用的整扇 MBR template 在相同槽位显式为0；
- current SAFE6 主注册链不调用 `BuildSector0`，只保留预读 LBA0 的该4B；
- Linux `CLabelManage::BuildSector0@diskfile.cpp:625` 使用 `m_nSectorSize` 计算
  partition sector count，但不把它序列化到 `+0x1A0`。

consumer 侧的负证据也已补齐。legacy 16-bit bootstrap 不读取本槽；对 current
`CEMSUsbRegsiter.dll` 全模块按 `+0x1A0` 数据偏移复核后，唯一真正的 sector-data
访问就是上述 SAFE1 writer store。另一个 `push 0x1A0` 已逐指令确认只是
`memset(local_buffer, 0, 0x1A0)` 的长度。`UDiskLabelRepair.dll` 没有盘面
`+0x1A0` 读点，Linux 没有 `ReadSector0` consumer；`EdpEDiskCtrl.dll` 中看似
`object+0x1A0` 的命中也已抽样追到相邻 vtable 槽调用，属于 C++ 方法表/对象布局，
不是 LBA0 staging buffer。

历史实盘进一步证明 0/512 不是两个 bootstrap profile，而只是同一 bootstrap 上的
独立 overlay。对 `nopwd_tool/backup + utils/backup` 当前可用完整历史快照重新聚类，
其中 **47份**具有完全相同的 official legacy `UsbMainBSec` 前400B：

- 10份 `SectorSize=0`；
- 37份 `SectorSize=512`。

把每份 LBA0 的 `+0x1A0..+0x1A3`、disk signature `+0x1B8..+0x1BB` 和标准
partition table `+0x1BE..+0x1FD` 三个独立变化区归一化为0后，47/47 整个512B
LBA0 的 SHA-256 都严格等于：

```text
2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f
```

这直接排除了“SectorSize=0/512 代表不同 bootstrap 主体”的解释。committed protocol
fixtures 又同时包含0与512两种真实状态，新增测试要求两态都持续存在。

因此 `+0x1A0..+0x1A3` 共 **4B PARTIAL -> COMPLETE**，按
**optional SAFE1 / legacy SectorSize compatibility overlay** 建模：512 表示该兼容
metadata 被历史 SAFE1/legacy writer 填充；0表示 overlay absent/unowned，current
SAFE6 只透明保留。COMPLETE 不意味着未来出现其它值时可以机械归零，未知值仍应
兼容保留并报告。

#### LBA1/LBA2 GPT：first-party virtual writer 正向闭环

此前 GPT 只闭合到“官方结构 + 静态 writer/consumer”，current 22份 physical SAFE6
reference 的 LBA1/LBA2 又全部为零，因此两扇都保守留在 PARTIAL。本轮不再用结构推测，
而是在不接触任何物理 raw device 的 Unicorn x86-64 环境里直接执行 Linux first-party
`libcemsfilesyscheck.so`：

```text
CLabelManage::BuildSector0_Gpt @ 0x1FD30
CLabelManage::BuildSector1_Gpt @ 0x1FDAA
CLabelManage::BuildSector2_Gpt @ 0x1FFF6
```

只替换 ELF 外部的 libc `memset/memcpy` ABI 边界；GPT builder 和内部
`calculate_crc32@0x1FC49` 原生执行。确定性输入为 2GiB disk、512B sector、partition GUID
`00112233445566778899aabbccddeeff`。`BuildSector2_Gpt` 先写 LBA2 entry0，随后
`BuildSector1_Gpt` 对 `base+2*sector_size` 起完整 `32*512=16KiB` GPT partition array
求 CRC，再生成整512B primary header。

first-party 输出经独立解析得到：

- `EFI PART` / GPT Version `0x00010000` / HeaderSize=92；
- CurrentLBA=1，BackupLBA=4194303，FirstUsable=34，LastUsable=4194270；
- Disk GUID=`a2a0d0ebe5b9334487c068b6b72699c7`；
- PartitionEntryLBA=2，entry count=128，entry size=128；
- 独立 IEEE CRC32 命中 header `0xA4B46C72` 与 array `0xD32CFEA7`；
- LBA1 `+0x5C..+0x1FF` 是官方512B模板自身的零尾，不是 harness padding；
- entry0 type GUID=`a2a0d0ebe5b9334487c068b6b72699c7`，partition GUID 为 caller 输入，
  start=63，end=4194270，attr=0，name[72]=0。

跨平台 consumer 又独立闭合：把同一34扇 official-builder staging image 交给 current
Windows `CEMSUsbRegsiter.dll::IsAllowRegisterCommonLabel/sub_1002AB70` 原生执行，返回
**2 = GPT**。而 Windows `sub_1002B2F0` 与 Linux `AnalyzeGptPartitionTable` 都按128B
stride 遍历 GPT entry，命中支持的 type GUID 后读取 `start/end/attr`。

这里不降低严格标准：`BuildSector2_Gpt` 一次只写128B active entry，当前 `.so` 内没有它的
active caller，也没有负责清理其它 entry slot 的 store；因此 first-party writer fixture 中
entries1..127 的零值仍不能当成“producer-owned zero”。后续继续追 parser 与 Windows
one-partition creator 后，才把 unused entry 的语义从“未知 residual”推进到“unowned residual”，
详见后文“LBA2 unused GPT entry”小节。

最终状态为：

- **LBA1：512B COMPLETE**；
- **LBA2：512B COMPLETE**，其中 entry0 是 active-entry first-party writer/consumer，
  entries1..3 则按 TypeGUID=0 的 unused discriminator + residual negative-consumer 闭合。

仓库 fixtures `official_virtual_gpt_lba1.hex` / `official_virtual_gpt_lba2.hex` 与
`official_virtual_gpt_builder_emits_valid_lba1_and_entry0` 回归固定 active entry/header 结构与CRC；
它们仍不把 harness-owned unused-entry zeros 冒充 producer 常量。

#### LBA6/LBA9 long-User：最大合法 profile 的 first-party writer→reader 闭环

继续追 `UsbWriteParam.m_usbowner/User` 后，短值与长值两条生命周期都已闭合。

短值 producer 链不是“固定32B字符串”：

- `sub_10047690` 把 request User 写到 `UsbWriteParam+0xFC`，capacity=`0x9C=156B`；
- 它调用的 `strcpy_s@0x10097F4F` 机器码逐字节复制，遇首个 NUL 立即返回，**不会清
  destination 剩余 capacity**；
- `RegsiterUsb@0x1003B616` 随后调用 `sub_100139F0`，把持久 `UsbWriteParam` 再复制到
  未初始化栈局部 `var_3F4`；User仍使用同一 `strcpy_s`，因此 NUL 后尾部继续继承
  writer-uninitialized backing；
- `BuildSector6` 在 `strlen(User)<32` 时固定复制该数组前32B到 LBA6 `+0x50..+0x6F`；
  current reader只按 C-string 消费首NUL前内容。committed originals 中真实非零尾字节与
  这条生命周期一致。

长值链又以 first-party runtime 正向执行验证。三套 current writer 机器码完全同构：

```text
Windows CEMSUsbRegsiter.dll  0x1001417D..0x100141B1
Linux   libcemsfilesyscheck  0x0001CDA3..0x0001CDE2
vrvaud_c.dll                 0x10119082..0x101190B5
```

都执行：

```text
LBA6+0x50 = 0x40245E2A || User[0..27]
dst = out + 3*sector_size + 0x100   // LBA9+0x100
src = User + 28
len = strlen(User) - 27             // 包含 terminating NUL
```

User 固定数组只有156B，所以最大合法 C-string 是155B；此时 continuation 恰为128B，
刚好覆盖 `LBA9+0x100..+0x17F`，最后1B为 NUL。隔离 Unicorn harness 直接执行 current
Windows `BuildSector6@0x10013FD0`，LBA9 其它区域先填 `0xCC` 作为边界探针：运行后仅
`+0x100..+0x17F` 被完整改写，`+0x000..+0x0FF` 与 `+0x180..` 仍保持 `0xCC`，证明
写所有权边界不是 harness 零初始化造成的假象。

同一 wire image 又直接交给 current Windows `ReadSector6/sub_100152A0`。harness 只跳过
Windows `FS:[0]` SEH/TEB bookkeeping，并替换不影响协议的 allocator/C++ string 环境边界；
reader 自身的 SAFE6 checksum、rolling XOR、marker 判定和 continuation copy 均原生执行。
运行命中 `0x15552` long-User 分支，最终在输出 `UsbWriteParam+0xFC` 恢复完整155B User
及 terminating NUL，与 writer 输入逐字节一致。

因此：

- **LBA6 `0x050..0x06F` 32B**：short C-string + writer-uninitialized backing 与 long
  marker+28B prefix 两种状态全部闭合，升 COMPLETE；
- **LBA9 `0x120..0x17F` 96B**：long-User continuation + short-profile preserve 生命周期、
  writer写边界与 official reader round-trip 全部闭合，升 COMPLETE；
- `LBA9+0x114..0x11F` 仍与 historical SAPF trailing/backing profile 重叠，旧SAPF producer
  未恢复，所以那12B继续 PARTIAL，不能被 long-User 正例顺带升级。

仓库新增 `official_virtual_long_user_lba6.hex` / `official_virtual_long_user_lba9.hex` 和
回归 `official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 固化最大长度、
marker/prefix/continuation 与写边界证据；physical 22盘没有 long User 的历史事实继续保留，
不与 virtual first-party positive-wire evidence 混计。

#### LBA2 unused GPT entry：TypeGUID discriminator + 336B unowned residual 全部闭合

继续回到 current Windows `CEMSUsbRegsiter.dll` 的真实 GPT 创建路径后，确认
`WriteNormalULabel` 在大盘分支调用 `sub_10037160(..., partition_count=1)`；该函数构造
`DRIVE_LAYOUT_INFORMATION_EX` 时设置 `PartitionStyle=1 (GPT)`、`PartitionCount=1`，
再通过 `IOCTL_DISK_CREATE_DISK(0x7C058)` 与
`IOCTL_DISK_SET_DRIVE_LAYOUT_EX(0x7C054)` 交给 Windows disk stack 落盘。

这使 entry1..3 的 **类型字段** 不再需要猜测。UEFI 2.10 §5.3.3 对 GPT entry 的
`PartitionTypeGUID@+0x00..0x0F` 有明确 wire 语义：16B 全零即 unused entry；current
Windows 路径又明确只提交1个 active partition。因此 LBA2 首扇区的 entry1/2/3 三个
type GUID（物理 `+0x080..08F / +0x100..10F / +0x180..18F`）可按 standards-defined
unused discriminator 闭合。

第一阶段证据按严格边界分层：

- official Linux GPT virtual fixture 中这三个 type GUID 均为0；
- 本机只读扫描20,538个候选文件找到1份真实 GPT image（Ubuntu 26.04 ISO）；其3个
  used entry 后至少125个 unused entry 的 type GUID/完整entry均为0；
- 但 Linux fixture 的其它 residual bytes 来自 harness 预清，Ubuntu image 也不是
  EDP/Windows first-party wire producer，所以这一阶段**只**升级三个16B type discriminator，
  不把 `UniqueGUID/start/end/attr/name` 的零值解释成 producer 常量。

随后继续追 current parser，第二阶段把剩余336B的**消费语义**直接钉死。Windows
`CPartitionType::AnalyzeGptPartitionTable/sub_1002B2F0` 对每个128B entry 的真实控制流是：

1. 先以 supported-GUID map 对 `entry+0x00..0x0F` 做 TypeGUID 比较；
2. 只有 GUID 匹配后才进入 `+0x20/+0x28/+0x30` 的 start/end/attr 读取与
   `PartitionInfo` 构造；
3. GUID 不匹配时直接进入下一条 entry，residual 不参与任何值相关判断。

Linux first-party `CPartitionType::AnalyzeGptPartitionTable@0xFB36` 独立给出同一边界：
`memcmp(type_guid, entry, 16)` 命中后才执行 `mov 0x20(entry)`、`mov 0x28(entry)`，再由
`AnalyzePartitionProperty` 读取 `+0x30`；未命中则只递增 iterator/entry index。

为了排除“反编译看起来没读，但运行时通过 STL helper 间接读 residual”的可能，本轮又做了
Windows first-party 动态 consumer probe：

- 直接调用 official `CPartitionType` constructor `sub_1002A530(512)`，supported-GUID map
  由原始 DLL 构造；只给 Unicorn 映射 `fs:[0]` SEH 零页，并把 CRT
  `HeapAlloc/HeapFree` 替换为等价 allocator/free 边界；
- 输入1个512B entry sector，4条 entry 都设置 `TypeGUID=zero[16]`，其余
  **112B/entry 全部故意填 `0xA5`**；
- 原生 `sub_1002B2F0(..., sector_count=1)` 返回0且无异常；内存 read hook 对这512B
  总共只观察到每条 entry TypeGUID 起点的短路比较读取，**336B residual 读取次数=0**；
- 该 `0xA5` 是 consumer probe，不是 writer fixture，专门用于证明 unused residual
  可以取任意值而不进入业务语义。

所以 entries1..3 的 `UniqueGUID/start/end/attr/name` 等336B不应继续建模为
“缺 Windows kernel 写零正例的字段”，而应建模为 **unused-entry unowned residual**：
TypeGUID=0 已经宣告 entry 不存在，后续112B/entry没有 EDP/official parser 语义消费。
这与 LBA4/LBA5 的 unowned backing 采用同一 COMPLETE 口径；实现可以 canonical 新盘为零，
但兼容 reader 不得把 residual 非零误判为隐藏分区或强制依赖其零值。

因此 LBA2 再增加 **336B COMPLETE**，至此 **512/512 COMPLETE**。回归
`one_partition_gpt_keeps_entries1_to3_partition_type_guids_unused` 继续锁定三个16B discriminator；
严格总账/完整LBA门禁则防止未来把 residual 又误退成 producer-owned zero 或 PARTIAL。

#### LBA6 GSerial / BeiZhu：按首个 NUL 边界再拆 10B

此前将 `GSerial[16]` / `BeiZhu[16]` 整槽回退 PARTIAL 是必要的，因为短字符串 NUL 后
确有 current backing 与 legacy MBR underlay 两种物理语义。但这也把**首个 NUL 之前
确定属于 C-string 的字节**一并低估了。

Windows/Linux current `BuildSector6` 对两槽的物理写法已经是机器码级一致：先清16B
temporary buffer，再从 `UsbWriteParam` 固定复制输入前15B，最后整16B写盘；reader从
`+0x1C0/+0x1D0` 均按 C-string 消费。结合 strict originals：

- GSerial 16/22 为 `322CA28A\0`，6/22 为 `322CA28A-D7D144`；因此
  `+0x1C0..+0x1C7` 8B 永远是有效字符串前缀，`+0x1C8` 也仍是字符串域：short 为NUL、
  long 为 `'-'`。历史目录另做20个去重 front census，无任何前8B反例，byte8仅出现
  `00/2D`；legacy MBR underlay 从 short NUL **之后** 才暴露；
- BeiZhu 20/22 为空、2/22 为 GBK“普通”。因此只有 `+0x1D0` 能跨所有 profile
  保证仍属于 C-string 本体：空串时是NUL，“普通”时是首字节 `C6`；从 `+0x1D1`
  开始，空串 profile 已允许进入 backing，不能继续升级。

据此新增 **9B GSerial + 1B BeiZhu = 10B COMPLETE**；其余 GSerial 7B、BeiZhu 15B
继续保留 PARTIAL。回归
`lba6_gserial_and_beizhu_semantic_prefixes_stop_before_profile_underlay` 固定 committed
profiles 的该边界，而不是把字符串具体值硬编码成未来协议常量。


## 11. 历史 DLL / profile 取证目标

> 本节只记录已经核验过的二进制版本、哈希、已确认调用链和仍缺失的历史 producer。
> “目标/候选”不等于协议事实；只有落到机器码/源码/真实盘并通过回归后才能升级主账本。

This note records exact public archive identities needed to close remaining legacy producer/profile
branches. It does not change the strict protocol COMPLETE byte count.

### Same-era 2020 artifacts

#### cemsusbregsiter.dll

- Version: 19.11.4.1
- 32-bit PE
- Archive date: 2020-12-17
- MD5: `783d01f19e998a514834bc5e5f4249ad`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/cemsusbregsiter.dll.shtml`

This binary has already been used by the main audit to recover the 2020 HDSerialInfo producer
family.

#### safeusbregsitercems.dll

- Version: 19.4.4.2
- 32-bit PE
- Size: 158.11KB
- Archive date: 2020-12-16
- MD5: `e516454e5b37da8a853702aca7d4261c`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/safeusbregsitercems.dll.shtml`

该二进制现已实际取得并以 SHA-256
`cb700a3fdca69b126264d800657a2e941e8b5e5d5e35501968ed40cb694edc31`
固化到 `audit/protocol/evidence_manifest.tsv`。机器码/字符串审计确认它会加载
`usb20dll`，但当前恢复到的接口只包括
`_IF_OpenDevEx/_IF_IIR_Manage/_IF_CloseDev`；二进制中没有 SAFE6/LLGB/EDPF
字段家族，也没有恢复到 `_IF_DiskFormat` 的解析。因此它是更早设备/IIR
兼容层证据，**不是 strict legacy LBA0-LBA12 的直接 writer**，不得再把它作为
“尚未取得的候选”或用函数名推断 LBA0 profile selector。

#### EdpEDiskCtrl.dll

- Version: 3.6.10.18
- 32-bit PE
- Archive date: 2020-05-19
- MD5: `95a06e0d466ba40a7d5c0e6a409e2114`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/edpediskctrl.dll.shtml`

This binary has already been used by the main audit as an independent 2020 runtime reader.

### Dr.Web 完整配套组件树：存在性已确认，但日期不能用于旧版本定年

Dr.Web 的 `Trojan.StartPage1.58410` 记录列出一个被展开到
`%TEMP%\\Vz0e3033\\cems\\edp\\safeudisklabeltool` 的完整组件树，其中同时存在：

- `cemssafeudisklabeltool.exe`
- `busmanage.dll`
- `cemsusbregsiter.dll`
- `cemssafeudiskregmanage.dll`
- `safeusbregsitercems.dll`
- `sectormanage.dll`
- `usbtools.dll`
- `udiskprivateinterface.dll`

公开记录：
`https://vms.drweb.cn/virus/?i=28263010`。

**纠错：不能把页面的 2019-11-12 当成这棵组件树的部署/构建日期。**
该日期是页面明确标注的 “Added to the Dr.Web virus database” 日期；同一页面又明确
写着 “Virus description added: 2024-05-05”，并且它列出的落地文件中直接包含
`%TEMP%\\install_2024_05_03_15_33_19.log`。因此这份文件树至多证明“某个被该
病毒名描述的样本/安装包包含一套完整 safeudisklabeltool 配套组件”，**不能证明这些
组件早于 2019-11-21**，也不能用来给其中 DLL 定代。

该网页仍可作为 component-set existence locator，但不再作为 “earlier historical
acquisition locator”。它没有给出这些 DLL 的逐文件版本、哈希或原始字节，因此
不属于 physical/virtual/static 协议证据，也不会让任何 PARTIAL 字节升级。

下一步取得真正有独立版本/PE 时间/哈希证明、且早于或不同于 19.11.4.1 的完整配套
组件后，必须先同时检查四个
指纹：join59 writer、非零 HSerialCRC 输入赋值、`UsbOnlyInfo=0`、
动态 MBR template。只有能把输入赋值一路追到最终 LBA store 的候选才进入主账本。

### Why the historical paired-set target matters

The unresolved LBA0 bootstrap-body profile selection currently sits above the already-recovered
low-level Netac formatting exports. The current product chain reaches a
`SafeUsbRegsiterCems.dll!GetUsbTegsiterObj` object and a
`CCEMSSafeUsbRegsiter::UsbFormat` method, but the exact historical condition that selected the
legacy/Netac MBR bootstrap profile is not yet closed.

已取得的 19.4.4.2 `safeusbregsitercems.dll` 已被上述审计排除为直接 writer，
所以缺口不再是“拿到这一个 DLL”，而是**取得与历史 writer 同代的完整配套调用链**。
当前 `usb20dll!_IF_DiskFormat -> NewUsb20!FormatExA_NetacAPI` 只证明低层
Netac formatter family；仍需从 historical `WriteLabel/WriteNormalULabel` 或其
上游 profile selector 连到该 formatter 仍可补足“哪条历史部署调用链选择 Netac”的 provenance；但真实 LBA0 Netac wire 已与 `Netac_USB_API.dll::sub_10003880` 内嵌模板逐字节闭合，因此 selector 不再影响 LBA0 字节语义 COMPLETE 判定。


## 12. Phison F2 / LBA3 专项取证

> 本节记录已验证的 Phison 制造生态证据与负边界。
> 它证明 manufacturer family / F2 lifecycle，但**不把 F2 INFO staging 直接等同物理 LBA3**。

This note preserves evidence gathered while tracing the external manufacturer-owned LBA3 profile.
It does **not** change the strict LBA0–LBA12 COMPLETE byte count.

### Confirmed sample family

Public static-analysis records for the SHA-256
`96614750c61e0ad6b05d19e74848c1679f6318dd21de6faee46c92fb05152142`
(`MPALL_F1_9000_v372_0B.exe`) expose all of the following strings in one binary:

- `CBaseController::DoF2`
- `CBaseController::read_write_f2`
- `CBaseController::U3_DoF2`
- `CBaseController::WriteF2Mark`
- `CU32SSBaseContoller::U3_DoF2`
- `CU32SSBaseContoller::WriteF2Mark`
- `F1-F2 MARK`
- `F2 Merged`

A later independent MPALL generation,
`mpall_f1_7f00_dl07_v503_0a.exe`
(SHA-256 `2cfd1c3ea9d6bec17d8237f0be79ae96f78fe77a40032987c52d3a30996e29bf`),
retains the same F2 lifecycle function-name family, including
`DoF2`, `read_write_f2`, `U3_DoF2`, `WriteF2Mark`, and `F2 Merged`.

Therefore the F2 lifecycle is a cross-generation Phison MP implementation family rather than a
single-build string residue. This strengthens the producer-family attribution already used by the
main protocol audit, but does not by itself identify the exact PS2307/PS2309 wire profile that
produced the committed Kingston LBA3 sectors.

### Negative boundary: MPALL `F2_MP_21` is configuration metadata, not LBA3 wire evidence

Historical MPALL configuration examples independently place `F2_MP_21` under the INI section
`[Parameter Mark]` as `Parameter Type=F2_MP_21`. One 2010 example uses
`IC Type=PS2251-32`; a separate 2009 PS2231 case shows the same parameter-type spelling.
Sources:

- https://flashboot.ru/forum/index.php?topic=2549.0
- https://flashboot.ru/forum/index.php?topic=2108.0

Therefore strings such as `F2_MP_21` / `F2_MP_23` found inside MPALL executables are evidence
for the tool's configuration/profile layer. They are **not** evidence that those ASCII values, or
their numeric suffixes, occur in the host-visible LBA3 sector. In particular they must not be used
to assign semantics to LBA3 `+0x020..+0x027` without an actual store/copy path into the F2 buffer
and matching real-device bytes.

### Machine-code facts recovered in the previous local analysis session

The v3.72.0B executable was obtained and inspected offline without executing the MP utility.

- `CBaseController::WriteF2Mark` was located near `0x00581B00`.
- It passes the object-owned buffer at approximately `this+0x1C00C` into the F2 write path.
- It then issues an `"INFO"` readback and compares the first 512 bytes with the same buffer.
  A mismatch follows an explicit error path.
- `CU32SSBaseContoller::WriteF2Mark` was located near `0x00487FA0`.
  This controller family uses a different target-command wrapper and passes 0x1C0 (448) bytes
  from the same object-owned area.
- Consequently, the shared `WriteF2Mark` name must not be interpreted as one universal
  512-byte transport layout across controller classes.
- Several references to `this+0x1C00C` were recovered inside class-specific virtual methods
  such as `C2273Controller::virtual_308`, `C2267Controller::virtual_308`, and
  `C2261Controller::virtual_308`.
- One class path tests the buffer prefix against `12 01 00 02`; this is **not** the committed
  Kingston LBA3 wire profile, whose observed prefix is `00 01 00 00`.
  Therefore that branch is a useful negative discriminator and must not be promoted as the
  LBA3 template.

### FW/BN final marker page is a real MPALL input, not a direct LBA3 image

The v3.72 archive contains four FW/BN BIN files.  All four end with one exact
512-byte marker page:

| file | final page file offset | page +0x000 | page +0x010 prefix |
|---|---:|---|---|
| `BN67V1292KM.BIN` | `0x8200` | `this is mp mark\0` | `67 01 00 10 01 29 24 42` |
| `BN67V132M.BIN` | `0x8200` | `this is mp mark\0` | `67 01 00 10 01 32 10 42` |
| `FW67FF01V60424M.BIN` | `0x16200` | `this is mp mark\0` | `67 01 01 10 06 04 24 46` |
| `FW67FF01V61110M.BIN` | `0x1C200` | `this is mp mark\0` | `67 01 01 10 06 11 10 46` |

RTTI/vtable recovery fixes the previously name-only marker-page consumers to
concrete addresses: Base=`0x55FC90/0x55F570`, Base30=`0x541290/0x541780`,
C2250=`0x535C40/0x5354A0`, C2260=`0x5333A0/0x532B80`, and
C2261=`0x5316B0/0x532B80`.  Direct machine code confirms the lifecycle: seek
`file_size - 0x200`, read `0x200` bytes, compare the first15 bytes with
`"this is mp mark"`, then consume adjacent bytes in controller/version
compatibility checks.  Thus the PC-side utility treats the final page as a
formal FW/BN **version marker page**.

The F2 functions are now fixed to code addresses too:
`CBaseController::WriteF2Mark@0x581B00` sends `this+0x1C00C` through the F2
vendor-write helper, reads back an `INFO` response, and compares `0x200` bytes;
`CU32SSBaseContoller::WriteF2Mark@0x487FA0` uses a different `F3 00 83...`
wrapper and `0x1C0` payload.  These functions therefore reinforce, rather than
remove, the boundary between F2 INFO and the host-visible sparse LBA3 record.

A reproducible cross-family invariant has nevertheless been added.  The two
independent physical nonzero LBA3 profiles have different first 40 bytes but an
identical `+0x028..+0x1FF` tail: 456 zero bytes followed by
`"this is mp mark\0"`, SHA-256
`5f88797f7273191052e7a9300316e1a4f0f31563db07110a86fa4e648379198f`.
For all four pinned v3.72 FW/BN files, moving the final marker page's first16
bytes to its end yields the same 472-byte tail exactly; BN pages then match the
physical sector 500/512 and FW pages 496/512.  The offline reproducer is
`scripts/protocol/audit_lba3_phison_marker_page.py`.

This still does not establish the host serializer: the PC executable contains
no recovered `496B+16B` rotation path, and the exact physical dynamic 8-byte
values occur locally only in the physical captures.  The observed 16-byte
layout reordering is therefore a structural relationship, not an attributed
MPALL transformation.  No recovered PC-side path constructs the sparse
`00 01 00 00 ... +0x020..027 ... marker@+0x1F0` host sector.

Simple-checksum explanations were independently rejected.  The two first
DWORDs `0x459C7EB5` / `0x22A482A8` are not standard CRC32 of the v3.72
FW/BN whole files or their marker-page subregions.  For the strict physical
sample, `0x459C7EB5` is also not the EDP `crc32_bare` of any individual
LBA0-LBA12 sector, the whole 6656-byte image, the image with LBA3 zeroed,
device_id, capacity, sector count or onlyid.  Therefore there is currently no
evidence that `+0x020..0x023` is a simple host-side or firmware-file checksum.

### Existing local capture metadata cannot select PS2307 versus PS2309

The historical `a8 82 a4 22 00 20 02 16` profile exists in two byte-identical
back-to-back captures from 2026-08-03.  Their sidecars record EDP device_id,
hash/time and partition facts only.  The 2026-08-27 Kingston sidecars add
VID/PID/capacity but still omit USB serial, USB/SCSI revision, controller,
firmware, ID_BLK and NAND ID.  Saved macOS ioreg snapshots likewise contain no
matching Kingston instance.  Consequently the local archive cannot identify
either nonzero LBA3 profile as PS2307 or PS2309; the two public same-identity
reports remain hypotheses rather than target attribution.

### Current strict interpretation

The committed LBA3 evidence remains:

- EDP itself preserves and ignores this sector.
- Real devices show a zero profile and at least two non-zero
  `"this is mp mark\0"` profiles; the two nonzero profiles share one exact 472B tail while their dynamic first40B differ.
- The manufacturer family is Phison MP/FW; v3.72 final FW/BN marker pages independently reproduce that 472B tail after the observed 16B layout reordering.
- Exact `WriteF2Mark` and FW/BN compatibility-reader addresses are now recovered, but they do **not** construct the host sparse record.
- The exact writer that constructs the committed `00 01 00 00 ... this is mp mark\0` profile,
  the meaning of `+0x020..+0x027`, and the controller-firmware consumer remain unresolved.

Therefore LBA3 is 512B COMPLETE at the EDP protocol layer as a manufacturer-owned opaque preserve-only sector; the unresolved items above remain manufacturer-provenance questions.

### Next concrete reverse-engineering targets

1. Acquire and fingerprint the exact target-era PS2307 MPALL v3.34.07 and
   PS2309 MPALL v5.35.35 families (plus matching FW/BN where available), keeping
   the two controller hypotheses separate.
2. Search those generations for the sparse host-record constructor, not merely
   the already-disproved INFO-page `WriteF2Mark` path.
3. Trace the `F2 Merged` / controller firmware path to determine how a formal
   FW/BN marker page becomes the host-visible
   `marker@+0x1F0` manufacturing record and what generates `+0x020..0x027`.
4. Obtain device-specific controller/FW/ID_BLK evidence for the exact physical
   target before selecting the PS2307 or PS2309 branch.
5. Keep any recovered Phison-private subfield semantics separate from the EDP byte ledger; they may enrich manufacturer provenance but do not change LBA3's preserve-only EDP ownership boundary.
